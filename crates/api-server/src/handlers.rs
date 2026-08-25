use {
    crate::{
        copy_lp::build_scaled_op_payload,
        copy_policy::{coefficient_ppm, validate_copy_op, PolicyReject},
        index_db::{
            CopyOpRow, CopySessionRow, IndexDb, IndexerStatus, PoolActivityRow, PoolActivitySummaryRow, PoolEventRow,
            PoolRollupRow,
        },
        pricing::{
            service::{PriceService, QuoteMeta},
            value::{coverage_for, xlm_quote_to_usd, QuoteCoverage, UsdPriceMap},
        },
        recorder::canonical_event,
        token_registry,
    },
    axum::{
        extract::{Path, Query, State},
        http::{header::CACHE_CONTROL, HeaderName, HeaderValue, StatusCode},
        response::IntoResponse,
        routing::{get, patch, post},
        Json, Router,
    },
    chrono::{DateTime, Duration, Utc},
    dex::{
        db::Db,
        positions::positions_for_venue,
        rpc::scval_to_symbol_string,
        support_matrix,
        sushi::{positions_for_candidates, positions_for_managed_pools, SushiPositionRangeCandidate},
        types::UserPosition,
        SorobanRpc, NATIVE_SAC,
    },
    redis::AsyncCommands,
    serde::{Deserialize, Serialize},
    serde_json::{json, Value},
    std::{
        collections::{HashMap, HashSet},
        sync::atomic::{AtomicBool, Ordering},
        sync::{Arc, Mutex},
        time::{Duration as StdDuration, Instant as StdInstant},
    },
    tokio::task::JoinSet,
    tracing::info,
};

#[derive(Clone)]
pub struct AppState {
    pub rpc: Arc<SorobanRpc>,
    pub db: Arc<Mutex<Db>>,
    pub index_db: Arc<Mutex<IndexDb>>,
    pub token_meta_cache: Arc<Mutex<HashMap<String, TokenMeta>>>,
    pub prices: Arc<PriceService>,
    pub pool_list_cache: Arc<Mutex<Option<(StdInstant, Value)>>>,
    /// Serializes a full pool-list rebuild so concurrent cold requests do not
    /// all repeat the RPC, pricing, and rollup work.
    pub pool_list_refresh: Arc<tokio::sync::Mutex<()>>,
    pub pool_list_refreshing: Arc<AtomicBool>,
    pub redis: Option<redis::Client>,
    pub leader_fee_scan_cursor: Arc<std::sync::atomic::AtomicUsize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenMeta {
    pub address: String,
    pub symbol: String,
    pub name: Option<String>,
    pub issuer: Option<String>,
    pub domain: Option<String>,
    pub icon: Option<String>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/health", get(health))
        .route("/v1/venues", get(list_venues))
        .route("/v1/indexer/status", get(indexer_status))
        .route("/v1/pools", get(list_pools))
        .route("/v1/pools/{address}", get(pool_detail))
        .route("/v1/pools/{address}/events", get(pool_events))
        .route("/v1/pools/{address}/history", get(pool_history))
        .route("/v1/positions", get(list_positions))
        .route("/v1/positions/summary", get(positions_summary))
        .route("/v1/lp/profile", get(lp_profile))
        .route("/v1/lp/leaders", get(lp_leaders))
        .route("/v1/copy/sessions", post(create_copy_session).get(list_copy_sessions))
        .route("/v1/copy/sessions/{id}", patch(update_copy_session_handler))
        .route("/v1/copy/sessions/{id}/ops", get(list_copy_ops))
        .route("/v1/copy/ops/{id}", get(get_copy_op))
        .route("/v1/copy/ops/{id}/status", post(set_copy_op_status))
        .route("/v1/copy/recorder/status", get(recorder_status))
}

const WINDOWS: [(&str, i64); 4] = [("5m", 5), ("1h", 60), ("6h", 360), ("24h", 1_440)];
const SCORE_TVL_FLOOR: f64 = 10.0;
const SCORE_TVL_FLOOR_USD: f64 = 0.05;
const DUST_TVL_FLOOR: f64 = 1e-6;
const REDIS_TOKEN_META_TTL_SECS: u64 = 3_600;
const REDIS_LP_PROFILE_TTL_SECS: u64 = 60;
const REDIS_LP_LEADERS_TTL_SECS: u64 = 60;

fn redis_token_meta_key(address: &str) -> String {
    format!("lumenlp:token-meta:v1:{address}")
}

fn redis_lp_profile_key(address: &str) -> String {
    format!("lumenlp:lp-profile:v3:{address}")
}

fn redis_lp_leaders_key(window_days: i64, limit: usize, sort: &str) -> String {
    format!("lumenlp:lp-leaders:v2:{window_days}:{limit}:{sort}")
}

async fn invalidate_lp_leaders_cache(redis: &redis::Client) {
    // The UI uses a small, bounded set of board variants. Delete them after
    // the background fee refresh so a successful refresh is visible at once.
    let keys = [1_i64, 7, 30, 90]
        .into_iter()
        .flat_map(|window| {
            [25_usize, 100, 500]
                .into_iter()
                .flat_map(move |limit| {
                    ["fees", "activity"]
                        .into_iter()
                        .map(move |sort| redis_lp_leaders_key(window, limit, sort))
                })
        })
        .collect::<Vec<_>>();
    if let Ok(mut connection) = redis.get_multiplexed_async_connection().await {
        let _: redis::RedisResult<usize> = redis::cmd("DEL").arg(keys).query_async(&mut connection).await;
    }
}
// Rollups are generated from bucketed snapshots. A short grace period covers
// bucket alignment and the indexer's polling interval, but prevents an old
// rollup from being presented as a current activity window.
const ROLLUP_FRESHNESS_GRACE_SECS: i64 = 5 * 60;

fn cadence_sort_value(value: Option<f64>) -> f64 {
    match value {
        Some(value) if value.is_finite() && value > 0.0 => 1.0 / value,
        _ => 0.0,
    }
}

fn pool_score_json(
    tvl: f64,
    window_metrics: &Value,
    activity_summary: Option<&PoolActivitySummaryRow>,
    volume_24h_override: Option<f64>,
    net_liq_override: Option<f64>,
    fee_24h_override: Option<f64>,
) -> Value {
    pool_score_json_with_floor(
        tvl,
        window_metrics,
        activity_summary,
        volume_24h_override,
        net_liq_override,
        fee_24h_override,
        SCORE_TVL_FLOOR,
    )
}

fn pool_score_json_with_floor(
    tvl: f64,
    window_metrics: &Value,
    activity_summary: Option<&PoolActivitySummaryRow>,
    volume_24h_override: Option<f64>,
    net_liq_override: Option<f64>,
    fee_24h_override: Option<f64>,
    score_tvl_floor: f64,
) -> Value {
    // Below this floor, fee/TVL is dominated by dust and stale rollup history
    // rather than usable liquidity. Keep these pools visible, but remove them
    // from ranking signals until they have meaningful current TVL.
    // A pool can be non-zero while still being economically meaningless. Use
    // the same usable-liquidity floor for every ranking component so cadence
    // or net-flow activity cannot push a dust pool to the top.
    let has_usable_liquidity = tvl.is_finite() && tvl >= score_tvl_floor;
    let metrics_24h = window_metrics.get("24h");
    let reported_fee_tvl = metrics_24h
        .and_then(|v| v.get("fee_tvl"))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let fee = fee_24h_override.or_else(|| {
        metrics_24h
            .and_then(|v| v.get("fee"))
            .and_then(|v| v.as_f64())
            .filter(|value| value.is_finite() && *value >= 0.0)
    });
    let volume = volume_24h_override.unwrap_or_else(|| {
        metrics_24h
            .and_then(|v| v.get("volume"))
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0)
    });
    // A tiny pool can produce a mathematically huge Fee/TVL from a few cents
    // of fees. Keep that signal visible, but prevent it from dominating rank.
    let liquidity = tvl.max(score_tvl_floor);
    let fee_tvl = fee
        .filter(|_| has_usable_liquidity)
        .map(|value| value / liquidity)
        .unwrap_or_else(|| if tvl >= score_tvl_floor { reported_fee_tvl } else { 0.0 });
    let volume_efficiency = if has_usable_liquidity { volume / liquidity } else { 0.0 };
    let net_liq =
        net_liq_override.unwrap_or_else(|| activity_summary.map(|s| s.net_liquidity_delta_quote_24h).unwrap_or(0.0));
    let net_liq_ratio = if has_usable_liquidity { net_liq / liquidity } else { 0.0 };
    let cadence = if has_usable_liquidity {
        cadence_sort_value(activity_summary.and_then(|s| s.avg_update_interval_secs_24h))
    } else {
        0.0
    };

    let fee_tvl_component = fee_tvl * 10_000.0;
    let volume_component = volume_efficiency * 200.0;
    let net_liq_component = net_liq_ratio * 100.0;
    let cadence_component = cadence * 3_600.0;
    let score = fee_tvl_component + volume_component + net_liq_component + cadence_component;

    json!({
        "score": score,
        "score_breakdown": {
            "fee_tvl_component": fee_tvl_component,
            "volume_component": volume_component,
            "net_liq_component": net_liq_component,
            "cadence_component": cadence_component,
            "inputs": {
                "fee_tvl_24h": fee_tvl,
                "volume_24h": volume,
                "tvl": tvl,
                "net_liquidity_delta_quote_24h": net_liq,
                "avg_update_interval_secs_24h": activity_summary.and_then(|s| s.avg_update_interval_secs_24h),
            }
        }
    })
}

fn quote_json(meta: &QuoteMeta) -> Value {
    json!({
        "currency": meta.currency,
        "as_of": meta.as_of,
        "source": meta.source,
        "xlm_usd": meta.xlm_usd,
        "coverage": meta.coverage,
    })
}

fn wanted_tokens_from_meta(
    token_meta_map: &HashMap<String, TokenMeta>,
) -> Vec<(String, Option<String>, Option<String>, Option<String>)> {
    let mut wanted: Vec<_> = token_meta_map
        .values()
        .map(|meta| {
            (
                meta.address.clone(),
                Some(meta.symbol.clone()),
                meta.name.clone(),
                meta.issuer.clone(),
            )
        })
        .collect();
    wanted.sort_by(|a, b| a.0.cmp(&b.0));
    wanted.dedup_by(|a, b| a.0 == b.0);
    wanted
}

fn coverage_label(tokens: &[String], prices: &UsdPriceMap) -> String {
    match coverage_for(tokens, prices) {
        QuoteCoverage::Full => "full".into(),
        QuoteCoverage::Partial => "partial".into(),
        QuoteCoverage::None => "none".into(),
    }
}

/// Bridge XLM quote amounts → USD for window rollups.
///
/// Snapshot `reserves` are raw u128 base units and `list_pools_with_latest`
/// does not expose them; token decimals are also unavailable here. Multiplying
/// raw reserves by Freighter human-unit prices would be wrong, so v1 always
/// bridges TVL via `xlm_usd`. True reserve×price can wait until decimals are
/// wired.
fn bridge_tvl_usd(latest_tvl: f64, xlm_usd: Option<f64>) -> Option<f64> {
    if !(latest_tvl.is_finite() && latest_tvl > 0.0) {
        return None;
    }
    xlm_usd.and_then(|px| xlm_quote_to_usd(latest_tvl, px))
}

/// Return the newest reserve quote and its event timestamp.
fn latest_reserves_quote_xlm_from_events(events: &[PoolEventRow]) -> Option<(i64, f64)> {
    for event in events {
        if event.kind != "update_reserves" && event.kind != "reserves_sync" {
            continue;
        }
        let quote = event
            .body
            .get("derived")
            .and_then(|d| d.get("reserves_quote_xlm"))
            .and_then(|v| v.as_f64())
            .or_else(|| {
                event
                    .body
                    .pointer("/derived/reserves_quote_xlm")
                    .and_then(|v| v.as_f64())
            });
        if let Some(q) = quote.filter(|v| v.is_finite() && *v > 0.0) {
            return Some((event.created_at, q));
        }
    }
    None
}

/// Prefer latest event-derived reserves quote when snapshot TVL is missing.
#[cfg(test)]
fn reserves_quote_xlm_from_events(events: &[PoolEventRow]) -> Option<f64> {
    latest_reserves_quote_xlm_from_events(events).map(|(_, quote)| quote)
}

/// When rollups/snapshots are empty but indexer has activity, seed the 24h
/// window.
fn fill_window_from_activity(metrics: &mut Value, summary: Option<&PoolActivitySummaryRow>) {
    let Some(summary) = summary else {
        return;
    };
    let Some(obj) = metrics.as_object_mut() else {
        return;
    };
    let entry = obj.entry("24h".to_string()).or_insert_with(|| {
        json!({
            "samples": 0,
            "volume": 0.0,
            "fee": 0.0,
            "avg_tvl": 0.0,
            "fee_tvl": 0.0,
        })
    });
    let Some(w) = entry.as_object_mut() else {
        return;
    };
    let volume = w.get("volume").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let fee = w.get("fee").and_then(|v| v.as_f64()).unwrap_or(0.0);
    if volume <= 0.0 && summary.volume_quote_24h > 0.0 {
        w.insert("volume".into(), json!(summary.volume_quote_24h));
        w.insert(
            "samples".into(),
            json!(w.get("samples").and_then(|v| v.as_u64()).unwrap_or(0).max(1)),
        );
    }
    if fee <= 0.0 && summary.fee_quote_24h > 0.0 {
        w.insert("fee".into(), json!(summary.fee_quote_24h));
    }
    if w.get("tx_count").and_then(|v| v.as_u64()).unwrap_or(0) == 0 && summary.swap_count_24h > 0 {
        w.insert("tx_count".into(), json!(summary.swap_count_24h));
    }
    let avg_tvl = w.get("avg_tvl").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let volume = w.get("volume").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let fee = w.get("fee").and_then(|v| v.as_f64()).unwrap_or(0.0);
    if avg_tvl > 0.0 {
        w.insert("fee_tvl".into(), json!(fee / avg_tvl));
    } else if volume > 0.0 {
        // leave fee_tvl at 0 when TVL unknown
    }
}

/// Fee/TVL is presented against the pool's current TVL. Rollups retain
/// average TVL for historical context, but using it for the headline ratio
/// can make a recently refilled pool look artificially profitable.
fn recompute_fee_tvl_with_current_tvl(metrics: &mut Value, current_tvl: f64) {
    let ratio = if current_tvl.is_finite() && current_tvl >= DUST_TVL_FLOOR {
        Some(current_tvl)
    } else {
        None
    };
    let Some(windows) = metrics.as_object_mut() else {
        return;
    };
    for window in windows.values_mut() {
        let Some(window) = window.as_object_mut() else {
            continue;
        };
        let fee = window.get("fee").and_then(Value::as_f64).unwrap_or(0.0);
        let fee_tvl = ratio
            .filter(|_| fee.is_finite() && fee >= 0.0)
            .map(|tvl| fee / tvl)
            .unwrap_or(0.0);
        window.insert("fee_tvl".into(), json!(fee_tvl));
    }
}

