use {
    crate::{
        db::{CachedPoolState, IndexDb},
        types::{PoolEvent, PoolEventKind, PoolSwap},
    },
    anyhow::{anyhow, Context, Result},
    base64::{engine::general_purpose::STANDARD as BASE64, Engine as _},
    chrono::DateTime,
    dex::{
        aquarius::{
            pool::hydrate_pool, pricing::price_book_from_pools, router::discover_pool_addresses,
        },
        soroswap::{discover_mainnet_pool_addresses, hydrate_pool as hydrate_soroswap_pool},
        PoolType, SharePoolState, SorobanRpc,
    },
    metrics::PriceBook,
    serde::Deserialize,
    serde_json::{json, Value},
    std::collections::HashMap,
    stellar_strkey::{ed25519::PublicKey, Contract},
    stellar_xdr::curr::{self as xdr, Limits, ReadXdr},
    tracing::{info, warn},
};

#[derive(Debug, Clone, Deserialize)]
struct ContractEvent {
    #[serde(rename = "type")]
    event_type: String,
    ledger: u32,
    #[serde(rename = "contractId")]
    contract_id: String,
    id: String,
    #[serde(rename = "txHash", default)]
    tx_hash: Option<String>,
    #[serde(rename = "ledgerClosedAt", default)]
    ledger_closed_at: Option<String>,
    #[serde(default)]
    value: Option<Value>,
    #[serde(default)]
    topic: Option<Vec<String>>,
}

pub struct EventScanResult {
    pub latest_ledger: u32,
    pub known_pools: usize,
    pub raw_event_count: usize,
    pub events: Vec<PoolEvent>,
    pub swaps: Vec<PoolSwap>,
}

struct PoolIndexContext {
    fee_bps_by_pool: HashMap<String, u32>,
    tokens_by_pool: HashMap<String, Vec<String>>,
    dex_by_pool: HashMap<String, String>,
    price_book: PriceBook,
}

pub struct PoolEventScanner {
    pools: Vec<String>,
    context: PoolIndexContext,
}

impl PoolEventScanner {
    pub async fn discover(rpc: &SorobanRpc, db: &IndexDb) -> Result<Self> {
        let venues = std::env::var("INDEXER_VENUES").unwrap_or_else(|_| "aquarius,soroswap".into());
        let mut pool_venues = Vec::new();
        for venue in venues
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            let discovered = match venue {
                "aquarius" => discover_pool_addresses(rpc).await?,
                "soroswap" | "soroswap_amm" => discover_mainnet_pool_addresses(rpc).await?,
                other => anyhow::bail!("unknown indexer venue: {other}"),
            };
            info!(
                venue,
                pools = discovered.len(),
                "indexer venue pools discovered"
            );
            pool_venues.extend(
                discovered
                    .into_iter()
                    .map(|address| (venue.to_string(), address)),
            );
        }
        pool_venues.sort_by(|a, b| a.1.cmp(&b.1));
        pool_venues.dedup_by(|a, b| a.1 == b.1);
        let pools = pool_venues
            .iter()
            .map(|(_, address)| address.clone())
            .collect::<Vec<_>>();
        let context = build_index_context(rpc, db, &pool_venues).await?;
        Ok(Self { pools, context })
    }

    pub async fn scan(
        &self,
        rpc: &SorobanRpc,
        start_ledger: u32,
        end_ledger_inclusive: u32,
    ) -> Result<EventScanResult> {
        let pools = &self.pools;
        let context = &self.context;
        // Soroban RPC `endLedger` is exclusive.
        let end_ledger_exclusive = end_ledger_inclusive.saturating_add(1);
        let mut raw_events = Vec::new();
        for contract_ids in pools.chunks(5) {
            let chunk_events = rpc
                .get_events(
                    start_ledger,
                    Some(end_ledger_exclusive),
                    contract_ids,
                    10_000,
                )
                .await
                .with_context(|| {
                    format!(
                        "getEvents for pools[{}..{}]",
                        raw_events.len(),
                        raw_events.len() + contract_ids.len()
                    )
                })?;
            raw_events.extend(chunk_events);
        }

        let mut events = Vec::new();
        let mut swaps = Vec::new();
        let mut tx_source_cache: HashMap<String, Option<String>> = HashMap::new();
        for raw in &raw_events {
            let event: ContractEvent = match serde_json::from_value(raw.clone()) {
                Ok(event) => event,
                Err(_) => continue,
            };
            if event.event_type != "contract" {
                continue;
            }
            let tx_actor = if event_needs_tx_actor(&event) {
                resolve_tx_actor(rpc, &mut tx_source_cache, event.tx_hash.as_deref()).await?
            } else {
                None
            };
            let Some(parsed) = parse_pool_event(&event, context, tx_actor.as_deref())? else {
                continue;
            };
            if let Some(swap) = pool_swap_from_event(&parsed) {
                swaps.push(swap);
            }
            events.push(parsed);
        }

        info!(
            start_ledger,
            end_ledger = end_ledger_inclusive,
            pools = pools.len(),
            raw_events = raw_events.len(),
            parsed_events = events.len(),
            swaps = swaps.len(),
            "pool event scan completed"
        );
        Ok(EventScanResult {
            latest_ledger: end_ledger_inclusive,
            known_pools: pools.len(),
            raw_event_count: raw_events.len(),
            events,
            swaps,
        })
    }
}

