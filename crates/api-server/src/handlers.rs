use {
    crate::copy_lp::build_scaled_op_payload,
    crate::index_db::{
        CopyOpRow, CopySessionRow, IndexDb, IndexerStatus, PoolActivityRow,
        PoolActivitySummaryRow, PoolEventRow, PoolRollupRow,
    },
    crate::pricing::service::{PriceService, QuoteMeta},
    crate::pricing::value::{coverage_for, xlm_quote_to_usd, QuoteCoverage, UsdPriceMap},
    crate::token_registry,
    dex::{
        aquarius::positions::positions_for_address,
        db::Db,
        rpc::scval_to_symbol_string,
        support_matrix, SorobanRpc, NATIVE_SAC,
    },
    axum::{
        extract::{Path, Query, State},
        http::StatusCode,
        response::IntoResponse,
        routing::{get, patch, post},
        Json, Router,
    },
    chrono::{Duration, Utc},
    serde::Deserialize,
    serde_json::{json, Value},
    std::{
        collections::{HashMap, HashSet},
        sync::{Arc, Mutex},
    },
};

#[derive(Clone)]
pub struct AppState {
    pub rpc: Arc<SorobanRpc>,
    pub db: Arc<Mutex<Db>>,
    pub index_db: Arc<Mutex<IndexDb>>,
    pub token_meta_cache: Arc<Mutex<HashMap<String, TokenMeta>>>,
    pub prices: Arc<PriceService>,
}

#[derive(Debug, Clone)]
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
        .route(
            "/v1/copy/sessions",
            post(create_copy_session).get(list_copy_sessions),
        )
        .route(
            "/v1/copy/sessions/{id}",
            patch(update_copy_session_handler),
        )
        .route("/v1/copy/sessions/{id}/ops", get(list_copy_ops))
        .route("/v1/copy/ops/{id}", get(get_copy_op))
        .route("/v1/copy/ops/{id}/status", post(set_copy_op_status))
}

const WINDOWS: [(&str, i64); 4] = [("5m", 5), ("1h", 60), ("6h", 360), ("24h", 1_440)];

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
) -> Value {
    let metrics_24h = window_metrics.get("24h");
    let fee_tvl = metrics_24h
        .and_then(|v| v.get("fee_tvl"))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let volume = volume_24h_override.unwrap_or_else(|| {
        metrics_24h
            .and_then(|v| v.get("volume"))
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0)
    });
    let liquidity = tvl.max(1.0);
    let volume_efficiency = volume / liquidity;
    let net_liq = net_liq_override.unwrap_or_else(|| {
        activity_summary
            .map(|s| s.net_liquidity_delta_quote_24h)
            .unwrap_or(0.0)
    });
    let net_liq_ratio = net_liq / liquidity;
    let cadence = cadence_sort_value(activity_summary.and_then(|s| s.avg_update_interval_secs_24h));

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
/// Snapshot `reserves` are raw u128 base units and `list_pools_with_latest` does not
/// expose them; token decimals are also unavailable here. Multiplying raw reserves by
/// Freighter human-unit prices would be wrong, so v1 always bridges TVL via `xlm_usd`.
/// True reserve×price can wait until decimals are wired.
fn bridge_tvl_usd(latest_tvl: f64, xlm_usd: Option<f64>) -> Option<f64> {
    if !(latest_tvl.is_finite() && latest_tvl > 0.0) {
        return None;
    }
    xlm_usd.and_then(|px| xlm_quote_to_usd(latest_tvl, px))
}

/// Prefer latest event-derived reserves quote when snapshot TVL is missing.
fn reserves_quote_xlm_from_events(events: &[PoolEventRow]) -> Option<f64> {
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
            return Some(q);
        }
    }
    None
}