fn enrich_window_metrics_usd(metrics: &mut Value, xlm_usd: Option<f64>) {
    let Some(xlm_usd) = xlm_usd else {
        return;
    };
    let Some(obj) = metrics.as_object_mut() else {
        return;
    };
    for window in obj.values_mut() {
        let Some(w) = window.as_object_mut() else {
            continue;
        };
        if let Some(volume) = w.get("volume").and_then(|v| v.as_f64()) {
            w.insert("volume_usd".into(), json!(xlm_quote_to_usd(volume, xlm_usd)));
        }
        if let Some(fee) = w.get("fee").and_then(|v| v.as_f64()) {
            w.insert("fee_usd".into(), json!(xlm_quote_to_usd(fee, xlm_usd)));
        }
    }
}

fn enrich_activity_summary_usd(summary: &mut Value, xlm_usd: Option<f64>) {
    let Some(xlm_usd) = xlm_usd else {
        return;
    };
    let Some(obj) = summary.as_object_mut() else {
        return;
    };
    for (src, dst) in [
        ("volume_quote_24h", "volume_usd_24h"),
        ("fee_quote_24h", "fee_usd_24h"),
        ("deposit_quote_24h", "deposit_usd_24h"),
        ("withdraw_quote_24h", "withdraw_usd_24h"),
        ("net_liquidity_delta_quote_24h", "net_liquidity_delta_usd_24h"),
        ("claim_quote_24h", "claim_usd_24h"),
    ] {
        if let Some(xlm) = obj.get(src).and_then(|v| v.as_f64()) {
            obj.insert(dst.into(), json!(xlm_quote_to_usd(xlm, xlm_usd)));
        }
    }
}

/// Bridge event `*_quote_xlm` → `*_quote_usd`.
///
/// Event amounts are raw base units and token decimals are not available here,
/// so v1 bridges via `xlm_usd` (same approach as pool list/detail rollups).
fn enrich_event_derived_usd(body: &mut Value, xlm_usd: Option<f64>) {
    let Some(xlm_usd) = xlm_usd else {
        return;
    };
    let Some(derived) = body.get_mut("derived").and_then(|v| v.as_object_mut()) else {
        return;
    };
    for (src, dst) in [
        ("volume_quote_xlm", "volume_quote_usd"),
        ("fee_quote_xlm", "fee_quote_usd"),
        ("reserves_quote_xlm", "reserves_quote_usd"),
        ("total_quote_xlm", "total_quote_usd"),
    ] {
        if let Some(xlm) = derived.get(src).and_then(|v| v.as_f64()) {
            derived.insert(dst.into(), json!(xlm_quote_to_usd(xlm, xlm_usd)));
        }
    }
}

fn score_with_usd_preference(
    latest_tvl: f64,
    tvl_usd: Option<f64>,
    metrics: &Value,
    activity_summary: Option<&PoolActivitySummaryRow>,
    xlm_usd: Option<f64>,
) -> Value {
    let volume_usd = metrics
        .get("24h")
        .and_then(|v| v.get("volume_usd"))
        .and_then(|v| v.as_f64());
    // `tvl_usd` and window USD values are currently bridged from the XLM quote
    // even when individual token prices are incomplete. Keep list and detail
    // scoring consistent instead of falling back based on global coverage.
    let use_usd = tvl_usd.is_some() && volume_usd.is_some();
    if use_usd {
        let net_liq_usd =
            activity_summary.and_then(|s| xlm_usd.and_then(|px| xlm_quote_to_usd(s.net_liquidity_delta_quote_24h, px)));
        let fee_usd = metrics
            .get("24h")
            .and_then(|v| v.get("fee_usd"))
            .and_then(|v| v.as_f64());
        pool_score_json_with_floor(
            tvl_usd.unwrap_or(latest_tvl),
            metrics,
            activity_summary,
            volume_usd,
            net_liq_usd,
            fee_usd,
            SCORE_TVL_FLOOR_USD,
        )
    } else {
        pool_score_json(latest_tvl, metrics, activity_summary, None, None, None)
    }
}

fn build_window_metrics(latest_tvl: f64, fee_bps: u32, rows: &[dex::types::PoolSnapshotRow]) -> serde_json::Value {
    let now = Utc::now();
    let fee_rate = f64::from(fee_bps) / 10_000.0;
    let mut out = serde_json::Map::new();

    for (label, minutes) in WINDOWS {
        let since = now - Duration::minutes(minutes);
        let mut samples = 0usize;
        let mut volume = 0.0f64;
        let mut tvl_sum = 0.0f64;

        for row in rows {
            let Ok(ts) = chrono::DateTime::parse_from_rfc3339(&row.ts) else {
                continue;
            };
            if ts.with_timezone(&Utc) < since {
                continue;
            }
            samples += 1;
            volume += row.volume_24h.max(0.0);
            tvl_sum += row.tvl.max(0.0);
        }

        let avg_tvl = if samples > 0 {
            tvl_sum / samples as f64
        } else {
            latest_tvl.max(0.0)
        };
        let fee = volume * fee_rate;
        let fee_tvl = if avg_tvl > 0.0 { fee / avg_tvl } else { 0.0 };

        out.insert(
            label.to_string(),
            json!({
                "samples": samples,
                "volume": volume,
                "fee": fee,
                "avg_tvl": avg_tvl,
                "fee_tvl": fee_tvl,
            }),
        );
    }

    serde_json::Value::Object(out)
}

fn rollups_to_window_metrics(rollups: &HashMap<String, PoolRollupRow>, now_ts: i64) -> serde_json::Value {
    let mut out = serde_json::Map::new();
    for row in rollups.values() {
        let Some((_, window_minutes)) = WINDOWS.iter().find(|(label, _)| *label == row.window) else {
            continue;
        };
        let window_secs = window_minutes * 60;
        if row.as_of_ts < now_ts - window_secs - ROLLUP_FRESHNESS_GRACE_SECS {
            continue;
        }
        out.insert(
            row.window.clone(),
            json!({
                "samples": row.sample_count,
                "volume": row.volume_quote,
                "fee": row.fee_quote,
                "avg_tvl": row.avg_tvl,
                "fee_tvl": row.fee_tvl,
                "tx_count": row.tx_count,
                "as_of_ts": row.as_of_ts,
            }),
        );
    }
    serde_json::Value::Object(out)
}

fn activity_json(activity: &PoolActivityRow) -> serde_json::Value {
    json!({
        "first_event_at": activity.first_event_at,
        "last_event_at": activity.last_event_at,
        "event_count": activity.event_count,
        "swap_count": activity.swap_count,
    })
}

fn activity_summary_json(summary: &PoolActivitySummaryRow) -> serde_json::Value {
    json!({
        "event_count_24h": summary.event_count_24h,
        "swap_count_24h": summary.swap_count_24h,
        "volume_quote_24h": summary.volume_quote_24h,
        "fee_quote_24h": summary.fee_quote_24h,
        "deposit_quote_24h": summary.deposit_quote_24h,
        "withdraw_quote_24h": summary.withdraw_quote_24h,
        "net_liquidity_delta_quote_24h": summary.net_liquidity_delta_quote_24h,
        "claim_quote_24h": summary.claim_quote_24h,
        "avg_update_interval_secs_24h": summary.avg_update_interval_secs_24h,
        "latest_update_at_24h": summary.latest_update_at_24h,
        "deposit_count_24h": summary.deposit_count_24h,
        "withdraw_count_24h": summary.withdraw_count_24h,
        "claim_count_24h": summary.claim_count_24h,
        "update_count_24h": summary.update_count_24h,
    })
}

fn event_row_json(event: &PoolEventRow) -> serde_json::Value {
    json!({
        "event_id": event.event_id,
        "tx_hash": event.tx_hash,
        "ledger": event.ledger,
        "created_at": event.created_at,
        "pool_address": event.pool_address,
        "kind": event.kind,
        "body": event.body,
    })
}

fn token_meta_json(meta: &TokenMeta) -> serde_json::Value {
    json!({
        "address": meta.address,
        "symbol": meta.symbol,
        "name": meta.name,
        "issuer": meta.issuer,
        "domain": meta.domain,
        "icon": meta.icon,
    })
}

async fn health() -> impl IntoResponse {
    Json(json!({ "ok": true }))
}

async fn recorder_status(State(state): State<AppState>) -> impl IntoResponse {
    let index_db = state.index_db.lock().unwrap();
    match index_db.recorder_outbox_status() {
        Ok(status) => Json(json!({
            "pending": status.pending,
            "submitted": status.submitted,
            "failed": status.failed,
            "oldest_pending_at": status.oldest_pending_at,
        }))
        .into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": error.to_string(), "code": "db_error" })),
        )
            .into_response(),
    }
}

/// Multi-DEX support matrix (`DexAdaptor` registry). Aquarius supports live
/// Copy LP; other venues expose read/indexed analytics while execution remains
/// fail-closed.
async fn list_venues() -> impl IntoResponse {
    let venues = support_matrix()
        .into_iter()
        .map(|row| {
            json!({
                "venue_id": row.venue_id,
                "name": row.name,
                "status": row.status,
                "copy_execution_enabled": row.copy_execution_enabled,
                "capabilities": row.capabilities,
                "notes": row.notes,
            })
        })
        .collect::<Vec<_>>();
    Json(json!({
        "venues": venues,
        "note": "Strategies bind to venue_id. Read/indexed analytics are available per venue; live Copy LP execution remains limited to production venues.",
    }))
}

fn indexer_status_json(status: &IndexerStatus) -> serde_json::Value {
    json!({
        "cursor_ledger": status.cursor_ledger,
        "event_count": status.event_count,
        "swap_count": status.swap_count,
        "rollup_count": status.rollup_count,
        "distinct_event_pools": status.distinct_event_pools,
        "distinct_rollup_pools": status.distinct_rollup_pools,
        "last_event_at": status.last_event_at,
        "last_rollup_at": status.last_rollup_at,
    })
}

async fn indexer_status(State(state): State<AppState>) -> impl IntoResponse {
    let index_db = state.index_db.lock().unwrap();
    match index_db.status() {
        Ok(status) => Json(indexer_status_json(&status)).into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": error.to_string(), "code": "db_error" })),
        )
            .into_response(),
    }
}

#[derive(Debug, Default, Deserialize)]
struct PoolListQuery {
    page: Option<usize>,
    /// Accept the common REST spelling as an alias for `limit`.
    #[serde(alias = "page_size")]
    limit: Option<usize>,
    q: Option<String>,
    /// `dex` is the public UI spelling; `venue` is retained for API clients.
    dex: Option<String>,
    venue: Option<String>,
    #[serde(rename = "refresh")]
    refresh: Option<bool>,
}

fn paginate_pool_body(mut body: Value, query: &PoolListQuery) -> Value {
    let Some(pools) = body.get_mut("pools").and_then(Value::as_array_mut) else {
        return body;
    };
    let requested_venue = query
        .dex
        .as_deref()
        .or(query.venue.as_deref())
        .map(str::trim)
        .filter(|venue| !venue.is_empty() && !venue.eq_ignore_ascii_case("all"));
    let requested_venue = requested_venue.map(|venue| {
        match venue.to_ascii_lowercase().as_str() {
            "soroswap" => "soroswap_amm",
            "sushi" => "sushi_v3",
            other => other,
        }
        .to_string()
    });
    if let Some(query_text) = query
        .q
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let query_text = query_text.to_ascii_lowercase();
        pools.retain(|pool| pool_matches_query(pool, &query_text));
    }
    if let Some(requested_venue) = requested_venue {
        pools.retain(|pool| {
            let Some(venue) = pool.get("venue").and_then(Value::as_str) else {
                return false;
            };
            match requested_venue.as_str() {
                "soroswap_amm" => venue.eq_ignore_ascii_case("soroswap") || venue.eq_ignore_ascii_case("soroswap_amm"),
                "sushi_v3" => venue.eq_ignore_ascii_case("sushi") || venue.eq_ignore_ascii_case("sushi_v3"),
                _ => venue.eq_ignore_ascii_case(&requested_venue),
            }
        });
    }
    let total = pools.len();
    // Keep the normal UI request to a single page while allowing clients to
    // fetch the full catalogue without opening one request per 100 pools.
    let limit = query.limit.unwrap_or(total.max(1)).clamp(1, 500);
    let page = query.page.unwrap_or(1).max(1);
    let start = page.saturating_sub(1).saturating_mul(limit).min(total);
    let end = start.saturating_add(limit).min(total);
    let page_rows = pools[start..end].to_vec();
    *pools = page_rows;
    body["pagination"] = json!({
        "page": page,
        "limit": limit,
        "total": total,
        "pages": total.div_ceil(limit),
    });
    body
}

fn pool_matches_query(pool: &Value, query: &str) -> bool {
    let mut haystack = String::new();
    for key in ["address", "venue", "pool_type"] {
        if let Some(value) = pool.get(key).and_then(Value::as_str) {
            haystack.push_str(value);
            haystack.push('\n');
        }
    }
    if let Some(tokens) = pool.get("tokens").and_then(Value::as_array) {
        for token in tokens.iter().filter_map(Value::as_str) {
            haystack.push_str(token);
            haystack.push('\n');
        }
    }
    if let Some(metadata) = pool.get("token_meta").and_then(Value::as_array) {
        for item in metadata {
            for key in ["address", "symbol", "name", "issuer", "domain"] {
                if let Some(value) = item.get(key).and_then(Value::as_str) {
                    haystack.push_str(value);
                    haystack.push('\n');
                }
            }
        }
    }
    haystack.to_ascii_lowercase().contains(query)
}