async fn build_index_context(
    rpc: &SorobanRpc,
    db: &IndexDb,
    pool_venues: &[(String, String)],
) -> Result<PoolIndexContext> {
    let pools = pool_venues
        .iter()
        .map(|(_, address)| address.clone())
        .collect::<Vec<_>>();
    let cached_states = db.cached_pool_states().unwrap_or_default();
    let mut cached_by_pool = HashMap::new();
    let mut hydrated: Vec<SharePoolState> = Vec::new();
    let mut fee_bps_by_pool = HashMap::new();
    let mut tokens_by_pool = HashMap::new();
    let dex_by_pool = pool_venues
        .iter()
        .map(|(venue, address)| (address.clone(), venue.clone()))
        .collect::<HashMap<_, _>>();

    for state in cached_states {
        fee_bps_by_pool.insert(state.address.clone(), state.fee_bps);
        tokens_by_pool.insert(state.address.clone(), state.tokens.clone());
        cached_by_pool.insert(state.address.clone(), state.clone());
        if !state.tokens.is_empty()
            && state.tokens.len() == state.reserves.len()
            && !state.reserves.is_empty()
        {
            hydrated.push(cached_to_share_pool_state(&state));
        }
    }

    let mut hydrated_missing = 0usize;
    for pool in &pools {
        if cached_by_pool.contains_key(pool) {
            continue;
        }
        let venue = dex_by_pool
            .get(pool)
            .map(String::as_str)
            .unwrap_or("aquarius");
        let hydrated_state = match venue {
            "soroswap" | "soroswap_amm" => hydrate_soroswap_pool(rpc, &pool).await,
            _ => hydrate_pool(rpc, &pool).await,
        };
        match hydrated_state {
            Ok(state) => {
                fee_bps_by_pool.insert(state.address.clone(), state.fee_bps);
                tokens_by_pool.insert(state.address.clone(), state.tokens.clone());
                hydrated.push(state);
                hydrated_missing += 1;
            }
            Err(error) => warn!(pool, %error, "hydrate pool metadata failed for indexer"),
        }
    }
    info!(
        total_pools = pools.len(),
        cached_pools = cached_by_pool.len(),
        hydrated_missing,
        priceable_pools = hydrated.len(),
        "built pool index context"
    );
    let price_book = price_book_from_pools(&hydrated);
    Ok(PoolIndexContext {
        fee_bps_by_pool,
        tokens_by_pool,
        dex_by_pool,
        price_book,
    })
}

fn cached_to_share_pool_state(state: &CachedPoolState) -> SharePoolState {
    SharePoolState {
        address: state.address.clone(),
        pool_type: PoolType::Unknown,
        tokens: state.tokens.clone(),
        reserves: state.reserves.clone(),
        fee_bps: state.fee_bps,
        total_shares: 0,
        share_token: None,
        amp: None,
    }
}

async fn resolve_tx_actor(
    rpc: &SorobanRpc,
    cache: &mut HashMap<String, Option<String>>,
    tx_hash: Option<&str>,
) -> Result<Option<String>> {
    let Some(tx_hash) = tx_hash else {
        return Ok(None);
    };
    if let Some(cached) = cache.get(tx_hash) {
        return Ok(cached.clone());
    }
    let source = rpc.get_transaction_source(tx_hash).await;
    Ok(cache_tx_source_lookup(cache, tx_hash, source))
}

/// RPC NOT_FOUND caches `None` so later events in the batch skip re-fetch.
/// Transient RPC failures are *not* cached so a later retry can still resolve the actor.
fn cache_tx_source_lookup(
    cache: &mut HashMap<String, Option<String>>,
    tx_hash: &str,
    source: Result<Option<String>>,
) -> Option<String> {
    match source {
        Ok(actor) => {
            cache.insert(tx_hash.to_string(), actor.clone());
            actor
        }
        Err(error) => {
            warn!(tx_hash, %error, "tx source lookup failed; will retry later");
            None
        }
    }
}

