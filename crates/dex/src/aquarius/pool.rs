//! Hydrate Aquarius share pools (CP / stable) and basic CL metadata.

use {
    crate::{
        rpc::{
            account_address_scval, parse_address_vec, parse_fee_bps_u32, parse_u128_vec,
            scval_to_address, scval_to_symbol_string, scval_to_u128, SorobanRpc,
        },
        types::{PoolType, SharePoolState},
    },
    anyhow::{anyhow, Result},
    tracing::debug,
};

pub async fn read_pool_type(rpc: &SorobanRpc, pool: &str) -> Result<PoolType> {
    let val = rpc.call_no_args(pool, "pool_type").await?;
    let s = scval_to_symbol_string(&val).unwrap_or_default();
    Ok(PoolType::parse(&s))
}

pub async fn hydrate_pool(rpc: &SorobanRpc, pool: &str) -> Result<SharePoolState> {
    let pool_type = read_pool_type(rpc, pool).await?;
    let tokens_val = rpc.call_no_args(pool, "get_tokens").await?;
    let tokens = parse_address_vec(&tokens_val).ok_or_else(|| anyhow!("get_tokens empty"))?;

    let fee_bps = match rpc.call_no_args(pool, "get_fee_fraction").await {
        Ok(v) => parse_fee_bps_u32(&v).unwrap_or(30),
        Err(_) => 30,
    };

    if pool_type == PoolType::Concentrated {
        // Reserves still available on CL pools for TVL approximation.
        let reserves = match rpc.call_no_args(pool, "get_reserves").await {
            Ok(v) => parse_u128_vec(&v).unwrap_or_default(),
            Err(_) => vec![],
        };
        return Ok(SharePoolState {
            address: pool.to_string(),
            pool_type,
            tokens,
            reserves,
            fee_bps,
            total_shares: 0,
            share_token: None,
            amp: None,
        });
    }

    let reserves_val = rpc.call_no_args(pool, "get_reserves").await?;
    let reserves = parse_u128_vec(&reserves_val).ok_or_else(|| anyhow!("get_reserves empty"))?;

    let total_shares = match rpc.call_no_args(pool, "get_total_shares").await {
        Ok(v) => scval_to_u128(&v).unwrap_or(0),
        Err(_) => 0,
    };

    let share_token = match rpc.call_no_args(pool, "share_id").await {
        Ok(v) => scval_to_address(&v).ok(),
        Err(_) => None,
    };

    let amp = if pool_type == PoolType::Stable {
        match rpc.call_no_args(pool, "a").await {
            Ok(v) => scval_to_u128(&v).ok(),
            Err(_) => Some(100),
        }
    } else {
        None
    };

    Ok(SharePoolState {
        address: pool.to_string(),
        pool_type,
        tokens,
        reserves,
        fee_bps,
        total_shares,
        share_token,
        amp,
    })
}

/// Read LP share balance via share token `balance(Address)`.
pub async fn share_balance(rpc: &SorobanRpc, share_token: &str, user: &str) -> Result<u128> {
    let user_val = account_address_scval(user)?;
    let val = rpc
        .simulate_call(share_token, "balance", vec![user_val])
        .await?;
    // SAC balance is i128
    match val {
        stellar_xdr::curr::ScVal::I128(parts) => {
            let v = ((parts.hi as i128) << 64) | (parts.lo as u64 as i128);
            Ok(u128::try_from(v).unwrap_or(0))
        }
        _ => scval_to_u128(&val).or_else(|_| {
            debug!(share_token, "unexpected balance ScVal");
            Ok(0)
        }),
    }
}

/// Heuristic depth for ranking pools (sum of reserves in base units).
pub fn reserve_depth(state: &SharePoolState) -> u128 {
    state.reserves.iter().copied().sum()
}