async fn list_pools(State(state): State<AppState>, Query(query): Query<PoolListQuery>) -> impl IntoResponse {
    const POOL_LIST_CACHE_SECS: u64 = 60;
    // Keep a shared Redis copy beyond the local refresh interval so a request
    // does not synchronously rebuild the full multi-DEX ranking catalogue.
    const REDIS_POOL_LIST_CACHE_SECS: u64 = 60;
    // Bump this when derived pool metrics change so a deploy cannot serve an
    // older catalogue with incompatible score or Fee/TVL calculations.
    const REDIS_POOL_LIST_KEY: &str = "lumenlp:pools:v2";
    if query.refresh != Some(true) {
        if let Some(body) = {
            let cache = state.pool_list_cache.lock().unwrap();
            cache
                .as_ref()
                .and_then(|(expires_at, body)| (*expires_at > StdInstant::now()).then(|| body.clone()))
        } {
            let mut response = Json(paginate_pool_body(body, &query)).into_response();
            response.headers_mut().insert(
                HeaderName::from_static("x-lumenlp-cache"),
                HeaderValue::from_static("pool-local-hit"),
            );
            response.headers_mut().insert(
                CACHE_CONTROL,
                HeaderValue::from_static("public, max-age=60, s-maxage=60, stale-while-revalidate=120"),
            );
            response.headers_mut().insert(
                HeaderName::from_static("cloudflare-cdn-cache-control"),
                HeaderValue::from_static("public, max-age=60, stale-while-revalidate=120"),
            );
            return response;
        }
    }
    // Do not make an expired catalogue a user-visible cold start. Serve the
    // last complete body immediately and let the refresh lock serialize a
    // background rebuild for the next request.
    if query.refresh != Some(true) {
        if let Some(body) = {
            let cache = state.pool_list_cache.lock().unwrap();
            cache.as_ref().map(|(_, body)| body.clone())
        } {
            if state
                .pool_list_refreshing
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                let refresh_state = state.clone();
                let refresh_flag = Arc::clone(&state.pool_list_refreshing);
                std::thread::spawn(move || {
                    let Ok(runtime) = tokio::runtime::Builder::new_current_thread().enable_all().build() else {
                        refresh_flag.store(false, Ordering::Release);
                        return;
                    };
                    runtime.block_on(async move {
                        let _ = list_pools(
                            State(refresh_state),
                            Query(PoolListQuery {
                                refresh: Some(true),
                                ..PoolListQuery::default()
                            }),
                        )
                        .await;
                    });
                    refresh_flag.store(false, Ordering::Release);
                });
            }
            let mut response = Json(paginate_pool_body(body, &query)).into_response();
            response.headers_mut().insert(
                CACHE_CONTROL,
                HeaderValue::from_static("public, max-age=60, s-maxage=60, stale-while-revalidate=120"),
            );
            response.headers_mut().insert(
                HeaderName::from_static("cloudflare-cdn-cache-control"),
                HeaderValue::from_static("public, max-age=60, stale-while-revalidate=120"),
            );
            response.headers_mut().insert(
                HeaderName::from_static("x-lumenlp-cache"),
                HeaderValue::from_static("stale"),
            );
            return response;
        }
    }
    let _refresh_guard = state.pool_list_refresh.lock().await;
    // Another request may have populated the local cache while this request
    // waited for the refresh lock.
    if let Some(body) = {
        let cache = state.pool_list_cache.lock().unwrap();
        cache
            .as_ref()
            .and_then(|(expires_at, body)| (*expires_at > StdInstant::now()).then(|| body.clone()))
    } {
        let mut response = Json(paginate_pool_body(body, &query)).into_response();
        response.headers_mut().insert(
            HeaderName::from_static("x-lumenlp-cache"),
            HeaderValue::from_static("pool-local-hit"),
        );
        response.headers_mut().insert(
            CACHE_CONTROL,
            HeaderValue::from_static("public, max-age=60, s-maxage=60, stale-while-revalidate=120"),
        );
        response.headers_mut().insert(
            HeaderName::from_static("cloudflare-cdn-cache-control"),
            HeaderValue::from_static("public, max-age=60, stale-while-revalidate=120"),
        );
        return response;
    }
    if let Some(client) = state.redis.clone() {
        if let Ok(mut connection) = client.get_multiplexed_async_connection().await {
            let cached: redis::RedisResult<Option<String>> = connection.get(REDIS_POOL_LIST_KEY).await;
            if let Ok(Some(serialized)) = cached {
                if let Ok(body) = serde_json::from_str::<Value>(&serialized) {
                    {
                        let mut cache = state.pool_list_cache.lock().unwrap();
                        *cache = Some((
                            StdInstant::now() + StdDuration::from_secs(POOL_LIST_CACHE_SECS),
                            body.clone(),
                        ));
                    }
                    let mut response = Json(paginate_pool_body(body, &query)).into_response();
                    response.headers_mut().insert(
                        HeaderName::from_static("x-lumenlp-cache"),
                        HeaderValue::from_static("pool-redis-hit"),
                    );
                    response.headers_mut().insert(
                        CACHE_CONTROL,
                        HeaderValue::from_static("public, max-age=60, s-maxage=60, stale-while-revalidate=120"),
                    );
                    response.headers_mut().insert(
                        HeaderName::from_static("cloudflare-cdn-cache-control"),
                        HeaderValue::from_static("public, max-age=60, stale-while-revalidate=120"),
                    );
                    return response;
                }
            }
        }
    }
    let (
        rows,
        stats,
        window_rows,
        rollups_map,
        activity_map,
        activity_summary_map,
        latest_reserves_map,
        indexer_status,
    ) = {
        let db = state.db.lock().unwrap();
        let index_db = state.index_db.lock().unwrap();
        (
            db.list_pools_with_latest(),
            db.stats(),
            db.snapshots_since(&(Utc::now() - Duration::hours(24)).to_rfc3339()),
            index_db.rollups_map().unwrap_or_default(),
            index_db.pool_activity_map().unwrap_or_default(),
            index_db
                .pool_activity_summary_map(Utc::now().timestamp() - 24 * 60 * 60)
                .unwrap_or_default(),
            index_db.latest_reserves_quote_xlm_map().unwrap_or_default(),
            index_db.status().ok(),
        )
    };
    match (rows, stats, window_rows) {
        (Ok(mut rows), Ok(stats), Ok(window_rows)) => {
            let mut grouped: HashMap<String, Vec<dex::types::PoolSnapshotRow>> = HashMap::new();
            for row in window_rows {
                grouped.entry(row.pool_address.clone()).or_default().push(row);
            }
            let token_ids: HashSet<String> = rows
                .iter()
                .filter_map(|row| row.get("tokens").and_then(|v| v.as_array()).cloned())
                .flat_map(|tokens| tokens.into_iter())
                .filter_map(|token| token.as_str().map(ToOwned::to_owned))
                .collect();
            let token_meta_map = resolve_token_meta_map(
                state.rpc.clone(),
                state.index_db.clone(),
                &state.token_meta_cache,
                state.redis.clone(),
                &token_ids,
            )
            .await;
            let wanted = wanted_tokens_from_meta(&token_meta_map);
            let (price_map, mut quote_meta) = state.prices.prices_for_tokens(&wanted).await;
            let all_token_ids: Vec<String> = token_ids.iter().cloned().collect();
            quote_meta.coverage = coverage_label(&all_token_ids, &price_map);
            let xlm_usd = quote_meta.xlm_usd;
            // v1 amounts are XLM-quote × xlm_usd (not reserve×token yet).
            if xlm_usd.is_some() {
                quote_meta.source = "xlm_bridge".into();
            }
            for row in &mut rows {
                let Some(obj) = row.as_object_mut() else {
                    continue;
                };
                let Some(address) = obj.get("address").and_then(|v| v.as_str()) else {
                    continue;
                };
                let address = address.to_string();
                let snapshot_tvl = obj.get("tvl").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let snapshot_ts = obj
                    .get("last_snapshot_at")
                    .and_then(|value| value.as_str())
                    .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
                    .map(|value| value.timestamp())
                    .unwrap_or(0);
                let latest_tvl = latest_reserves_map
                    .get(&address)
                    .filter(|(event_ts, _)| snapshot_tvl <= 0.0 || event_ts > &snapshot_ts)
                    .map(|(_, quote)| *quote)
                    .unwrap_or(snapshot_tvl);
                if (latest_tvl - snapshot_tvl).abs() > f64::EPSILON {
                    obj.insert("tvl".into(), json!(latest_tvl));
                    obj.insert("tvl_source".into(), json!("event_reserves"));
                }
                let fee_bps = obj.get("fee_bps").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                let now_ts = Utc::now().timestamp();
                let mut metrics = rollups_map
                    .get(&address)
                    .map(|rows| rollups_to_window_metrics(rows, now_ts))
                    .filter(|value| value.as_object().is_some_and(|object| !object.is_empty()))
                    .unwrap_or_else(|| {
                        grouped
                            .get(&address)
                            .map(|rows| build_window_metrics(latest_tvl, fee_bps, rows))
                            .unwrap_or_else(|| build_window_metrics(latest_tvl, fee_bps, &[]))
                    });
                // Snapshot-derived fallback metrics do not contain swap counts;
                // restore the 24h count from the indexed activity summary.
                fill_window_from_activity(&mut metrics, activity_summary_map.get(&address));
                recompute_fee_tvl_with_current_tvl(&mut metrics, latest_tvl);
                enrich_window_metrics_usd(&mut metrics, xlm_usd);
                let tvl_usd = bridge_tvl_usd(latest_tvl, xlm_usd);
                let pool_tokens = obj
                    .get("tokens")
                    .and_then(|value| value.as_array())
                    .map(|tokens| {
                        tokens
                            .iter()
                            .filter_map(|token| token.as_str().map(ToOwned::to_owned))
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                let pool_quote_coverage = coverage_label(&pool_tokens, &price_map);
                let tvl_status = if latest_tvl > 0.0 {
                    "ok"
                } else if pool_quote_coverage == "none" {
                    "missing_price"
                } else {
                    "empty_reserves"
                };
                let activity_summary = activity_summary_map.get(&address);
                let score_json = score_with_usd_preference(latest_tvl, tvl_usd, &metrics, activity_summary, xlm_usd);
                obj.insert("tvl_usd".into(), json!(tvl_usd));
                obj.insert("tvl_status".into(), json!(tvl_status));
                obj.insert("window_metrics".into(), metrics);
                if let Some(activity) = activity_map.get(&address) {
                    obj.insert("activity".into(), activity_json(activity));
                }
                if let Some(summary) = activity_summary {
                    let mut summary_json = activity_summary_json(summary);
                    enrich_activity_summary_usd(&mut summary_json, xlm_usd);
                    obj.insert("activity_summary".into(), summary_json);
                }
                if let Some(tokens) = obj.get("tokens").and_then(|v| v.as_array()) {
                    let meta = tokens
                        .iter()
                        .filter_map(|token| token.as_str())
                        .filter_map(|token| token_meta_map.get(token))
                        .map(token_meta_json)
                        .collect::<Vec<_>>();
                    if !meta.is_empty() {
                        obj.insert("token_meta".into(), Value::Array(meta));
                    }
                }
                if let Some(score) = score_json.get("score") {
                    obj.insert("score".into(), score.clone());
                }
                if let Some(score_breakdown) = score_json.get("score_breakdown") {
                    obj.insert("score_breakdown".into(), score_breakdown.clone());
                }
            }
            let body = json!({
                "pools": rows,
                "quote": quote_json(&quote_meta),
                "indexed_pool_count": stats.pool_count,
                "last_snapshot_at": stats.latest_snapshot_at,
                "indexer_status": indexer_status.as_ref().map(indexer_status_json),
                "note": if stats.pool_count == 0 {
                    Some("No indexed pools yet — run snapshotter first")
                } else {
                    None::<&str>
                }
            });
            {
                let mut cache = state.pool_list_cache.lock().unwrap();
                *cache = Some((
                    StdInstant::now() + StdDuration::from_secs(POOL_LIST_CACHE_SECS),
                    body.clone(),
                ));
            }
            if let Some(client) = state.redis.clone() {
                if let Ok(serialized) = serde_json::to_string(&body) {
                    tokio::spawn(async move {
                        if let Ok(mut connection) = client.get_multiplexed_async_connection().await {
                            let _: redis::RedisResult<()> = connection
                                .set_ex(REDIS_POOL_LIST_KEY, serialized, REDIS_POOL_LIST_CACHE_SECS)
                                .await;
                        }
                    });
                }
            }
            let mut response = Json(paginate_pool_body(body, &query)).into_response();
            response.headers_mut().insert(
                HeaderName::from_static("x-lumenlp-cache"),
                HeaderValue::from_static("pool-origin"),
            );
            response.headers_mut().insert(
                CACHE_CONTROL,
                HeaderValue::from_static("public, max-age=60, s-maxage=60, stale-while-revalidate=120"),
            );
            response.headers_mut().insert(
                HeaderName::from_static("cloudflare-cdn-cache-control"),
                HeaderValue::from_static("public, max-age=60, stale-while-revalidate=120"),
            );
            response
        }
        (Err(e), _, _) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string(), "code": "db_error" })),
        )
            .into_response(),
        (_, Err(e), _) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string(), "code": "db_error" })),
        )
            .into_response(),
        (_, _, Err(e)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string(), "code": "db_error" })),
        )
            .into_response(),
    }
}

/// Build the expensive pool ranking cache outside the first user request.
pub async fn warm_pool_list_cache(state: AppState) {
    let _ = list_pools(State(state), Query(PoolListQuery::default())).await;
}