/// Fill `derived.actor` on recent deposit/withdraw rows that were stored without one.
pub async fn backfill_missing_actors(
    rpc: &SorobanRpc,
    db: &IndexDb,
    limit: usize,
) -> Result<usize> {
    let missing = db.list_liquidity_events_missing_actor(limit)?;
    if missing.is_empty() {
        return Ok(0);
    }
    let mut cache = HashMap::new();
    let mut patched = 0usize;
    for (event_id, tx_hash, body_json) in missing {
        let Some(actor) = resolve_tx_actor(rpc, &mut cache, Some(&tx_hash)).await? else {
            continue;
        };
        let Ok(mut body) = serde_json::from_str::<Value>(&body_json) else {
            continue;
        };
        let Some(derived) = body.get_mut("derived") else {
            continue;
        };
        let Some(obj) = derived.as_object_mut() else {
            continue;
        };
        obj.insert("actor".to_string(), json!(actor));
        if db.patch_event_actor(&event_id, &body.to_string())? {
            patched += 1;
        }
    }
    if patched > 0 {
        info!(patched, "backfilled missing liquidity event actors");
    }
    Ok(patched)
}

fn event_needs_tx_actor(event: &ContractEvent) -> bool {
    let Some(topics) = &event.topic else {
        return false;
    };
    let Ok(Some(kind_name)) = topic_symbol_name(topics.first()) else {
        return false;
    };
    matches!(
        kind_name.as_str(),
        "deposit_liquidity" | "withdraw_liquidity"
    )
}

fn parse_pool_event(
    event: &ContractEvent,
    context: &PoolIndexContext,
    tx_actor: Option<&str>,
) -> Result<Option<PoolEvent>> {
    let Some(topics) = &event.topic else {
        return Ok(None);
    };
    let Some(first_topic) = topic_symbol_name(topics.first())? else {
        return Ok(None);
    };
    let is_soroswap = context
        .dex_by_pool
        .get(&event.contract_id)
        .is_some_and(|venue| venue == "soroswap" || venue == "soroswap_amm");
    let kind_name = if is_soroswap && first_topic == "SoroswapPair" {
        topic_symbol_name(topics.get(1))?.unwrap_or_default()
    } else {
        first_topic
    };
    let kind = match (is_soroswap, kind_name.as_str()) {
        (true, "deposit") => PoolEventKind::DepositLiquidity,
        (true, "withdraw") => PoolEventKind::WithdrawLiquidity,
        (true, "swap") => PoolEventKind::Trade,
        (true, "sync") => PoolEventKind::ReservesSync,
        _ => PoolEventKind::parse(&kind_name),
    };
    if kind == PoolEventKind::Unknown {
        return Ok(None);
    }

    let decoded_topics = topics
        .iter()
        .map(|item| decode_scval_b64(item).and_then(|value| scval_to_json(&value)))
        .collect::<Result<Vec<_>>>()?;
    let decoded_body = event_body_json(event.value.as_ref())?;
    let topic_actor = if is_soroswap {
        actor_from_soroswap_data(&decoded_body)
    } else {
        actor_from_topics(kind.as_str(), &decoded_topics)
    };
    let actor = topic_actor.as_deref().or(tx_actor);
    let derived = derive_event_fields(
        &kind,
        &event.contract_id,
        &decoded_topics,
        &decoded_body,
        context,
        actor,
        is_soroswap,
    );
    let created_at = ledger_closed_at_to_unix(event.ledger_closed_at.as_deref(), event.ledger);
    let body_json = json!({
        "contract_id": event.contract_id,
        "topic": decoded_topics,
        "data": decoded_body,
        "derived": derived,
    })
    .to_string();

    Ok(Some(PoolEvent {
        event_id: event.id.clone(),
        tx_hash: event.tx_hash.clone(),
        ledger: event.ledger,
        created_at,
        pool_address: event.contract_id.clone(),
        kind,
        body_json,
    }))
}

fn actor_from_topics(kind: &str, topic: &[Value]) -> Option<String> {
    match kind {
        "claim_fees" | "claim_protocol_fee" => topic
            .iter()
            .filter_map(value_address_string)
            .find(|addr| is_stellar_account_address(addr)),
        "trade" => topic
            .get(3)
            .and_then(value_address_string)
            .filter(|addr| is_stellar_account_address(addr)),
        "deposit_liquidity" | "withdraw_liquidity" => None,
        _ => None,
    }
}

fn is_stellar_account_address(addr: &str) -> bool {
    addr.starts_with('G')
}