/// When rollups/snapshots are empty but indexer has activity, seed the 24h window.
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
        w.insert("samples".into(), json!(w.get("samples").and_then(|v| v.as_u64()).unwrap_or(0).max(1)));
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
            w.insert(
                "volume_usd".into(),
                json!(xlm_quote_to_usd(volume, xlm_usd)),
            );
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
        (
            "net_liquidity_delta_quote_24h",
            "net_liquidity_delta_usd_24h",
        ),
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
    coverage: &str,
    xlm_usd: Option<f64>,
) -> Value {
    let volume_usd = metrics
        .get("24h")
        .and_then(|v| v.get("volume_usd"))
        .and_then(|v| v.as_f64());
    let use_usd = coverage == "full" && tvl_usd.is_some() && volume_usd.is_some();
    if use_usd {
        let net_liq_usd = activity_summary.and_then(|s| {
            xlm_usd.and_then(|px| xlm_quote_to_usd(s.net_liquidity_delta_quote_24h, px))
        });
        pool_score_json(
            tvl_usd.unwrap_or(latest_tvl),
            metrics,
            activity_summary,
            volume_usd,
            net_liq_usd,
        )
    } else {
        pool_score_json(latest_tvl, metrics, activity_summary, None, None)
    }
}

fn build_window_metrics(
    latest_tvl: f64,
    fee_bps: u32,
    rows: &[dex::types::PoolSnapshotRow],
) -> serde_json::Value {
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

fn rollups_to_window_metrics(rollups: &HashMap<String, PoolRollupRow>) -> serde_json::Value {
    let mut out = serde_json::Map::new();
    for row in rollups.values() {
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

/// Multi-DEX support matrix (`DexAdaptor` registry). Aquarius is production; others scaffold.
async fn list_venues() -> impl IntoResponse {
    let venues = support_matrix()
        .into_iter()
        .map(|row| {
            json!({
                "venue_id": row.venue_id,
                "name": row.name,
                "status": row.status,
                "capabilities": row.capabilities,
                "notes": row.notes,
            })
        })
        .collect::<Vec<_>>();
    Json(json!({
        "venues": venues,
        "note": "Strategies bind to venue_id. Only production venues accept live copy/index paths today.",
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

async fn list_pools(State(state): State<AppState>) -> impl IntoResponse {
    let (
        rows,
        stats,
        window_rows,
        rollups_map,
        activity_map,
        activity_summary_map,
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
            index_db.status().ok(),
        )
    };
    match (rows, stats, window_rows) {
        (Ok(mut rows), Ok(stats), Ok(window_rows)) => {
            let mut grouped: HashMap<String, Vec<dex::types::PoolSnapshotRow>> = HashMap::new();
            for row in window_rows {
                grouped
                    .entry(row.pool_address.clone())
                    .or_default()
                    .push(row);
            }
            let token_ids: HashSet<String> = rows
                .iter()
                .filter_map(|row| row.get("tokens").and_then(|v| v.as_array()).cloned())
                .flat_map(|tokens| tokens.into_iter())
                .filter_map(|token| token.as_str().map(ToOwned::to_owned))
                .collect();
            let token_meta_map =
                resolve_token_meta_map(state.rpc.as_ref(), &state.token_meta_cache, &token_ids)
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
            let coverage = quote_meta.coverage.clone();

            for row in &mut rows {
                let Some(obj) = row.as_object_mut() else {
                    continue;
                };
                let Some(address) = obj.get("address").and_then(|v| v.as_str()) else {
                    continue;
                };
                let address = address.to_string();
                let latest_tvl = obj.get("tvl").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let fee_bps = obj.get("fee_bps").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                let mut metrics = rollups_map
                    .get(&address)
                    .map(rollups_to_window_metrics)
                    .unwrap_or_else(|| {
                        grouped
                            .get(&address)
                            .map(|rows| build_window_metrics(latest_tvl, fee_bps, rows))
                            .unwrap_or_else(|| build_window_metrics(latest_tvl, fee_bps, &[]))
                    });
                enrich_window_metrics_usd(&mut metrics, xlm_usd);
                let tvl_usd = bridge_tvl_usd(latest_tvl, xlm_usd);
                let activity_summary = activity_summary_map.get(&address);
                let score_json = score_with_usd_preference(
                    latest_tvl,
                    tvl_usd,
                    &metrics,
                    activity_summary,
                    &coverage,
                    xlm_usd,
                );
                obj.insert("tvl_usd".into(), json!(tvl_usd));
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
            Json(json!({
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
            }))
            .into_response()
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

async fn pool_detail(
    State(state): State<AppState>,
    Path(address): Path<String>,
) -> impl IntoResponse {
    let (meta, history, stats) = {
        let db = state.db.lock().unwrap();
        (
            db.pool_meta(&address).ok().flatten(),
            db.history(&address, 90).unwrap_or_default(),
            db.stats().ok(),
        )
    };
    let latest = history.last();
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
    let (token_meta, token_meta_map, tokens) = if let Some((_, tokens_json, _)) = meta.as_ref() {
        let tokens: Vec<String> = serde_json::from_str(tokens_json).unwrap_or_default();
        if tokens.is_empty() {
            (Vec::new(), HashMap::new(), tokens)
        } else {
            let token_ids: HashSet<String> = tokens.iter().cloned().collect();
            let map =
                resolve_token_meta_map(state.rpc.as_ref(), &state.token_meta_cache, &token_ids)
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
    let coverage = quote_meta.coverage.clone();

    let fee_bps = meta.as_ref().map(|m| m.2 as u32).unwrap_or(0);
    let mut latest_tvl = latest.map(|row| row.tvl).unwrap_or(0.0);
    let mut tvl_source = if latest_tvl > 0.0 {
        "snapshot"
    } else {
        "none"
    };
    if latest_tvl <= 0.0 {
        if let Ok(events) = state.index_db.lock().unwrap().recent_pool_events(&address, 40) {
            if let Some(reserves_xlm) = reserves_quote_xlm_from_events(&events) {
                latest_tvl = reserves_xlm;
                tvl_source = "event_reserves";
            }
        }
    }
    let mut window_metrics = if rollups.is_empty() {
        build_window_metrics(latest_tvl, fee_bps, &history)
    } else {
        rollups_to_window_metrics(&rollups)
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
    enrich_window_metrics_usd(&mut window_metrics, xlm_usd);
    let tvl_usd = bridge_tvl_usd(latest_tvl, xlm_usd);
    let score_json = score_with_usd_preference(
        latest_tvl,
        tvl_usd,
        &window_metrics,
        activity_summary.as_ref(),
        &coverage,
        xlm_usd,
    );
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
        "pool_type": meta.as_ref().map(|m| m.0.clone()),
        "tokens": meta.as_ref().and_then(|m| serde_json::from_str::<Value>(&m.1).ok()),
        "fee_bps": meta.as_ref().map(|m| m.2),
        "token_meta": if token_meta.is_empty() { None } else { Some(token_meta) },
        "latest": latest,
        "tvl": if latest_tvl > 0.0 { Some(latest_tvl) } else { None },
        "tvl_usd": tvl_usd,
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
    rpc: &SorobanRpc,
    cache: &Arc<Mutex<HashMap<String, TokenMeta>>>,
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

    for token in missing {
        let meta = resolve_one_token_meta(rpc, &token).await;
        {
            let mut cache_guard = cache.lock().unwrap();
            cache_guard.insert(token.clone(), meta.clone());
        }
        out.insert(token, meta);
    }
    out
}

async fn resolve_one_token_meta(rpc: &SorobanRpc, token: &str) -> TokenMeta {
    let curated = token_registry::find_token(token);
    let symbol = rpc
        .call_no_args(token, "symbol")
        .await
        .ok()
        .and_then(|val| scval_to_symbol_string(&val).ok())
        .filter(|v| !v.trim().is_empty())
        .or_else(|| curated.map(|entry| entry.symbol.to_string()))
        .unwrap_or_else(|| short_token_label(token));
    let name = rpc
        .call_no_args(token, "name")
        .await
        .ok()
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
            .and_then(|(_, tokens_json, _)| serde_json::from_str(&tokens_json).ok())
            .unwrap_or_default()
    };
    let token_ids: HashSet<String> = tokens.iter().cloned().collect();
    let token_meta_map = if token_ids.is_empty() {
        HashMap::new()
    } else {
        resolve_token_meta_map(state.rpc.as_ref(), &state.token_meta_cache, &token_ids).await
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
}

/// Ranked Aquarius LP actors by claimed fees / deposits (Copy scouting board).
async fn lp_leaders(
    State(state): State<AppState>,
    Query(q): Query<LeadersBoardQuery>,
) -> impl IntoResponse {
    let limit = q.limit.unwrap_or(25).clamp(1, 100);
    let window_days = q.window_days.unwrap_or(30).clamp(1, 90);
    let since_ts = Utc::now().timestamp() - window_days * 24 * 3600;
    let leaders = {
        let index_db = state.index_db.lock().unwrap();
        match index_db.top_liquidity_actors(since_ts, limit) {
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

    let rows: Vec<Value> = leaders
        .into_iter()
        .map(|a| {
            let fee_capital_ratio = if a.deposit_quote_xlm > 0.0 {
                Some(a.claim_quote_xlm / a.deposit_quote_xlm)
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
                "net_liquidity_quote_xlm": a.deposit_quote_xlm - a.withdraw_quote_xlm,
                "fee_capital_ratio": fee_capital_ratio,
                "distinct_pools": a.distinct_pools,
                "last_activity_at": a.last_activity_at,
            })
        })
        .collect();

    Json(json!({
        "window_days": window_days,
        "since_ts": since_ts,
        "xlm_usd": xlm_usd,
        "leaders": rows,
        "sort": "claim_quote_xlm_desc",
        "honesty": "Claimed fee quote is the best indexed earnings proxy; fee_capital_ratio = claimed/deposits (not ROI/win rate).",
    }))
    .into_response()
}

async fn list_positions(
    State(state): State<AppState>,
    Query(q): Query<AddressQuery>,
) -> impl IntoResponse {
    if !q.address.starts_with('G') || q.address.len() < 56 {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "invalid stellar address", "code": "bad_address" })),
        )
            .into_response();
    }
    let pools = {
        let db = state.db.lock().unwrap();
        db.list_pool_addresses().unwrap_or_default()
    };
    let stats = {
        let db = state.db.lock().unwrap();
        db.stats().ok()
    };
    if pools.is_empty() {
        return Json(json!({
            "address": q.address,
            "positions": [],
            "indexed_pool_count": 0,
            "last_snapshot_at": stats.and_then(|s| s.latest_snapshot_at),
            "note": "No indexed pools yet — run snapshotter first"
        }))
        .into_response();
    }
    let pricing = {
        let db = state.db.lock().unwrap();
        db.pool_states_for_pricing().unwrap_or_default()
    };
    let positions = positions_for_address(state.rpc.as_ref(), &q.address, &pools, &pricing).await;
    let stats = {
        let db = state.db.lock().unwrap();
        db.stats().ok()
    };
    let pool_count = stats.as_ref().map(|s| s.pool_count).unwrap_or(pools.len());
    let last_snapshot_at = stats.and_then(|s| s.latest_snapshot_at);
    let note = if positions.is_empty() {
        Some("No Aquarius LP found for this address in the current indexed pool set")
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

async fn positions_summary(
    State(state): State<AppState>,
    Query(q): Query<AddressQuery>,
) -> impl IntoResponse {
    if !q.address.starts_with('G') || q.address.len() < 56 {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "invalid stellar address", "code": "bad_address" })),
        )
            .into_response();
    }
    let pools = {
        let db = state.db.lock().unwrap();
        db.list_pool_addresses().unwrap_or_default()
    };
    let stats = {
        let db = state.db.lock().unwrap();
        db.stats().ok()
    };
    let pricing = {
        let db = state.db.lock().unwrap();
        db.pool_states_for_pricing().unwrap_or_default()
    };
    let positions = positions_for_address(state.rpc.as_ref(), &q.address, &pools, &pricing).await;
    let mut net_worth = 0.0;
    let mut fees = 0.0;
    let mut il_sum = 0.0;
    let mut il_n = 0usize;
    for p in &positions {
        if let Some(v) = p.value_quote {
            net_worth += v;
        }
        if let Some(f) = p.fees_unclaimed_quote {
            fees += f;
        }
        if let Some(il) = p.il_est {
            il_sum += il;
            il_n += 1;
        }
    }
    Json(json!({
        "address": q.address,
        "net_worth": net_worth,
        "fees_unclaimed": fees,
        "il_est_avg": if il_n > 0 { Some(il_sum / il_n as f64) } else { None },
        "position_count": positions.len(),
        "indexed_pool_count": stats.as_ref().map(|s| s.pool_count).unwrap_or(pools.len()),
        "last_snapshot_at": stats.and_then(|s| s.latest_snapshot_at),
        "note": if pools.is_empty() {
            Some("No indexed pools yet — run snapshotter first")
        } else if positions.is_empty() {
            Some("No Aquarius LP found for this address in the current indexed pool set")
        } else {
            None::<&str>
        },
        "quote_asset": "XLM",
    }))
    .into_response()
}

/// Portfolio + recent liquidity activity for scouting Copy leaders (Smart LP–style).
async fn lp_profile(
    State(state): State<AppState>,
    Query(q): Query<AddressQuery>,
) -> impl IntoResponse {
    if !valid_stellar_address(&q.address) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "invalid stellar address", "code": "bad_address" })),
        )
            .into_response();
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
        db.list_pool_addresses()
            .map(|p| p.len())
            .unwrap_or(0)
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
            (Err(error), _, _, _)
            | (_, Err(error), _, _)
            | (_, _, Err(error), _)
            | (_, _, _, Err(error)) => {
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
    let positions = if scan_pools.is_empty() {
        Vec::new()
    } else {
        positions_for_address(state.rpc.as_ref(), &q.address, &scan_pools, &pricing).await
    };

    let mut net_worth = 0.0;
    let mut fees = 0.0;
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

    let window_json = |a: &crate::index_db::ActorLiquidityActivity| {
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
    let activity_30d_json = window_json(&activity_30d);
    let activity_7d_json = window_json(&activity_7d);

    let fee_capital = |claim: f64, deposit: f64| -> Option<f64> {
        if deposit > 0.0 && claim.is_finite() && deposit.is_finite() {
            Some(claim / deposit)
        } else {
            None
        }
    };
    let months_active = match (first_activity_at, last_activity_at) {
        (Some(first), Some(last)) if last >= first => {
            ((last - first) as f64 / (30.0 * 86_400.0)).max(1.0 / 30.0)
        }
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
                "created_at": e.created_at,
                "tx_hash": e.tx_hash,
                "quote_xlm": e.body.pointer("/derived/total_quote_xlm").and_then(|v| v.as_f64())
                    .or_else(|| e.body.pointer("/derived/fee_quote_xlm").and_then(|v| v.as_f64())),
            })
        })
        .collect();

    let position_scan_note = if scan_pools.is_empty() {
        Some("Open positions skipped — no indexed liquidity events for this address in 90d (RPC scan would hit all pools)")
    } else {
        None
    };

    Json(json!({
        "address": q.address,
        "venue_id": "aquarius",
        "portfolio": {
            "net_worth_xlm": net_worth,
            "net_worth_usd": to_usd(net_worth),
            "fees_unclaimed_xlm": fees,
            "fees_unclaimed_usd": to_usd(fees),
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
        "positions": positions,
        "position_pools_scanned": scan_pools.len(),
        "recent_events": recent_json,
        "indexed_pool_count": stats.as_ref().map(|s| s.pool_count).unwrap_or(indexed_pool_count),
        "last_snapshot_at": stats.and_then(|s| s.latest_snapshot_at),
        "xlm_usd": xlm_usd,
        "note": position_scan_note.or(if indexed_pool_count == 0 {
            Some("No indexed pools yet — run snapshotter first")
        } else if positions.is_empty() && empty_activity && lifetime.claim_count == 0 {
            Some("No Aquarius LP or recent liquidity events for this address in the indexed set")
        } else {
            None
        }),
        "honesty": "Proxies use indexed claim/deposit quotes — not full PnL vs entry, not win rate. Open positions scan pools touched in ~90d only.",
    }))
    .into_response()
}

const COPY_RECONCILE_BATCH: usize = 500;

const COPY_OP_STATUSES: &[&str] = &[
    "drafted", "skipped", "signed", "failed", "insufficient",
];

const COPY_SESSION_STATUSES: &[&str] = &["active", "paused", "stopped"];

fn valid_stellar_address(address: &str) -> bool {
    address.starts_with('G') && address.len() >= 56
}

fn new_copy_entity_id() -> String {
    let ts = chrono::Utc::now().timestamp_nanos_opt().unwrap_or_else(|| {
        chrono::Utc::now()
            .timestamp()
            .saturating_mul(1_000_000_000)
    });
    format!("{ts:x}")
}

fn copy_session_json(session: &CopySessionRow) -> Value {
    json!({
        "id": session.id,
        "follower_address": session.follower_address,
        "leader_address": session.leader_address,
        "coefficient": session.coefficient,
        "status": session.status,
        "include_claims": session.include_claims,
        "cursor_ts": session.cursor_ts,
        "watermark_ts": session.watermark_ts,
        "watermark_event_id": session.watermark_event_id,
        "created_at": session.created_at,
        "updated_at": session.updated_at,
    })
}

fn copy_op_json(op: &CopyOpRow) -> Value {
    let leader_amounts =
        serde_json::from_str(&op.leader_amounts_json).unwrap_or(Value::Null);
    let scaled_amounts =
        serde_json::from_str(&op.scaled_amounts_json).unwrap_or(Value::Null);
    json!({
        "id": op.id,
        "session_id": op.session_id,
        "source_event_id": op.source_event_id,
        "pool_address": op.pool_address,
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
        let events = index_db.events_for_actor_since(
            &session.leader_address,
            since,
            after_event_id,
            COPY_RECONCILE_BATCH,
        )?;
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
                status: "pending".into(),
                note: None,
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
    if !body.coefficient.is_finite() || body.coefficient <= 0.0 {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "coefficient must be > 0", "code": "bad_coefficient" })),
        )
            .into_response();
    }

    let include_claims = body.include_claims.unwrap_or(false);
    let index_db = state.index_db.lock().unwrap();
    match index_db.create_copy_session(
        &body.follower_address,
        &body.leader_address,
        body.coefficient,
        include_claims,
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
        if !coefficient.is_finite() || coefficient <= 0.0 {
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
            let ops_json = ops.iter().map(copy_op_json).collect::<Vec<_>>();
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

async fn get_copy_op(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let index_db = state.index_db.lock().unwrap();
    match index_db.get_copy_op(&id) {
        Ok(Some(op)) => Json(copy_op_json(&op)).into_response(),
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
                "error": "status must be drafted, skipped, signed, failed, or insufficient",
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
    use super::*;
    use serde_json::json;

    #[test]
    fn bridge_tvl_usd_skips_zero_and_negative() {
        assert_eq!(bridge_tvl_usd(0.0, Some(0.17)), None);
        assert_eq!(bridge_tvl_usd(-1.0, Some(0.17)), None);
        assert!(bridge_tvl_usd(100.0, Some(0.17)).unwrap() > 0.0);
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
        assert_eq!(
            reserves_quote_xlm_from_events(&events),
            Some(7669055.87)
        );
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