async fn pool_detail(State(state): State<AppState>, Path(address): Path<String>) -> impl IntoResponse {
    let (meta, history, stats) = {
        let db = state.db.lock().unwrap();
        (
            db.pool_meta(&address).ok().flatten(),
            db.history(&address, 90).unwrap_or_default(),
            db.stats().ok(),
        )
    };
    let latest = history.last();
    let snapshot_ts = latest
        .and_then(|row| DateTime::parse_from_rfc3339(&row.ts).ok())
        .map(|ts| ts.timestamp())
        .unwrap_or(i64::MIN);
    let (rollups, activity, activity_summary) = {
        let index_db = state.index_db.lock().unwrap();
        (
            index_db.rollups_for_pool(&address).unwrap_or_default(),
            index_db.pool_activity(&address).unwrap_or_default(),
            index_db
                .pool_activity_summary(&address, Utc::now().timestamp() - 24 * 60 * 60)
                .ok(),
        )
    };
    let (token_meta, token_meta_map, tokens) = if let Some((_, tokens_json, _, _)) = meta.as_ref() {
        let tokens: Vec<String> = serde_json::from_str(tokens_json).unwrap_or_default();
        if tokens.is_empty() {
            (Vec::new(), HashMap::new(), tokens)
        } else {
            let token_ids: HashSet<String> = tokens.iter().cloned().collect();
            let map = resolve_token_meta_map(
                state.rpc.clone(),
                state.index_db.clone(),
                &state.token_meta_cache,
                state.redis.clone(),
                &token_ids,
            )
            .await;
            let token_meta = tokens
                .iter()
                .filter_map(|token| map.get(token))
                .map(token_meta_json)
                .collect::<Vec<_>>();
            (token_meta, map, tokens)
        }
    } else {
        (Vec::new(), HashMap::new(), Vec::new())
    };
    let wanted = wanted_tokens_from_meta(&token_meta_map);
    let (price_map, mut quote_meta) = state.prices.prices_for_tokens(&wanted).await;
    quote_meta.coverage = coverage_label(&tokens, &price_map);
    let xlm_usd = quote_meta.xlm_usd;
    if xlm_usd.is_some() {
        quote_meta.source = "xlm_bridge".into();
    }
    let fee_bps = meta.as_ref().map(|m| m.2 as u32).unwrap_or(0);
    let mut latest_tvl = latest.map(|row| row.tvl).unwrap_or(0.0);
    let mut tvl_source = if latest_tvl > 0.0 { "snapshot" } else { "none" };
    if let Ok(events) = state.index_db.lock().unwrap().recent_pool_events(&address, 40) {
        if let Some((event_ts, reserves_xlm)) = latest_reserves_quote_xlm_from_events(&events) {
            // A pool can receive reserve updates between snapshotter runs. Do not
            // serve an older snapshot as the current TVL in that case.
            if latest_tvl <= 0.0 || event_ts > snapshot_ts {
                latest_tvl = reserves_xlm;
                tvl_source = "event_reserves";
            }
        }
    }
    let now_ts = Utc::now().timestamp();
    let mut window_metrics = if rollups.is_empty() {
        build_window_metrics(latest_tvl, fee_bps, &history)
    } else {
        let fresh = rollups_to_window_metrics(&rollups, now_ts);
        if fresh.as_object().is_some_and(|object| !object.is_empty()) {
            fresh
        } else {
            build_window_metrics(latest_tvl, fee_bps, &history)
        }
    };
    fill_window_from_activity(&mut window_metrics, activity_summary.as_ref());
    // Keep avg_tvl / fee_tvl coherent when we only have event-derived TVL.
    if let Some(w24) = window_metrics.get_mut("24h").and_then(|v| v.as_object_mut()) {
        let avg = w24.get("avg_tvl").and_then(|v| v.as_f64()).unwrap_or(0.0);
        if avg <= 0.0 && latest_tvl > 0.0 {
            w24.insert("avg_tvl".into(), json!(latest_tvl));
            let fee = w24.get("fee").and_then(|v| v.as_f64()).unwrap_or(0.0);
            w24.insert("fee_tvl".into(), json!(fee / latest_tvl));
        }
    }
    recompute_fee_tvl_with_current_tvl(&mut window_metrics, latest_tvl);
    enrich_window_metrics_usd(&mut window_metrics, xlm_usd);
    let tvl_usd = bridge_tvl_usd(latest_tvl, xlm_usd);
    let detail_tokens = meta
        .as_ref()
        .and_then(|row| serde_json::from_str::<Vec<String>>(&row.1).ok())
        .unwrap_or_default();
    let tvl_status = if latest_tvl > 0.0 {
        "ok"
    } else if coverage_label(&detail_tokens, &price_map) == "none" {
        "missing_price"
    } else {
        "empty_reserves"
    };
    let score_json =
        score_with_usd_preference(latest_tvl, tvl_usd, &window_metrics, activity_summary.as_ref(), xlm_usd);
    let activity_summary_value = activity_summary.as_ref().map(|summary| {
        let mut summary_json = activity_summary_json(summary);
        enrich_activity_summary_usd(&mut summary_json, xlm_usd);
        summary_json
    });
    let note = if latest.is_none() {
        Some(if tvl_source == "event_reserves" {
            "No snapshots for this pool yet — TVL estimated from recent reserve events; volume from indexed swaps"
        } else {
            "No snapshots found for this pool yet"
        })
    } else {
        None
    };
    Json(json!({
        "address": address,
        "venue": meta.as_ref().map(|m| m.3.clone()),
        "pool_type": meta.as_ref().map(|m| m.0.clone()),
        "tokens": meta.as_ref().and_then(|m| serde_json::from_str::<Value>(&m.1).ok()),
        "fee_bps": meta.as_ref().map(|m| m.2),
        "token_meta": if token_meta.is_empty() { None } else { Some(token_meta) },
        "latest": latest,
        "tvl": if latest_tvl > 0.0 { Some(latest_tvl) } else { None },
        "tvl_usd": tvl_usd,
        "tvl_status": tvl_status,
        "tvl_source": tvl_source,
        "activity": activity.as_ref().map(activity_json),
        "activity_summary": activity_summary_value,
        "window_metrics": window_metrics,
        "score": score_json.get("score"),
        "score_breakdown": score_json.get("score_breakdown"),
        "quote": quote_json(&quote_meta),
        "last_snapshot_at": latest.map(|row| row.ts.clone()),
        "indexed_pool_count": stats.map(|s| s.pool_count),
        "note": note,
    }))
}

async fn resolve_token_meta_map(
    rpc: Arc<SorobanRpc>,
    index_db: Arc<Mutex<IndexDb>>,
    cache: &Arc<Mutex<HashMap<String, TokenMeta>>>,
    redis: Option<redis::Client>,
    token_ids: &HashSet<String>,
) -> HashMap<String, TokenMeta> {
    let mut out = HashMap::new();
    let mut missing = Vec::new();
    {
        let cache_guard = cache.lock().unwrap();
        for token in token_ids {
            if let Some(meta) = cache_guard.get(token) {
                out.insert(token.clone(), meta.clone());
            } else {
                missing.push(token.clone());
            }
        }
    }

    // The local map is the hot path, but it is process-local. Check the shared
    // Redis cache before falling back to the database or Soroban RPC so API
    // workers do not independently hydrate the same token metadata.
    if let Some(client) = redis.as_ref() {
        if let Ok(mut connection) = client.get_multiplexed_async_connection().await {
            let candidate_tokens = std::mem::take(&mut missing);
            let keys: Vec<String> = candidate_tokens
                .iter()
                .map(|token| redis_token_meta_key(token))
                .collect();
            let cached: redis::RedisResult<Vec<Option<String>>> = connection.mget(keys).await;
            match cached {
                Ok(values) if values.len() == candidate_tokens.len() => {
                    for (token, serialized) in candidate_tokens.into_iter().zip(values) {
                        match serialized.and_then(|value| serde_json::from_str::<TokenMeta>(&value).ok()) {
                            Some(meta) => {
                                cache.lock().unwrap().insert(token.clone(), meta.clone());
                                out.insert(token, meta);
                            }
                            None => missing.push(token),
                        }
                    }
                }
                _ => missing = candidate_tokens,
            }
        }
    }

    let mut unresolved = Vec::new();
    for token in missing {
        let persisted = index_db
            .lock()
            .ok()
            .and_then(|db| db.token_metadata(&token).ok().flatten());
        if let Some(metadata) = persisted {
            let meta = TokenMeta {
                address: metadata.address,
                symbol: metadata.symbol,
                name: metadata.name,
                issuer: metadata.issuer,
                domain: metadata.domain,
                icon: metadata.icon,
            };
            cache.lock().unwrap().insert(token.clone(), meta.clone());
            if let Some(client) = redis.as_ref() {
                let _ = write_token_meta_redis(client, &token, &meta).await;
            }
            out.insert(token, meta);
        } else if token_registry::find_token(&token).is_none() {
            // Unknown assets do not need an RPC round trip just to render a
            // compact label. They can be hydrated later by the metadata job.
            let meta = TokenMeta {
                address: token.clone(),
                symbol: short_token_label(&token),
                name: None,
                issuer: None,
                domain: None,
                icon: None,
            };
            cache.lock().unwrap().insert(token.clone(), meta.clone());
            out.insert(token, meta);
        } else {
            unresolved.push(token);
        }
    }

    let mut tasks = tokio::task::JoinSet::new();
    for token in unresolved {
        let rpc = Arc::clone(&rpc);
        let index_db = Arc::clone(&index_db);
        tasks.spawn(async move {
            let meta = resolve_one_token_meta(&rpc, &token).await;
            let row = crate::index_db::TokenMetadataRow {
                address: meta.address.clone(),
                symbol: meta.symbol.clone(),
                name: meta.name.clone(),
                issuer: meta.issuer.clone(),
                domain: meta.domain.clone(),
                icon: meta.icon.clone(),
            };
            if let Ok(db) = index_db.lock() {
                let _ = db.upsert_token_metadata(&row);
            }
            (token, meta)
        });
    }
    while let Some(result) = tasks.join_next().await {
        let Ok((token, meta)) = result else {
            continue;
        };
        {
            let mut cache_guard = cache.lock().unwrap();
            cache_guard.insert(token.clone(), meta.clone());
        }
        if let Some(client) = redis.as_ref() {
            let _ = write_token_meta_redis(client, &token, &meta).await;
        }
        out.insert(token, meta);
    }
    out
}

async fn write_token_meta_redis(client: &redis::Client, token: &str, meta: &TokenMeta) -> redis::RedisResult<()> {
    let mut connection = client.get_multiplexed_async_connection().await?;
    let serialized = serde_json::to_string(meta).map_err(|error| {
        redis::RedisError::from((
            redis::ErrorKind::TypeError,
            "token metadata serialization",
            error.to_string(),
        ))
    })?;
    connection
        .set_ex(redis_token_meta_key(token), serialized, REDIS_TOKEN_META_TTL_SECS)
        .await
}

async fn resolve_one_token_meta(rpc: &SorobanRpc, token: &str) -> TokenMeta {
    const TOKEN_METADATA_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(800);
    let curated = token_registry::find_token(token);
    let symbol = tokio::time::timeout(TOKEN_METADATA_TIMEOUT, rpc.call_no_args(token, "symbol"))
        .await
        .ok()
        .and_then(Result::ok)
        .and_then(|val| scval_to_symbol_string(&val).ok())
        .filter(|v| !v.trim().is_empty())
        .or_else(|| curated.map(|entry| entry.symbol.to_string()))
        .unwrap_or_else(|| short_token_label(token));
    let name = tokio::time::timeout(TOKEN_METADATA_TIMEOUT, rpc.call_no_args(token, "name"))
        .await
        .ok()
        .and_then(Result::ok)
        .and_then(|val| scval_to_symbol_string(&val).ok())
        .filter(|v| !v.trim().is_empty())
        .or_else(|| curated.map(|entry| entry.name.to_string()));
    TokenMeta {
        address: token.to_string(),
        symbol,
        name,
        issuer: curated
            .map(|entry| entry.issuer.trim())
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned),
        domain: curated
            .map(|entry| entry.domain.trim())
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned),
        icon: curated
            .map(|entry| entry.icon.trim())
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned),
    }
}

fn short_token_label(token: &str) -> String {
    if token.len() <= 10 {
        token.to_string()
    } else {
        format!("{}…{}", &token[..4], &token[token.len() - 4..])
    }
}

#[derive(Deserialize)]
struct HistoryQuery {
    limit: Option<usize>,
}

#[derive(Deserialize)]
struct EventsQuery {
    limit: Option<usize>,
}

async fn pool_history(
    State(state): State<AppState>,
    Path(address): Path<String>,
    Query(q): Query<HistoryQuery>,
) -> impl IntoResponse {
    let limit = q.limit.unwrap_or(90).min(365);
    let db = state.db.lock().unwrap();
    match db.history(&address, limit) {
        Ok(rows) => Json(json!({ "address": address, "points": rows })).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string(), "code": "db_error" })),
        )
            .into_response(),
    }
}

async fn pool_events(
    State(state): State<AppState>,
    Path(address): Path<String>,
    Query(q): Query<EventsQuery>,
) -> impl IntoResponse {
    let limit = q.limit.unwrap_or(20).min(100);
    let tokens: Vec<String> = {
        let db = state.db.lock().unwrap();
        db.pool_meta(&address)
            .ok()
            .flatten()
            .and_then(|(_, tokens_json, _, _)| serde_json::from_str(&tokens_json).ok())
            .unwrap_or_default()
    };
    let token_ids: HashSet<String> = tokens.iter().cloned().collect();
    let token_meta_map = if token_ids.is_empty() {
        HashMap::new()
    } else {
        resolve_token_meta_map(
            state.rpc.clone(),
            state.index_db.clone(),
            &state.token_meta_cache,
            state.redis.clone(),
            &token_ids,
        )
        .await
    };
    let wanted = wanted_tokens_from_meta(&token_meta_map);
    let (price_map, mut quote_meta) = state.prices.prices_for_tokens(&wanted).await;
    quote_meta.coverage = coverage_label(&tokens, &price_map);
    let xlm_usd = quote_meta.xlm_usd;

    let index_db = state.index_db.lock().unwrap();
    match index_db.recent_pool_events(&address, limit) {
        Ok(events) => {
            let events_json = events
                .iter()
                .map(|event| {
                    let mut row = event_row_json(event);
                    if let Some(body) = row.get_mut("body") {
                        enrich_event_derived_usd(body, xlm_usd);
                    }
                    row
                })
                .collect::<Vec<_>>();
            Json(json!({
                "address": address,
                "events": events_json,
                "quote": quote_json(&quote_meta),
            }))
            .into_response()
        }
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": error.to_string(), "code": "db_error" })),
        )
            .into_response(),
    }
}

#[derive(Deserialize)]
struct AddressQuery {
    address: String,
}

#[derive(Deserialize)]
struct LeadersBoardQuery {
    limit: Option<usize>,
    window_days: Option<i64>,
    sort: Option<String>,
}