fn derive_event_fields(
    kind: &PoolEventKind,
    pool_address: &str,
    topic: &[Value],
    data: &[Value],
    context: &PoolIndexContext,
    actor: Option<&str>,
    is_soroswap: bool,
) -> Value {
    let pool_fee_bps = context.fee_bps_by_pool.get(pool_address).copied();
    let pool_tokens = context
        .tokens_by_pool
        .get(pool_address)
        .cloned()
        .unwrap_or_default();

    match kind {
        PoolEventKind::Trade => {
            if is_soroswap {
                return derive_soroswap_swap(pool_fee_bps, &pool_tokens, data, context, actor);
            }
            let token_in = topic.get(1).and_then(value_address_string);
            let token_out = topic.get(2).and_then(value_address_string);
            let amount_in = data.get(0).and_then(value_amount_string);
            let amount_out = data.get(1).and_then(value_amount_string);
            let fee_amount = data.get(2).and_then(value_amount_string);
            let volume_in =
                token_in
                    .as_deref()
                    .zip(amount_in.as_deref())
                    .and_then(|(token, amount)| {
                        estimate_amount_xlm(&context.price_book, token, amount)
                    });
            let volume_out =
                token_out
                    .as_deref()
                    .zip(amount_out.as_deref())
                    .and_then(|(token, amount)| {
                        estimate_amount_xlm(&context.price_book, token, amount)
                    });
            // One-sided notional (Aquarius-style): prefer amount_in, fall back to amount_out
            // when the input token is missing from the price book.
            let volume_quote_xlm = volume_in.or(volume_out);
            let fee_quote_xlm = token_in
                .as_deref()
                .zip(fee_amount.as_deref())
                .and_then(|(token, amount)| estimate_amount_xlm(&context.price_book, token, amount))
                .or_else(|| {
                    // fee is in token_in; if unpriced, approximate via volume * fee_bps
                    pool_fee_bps
                        .and_then(|bps| volume_quote_xlm.map(|v| v * (bps as f64) / 10_000.0))
                });
            derived_with_actor(
                json!({
                    "pool_fee_bps": pool_fee_bps,
                    "token_in": token_in,
                    "token_out": token_out,
                    "amount_in": amount_in,
                    "amount_out": amount_out,
                    "fee_amount": fee_amount,
                    "volume_quote_xlm": volume_quote_xlm,
                    "fee_quote_xlm": fee_quote_xlm,
                }),
                actor,
            )
        }
        PoolEventKind::ClaimFees => {
            let token0 = topic.get(2).and_then(value_address_string);
            let token1 = topic.get(3).and_then(value_address_string);
            let amount0 = data.get(0).and_then(value_amount_string);
            let amount1 = data.get(1).and_then(value_amount_string);
            let fee_quote_xlm = estimate_two_token_amounts_xlm(
                &context.price_book,
                token0.as_deref(),
                amount0.as_deref(),
                token1.as_deref(),
                amount1.as_deref(),
            );
            derived_with_actor(
                json!({
                    "pool_fee_bps": pool_fee_bps,
                    "token0": token0,
                    "token1": token1,
                    "amount0": amount0,
                    "amount1": amount1,
                    "fee_quote_xlm": fee_quote_xlm,
                }),
                actor,
            )
        }
        PoolEventKind::ClaimProtocolFee => {
            let token = topic.get(1).and_then(value_address_string);
            let amount = data.get(0).and_then(value_amount_string);
            let fee_quote_xlm =
                token
                    .as_deref()
                    .zip(amount.as_deref())
                    .and_then(|(token, amount)| {
                        estimate_amount_xlm(&context.price_book, token, amount)
                    });
            derived_with_actor(
                json!({
                    "pool_fee_bps": pool_fee_bps,
                    "token": token,
                    "amount": amount,
                    "fee_quote_xlm": fee_quote_xlm,
                }),
                actor,
            )
        }
        PoolEventKind::DepositLiquidity | PoolEventKind::WithdrawLiquidity => {
            if is_soroswap {
                return derive_soroswap_liquidity(
                    kind,
                    pool_fee_bps,
                    &pool_tokens,
                    data,
                    context,
                    actor,
                );
            }
            let share_amount = data.first().and_then(value_amount_string);
            let token_amounts = pool_tokens
                .iter()
                .enumerate()
                .filter_map(|(idx, token)| {
                    let amount = data.get(idx + 1).and_then(value_amount_string)?;
                    Some(json!({
                        "token": token,
                        "amount": amount,
                    }))
                })
                .collect::<Vec<_>>();
            let total_quote_xlm = pool_tokens
                .iter()
                .enumerate()
                .filter_map(|(idx, token)| {
                    let amount = data.get(idx + 1).and_then(value_amount_string)?;
                    estimate_amount_xlm(&context.price_book, token, &amount)
                })
                .sum::<f64>();
            derived_with_actor(
                json!({
                    "pool_fee_bps": pool_fee_bps,
                    "share_amount": share_amount,
                    "token_amounts": token_amounts,
                    "total_quote_xlm": if total_quote_xlm > 0.0 { Some(total_quote_xlm) } else { None::<f64> },
                }),
                actor,
            )
        }
        PoolEventKind::UpdateReserves | PoolEventKind::ReservesSync => {
            if is_soroswap {
                return derive_soroswap_sync(pool_fee_bps, &pool_tokens, data, context);
            }
            let reserves = data
                .iter()
                .map(value_amount_string)
                .collect::<Option<Vec<_>>>()
                .unwrap_or_default();
            let reserve_rows = pool_tokens
                .iter()
                .zip(reserves.iter())
                .map(|(token, amount)| {
                    json!({
                        "token": token,
                        "amount": amount,
                    })
                })
                .collect::<Vec<_>>();
            let reserves_quote_xlm =
                estimate_reserves_xlm(&context.price_book, &pool_tokens, &reserves);
            json!({
                "pool_fee_bps": pool_fee_bps,
                "reserves": reserve_rows,
                "reserves_quote_xlm": reserves_quote_xlm,
            })
        }
        PoolEventKind::Unknown => Value::Null,
    }
}

