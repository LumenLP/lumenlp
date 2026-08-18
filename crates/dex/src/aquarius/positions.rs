//! User LP positions from on-chain state (RPC-first).

use {
    crate::aquarius::{
        pool::{hydrate_pool, share_balance},
        pricing::price_book_from_pools,
    },
    crate::{
        rpc::{account_address_scval, parse_u128_vec, scval_to_i32, scval_to_u128, SorobanRpc},
        types::{ClPositionRange, PoolType, SharePoolState, UserPosition},
    },
    anyhow::Result,
    metrics::{cl_position_amounts, cp_position_amounts, value_xlm, PriceBook},
    stellar_xdr::curr as xdr,
    tracing::warn,
};

/// Scan known pools for non-zero user positions.
///
/// `pricing_pools` should be hydrated catalogue rows (with reserves) used to
/// build an XLM price book — typically from SQLite latest snapshots.
pub async fn positions_for_address(
    rpc: &SorobanRpc,
    user: &str,
    pool_addresses: &[String],
    pricing_pools: &[SharePoolState],
) -> Vec<UserPosition> {
    let book = price_book_from_pools(pricing_pools);
    let mut out = Vec::new();
    for addr in pool_addresses {
        match load_position(rpc, user, addr, &book).await {
            Ok(Some(p)) => out.push(p),
            Ok(None) => {}
            Err(e) => {
                warn!(pool = %addr, error = %e, "position load failed");
                out.push(UserPosition {
                    pool_address: addr.clone(),
                    pool_type: PoolType::Unknown,
                    tokens: vec![],
                    fee_bps: 0,
                    amounts: vec![],
                    value_quote: None,
                    il_est: None,
                    pnl: None,
                    fees_unclaimed_quote: None,
                    status: "error".into(),
                    shares: None,
                    cl_ranges: None,
                    note: Some(e.to_string()),
                });
            }
        }
    }
    out
}

async fn load_position(
    rpc: &SorobanRpc,
    user: &str,
    pool: &str,
    book: &PriceBook,
) -> Result<Option<UserPosition>> {
    let state = hydrate_pool(rpc, pool).await?;
    match state.pool_type {
        PoolType::ConstantProduct | PoolType::Stable => {
            let Some(share_token) = state.share_token.as_deref() else {
                return Ok(None);
            };
            let shares = share_balance(rpc, share_token, user).await?;
            if shares == 0 {
                return Ok(None);
            }
            let (a, b) = if state.reserves.len() >= 2 {
                cp_position_amounts(
                    shares,
                    state.total_shares,
                    state.reserves[0],
                    state.reserves[1],
                )
            } else {
                (0.0, 0.0)
            };
            let amounts = vec![a, b];
            let prices = book.required(&state.tokens);
            let value = prices.as_ref().and_then(|p| value_xlm(&amounts, p));
            let priced = prices.is_some();
            Ok(Some(UserPosition {
                pool_address: state.address,
                pool_type: state.pool_type,
                tokens: state.tokens,
                fee_bps: state.fee_bps,
                amounts,
                value_quote: value,
                il_est: None,
                pnl: None,
                fees_unclaimed_quote: None,
                status: "ok".into(),
                shares: Some(shares),
                cl_ranges: None,
                note: Some(if priced {
                    "value_quote=XLM via native/hop pools; il/pnl=n/a without cost basis".into()
                } else {
                    "missing XLM price path for one or more tokens; il/pnl=n/a".into()
                }),
            }))
        }
        PoolType::Concentrated => load_cl_position(rpc, user, &state, book).await,
        PoolType::Unknown | PoolType::Weighted => Ok(None),
    }
}