/// Ranked Stellar LP actors by accrued fees / deposits (Copy scouting board).
async fn lp_leaders(State(state): State<AppState>, Query(q): Query<LeadersBoardQuery>) -> impl IntoResponse {
    let limit = q.limit.unwrap_or(25).clamp(1, 500);
    let window_days = q.window_days.unwrap_or(30).clamp(1, 90);
    let sort = (q.sort.as_deref() == Some("activity"))
        .then_some("activity")
        .unwrap_or("fees");
    let cache_key = redis_lp_leaders_key(window_days, limit, sort);
    if let Some(client) = state.redis.clone() {
        if let Ok(mut connection) = client.get_multiplexed_async_connection().await {
            let cached: redis::RedisResult<Option<String>> = connection.get(&cache_key).await;
            if let Ok(Some(serialized)) = cached {
                if let Ok(value) = serde_json::from_str::<Value>(&serialized) {
                    let mut response = Json(value).into_response();
                    response.headers_mut().insert(
                        HeaderName::from_static("x-lumenlp-cache"),
                        HeaderValue::from_static("leaders-redis-hit"),
                    );
                    response.headers_mut().insert(
                        CACHE_CONTROL,
                        HeaderValue::from_static("public, max-age=60, s-maxage=60, stale-while-revalidate=120"),
                    );
                    response.headers_mut().insert(
                        HeaderName::from_static("cloudflare-cdn-cache-control"),
                        HeaderValue::from_static("public, max-age=60, stale-while-revalidate=120"),
                    );
                    return response;
                }
            }
        }
    }
    let since_ts = Utc::now().timestamp() - window_days * 24 * 3600;
    let mut leaders = {
        let index_db = state.index_db.lock().unwrap();
        // Rank after joining the fee snapshots. Limiting by claimed fees first
        // could hide a leader whose current unclaimed fees are substantial.
        match index_db.top_liquidity_actors(since_ts, 10_000, sort) {
            Ok(v) => v,
            Err(error) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": error.to_string(), "code": "db_error" })),
                )
                    .into_response();
            }
        }
    };
    // A current position snapshot is also a valid discovery signal. Include
    // actors whose latest fee snapshot is outside the selected activity
    // window, otherwise a quiet LP with accruing fees disappears from the
    // default 1d board.
    {
        let index_db = state.index_db.lock().unwrap();
        let known = leaders
            .iter()
            .map(|leader| leader.address.clone())
            .collect::<HashSet<_>>();
        let snapshot_totals = index_db.actor_fee_snapshot_totals().unwrap_or_default();
        for actor in snapshot_totals.keys() {
            if known.contains(actor) {
                continue;
            }
            let snapshot_pool_count = snapshot_totals
                .get(actor)
                .map(|snapshot| snapshot.pool_count)
                .unwrap_or_default();
            leaders.push(crate::index_db::TopLiquidityActor {
                address: actor.clone(),
                event_count: 0,
                deposit_count: 0,
                withdraw_count: 0,
                claim_count: 0,
                deposit_quote_xlm: 0.0,
                withdraw_quote_xlm: 0.0,
                claim_quote_xlm: 0.0,
                distinct_pools: snapshot_pool_count,
                last_activity_at: None,
            });
        }
    }

    let (_, quote_meta) = state
        .prices
        .prices_for_tokens(&[(
            NATIVE_SAC.to_string(),
            Some("native".into()),
            Some("native".into()),
            None,
        )])
        .await;
    let xlm_usd = quote_meta.xlm_usd;
    let to_usd = |xlm: f64| xlm_usd.and_then(|px| xlm_quote_to_usd(xlm, px));
    let fee_snapshots = {
        let index_db = state.index_db.lock().unwrap();
        index_db.actor_fee_snapshot_totals().unwrap_or_default()
    };
    let unclaimed_fee_deltas = {
        let index_db = state.index_db.lock().unwrap();
        index_db
            .actor_fee_snapshot_deltas(since_ts)
            .unwrap_or_default()
    };
    if sort == "fees" {
        leaders.sort_by(|a, b| {
            let total = |leader: &crate::index_db::TopLiquidityActor| {
                leader.claim_quote_xlm
                    + unclaimed_fee_deltas.get(&leader.address).copied().unwrap_or(0.0)
            };
            total(b).partial_cmp(&total(a)).unwrap_or(std::cmp::Ordering::Equal)
        });
    }
    leaders.truncate(limit);

    let rows: Vec<Value> = leaders
        .into_iter()
        .map(|a| {
            let snapshot = fee_snapshots.get(&a.address).cloned().unwrap_or_default();
            let unclaimed_fee = snapshot.observed_at.map(|_| snapshot.unclaimed_quote_xlm);
            let unclaimed_fee_delta = unclaimed_fee_deltas.get(&a.address).copied();
            let accrued_fee = unclaimed_fee_delta.map(|delta| a.claim_quote_xlm + delta);
            let fee_status = if snapshot.position_count == 0 {
                "not_verified"
            } else if snapshot.observed_at.is_some() {
                "verified"
            } else {
                "unavailable"
            };
            let fee_capital_ratio = if a.deposit_quote_xlm > 0.0 {
                Some(accrued_fee.unwrap_or(a.claim_quote_xlm) / a.deposit_quote_xlm)
            } else {
                None
            };
            json!({
                "address": a.address,
                "event_count": a.event_count,
                "deposit_count": a.deposit_count,
                "withdraw_count": a.withdraw_count,
                "claim_count": a.claim_count,
                "deposit_quote_xlm": a.deposit_quote_xlm,
                "withdraw_quote_xlm": a.withdraw_quote_xlm,
                "claim_quote_xlm": a.claim_quote_xlm,
                "deposit_quote_usd": to_usd(a.deposit_quote_xlm),
                "withdraw_quote_usd": to_usd(a.withdraw_quote_xlm),
                "claim_quote_usd": to_usd(a.claim_quote_xlm),
                "unclaimed_fee_quote_xlm": unclaimed_fee,
                "unclaimed_fee_quote_usd": unclaimed_fee.and_then(to_usd),
                "unclaimed_fee_delta_quote_xlm": unclaimed_fee_delta,
                "unclaimed_fee_delta_quote_usd": unclaimed_fee_delta.and_then(to_usd),
                "accrued_fee_quote_xlm": accrued_fee,
                "accrued_fee_quote_usd": accrued_fee.and_then(to_usd),
                "fee_status": fee_status,
                "fee_snapshot_at": snapshot.observed_at,
                "fee_snapshot_position_count": snapshot.position_count,
                "position_value_quote_xlm": (snapshot.position_count > 0)
                    .then_some(snapshot.position_value_quote_xlm),
                "net_liquidity_quote_xlm": a.deposit_quote_xlm - a.withdraw_quote_xlm,
                "fee_capital_ratio": fee_capital_ratio,
                "distinct_pools": a.distinct_pools,
                "last_activity_at": a.last_activity_at,
            })
        })
        .collect();
    let latest_fee_snapshot_at = fee_snapshots.values().filter_map(|snapshot| snapshot.observed_at).max();
    let verified_fee_actor_count = fee_snapshots
        .values()
        .filter(|snapshot| snapshot.observed_at.is_some())
        .count();

    let response_body = json!({
        "window_days": window_days,
        "since_ts": since_ts,
        "xlm_usd": xlm_usd,
        "leaders": rows,
        "sort": sort,
        "fee_data": {
            "latest_snapshot_at": latest_fee_snapshot_at,
            "verified_actor_count": verified_fee_actor_count,
            "actor_count": fee_snapshots.len(),
            "refresh_cadence_seconds": 60,
        },
        "honesty": "Windowed claimed fees are indexed claim events. Windowed accrued fees add the change in verified unclaimed position fees between the window boundary and the latest snapshot; missing venue coverage is left unavailable rather than estimated.",
    });
    if let Some(client) = state.redis.as_ref() {
        if let Ok(serialized) = serde_json::to_string(&response_body) {
            if let Ok(mut connection) = client.get_multiplexed_async_connection().await {
                let _: redis::RedisResult<()> = connection
                    .set_ex(&cache_key, serialized, REDIS_LP_LEADERS_TTL_SECS)
                    .await;
            }
        }
    }
    let mut response = Json(response_body).into_response();
    response.headers_mut().insert(
        HeaderName::from_static("x-lumenlp-cache"),
        HeaderValue::from_static("leaders-origin"),
    );
    // Leader data is refreshed on the same minute cadence as fee snapshots.
    // Cache the query variant at the edge so repeated page loads do not block
    // on the origin API and its RPC-backed refresh work.
    response.headers_mut().insert(
        CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=60, s-maxage=60, stale-while-revalidate=120"),
    );
    response.headers_mut().insert(
        HeaderName::from_static("cloudflare-cdn-cache-control"),
        HeaderValue::from_static("public, max-age=60, stale-while-revalidate=120"),
    );
    response
}

/// Refresh a rotating batch of current position fee snapshots. This is
/// deliberately off the request path: RPC position reads are venue-specific
/// and must not make the leaders endpoint slow or unreliable. Batch size and
/// concurrency are configurable so operators can tune RPC pressure.
pub async fn refresh_leader_fee_snapshots(state: AppState) {
    const ACTOR_LIMIT: usize = 10_000;
    const MAX_POOLS_PER_ACTOR: usize = 40;
    let batch_size = std::env::var("LEADER_FEE_SNAPSHOT_BATCH")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .map(|value| value.clamp(20, 200))
        .unwrap_or(80);
    let hot_batch_size = std::env::var("LEADER_FEE_SNAPSHOT_HOT_BATCH")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .map(|value| value.clamp(10, batch_size))
        .unwrap_or(40)
        .min(batch_size);
    let concurrency = std::env::var("LEADER_FEE_SNAPSHOT_CONCURRENCY")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .map(|value| value.clamp(1, 16))
        .unwrap_or(8);

    let actors = {
        let db = state.index_db.lock().unwrap();
        db.known_liquidity_actors(ACTOR_LIMIT).unwrap_or_default()
    };
    if actors.is_empty() {
        return;
    }
    // Keep the most active actors fresh on every tick, while using the
    // remaining slots to rotate through the long tail. This avoids a cursor
    // over a moving activity-sorted list starving the current top of the board.
    let hot_count = hot_batch_size.min(actors.len());
    let tail_len = actors.len().saturating_sub(hot_count);
    let rotating_count = batch_size.saturating_sub(hot_count).min(tail_len);
    let start = if tail_len == 0 {
        0
    } else {
        state
            .leader_fee_scan_cursor
            .fetch_add(rotating_count.max(1), std::sync::atomic::Ordering::Relaxed)
            % tail_len
    };
    let mut selected = actors[..hot_count].to_vec();
    selected.extend((0..rotating_count).map(|offset| actors[hot_count + (start + offset) % tail_len].clone()));
    info!(
        actor_count = actors.len(),
        batch_size = selected.len(),
        hot_batch_size = hot_count,
        rotating_batch_size = rotating_count,
        concurrency,
        start,
        "refreshing leader fee snapshots"
    );
    let pricing = {
        let db = state.db.lock().unwrap();
        db.pool_states_for_pricing().unwrap_or_default()
    };

    let mut pending = selected.into_iter();
    let mut tasks = JoinSet::new();
    for _ in 0..concurrency.min(batch_size) {
        let Some(actor) = pending.next() else { break };
        tasks.spawn(refresh_one_actor(
            state.clone(),
            actor,
            pricing.clone(),
            MAX_POOLS_PER_ACTOR,
        ));
    }

    let mut refreshed = 0usize;
    while let Some(result) = tasks.join_next().await {
        if let Ok((actor, positions)) = result {
            // Rebuild this actor's current position set so closed positions do
            // not leave stale unclaimed fees in the accrued total.
            let observed_at = Utc::now().timestamp();
            let db = state.index_db.lock().unwrap();
            let _ = db.record_actor_fee_snapshot_history_zeroed(&actor, observed_at);
            let _ = db.clear_actor_fee_snapshots(&actor);
            for position in positions {
                // Persist the verified position even when this venue does not
                // expose an independent fee accumulator. The null fee stays
                // unavailable, but the LP remains discoverable by the board.
                let status = if position.fees_unclaimed_quote.is_some() {
                    "ok"
                } else {
                    "fee_unavailable"
                };
                let _ = db.upsert_actor_fee_snapshot(
                    &actor,
                    &position.pool_address,
                    &position.venue,
                    position.fees_unclaimed_quote,
                    position.value_quote,
                    status,
                    observed_at,
                );
                let _ = db.insert_actor_fee_snapshot_history(
                    &actor,
                    &position.pool_address,
                    &position.venue,
                    position.fees_unclaimed_quote,
                    position.value_quote,
                    status,
                    observed_at,
                );
            }
            refreshed += 1;
        }
        if let Some(actor) = pending.next() {
            tasks.spawn(refresh_one_actor(
                state.clone(),
                actor,
                pricing.clone(),
                MAX_POOLS_PER_ACTOR,
            ));
        }
    }
    info!(refreshed, "leader fee snapshot refresh complete");
    if refreshed > 0 {
        if let Some(redis) = state.redis.as_ref() {
            invalidate_lp_leaders_cache(redis).await;
        }
    }
}

async fn refresh_one_actor(
    state: AppState,
    actor: String,
    pricing: Vec<dex::types::SharePoolState>,
    max_pools: usize,
) -> (String, Vec<UserPosition>) {
    let mut pools = {
        let db = state.index_db.lock().unwrap();
        db.actor_pool_addresses(&actor, 0).unwrap_or_default()
    };
    pools.sort();
    pools.dedup();
    pools.truncate(max_pools);

    let grouped = {
        let db = state.db.lock().unwrap();
        let mut grouped: HashMap<String, Vec<String>> = HashMap::new();
        for pool in pools {
            let Some(venue) = db
                .pool_meta(&pool)
                .ok()
                .flatten()
                .map(|(_, _, _, venue)| venue)
            else {
                continue;
            };
            grouped.entry(venue).or_default().push(pool);
        }
        grouped
    };

    let mut positions = Vec::new();
    for (venue, venue_pools) in grouped {
        let mut venue_positions = if venue.eq_ignore_ascii_case("sushi") || venue.eq_ignore_ascii_case("sushi_v3") {
            load_sushi_positions_for_pools(&state, &actor, &venue_pools, &pricing).await
        } else {
            positions_for_venue(state.rpc.as_ref(), &actor, &venue, &venue_pools, &pricing).await
        };
        positions.append(&mut venue_positions);
    }
    (actor, positions)
}

async fn load_sushi_positions_for_pools(
    state: &AppState,
    address: &str,
    pools: &[String],
    pricing: &[dex::types::SharePoolState],
) -> Vec<UserPosition> {
    let pool_set = pools.iter().cloned().collect::<HashSet<_>>();
    let candidates = {
        let db = state.index_db.lock().unwrap();
        db.sushi_position_range_candidates(address, Utc::now().timestamp() - 90 * 86_400, 500)
            .unwrap_or_default()
            .into_iter()
            .filter(|candidate| pool_set.contains(&candidate.pool_address))
            .map(|candidate| SushiPositionRangeCandidate {
                pool_address: candidate.pool_address,
                tick_lower: candidate.tick_lower,
                tick_upper: candidate.tick_upper,
            })
            .collect::<Vec<_>>()
    };
    let mut positions = positions_for_managed_pools(state.rpc.as_ref(), address, pools, pricing).await;
    if !candidates.is_empty() {
        positions.extend(positions_for_candidates(state.rpc.as_ref(), address, &candidates, pricing).await);
    }
    let mut seen = HashSet::new();
    positions.retain(|position| {
        let range = position
            .cl_ranges
            .as_ref()
            .and_then(|ranges| ranges.first())
            .map(|range| format!(":{}:{}", range.tick_lower, range.tick_upper))
            .unwrap_or_default();
        seen.insert(format!("{}{}", position.pool_address, range))
    });
    positions
}