fn pool_swap_from_event(event: &PoolEvent) -> Option<PoolSwap> {
    if event.kind != PoolEventKind::Trade {
        return None;
    }
    let tx_hash = event.tx_hash.clone()?;
    let body: Value = serde_json::from_str(&event.body_json).ok()?;
    let topic = body.get("topic")?.as_array()?;
    let data = body.get("data")?.as_array()?;
    let derived = body.get("derived")?;

    let fee_quote = derived.get("fee_quote_xlm").and_then(Value::as_f64);
    let volume_quote = derived.get("volume_quote_xlm").and_then(Value::as_f64);
    let fee_bps = derived
        .get("pool_fee_bps")
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok());

    let soroswap = topic
        .first()
        .and_then(|value| value.get("value"))
        .and_then(Value::as_str)
        == Some("SoroswapPair");
    let token_in = derived
        .get("token_in")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .or_else(|| topic.get(1).and_then(value_address_string));
    let token_out = derived
        .get("token_out")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .or_else(|| topic.get(2).and_then(value_address_string));
    let amount_in = derived
        .get("amount_in")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .or_else(|| data.first().and_then(value_amount_string));
    let amount_out = derived
        .get("amount_out")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .or_else(|| data.get(1).and_then(value_amount_string));

    Some(PoolSwap {
        tx_hash,
        event_id: event.event_id.clone(),
        ledger: event.ledger,
        created_at: event.created_at,
        pool_address: event.pool_address.clone(),
        dex: if soroswap { "soroswap_amm" } else { "aquarius" }.to_string(),
        token_in,
        token_out,
        amount_in,
        amount_out,
        fee_bps,
        volume_quote,
        fee_quote,
    })
}

fn actor_from_soroswap_data(data: &[Value]) -> Option<String> {
    data.first()
        .and_then(|value| value.get("to"))
        .and_then(value_address_string)
        .filter(|address| is_stellar_account_address(address))
}

fn soroswap_field<'a>(data: &'a [Value], name: &str) -> Option<&'a Value> {
    data.first()?.get(name)
}

fn soroswap_amount(data: &[Value], name: &str) -> Option<String> {
    soroswap_field(data, name).and_then(value_amount_string)
}

fn positive_amount(value: Option<String>) -> Option<String> {
    value.filter(|amount| {
        amount
            .parse::<i128>()
            .map(|value| value > 0)
            .unwrap_or(false)
    })
}

fn derive_soroswap_swap(
    pool_fee_bps: Option<u32>,
    pool_tokens: &[String],
    data: &[Value],
    context: &PoolIndexContext,
    actor: Option<&str>,
) -> Value {
    let amount0_in = soroswap_amount(data, "amount_0_in");
    let amount1_in = soroswap_amount(data, "amount_1_in");
    let amount0_out = soroswap_amount(data, "amount_0_out");
    let amount1_out = soroswap_amount(data, "amount_1_out");
    let (token_in, amount_in) = if let Some(amount) = positive_amount(amount0_in.clone()) {
        (pool_tokens.first().cloned(), Some(amount))
    } else {
        (
            pool_tokens.get(1).cloned(),
            positive_amount(amount1_in.clone()),
        )
    };
    let (token_out, amount_out) = if let Some(amount) = positive_amount(amount0_out.clone()) {
        (pool_tokens.first().cloned(), Some(amount))
    } else {
        (
            pool_tokens.get(1).cloned(),
            positive_amount(amount1_out.clone()),
        )
    };
    let volume_quote_xlm = token_in
        .as_deref()
        .zip(amount_in.as_deref())
        .and_then(|(token, amount)| estimate_amount_xlm(&context.price_book, token, amount));
    let fee_quote_xlm =
        pool_fee_bps.and_then(|bps| volume_quote_xlm.map(|volume| volume * bps as f64 / 10_000.0));
    derived_with_actor(
        json!({
            "pool_fee_bps": pool_fee_bps,
            "token_in": token_in,
            "token_out": token_out,
            "amount_in": amount_in,
            "amount_out": amount_out,
            "volume_quote_xlm": volume_quote_xlm,
            "fee_quote_xlm": fee_quote_xlm,
        }),
        actor,
    )
}