async fn load_cl_position(
    rpc: &SorobanRpc,
    user: &str,
    state: &SharePoolState,
    book: &PriceBook,
) -> Result<Option<UserPosition>> {
    let user_val = account_address_scval(user)?;
    let snap = rpc
        .simulate_call(
            state.address.as_str(),
            "get_user_position_snapshot",
            vec![user_val],
        )
        .await?;

    let ranges = parse_cl_ranges(&snap)?;
    if ranges.is_empty() {
        return Ok(None);
    }

    let slot0 = rpc.call_no_args(&state.address, "get_slot0").await.ok();
    let current_tick = slot0
        .as_ref()
        .and_then(|v| parse_slot0_tick(v))
        .unwrap_or(0);

    let mut cl_ranges = Vec::new();
    let mut amount0 = 0.0f64;
    let mut amount1 = 0.0f64;
    for (lo, hi) in ranges {
        let pos = fetch_position_data(rpc, &state.address, user, lo, hi).await;
        let (liq, owed0, owed1) = pos.unwrap_or((0, 0, 0));
        if liq == 0 && owed0 == 0 && owed1 == 0 {
            continue;
        }
        let (a0, a1) = cl_position_amounts(liq, lo, hi, current_tick);
        amount0 += a0 + owed0 as f64;
        amount1 += a1 + owed1 as f64;
        cl_ranges.push(ClPositionRange {
            tick_lower: lo,
            tick_upper: hi,
            liquidity: liq,
            tokens_owed_0: owed0,
            tokens_owed_1: owed1,
            in_range: current_tick >= lo && current_tick < hi,
        });
    }
    if cl_ranges.is_empty() {
        return Ok(None);
    }

    let amounts = vec![amount0, amount1];
    let prices = book.required(&state.tokens);
    let value = prices.as_ref().and_then(|p| value_xlm(&amounts, p));
    let fees_unclaimed = prices.as_ref().map(|p| {
        let f0: f64 = cl_ranges.iter().map(|r| r.tokens_owed_0 as f64).sum();
        let f1: f64 = cl_ranges.iter().map(|r| r.tokens_owed_1 as f64).sum();
        f0 * p[0] + f1 * p[1]
    });

    Ok(Some(UserPosition {
        pool_address: state.address.clone(),
        pool_type: PoolType::Concentrated,
        tokens: state.tokens.clone(),
        fee_bps: state.fee_bps,
        amounts,
        value_quote: value,
        il_est: None,
        pnl: None,
        fees_unclaimed_quote: fees_unclaimed,
        status: "ok".into(),
        shares: None,
        cl_ranges: Some(cl_ranges),
        note: Some(
            "CL amounts from liquidity+ticks; fees in amounts+fees_unclaimed; il/pnl=n/a".into(),
        ),
    }))
}

fn parse_cl_ranges(snap: &xdr::ScVal) -> Result<Vec<(i32, i32)>> {
    let map = match snap {
        xdr::ScVal::Map(Some(m)) => m,
        _ => return Ok(vec![]),
    };
    let ranges_val = map.0.iter().find_map(|e| match &e.key {
        xdr::ScVal::Symbol(s) if s.to_string() == "ranges" => Some(&e.val),
        _ => None,
    });
    let Some(xdr::ScVal::Vec(Some(vec))) = ranges_val else {
        return Ok(vec![]);
    };
    let mut out = Vec::new();
    for item in vec.0.iter() {
        if let xdr::ScVal::Map(Some(rm)) = item {
            let mut lo = None;
            let mut hi = None;
            for e in rm.0.iter() {
                if let xdr::ScVal::Symbol(s) = &e.key {
                    match s.to_string().as_str() {
                        "tick_lower" => lo = scval_to_i32(&e.val).ok(),
                        "tick_upper" => hi = scval_to_i32(&e.val).ok(),
                        _ => {}
                    }
                }
            }
            if let (Some(a), Some(b)) = (lo, hi) {
                out.push((a, b));
            }
        }
    }
    Ok(out)
}

fn parse_slot0_tick(val: &xdr::ScVal) -> Option<i32> {
    let xdr::ScVal::Map(Some(m)) = val else {
        return None;
    };
    for e in m.0.iter() {
        if let xdr::ScVal::Symbol(s) = &e.key {
            if s.to_string() == "tick" {
                return scval_to_i32(&e.val).ok();
            }
        }
    }
    None
}

async fn fetch_position_data(
    rpc: &SorobanRpc,
    pool: &str,
    user: &str,
    tick_lower: i32,
    tick_upper: i32,
) -> Option<(u128, u128, u128)> {
    let user_val = account_address_scval(user).ok()?;
    let lo = xdr::ScVal::I32(tick_lower);
    let hi = xdr::ScVal::I32(tick_upper);
    let val = rpc
        .simulate_call(pool, "get_position", vec![user_val, lo, hi])
        .await
        .ok()?;
    let xdr::ScVal::Map(Some(m)) = val else {
        return None;
    };
    let mut liq = 0u128;
    let mut o0 = 0u128;
    let mut o1 = 0u128;
    for e in m.0.iter() {
        if let xdr::ScVal::Symbol(s) = &e.key {
            match s.to_string().as_str() {
                "liquidity" => liq = scval_to_u128(&e.val).unwrap_or(0),
                "tokens_owed_0" => o0 = scval_to_u128(&e.val).unwrap_or(0),
                "tokens_owed_1" => o1 = scval_to_u128(&e.val).unwrap_or(0),
                _ => {}
            }
        }
    }
    Some((liq, o0, o1))
}

#[allow(dead_code)]
pub async fn try_pending_rewards(rpc: &SorobanRpc, pool: &str, user: &str) -> Option<u128> {
    let user_val = account_address_scval(user).ok()?;
    let val = rpc
        .simulate_call(pool, "get_user_reward", vec![user_val])
        .await
        .ok()?;
    scval_to_u128(&val).ok()
}

#[allow(dead_code)]
pub async fn try_get_all_position_fees(
    rpc: &SorobanRpc,
    pool: &str,
    user: &str,
) -> Option<Vec<u128>> {
    let user_val = account_address_scval(user).ok()?;
    let val = rpc
        .simulate_call(pool, "get_all_position_fees", vec![user_val])
        .await
        .ok()?;
    parse_u128_vec(&val)
}