/// Route actor-touched pools to their owning DEX reader. Pool ABIs are not
/// interchangeable; in particular, an Aquarius probe must never be used as a
/// generic fallback for another venue.
async fn load_actor_positions(
    state: &AppState,
    address: &str,
    pools: &[String],
    pricing: &[dex::types::SharePoolState],
) -> Vec<UserPosition> {
    let grouped = {
        let db = state.db.lock().unwrap();
        let mut grouped: HashMap<String, Vec<String>> = HashMap::new();
        for pool in pools {
            let Some(venue) = db
                .pool_meta(pool)
                .ok()
                .flatten()
                .map(|(_, _, _, venue)| venue)
            else {
                continue;
            };
            grouped.entry(venue).or_default().push(pool.clone());
        }
        grouped
    };
    let mut tasks = JoinSet::new();
    for (venue, venue_pools) in grouped {
        let state = state.clone();
        let address = address.to_owned();
        let pricing = pricing.to_vec();
        tasks.spawn(async move {
            if venue.eq_ignore_ascii_case("sushi") || venue.eq_ignore_ascii_case("sushi_v3") {
                // Profile scans are already narrowed to pools this actor touched.
                // Do not rediscover and probe the entire Sushi catalogue here.
                load_sushi_positions_for_pools(&state, &address, &venue_pools, &pricing).await
            } else {
                positions_for_venue(state.rpc.as_ref(), &address, &venue, &venue_pools, &pricing).await
            }
        });
    }
    let mut positions = Vec::new();
    while let Some(result) = tasks.join_next().await {
        if let Ok(mut venue_positions) = result {
            positions.append(&mut venue_positions);
        }
    }
    positions
}

async fn list_positions(State(state): State<AppState>, Query(q): Query<AddressQuery>) -> impl IntoResponse {
    if !q.address.starts_with('G') || q.address.len() < 56 {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "invalid stellar address", "code": "bad_address" })),
        )
            .into_response();
    }
    let indexed_pools = {
        let db = state.db.lock().unwrap();
        db.list_pool_addresses().unwrap_or_default()
    };
    let pools = {
        let db = state.index_db.lock().unwrap();
        db.actor_pool_addresses(&q.address, Utc::now().timestamp() - 90 * 86_400)
            .unwrap_or_default()
    };
    let pricing = {
        let db = state.db.lock().unwrap();
        db.pool_states_for_pricing().unwrap_or_default()
    };
    let positions = load_actor_positions(&state, &q.address, &pools, &pricing).await;
    let stats = {
        let db = state.db.lock().unwrap();
        db.stats().ok()
    };
    let pool_count = stats.as_ref().map(|s| s.pool_count).unwrap_or(indexed_pools.len());
    let last_snapshot_at = stats.and_then(|s| s.latest_snapshot_at);
    let note = if positions.is_empty() {
        Some("No active LP position found in the recent indexed pool set")
    } else {
        None
    };
    Json(json!({
        "address": q.address,
        "positions": positions,
        "indexed_pool_count": pool_count,
        "last_snapshot_at": last_snapshot_at,
        "note": note,
    }))
    .into_response()
}

async fn positions_summary(State(state): State<AppState>, Query(q): Query<AddressQuery>) -> impl IntoResponse {
    if !q.address.starts_with('G') || q.address.len() < 56 {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "invalid stellar address", "code": "bad_address" })),
        )
            .into_response();
    }
    let indexed_pools = {
        let db = state.db.lock().unwrap();
        db.list_pool_addresses().unwrap_or_default()
    };
    let pools = {
        let db = state.index_db.lock().unwrap();
        db.actor_pool_addresses(&q.address, Utc::now().timestamp() - 90 * 86_400)
            .unwrap_or_default()
    };
    let stats = {
        let db = state.db.lock().unwrap();
        db.stats().ok()
    };
    let pricing = {
        let db = state.db.lock().unwrap();
        db.pool_states_for_pricing().unwrap_or_default()
    };
    let positions = load_actor_positions(&state, &q.address, &pools, &pricing).await;
    let mut net_worth = 0.0;
    let mut fees = 0.0;
    let mut fee_legs = 0usize;
    let mut il_sum = 0.0;
    let mut il_n = 0usize;
    for p in &positions {
        if let Some(v) = p.value_quote {
            net_worth += v;
        }
        if let Some(f) = p.fees_unclaimed_quote {
            fees += f;
            fee_legs += 1;
        }
        if let Some(il) = p.il_est {
            il_sum += il;
            il_n += 1;
        }
    }
    Json(json!({
        "address": q.address,
        "net_worth": net_worth,
        "fees_unclaimed": (fee_legs > 0).then_some(fees),
        "il_est_avg": if il_n > 0 { Some(il_sum / il_n as f64) } else { None },
        "position_count": positions.len(),
        "indexed_pool_count": stats.as_ref().map(|s| s.pool_count).unwrap_or(indexed_pools.len()),
        "last_snapshot_at": stats.and_then(|s| s.latest_snapshot_at),
        "note": if pools.is_empty() {
            Some("No indexed pools yet — run snapshotter first")
        } else if positions.is_empty() {
            Some("No active LP position found in the recent indexed pool set")
        } else {
            None::<&str>
        },
        "quote_asset": "XLM",
    }))
    .into_response()
}

/// Portfolio + recent liquidity activity for scouting Copy leaders (Smart
/// LP–style).
async fn lp_profile(State(state): State<AppState>, Query(q): Query<AddressQuery>) -> impl IntoResponse {
    if !valid_stellar_address(&q.address) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "invalid stellar address", "code": "bad_address" })),
        )
            .into_response();
    }

    if let Some(client) = state.redis.clone() {
        if let Ok(mut connection) = client.get_multiplexed_async_connection().await {
            let cached: redis::RedisResult<Option<String>> = connection.get(redis_lp_profile_key(&q.address)).await;
            if let Ok(Some(serialized)) = cached {
                if let Ok(value) = serde_json::from_str::<Value>(&serialized) {
                    let mut response = Json(value).into_response();
                    response.headers_mut().insert(
                        HeaderName::from_static("x-lumenlp-cache"),
                        HeaderValue::from_static("profile-redis-hit"),
                    );
                    response.headers_mut().insert(
                        CACHE_CONTROL,
                        HeaderValue::from_static("public, max-age=60, s-maxage=60, stale-while-revalidate=120"),
                    );
                    response.headers_mut().insert(
                        HeaderName::from_static("cloudflare-cdn-cache-control"),
                        HeaderValue::from_static("public, max-age=60, stale-while-revalidate=120"),
                    );
                    return response;
                }
            }
        }
    }

    let stats = {
        let db = state.db.lock().unwrap();
        db.stats().ok()
    };
    let pricing = {
        let db = state.db.lock().unwrap();
        db.pool_states_for_pricing().unwrap_or_default()
    };
    let indexed_pool_count = {
        let db = state.db.lock().unwrap();
        db.list_pool_addresses().map(|p| p.len()).unwrap_or(0)
    };

    let now_ts = Utc::now().timestamp();
    let since_7d = now_ts - 7 * 24 * 3600;
    let since_30d = now_ts - 30 * 24 * 3600;
    let scan_since_ts = now_ts - 90 * 24 * 3600;
    let (activity_7d, activity_30d, recent_events, actor_pools, lifetime) = {
        let index_db = state.index_db.lock().unwrap();
        let a7 = index_db.actor_liquidity_activity(&q.address, since_7d, 200);
        let a30 = index_db.actor_liquidity_activity(&q.address, since_30d, 200);
        let pools = index_db.actor_pool_addresses(&q.address, scan_since_ts);
        let life = index_db.actor_lifetime_totals(&q.address);
        match (a7, a30, pools, life) {
            (Ok((activity_7d, _)), Ok((activity_30d, events)), Ok(pools), Ok(lifetime)) => {
                (activity_7d, activity_30d, events, pools, lifetime)
            }
            (Err(error), _, _, _) | (_, Err(error), _, _) | (_, _, Err(error), _) | (_, _, _, Err(error)) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": error.to_string(), "code": "db_error" })),
                )
                    .into_response();
            }
        }
    };
    let first_activity_at = lifetime.first_activity_at;
    let last_activity_at = lifetime.last_activity_at;

    // Narrow RPC scan to pools this actor actually touched — full catalogue (~300+)
    // times out nginx (~60s) and is unnecessary for Copy scouting.
    let mut scan_pools = actor_pools;
    scan_pools.sort();
    scan_pools.dedup();
    if scan_pools.len() > 40 {
        scan_pools.truncate(40);
    }
    let positions = load_actor_positions(&state, &q.address, &scan_pools, &pricing).await;

    let pool_metadata: HashMap<String, (String, Vec<String>, i64, String)> = {
        let db = state.db.lock().unwrap();
        scan_pools
            .iter()
            .filter_map(|pool| {
                db.pool_meta(pool)
                    .ok()
                    .flatten()
                    .and_then(|(pool_type, tokens_json, fee_bps, venue)| {
                        serde_json::from_str(&tokens_json)
                            .ok()
                            .map(|tokens| (pool.clone(), (pool_type, tokens, fee_bps, venue)))
                    })
            })
            .collect()
    };
    let pool_tokens: HashMap<String, Vec<String>> = pool_metadata
        .iter()
        .map(|(pool, (_, tokens, _, _))| (pool.clone(), tokens.clone()))
        .collect();
    let token_ids: HashSet<String> = positions
        .iter()
        .flat_map(|position| position.tokens.iter().cloned())
        .chain(pool_tokens.values().flat_map(|tokens| tokens.iter().cloned()))
        .collect();
    let token_meta_map = resolve_token_meta_map(
        state.rpc.clone(),
        state.index_db.clone(),
        &state.token_meta_cache,
        state.redis.clone(),
        &token_ids,
    )
    .await;
    let token_labels = |tokens: &[String]| {
        tokens
            .iter()
            .map(|token| {
                token_meta_map
                    .get(token)
                    .map(|meta| meta.symbol.clone())
                    .filter(|symbol| !symbol.is_empty() && symbol != "unknown")
                    .unwrap_or_else(|| short_token_label(token))
            })
            .collect::<Vec<_>>()
    };
    let mut positions_json = serde_json::to_value(&positions).unwrap_or_else(|_| json!([]));
    if let Some(rows) = positions_json.as_array_mut() {
        for (row, position) in rows.iter_mut().zip(&positions) {
            if let Some(object) = row.as_object_mut() {
                object.insert("token_labels".into(), json!(token_labels(&position.tokens)));
            }
        }
    }

    let mut net_worth = 0.0;
    let mut fees = 0.0;
    let mut fee_legs = 0usize;
    let mut il_sum = 0.0;
    let mut il_n = 0usize;
    let mut in_range = 0usize;
    let mut out_range = 0usize;
    for p in &positions {
        if let Some(v) = p.value_quote {
            net_worth += v;
        }
        if let Some(f) = p.fees_unclaimed_quote {
            fees += f;
            fee_legs += 1;
        }
        if let Some(il) = p.il_est {
            il_sum += il;
            il_n += 1;
        }
        if let Some(ranges) = &p.cl_ranges {
            for r in ranges {
                if r.in_range {
                    in_range += 1;
                } else {
                    out_range += 1;
                }
            }
        }
    }

    let (_, quote_meta) = state
        .prices
        .prices_for_tokens(&[(
            NATIVE_SAC.to_string(),
            Some("native".into()),
            Some("native".into()),
            None,
        )])
        .await;
    let xlm_usd = quote_meta.xlm_usd;
    let to_usd = |xlm: f64| xlm_usd.and_then(|px| xlm_quote_to_usd(xlm, px));

    let unclaimed_deltas = {
        let index_db = state.index_db.lock().unwrap();
        (
            index_db
                .actor_fee_snapshot_delta(&q.address, since_7d)
                .ok()
                .flatten(),
            index_db
                .actor_fee_snapshot_delta(&q.address, since_30d)
                .ok()
                .flatten(),
        )
    };

    let window_json = |a: &crate::index_db::ActorLiquidityActivity, unclaimed_delta: Option<f64>| {
        let accrued_fee = unclaimed_delta.map(|delta| a.claim_quote_xlm + delta);
        json!({
            "since_ts": a.since_ts,
            "event_count": a.event_count,
            "deposit_count": a.deposit_count,
            "withdraw_count": a.withdraw_count,
            "claim_count": a.claim_count,
            "deposit_quote_xlm": a.deposit_quote_xlm,
            "withdraw_quote_xlm": a.withdraw_quote_xlm,
            "claim_quote_xlm": a.claim_quote_xlm,
            "deposit_quote_usd": to_usd(a.deposit_quote_xlm),
            "withdraw_quote_usd": to_usd(a.withdraw_quote_xlm),
            "claim_quote_usd": to_usd(a.claim_quote_xlm),
            "unclaimed_fee_delta_quote_xlm": unclaimed_delta,
            "unclaimed_fee_delta_quote_usd": unclaimed_delta.and_then(to_usd),
            "accrued_fee_quote_xlm": accrued_fee,
            "accrued_fee_quote_usd": accrued_fee.and_then(to_usd),
            "distinct_pools": a.distinct_pools,
            "last_activity_at": a.last_activity_at,
            "net_liquidity_quote_xlm": a.deposit_quote_xlm - a.withdraw_quote_xlm,
            "avg_deposit_quote_xlm": if a.deposit_count > 0 {
                Some(a.deposit_quote_xlm / a.deposit_count as f64)
            } else {
                None
            },
            "avg_deposit_quote_usd": if a.deposit_count > 0 {
                to_usd(a.deposit_quote_xlm / a.deposit_count as f64)
            } else {
                None
            },
        })
    };

    let empty_activity = activity_7d.event_count == 0 && activity_30d.event_count == 0;
    let activity_30d_json = window_json(&activity_30d, unclaimed_deltas.1);
    let activity_7d_json = window_json(&activity_7d, unclaimed_deltas.0);

    let fee_capital = |claim: f64, deposit: f64| -> Option<f64> {
        if deposit > 0.0 && claim.is_finite() && deposit.is_finite() {
            Some(claim / deposit)
        } else {
            None
        }
    };
    let months_active = match (first_activity_at, last_activity_at) {
        (Some(first), Some(last)) if last >= first => ((last - first) as f64 / (30.0 * 86_400.0)).max(1.0 / 30.0),
        _ => 0.0,
    };
    // Floor divisor at 1 month so short indexed history doesn't explode the rate.
    let avg_monthly_claimed_xlm = if lifetime.claim_quote_xlm > 0.0 && months_active > 0.0 {
        Some(lifetime.claim_quote_xlm / months_active.max(1.0))
    } else {
        None
    };
    let claim_intensity_30d = if activity_30d.deposit_count > 0 {
        Some(activity_30d.claim_count as f64 / activity_30d.deposit_count as f64)
    } else if activity_30d.claim_count > 0 {
        Some(activity_30d.claim_count as f64)
    } else {
        None
    };

    let recent_json: Vec<Value> = recent_events
        .iter()
        .take(25)
        .map(|e| {
            json!({
                "event_id": e.event_id,
                "kind": e.kind,
                "pool_address": e.pool_address,
                "token_labels": pool_tokens
                    .get(&e.pool_address)
                    .map(|tokens| token_labels(tokens))
                    .unwrap_or_default(),
                "pool_type": pool_metadata.get(&e.pool_address).map(|meta| meta.0.clone()),
                "fee_bps": pool_metadata.get(&e.pool_address).map(|meta| meta.2),
                "venue": pool_metadata.get(&e.pool_address).map(|meta| meta.3.clone()),
                "created_at": e.created_at,
                "tx_hash": e.tx_hash,
                "quote_xlm": e.body.pointer("/derived/total_quote_xlm").and_then(|v| v.as_f64())
                    .or_else(|| e.body.pointer("/derived/fee_quote_xlm").and_then(|v| v.as_f64())),
            })
        })
        .collect();

    let position_scan_note = if scan_pools.is_empty() {
        Some("Open positions skipped — no indexed liquidity events for this address in 90d (non-Sushi RPC scan would hit all pools)")
    } else {
        None
    };

    let response_body = json!({
        "address": q.address,
        "venue_id": "multi_venue",
        "portfolio": {
            "net_worth_xlm": net_worth,
            "net_worth_usd": to_usd(net_worth),
            "fees_unclaimed_xlm": (fee_legs > 0).then_some(fees),
            "fees_unclaimed_usd": (fee_legs > 0).then(|| to_usd(fees)).flatten(),
            "il_est_avg": if il_n > 0 { Some(il_sum / il_n as f64) } else { None },
            "position_count": positions.len(),
            "cl_in_range": in_range,
            "cl_out_of_range": out_range,
        },
        "first_activity_at": first_activity_at,
        "last_activity_at": last_activity_at,
        "windows": {
            "7d": activity_7d_json,
            "30d": activity_30d_json.clone(),
        },
        "activity_30d": activity_30d_json,
        "lifetime": {
            "deposit_count": lifetime.deposit_count,
            "withdraw_count": lifetime.withdraw_count,
            "claim_count": lifetime.claim_count,
            "deposit_quote_xlm": lifetime.deposit_quote_xlm,
            "withdraw_quote_xlm": lifetime.withdraw_quote_xlm,
            "claim_quote_xlm": lifetime.claim_quote_xlm,
            "deposit_quote_usd": to_usd(lifetime.deposit_quote_xlm),
            "withdraw_quote_usd": to_usd(lifetime.withdraw_quote_xlm),
            "claim_quote_usd": to_usd(lifetime.claim_quote_xlm),
            "distinct_pools": lifetime.distinct_pools,
            "net_liquidity_quote_xlm": lifetime.deposit_quote_xlm - lifetime.withdraw_quote_xlm,
        },
        "proxies": {
            "fee_capital_ratio_7d": fee_capital(activity_7d.claim_quote_xlm, activity_7d.deposit_quote_xlm),
            "fee_capital_ratio_30d": fee_capital(activity_30d.claim_quote_xlm, activity_30d.deposit_quote_xlm),
            "fee_capital_ratio_lifetime": fee_capital(lifetime.claim_quote_xlm, lifetime.deposit_quote_xlm),
            "claim_intensity_30d": claim_intensity_30d,
            "avg_monthly_claimed_xlm": avg_monthly_claimed_xlm,
            "avg_monthly_claimed_usd": avg_monthly_claimed_xlm.and_then(|xlm| to_usd(xlm)),
            "months_active_indexed": if months_active > 0.0 { Some(months_active) } else { None },
            "labels": {
                "fee_capital_ratio": "claimed_fees / deposits (not ROI)",
                "claim_intensity_30d": "claim_events / deposit_events (not win rate)",
                "avg_monthly_claimed": "lifetime claimed fees / indexed active months (proxy)",
            },
        },
        "positions": positions_json,
        "position_pools_scanned": scan_pools.len(),
        "recent_events": recent_json,
        "indexed_pool_count": stats.as_ref().map(|s| s.pool_count).unwrap_or(indexed_pool_count),
        "last_snapshot_at": stats.and_then(|s| s.latest_snapshot_at),
        "xlm_usd": xlm_usd,
        "note": position_scan_note.or(if indexed_pool_count == 0 {
            Some("No indexed pools yet — run snapshotter first")
        } else if positions.is_empty() && empty_activity && lifetime.claim_count == 0 {
            Some("No LP or recent liquidity events for this address in the indexed set")
        } else {
            None
        }),
        "honesty": "Profile windows use indexed claimed fees and, when snapshot boundaries are available, verified unclaimed-fee changes to form accrued fees. These are not full PnL vs entry and not a win rate. Open positions scan pools touched in ~90d; Sushi V3 also verifies the Position Manager list against known pools.",
    });
    if let Some(client) = state.redis.as_ref() {
        if let Ok(serialized) = serde_json::to_string(&response_body) {
            if let Ok(mut connection) = client.get_multiplexed_async_connection().await {
                let _: redis::RedisResult<()> = connection
                    .set_ex(redis_lp_profile_key(&q.address), serialized, REDIS_LP_PROFILE_TTL_SECS)
                    .await;
            }
        }
    }
    let mut response = Json(response_body).into_response();
    response.headers_mut().insert(
        HeaderName::from_static("x-lumenlp-cache"),
        HeaderValue::from_static("profile-origin"),
    );
    response.headers_mut().insert(
        CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=60, s-maxage=60, stale-while-revalidate=120"),
    );
    response.headers_mut().insert(
        HeaderName::from_static("cloudflare-cdn-cache-control"),
        HeaderValue::from_static("public, max-age=60, stale-while-revalidate=120"),
    );
    response
}