fn derive_soroswap_liquidity(
    kind: &PoolEventKind,
    pool_fee_bps: Option<u32>,
    pool_tokens: &[String],
    data: &[Value],
    context: &PoolIndexContext,
    actor: Option<&str>,
) -> Value {
    let (share_amount, amount0, amount1) = match kind {
        PoolEventKind::DepositLiquidity => (
            soroswap_amount(data, "liquidity"),
            soroswap_amount(data, "amount_0"),
            soroswap_amount(data, "amount_1"),
        ),
        _ => (
            soroswap_amount(data, "liquidity"),
            soroswap_amount(data, "amount_0"),
            soroswap_amount(data, "amount_1"),
        ),
    };
    let total_quote_xlm = estimate_two_token_amounts_xlm(
        &context.price_book,
        pool_tokens.first().map(String::as_str),
        amount0.as_deref(),
        pool_tokens.get(1).map(String::as_str),
        amount1.as_deref(),
    );
    derived_with_actor(
        json!({
            "pool_fee_bps": pool_fee_bps,
            "share_amount": share_amount,
            "token_amounts": [
                {"token": pool_tokens.first(), "amount": amount0},
                {"token": pool_tokens.get(1), "amount": amount1}
            ],
            "total_quote_xlm": total_quote_xlm,
        }),
        actor,
    )
}

fn derive_soroswap_sync(
    pool_fee_bps: Option<u32>,
    pool_tokens: &[String],
    data: &[Value],
    context: &PoolIndexContext,
) -> Value {
    let reserves = vec![
        soroswap_amount(data, "new_reserve_0"),
        soroswap_amount(data, "new_reserve_1"),
    ];
    let amounts = reserves.iter().flatten().cloned().collect::<Vec<_>>();
    let reserves_quote_xlm = estimate_reserves_xlm(&context.price_book, pool_tokens, &amounts);
    json!({
        "pool_fee_bps": pool_fee_bps,
        "reserves": pool_tokens.iter().zip(amounts.iter()).map(|(token, amount)| json!({"token": token, "amount": amount})).collect::<Vec<_>>(),
        "reserves_quote_xlm": reserves_quote_xlm,
    })
}

fn topic_symbol_name(first_topic: Option<&String>) -> Result<Option<String>> {
    let Some(first_topic) = first_topic else {
        return Ok(None);
    };
    let scval = decode_scval_b64(first_topic)?;
    match scval {
        xdr::ScVal::Symbol(symbol) => Ok(Some(symbol.to_string())),
        _ => Ok(None),
    }
}

fn event_body_json(value: Option<&Value>) -> Result<Vec<Value>> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let xdr_b64 = event_value_xdr(value).ok_or_else(|| anyhow!("unsupported event value"))?;
    let scval = decode_scval_b64(xdr_b64)?;
    match &scval {
        xdr::ScVal::Vec(Some(values)) => values.iter().map(scval_to_json).collect(),
        xdr::ScVal::Void => Ok(Vec::new()),
        _ => Ok(vec![scval_to_json(&scval)?]),
    }
}

fn event_value_xdr(value: &Value) -> Option<&str> {
    value
        .as_str()
        .or_else(|| value.get("xdr").and_then(|inner| inner.as_str()))
}

fn decode_scval_b64(b64: &str) -> Result<xdr::ScVal> {
    let _ = BASE64
        .decode(b64.trim())
        .context("decode event xdr base64")?;
    xdr::ScVal::from_xdr_base64(b64.trim(), Limits::none()).context("decode event ScVal")
}