const COPY_RECONCILE_BATCH: usize = 500;

const COPY_OP_STATUSES: &[&str] = &["drafted", "skipped", "signed", "failed", "insufficient", "rejected"];

const COPY_SESSION_STATUSES: &[&str] = &["active", "paused", "stopped"];

fn valid_stellar_address(address: &str) -> bool {
    address.starts_with('G') && address.len() >= 56
}

fn new_copy_entity_id() -> String {
    let ts = chrono::Utc::now()
        .timestamp_nanos_opt()
        .unwrap_or_else(|| chrono::Utc::now().timestamp().saturating_mul(1_000_000_000));
    format!("{ts:x}")
}

fn copy_session_json(session: &CopySessionRow) -> Value {
    json!({
        "id": session.id,
        "follower_address": session.follower_address,
        "leader_address": session.leader_address,
        "coefficient": session.coefficient,
        "coefficient_ppm": coefficient_ppm(session.coefficient),
        "status": session.status,
        "include_claims": session.include_claims,
        "policy": {
            "allowed_pools": session.allowed_pools,
            "max_per_op_quote_xlm": session.max_per_op_quote_xlm,
            "max_daily_quote_xlm": session.max_daily_quote_xlm,
            "expires_at": session.expires_at,
        },
        "cursor_ts": session.cursor_ts,
        "watermark_ts": session.watermark_ts,
        "watermark_event_id": session.watermark_event_id,
        "created_at": session.created_at,
        "updated_at": session.updated_at,
    })
}

fn copy_op_json(op: &CopyOpRow, venue: Option<&str>) -> Value {
    let leader_amounts = serde_json::from_str(&op.leader_amounts_json).unwrap_or(Value::Null);
    let scaled_amounts = serde_json::from_str(&op.scaled_amounts_json).unwrap_or(Value::Null);
    json!({
        "id": op.id,
        "session_id": op.session_id,
        "source_event_id": op.source_event_id,
        "pool_address": op.pool_address,
        "venue": venue,
        "kind": op.kind,
        "position_key": op.position_key,
        "leader_amounts": leader_amounts,
        "scaled_amounts": scaled_amounts,
        "leader_quote_xlm": op.leader_quote_xlm,
        "scaled_quote_xlm": op.scaled_quote_xlm,
        "status": op.status,
        "note": op.note,
        "created_at": op.created_at,
        "updated_at": op.updated_at,
    })
}

fn copy_op_status(policy_result: Result<(), PolicyReject>, recorder_ready: bool) -> (String, Option<String>) {
    let (status, note) = match policy_result {
        Ok(()) => ("pending".to_string(), None),
        Err(reason) => (
            "rejected".to_string(),
            Some(format!("{}: copy policy rejected", reason.code())),
        ),
    };
    if status == "pending" && !recorder_ready {
        return (
            "rejected".to_string(),
            Some("event_not_recordable: incomplete recorder payload".to_string()),
        );
    }
    (status, note)
}

fn reconcile_copy_ops(index_db: &IndexDb, session: &mut CopySessionRow) -> Result<(), anyhow::Error> {
    if session.status != "active" {
        return Ok(());
    }

    loop {
        let since = session.watermark_ts.max(session.cursor_ts);
        let after_event_id = if since == session.watermark_ts {
            session.watermark_event_id.as_str()
        } else {
            // Cursor jumped ahead of watermark (shouldn't happen); restart exclusive.
            ""
        };
        let events =
            index_db.events_for_actor_since(&session.leader_address, since, after_event_id, COPY_RECONCILE_BATCH)?;
        if events.is_empty() {
            break;
        }

        let now = Utc::now().timestamp();
        let mut last_created_at = since;
        let mut last_event_id = after_event_id.to_string();
        for event in &events {
            last_created_at = event.created_at;
            last_event_id = event.event_id.clone();
            let Some(draft) = build_scaled_op_payload(
                &event.body,
                &event.pool_address,
                session.coefficient,
                session.include_claims,
            ) else {
                continue;
            };
            let daily_start = Utc::now()
                .date_naive()
                .and_hms_opt(0, 0, 0)
                .map(|value| value.and_utc().timestamp())
                .unwrap_or(0);
            // Older Aquarius rows predate the explicit venue marker. Treat
            // missing metadata as Aquarius for migration compatibility, but
            // require an explicit allow-list for every other venue.
            let venue = event
                .body
                .get("derived")
                .and_then(|derived| derived.get("venue"))
                .and_then(Value::as_str)
                .unwrap_or("aquarius");
            let policy_result = validate_copy_op(
                session,
                venue,
                &draft.kind,
                &event.pool_address,
                draft.scaled_quote_xlm,
                Utc::now().timestamp(),
                index_db.copy_quote_used_since(&session.id, daily_start)?,
            );
            let recorder_event = canonical_event(event, &session.leader_address);
            let (status, note) = copy_op_status(policy_result, recorder_event.is_some());
            if status == "pending" {
                if let Some(recorder_event) = recorder_event {
                    index_db.enqueue_recorder_event(&recorder_event)?;
                }
            }
            let op = CopyOpRow {
                id: new_copy_entity_id(),
                session_id: session.id.clone(),
                source_event_id: event.event_id.clone(),
                pool_address: event.pool_address.clone(),
                kind: draft.kind,
                position_key: draft.position_key,
                leader_amounts_json: draft.leader_amounts_json.to_string(),
                scaled_amounts_json: draft.scaled_amounts_json.to_string(),
                leader_quote_xlm: draft.leader_quote_xlm,
                scaled_quote_xlm: draft.scaled_quote_xlm,
                status,
                note,
                created_at: now,
                updated_at: now,
            };
            index_db.insert_copy_op(&op)?;
        }

        index_db.update_copy_session(
            &session.id,
            None,
            None,
            Some(last_created_at),
            Some(&last_event_id),
            None,
        )?;
        session.watermark_ts = last_created_at;
        session.watermark_event_id = last_event_id;

        if events.len() < COPY_RECONCILE_BATCH {
            break;
        }
    }

    Ok(())
}

#[derive(Deserialize)]
struct CreateCopySessionBody {
    follower_address: String,
    leader_address: String,
    coefficient: f64,
    include_claims: Option<bool>,
    allowed_pools: Option<Vec<String>>,
    max_per_op_quote_xlm: Option<f64>,
    max_daily_quote_xlm: Option<f64>,
    expires_at: Option<i64>,
}

async fn create_copy_session(
    State(state): State<AppState>,
    Json(body): Json<CreateCopySessionBody>,
) -> impl IntoResponse {
    if !valid_stellar_address(&body.follower_address) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "invalid follower address", "code": "bad_address" })),
        )
            .into_response();
    }
    if !valid_stellar_address(&body.leader_address) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "invalid leader address", "code": "bad_address" })),
        )
            .into_response();
    }
    if coefficient_ppm(body.coefficient).is_none() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "coefficient must be > 0", "code": "bad_coefficient" })),
        )
            .into_response();
    }

    let max_per_op = body.max_per_op_quote_xlm.unwrap_or(0.0);
    let max_daily = body.max_daily_quote_xlm.unwrap_or(0.0);
    if !max_per_op.is_finite() || max_per_op < 0.0 || !max_daily.is_finite() || max_daily < 0.0 {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "policy limits must be finite and >= 0", "code": "bad_policy" })),
        )
            .into_response();
    }
    if let Some(expires_at) = body.expires_at {
        if expires_at <= Utc::now().timestamp() {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "expires_at must be in the future", "code": "bad_policy" })),
            )
                .into_response();
        }
    }

    let include_claims = body.include_claims.unwrap_or(false);
    let index_db = state.index_db.lock().unwrap();
    match index_db.create_copy_session(
        &body.follower_address,
        &body.leader_address,
        body.coefficient,
        include_claims,
        body.allowed_pools.as_deref().unwrap_or(&[]),
        max_per_op,
        max_daily,
        body.expires_at,
    ) {
        Ok(session) => Json(copy_session_json(&session)).into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": error.to_string(), "code": "db_error" })),
        )
            .into_response(),
    }
}

#[derive(Deserialize)]
struct ListCopySessionsQuery {
    follower: String,
}

async fn list_copy_sessions(
    State(state): State<AppState>,
    Query(q): Query<ListCopySessionsQuery>,
) -> impl IntoResponse {
    if !valid_stellar_address(&q.follower) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "invalid follower address", "code": "bad_address" })),
        )
            .into_response();
    }

    let index_db = state.index_db.lock().unwrap();
    match index_db.list_copy_sessions(&q.follower) {
        Ok(sessions) => {
            let sessions_json = sessions.iter().map(copy_session_json).collect::<Vec<_>>();
            Json(json!({ "sessions": sessions_json })).into_response()
        }
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": error.to_string(), "code": "db_error" })),
        )
            .into_response(),
    }
}