fn scval_to_json(val: &xdr::ScVal) -> Result<Value> {
    Ok(match val {
        xdr::ScVal::Vec(Some(values)) => Value::Array(
            values
                .iter()
                .into_iter()
                .map(scval_to_json)
                .collect::<Result<Vec<_>>>()?,
        ),
        xdr::ScVal::Address(address) => json!({
            "type": "address",
            "value": sc_address_to_string(&address)?,
        }),
        xdr::ScVal::Symbol(symbol) => json!({
            "type": "symbol",
            "value": symbol.to_string(),
        }),
        xdr::ScVal::String(text) => json!({
            "type": "string",
            "value": text.to_string(),
        }),
        xdr::ScVal::Bool(value) => json!(value),
        xdr::ScVal::U32(value) => json!(value),
        xdr::ScVal::I32(value) => json!(value),
        xdr::ScVal::U64(value) => json!(value.to_string()),
        xdr::ScVal::I64(value) => json!(value.to_string()),
        xdr::ScVal::U128(parts) => json!({
            "type": "u128",
            "value": u128_from_parts(parts.clone()).to_string(),
        }),
        xdr::ScVal::I128(parts) => json!({
            "type": "i128",
            "value": i128_from_parts(parts.clone()).to_string(),
        }),
        xdr::ScVal::Map(Some(entries)) => {
            let mut object = serde_json::Map::new();
            for entry in entries.iter() {
                let Some(key) = scval_map_key(&entry.key) else {
                    continue;
                };
                object.insert(key, scval_to_json(&entry.val)?);
            }
            Value::Object(object)
        }
        xdr::ScVal::Void => Value::Null,
        other => json!({
            "type": "unsupported",
            "value": format!("{other:?}"),
        }),
    })
}

fn scval_map_key(value: &xdr::ScVal) -> Option<String> {
    match value {
        xdr::ScVal::Symbol(symbol) => Some(symbol.to_string()),
        xdr::ScVal::String(text) => Some(text.to_string()),
        _ => None,
    }
}

fn sc_address_to_string(address: &xdr::ScAddress) -> Result<String> {
    match address {
        xdr::ScAddress::Contract(xdr::ContractId(xdr::Hash(hash))) => {
            Ok(Contract(*hash).to_string().to_string())
        }
        xdr::ScAddress::Account(xdr::AccountId(xdr::PublicKey::PublicKeyTypeEd25519(
            xdr::Uint256(key),
        ))) => Ok(PublicKey(*key).to_string().to_string()),
        other => Err(anyhow!("unsupported address variant: {other:?}")),
    }
}

fn u128_from_parts(parts: xdr::UInt128Parts) -> u128 {
    ((parts.hi as u128) << 64) | (parts.lo as u128)
}

fn i128_from_parts(parts: xdr::Int128Parts) -> i128 {
    ((parts.hi as i128) << 64) | (parts.lo as i128)
}

fn ledger_closed_at_to_unix(ledger_closed_at: Option<&str>, fallback_ledger: u32) -> i64 {
    ledger_closed_at
        .and_then(|raw| {
            DateTime::parse_from_rfc3339(raw)
                .ok()
                .map(|dt| dt.timestamp())
        })
        .unwrap_or_else(|| fallback_ledger as i64)
}

fn derived_with_actor(derived: Value, actor: Option<&str>) -> Value {
    let Some(actor) = actor else {
        return derived;
    };
    match derived {
        Value::Object(mut obj) => {
            obj.insert("actor".to_string(), json!(actor));
            Value::Object(obj)
        }
        other => other,
    }
}

fn value_address_string(value: &Value) -> Option<String> {
    let obj = value.as_object()?;
    if obj.get("type").and_then(Value::as_str) != Some("address") {
        return None;
    }
    obj.get("value")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

fn value_amount_string(value: &Value) -> Option<String> {
    if let Some(text) = value.as_str() {
        return Some(text.to_string());
    }
    value
        .as_object()
        .and_then(|obj| obj.get("value"))
        .and_then(|inner| inner.as_str())
        .map(ToOwned::to_owned)
}

fn estimate_amount_xlm(book: &PriceBook, token: &str, amount: &str) -> Option<f64> {
    let price = book.get(token)?;
    let amount = amount.parse::<f64>().ok()?;
    Some(amount * price)
}

fn estimate_two_token_amounts_xlm(
    book: &PriceBook,
    token0: Option<&str>,
    amount0: Option<&str>,
    token1: Option<&str>,
    amount1: Option<&str>,
) -> Option<f64> {
    let mut total = 0.0;
    let mut any = false;
    if let Some(value) = token0
        .zip(amount0)
        .and_then(|(token, amount)| estimate_amount_xlm(book, token, amount))
    {
        total += value;
        any = true;
    }
    if let Some(value) = token1
        .zip(amount1)
        .and_then(|(token, amount)| estimate_amount_xlm(book, token, amount))
    {
        total += value;
        any = true;
    }
    any.then_some(total)
}

fn estimate_reserves_xlm(book: &PriceBook, tokens: &[String], amounts: &[String]) -> Option<f64> {
    if tokens.len() != amounts.len() || tokens.is_empty() {
        return None;
    }
    let mut total = 0.0;
    for (token, amount) in tokens.iter().zip(amounts.iter()) {
        total += estimate_amount_xlm(book, token, amount)?;
    }
    Some(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ledger_closed_at_parses_rfc3339() {
        assert_eq!(
            ledger_closed_at_to_unix(Some("2026-07-26T10:05:00Z"), 123),
            1_785_060_300
        );
    }

    #[test]
    fn ledger_closed_at_falls_back_to_ledger() {
        assert_eq!(ledger_closed_at_to_unix(None, 123), 123);
    }

    #[test]
    fn claim_fees_actor_from_topic1() {
        let topic = vec![
            json!({"type":"symbol","value":"claim_fees"}),
            json!({"type":"address","value":"GCL5ZDPP4YWKBLFAYIQHZSHP63KHWPI6L4O2F7TQ5V27UKQDEWWKHZIU"}),
            json!({"type":"address","value":"CAS3UIKL3IYGMERDRUN6YSCLWVAKIFG5Q4YJHUKM4S4NJZQIA3BAS6OJPK"}),
        ];
        assert_eq!(
            actor_from_topics("claim_fees", &topic).as_deref(),
            Some("GCL5ZDPP4YWKBLFAYIQHZSHP63KHWPI6L4O2F7TQ5V27UKQDEWWKHZIU")
        );
    }

    #[test]
    fn deposit_topics_have_no_g_actor() {
        let topic = vec![
            json!({"type":"symbol","value":"deposit_liquidity"}),
            json!({"type":"address","value":"CAUIKL3IYGMERDRUN6YSCLWVAKIFG5Q4YJHUKM4S4NJZQIA3BAS6OJPK"}),
            json!({"type":"address","value":"CBMFDIRY5OKI4JJURXC4SMEQPWB4UUADIADJK4NA6CYBNOYK4W4TMLLF"}),
        ];
        assert_eq!(actor_from_topics("deposit_liquidity", &topic), None);
    }

    #[test]
    fn trade_actor_from_topic3() {
        let topic = vec![
            json!({"type":"symbol","value":"trade"}),
            json!({"type":"address","value":"CAUIKL3IYGMERDRUN6YSCLWVAKIFG5Q4YJHUKM4S4NJZQIA3BAS6OJPK"}),
            json!({"type":"address","value":"CBMFDIRY5OKI4JJURXC4SMEQPWB4UUADIADJK4NA6CYBNOYK4W4TMLLF"}),
            json!({"type":"address","value":"GBBO4ZDDZTSM2IUKQYBAST3CFHNPFXECGEFTGWTA3FXZASUBSONBDN3XL"}),
        ];
        assert_eq!(
            actor_from_topics("trade", &topic).as_deref(),
            Some("GBBO4ZDDZTSM2IUKQYBAST3CFHNPFXECGEFTGWTA3FXZASUBSONBDN3XL")
        );
    }

    #[test]
    fn derived_with_actor_injects_field() {
        let derived = derived_with_actor(json!({"share_amount": "1000"}), Some("GABC"));
        assert_eq!(derived.get("actor").and_then(Value::as_str), Some("GABC"));
    }

    #[test]
    fn tx_source_lookup_error_does_not_cache() {
        let mut cache = HashMap::new();
        let tx_hash = "abc123";
        assert_eq!(
            cache_tx_source_lookup(&mut cache, tx_hash, Err(anyhow!("RPC error"))),
            None
        );
        assert!(!cache.contains_key(tx_hash));
    }

    #[test]
    fn tx_source_lookup_success_caches_actor() {
        let mut cache = HashMap::new();
        let tx_hash = "abc123";
        let actor = "GCL5ZDPP4YWKBLFAYIQHZSHP63KHWPI6L4O2F7TQ5V27UKQDEWWKHZIU".to_string();
        assert_eq!(
            cache_tx_source_lookup(&mut cache, tx_hash, Ok(Some(actor.clone()))),
            Some(actor.clone())
        );
        assert_eq!(cache.get(tx_hash), Some(&Some(actor)));
    }

    #[test]
    fn tx_source_lookup_not_found_caches_none() {
        let mut cache = HashMap::new();
        let tx_hash = "abc123";
        assert_eq!(cache_tx_source_lookup(&mut cache, tx_hash, Ok(None)), None);
        assert_eq!(cache.get(tx_hash), Some(&None));
    }
}