#[derive(Deserialize)]
struct UpdateCopySessionBody {
    status: Option<String>,
    coefficient: Option<f64>,
    include_claims: Option<bool>,
}

async fn update_copy_session_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<UpdateCopySessionBody>,
) -> impl IntoResponse {
    if let Some(ref status) = body.status {
        if !COPY_SESSION_STATUSES.contains(&status.as_str()) {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": "status must be active, paused, or stopped",
                    "code": "bad_status"
                })),
            )
                .into_response();
        }
    }
    if let Some(coefficient) = body.coefficient {
        if coefficient_ppm(coefficient).is_none() {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "coefficient must be > 0", "code": "bad_coefficient" })),
            )
                .into_response();
        }
    }

    let index_db = state.index_db.lock().unwrap();
    match index_db.get_copy_session(&id) {
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "copy session not found", "code": "not_found" })),
        )
            .into_response(),
        Ok(Some(_)) => match index_db.update_copy_session(
            &id,
            body.status.as_deref(),
            body.coefficient,
            None,
            None,
            body.include_claims,
        ) {
            Ok(()) => match index_db.get_copy_session(&id) {
                Ok(Some(session)) => Json(copy_session_json(&session)).into_response(),
                Ok(None) => (
                    StatusCode::NOT_FOUND,
                    Json(json!({ "error": "copy session not found", "code": "not_found" })),
                )
                    .into_response(),
                Err(error) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": error.to_string(), "code": "db_error" })),
                )
                    .into_response(),
            },
            Err(error) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": error.to_string(), "code": "db_error" })),
            )
                .into_response(),
        },
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": error.to_string(), "code": "db_error" })),
        )
            .into_response(),
    }
}

#[derive(Deserialize)]
struct ListCopyOpsQuery {
    status: Option<String>,
}

async fn list_copy_ops(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<ListCopyOpsQuery>,
) -> impl IntoResponse {
    let index_db = state.index_db.lock().unwrap();
    let mut session = match index_db.get_copy_session(&id) {
        Ok(Some(session)) => session,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "copy session not found", "code": "not_found" })),
            )
                .into_response();
        }
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": error.to_string(), "code": "db_error" })),
            )
                .into_response();
        }
    };

    if let Err(error) = reconcile_copy_ops(&index_db, &mut session) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": error.to_string(), "code": "db_error" })),
        )
            .into_response();
    }

    match index_db.list_copy_ops(&id, q.status.as_deref()) {
        Ok(ops) => {
            drop(index_db);
            let db = state.db.lock().unwrap();
            let ops_json = ops
                .iter()
                .map(|op| {
                    let venue = db.pool_meta(&op.pool_address).ok().flatten().map(|meta| meta.3);
                    copy_op_json(op, venue.as_deref())
                })
                .collect::<Vec<_>>();
            Json(json!({
                "session_id": id,
                "ops": ops_json,
            }))
            .into_response()
        }
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": error.to_string(), "code": "db_error" })),
        )
            .into_response(),
    }
}

#[derive(Deserialize)]
struct SetCopyOpStatusBody {
    status: String,
    note: Option<String>,
}

async fn get_copy_op(State(state): State<AppState>, Path(id): Path<String>) -> impl IntoResponse {
    let index_db = state.index_db.lock().unwrap();
    let result = index_db.get_copy_op(&id);
    drop(index_db);
    match result {
        Ok(Some(op)) => {
            let venue = state
                .db
                .lock()
                .ok()
                .and_then(|db| db.pool_meta(&op.pool_address).ok().flatten())
                .map(|meta| meta.3);
            Json(copy_op_json(&op, venue.as_deref())).into_response()
        }
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "copy op not found", "code": "not_found" })),
        )
            .into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": error.to_string(), "code": "db_error" })),
        )
            .into_response(),
    }
}

async fn set_copy_op_status(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<SetCopyOpStatusBody>,
) -> impl IntoResponse {
    if !COPY_OP_STATUSES.contains(&body.status.as_str()) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "status must be drafted, skipped, signed, failed, insufficient, or rejected",
                "code": "bad_status"
            })),
        )
            .into_response();
    }

    let index_db = state.index_db.lock().unwrap();
    match index_db.update_copy_op_status(&id, &body.status, body.note.as_deref()) {
        Ok(()) => Json(json!({ "id": id, "status": body.status })).into_response(),
        Err(error) if error.to_string().contains("copy op not found") => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "copy op not found", "code": "not_found" })),
        )
            .into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": error.to_string(), "code": "db_error" })),
        )
            .into_response(),
    }
}

#[cfg(test)]
mod tests {
    use {super::*, serde_json::json};

    #[test]
    fn bridge_tvl_usd_skips_zero_and_negative() {
        assert_eq!(bridge_tvl_usd(0.0, Some(0.17)), None);
        assert_eq!(bridge_tvl_usd(-1.0, Some(0.17)), None);
        assert!(bridge_tvl_usd(100.0, Some(0.17)).unwrap() > 0.0);
    }

    #[test]
    fn score_floor_prevents_micro_pool_fee_tvl_from_dominating() {
        let metrics = json!({
            "24h": {"fee_tvl": 10.48, "volume": 0.20}
        });
        let score = pool_score_json(0.031, &metrics, None, None, None, None)
            .get("score")
            .and_then(Value::as_f64)
            .unwrap();
        assert!(score < 10.0, "micro-pool score={score}");
    }

    #[test]
    fn score_floor_disables_micro_pool_activity_signals() {
        let metrics = json!({
            "24h": {"fee": 0.01, "volume": 10.0, "fee_tvl": 10.0}
        });
        let summary = PoolActivitySummaryRow {
            swap_count_24h: 100,
            event_count_24h: 100,
            volume_quote_24h: 10.0,
            fee_quote_24h: 0.01,
            deposit_quote_24h: 10.0,
            withdraw_quote_24h: 0.0,
            claim_quote_24h: 0.0,
            net_liquidity_delta_quote_24h: 10.0,
            avg_update_interval_secs_24h: Some(1.0),
            latest_update_at_24h: Some(1),
            deposit_count_24h: 10,
            withdraw_count_24h: 0,
            claim_count_24h: 0,
            update_count_24h: 100,
        };
        let score = pool_score_json(0.031, &metrics, Some(&summary), None, None, None)
            .get("score")
            .and_then(Value::as_f64)
            .unwrap();
        assert_eq!(score, 0.0);
    }

    #[test]
    fn zero_liquidity_score_is_zero_even_with_activity() {
        let metrics = json!({
            "24h": {"fee": 0.01, "volume": 10.0, "fee_tvl": 10.0}
        });
        let score = pool_score_json(0.0, &metrics, None, None, None, None)
            .get("score")
            .and_then(Value::as_f64)
            .unwrap();
        assert_eq!(score, 0.0);
    }

    #[test]
    fn pool_pagination_filters_by_dex_before_counting_pages() {
        let body = json!({
            "pools": [
                {"address": "A", "venue": "aquarius"},
                {"address": "B", "venue": "soroswap_amm"},
                {"address": "C", "venue": "aquarius"}
            ]
        });
        let query = PoolListQuery {
            page: Some(1),
            limit: Some(1),
            q: None,
            dex: Some("soroswap".into()),
            venue: None,
            refresh: None,
        };
        let filtered = paginate_pool_body(body, &query);
        assert_eq!(filtered["pagination"]["total"], 1);
        assert_eq!(filtered["pools"][0]["address"], "B");

        let query = PoolListQuery {
            page: Some(1),
            limit: Some(1),
            q: None,
            dex: Some("soroswap_amm".into()),
            venue: None,
            refresh: None,
        };
        let filtered = paginate_pool_body(
            json!({
                "pools": [
                    {"address": "A", "venue": "aquarius"},
                    {"address": "B", "venue": "soroswap_amm"},
                    {"address": "C", "venue": "aquarius"}
                ]
            }),
            &query,
        );
        assert_eq!(filtered["pagination"]["total"], 1);
        assert_eq!(filtered["pools"][0]["address"], "B");

        let query = PoolListQuery {
            page: Some(1),
            limit: Some(10),
            q: None,
            dex: Some("sushi_v3".into()),
            venue: None,
            refresh: None,
        };
        let filtered = paginate_pool_body(json!({"pools": [{"address": "D", "venue": "sushi"}]}), &query);
        assert_eq!(filtered["pagination"]["total"], 1);
    }

    #[test]
    fn pool_pagination_accepts_page_size_alias() {
        let query: PoolListQuery = serde_json::from_value(json!({"page_size": 25})).unwrap();
        assert_eq!(query.limit, Some(25));
    }

    #[test]
    fn copy_status_rejects_policy_approved_event_without_recorder_payload() {
        assert_eq!(
            copy_op_status(Ok(()), false),
            (
                "rejected".to_string(),
                Some("event_not_recordable: incomplete recorder payload".to_string())
            )
        );
        assert_eq!(copy_op_status(Ok(()), true), ("pending".to_string(), None));
        assert_eq!(
            copy_op_status(Err(PolicyReject::PoolNotAllowed), true),
            (
                "rejected".to_string(),
                Some("pool_not_allowed: copy policy rejected".to_string())
            )
        );
    }

    #[test]
    fn pool_pagination_searches_addresses_and_token_metadata() {
        let query = PoolListQuery {
            page: Some(1),
            limit: Some(10),
            q: Some("usdc".into()),
            dex: None,
            venue: None,
            refresh: None,
        };
        let filtered = paginate_pool_body(
            json!({
                "pools": [
                    {"address": "CAAAAA", "venue": "aquarius", "tokens": ["XLM"]},
                    {"address": "CBBBBB", "venue": "soroswap_amm", "token_meta": [{"symbol": "USDC"}]}
                ]
            }),
            &query,
        );
        assert_eq!(filtered["pagination"]["total"], 1);
        assert_eq!(filtered["pools"][0]["address"], "CBBBBB");
    }

    #[test]
    fn fee_tvl_uses_current_tvl_not_historical_average() {
        let mut metrics = json!({
            "24h": {"fee": 0.0009955694677558605, "avg_tvl": 0.00009491574383001295, "fee_tvl": 10.48},
            "1h": {"fee": 0.0, "avg_tvl": 0.0, "fee_tvl": 0.0}
        });
        recompute_fee_tvl_with_current_tvl(&mut metrics, 0.3946358111943709);
        let ratio = metrics["24h"]["fee_tvl"].as_f64().unwrap();
        assert!((ratio - 0.002522).abs() < 0.000001, "ratio={ratio}");
        assert_eq!(metrics["1h"]["fee_tvl"], 0.0);
    }

    #[test]
    fn fee_tvl_is_zero_for_dust_liquidity() {
        let mut metrics = json!({
            "24h": {"fee": 0.01, "avg_tvl": 100.0, "fee_tvl": 0.0001}
        });
        recompute_fee_tvl_with_current_tvl(&mut metrics, 0.00000007);
        assert_eq!(metrics["24h"]["fee_tvl"], 0.0);
    }

    #[test]
    fn stale_rollups_are_not_presented_as_current_windows() {
        let now = 1_800_000_000;
        let mut rollups = HashMap::new();
        rollups.insert(
            "5m".to_string(),
            PoolRollupRow {
                pool_address: "pool".to_string(),
                window: "5m".to_string(),
                as_of_ts: now - 20 * 60,
                sample_count: 2,
                volume_quote: 1.0,
                fee_quote: 0.01,
                avg_tvl: 1.0,
                fee_tvl: 0.01,
                tx_count: 2,
            },
        );
        let metrics = rollups_to_window_metrics(&rollups, now);
        assert!(metrics.as_object().is_some_and(|object| object.is_empty()));
    }

    #[test]
    fn score_uses_usd_bridge_consistently_for_partial_global_coverage() {
        let metrics = json!({
            "24h": {
                "fee": 0.001,
                "fee_usd": 0.00015,
                "volume": 0.20,
                "volume_usd": 0.03
            }
        });
        let score = score_with_usd_preference(0.4, Some(0.06), &metrics, None, Some(0.15));
        let inputs = score.get("score_breakdown").and_then(|v| v.get("inputs")).unwrap();
        assert_eq!(inputs.get("tvl").and_then(Value::as_f64), Some(0.06));
        assert_eq!(inputs.get("volume_24h").and_then(Value::as_f64), Some(0.03));
        let fee_tvl = inputs.get("fee_tvl_24h").and_then(Value::as_f64).unwrap();
        assert!((fee_tvl - 0.0025).abs() < 1e-12, "fee_tvl={fee_tvl}");
    }

    #[test]
    fn reserves_quote_from_latest_update_event() {
        let events = vec![
            PoolEventRow {
                event_id: "1".into(),
                tx_hash: None,
                ledger: 1,
                created_at: 2,
                pool_address: "C".into(),
                kind: "trade".into(),
                body: json!({}),
            },
            PoolEventRow {
                event_id: "2".into(),
                tx_hash: None,
                ledger: 1,
                created_at: 1,
                pool_address: "C".into(),
                kind: "update_reserves".into(),
                body: json!({"derived": {"reserves_quote_xlm": 7669055.87}}),
            },
        ];
        assert_eq!(reserves_quote_xlm_from_events(&events), Some(7669055.87));
    }

    #[test]
    fn fill_window_seeds_volume_from_activity() {
        let mut metrics = json!({
            "24h": {
                "samples": 0,
                "volume": 0.0,
                "fee": 0.0,
                "avg_tvl": 0.0,
                "fee_tvl": 0.0
            }
        });
        let summary = PoolActivitySummaryRow {
            event_count_24h: 10,
            swap_count_24h: 7,
            volume_quote_24h: 1000.0,
            fee_quote_24h: 1.0,
            deposit_quote_24h: 0.0,
            withdraw_quote_24h: 0.0,
            net_liquidity_delta_quote_24h: 0.0,
            claim_quote_24h: 0.0,
            avg_update_interval_secs_24h: None,
            latest_update_at_24h: None,
            deposit_count_24h: 0,
            withdraw_count_24h: 0,
            claim_count_24h: 0,
            update_count_24h: 0,
        };
        fill_window_from_activity(&mut metrics, Some(&summary));
        let w = metrics.get("24h").unwrap();
        assert_eq!(w.get("volume").and_then(|v| v.as_f64()), Some(1000.0));
        assert_eq!(w.get("fee").and_then(|v| v.as_f64()), Some(1.0));
        assert_eq!(w.get("tx_count").and_then(|v| v.as_u64()), Some(7));
    }
}
