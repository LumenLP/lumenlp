//! Soroswap AMM adaptor.
//!
//! The reader is intentionally read-only. This module covers the factory and
//! pair query boundary; liquidity event and Copy LP operation support remain
//! behind the scaffold capability matrix.

use {
    crate::{
        adaptor::{DexAdaptor, LiquidityEventKind, ScaffoldAdaptor, VenueId},
        rpc::{scval_to_address, scval_to_u128, SorobanRpc},
        types::{PoolType, SharePoolState},
    },
    anyhow::{anyhow, Result},
    stellar_xdr::curr as xdr,
};

pub const FACTORY_ALL_PAIRS_LENGTH_METHOD: &str = "all_pairs_length";
pub const FACTORY_ALL_PAIRS_METHOD: &str = "all_pairs";
pub const PAIR_TOKEN_0_METHOD: &str = "token_0";
pub const PAIR_TOKEN_1_METHOD: &str = "token_1";
pub const PAIR_GET_RESERVES_METHOD: &str = "get_reserves";
pub const DEPOSIT_EVENT: &str = "deposit";
pub const WITHDRAW_EVENT: &str = "withdraw";
pub const SWAP_EVENT: &str = "swap";
pub const SYNC_EVENT: &str = "sync";
pub const SOROSWAP_MAINNET_FACTORY: &str = "CA4HEQTL2WPEUYKYKCDOHCDNIV4QHNJ7EL4J4NQ6VADP7SYHVRYZ7AW2";

pub fn scaffold() -> ScaffoldAdaptor {
    ScaffoldAdaptor {
        venue_id: VenueId::SoroswapAmm,
        name: "Soroswap AMM",
        notes: "Scaffold. AMM only (not aggregator).",
    }
}

pub fn adaptor() -> impl DexAdaptor {
    scaffold()
}

pub async fn discover_pool_addresses(rpc: &SorobanRpc, factory_address: &str) -> Result<Vec<String>> {
    let length = parse_u32(
        &rpc.call_no_args(factory_address, FACTORY_ALL_PAIRS_LENGTH_METHOD)
            .await?,
    )?;
    let mut pools = Vec::with_capacity(length as usize);
    for index in 0..length {
        let value = rpc
            .simulate_call(factory_address, FACTORY_ALL_PAIRS_METHOD, vec![xdr::ScVal::U32(index)])
            .await?;
        pools.push(scval_to_address(&value)?);
    }
    pools.sort_unstable();
    pools.dedup();
    Ok(pools)
}

pub async fn discover_mainnet_pool_addresses(rpc: &SorobanRpc) -> Result<Vec<String>> {
    discover_pool_addresses(rpc, SOROSWAP_MAINNET_FACTORY).await
}

/// Classify the final SoroswapPair event topic as a user LP lifecycle event.
/// Swap and reserve-maintenance events are intentionally not Copy LP actions.
pub fn classify_liquidity_event(topic: &str) -> Option<LiquidityEventKind> {
    match topic {
        DEPOSIT_EVENT => Some(LiquidityEventKind::Deposit),
        WITHDRAW_EVENT => Some(LiquidityEventKind::Withdraw),
        _ => None,
    }
}

pub async fn hydrate_pool(rpc: &SorobanRpc, pool_address: &str) -> Result<SharePoolState> {
    let (token_a, token_b, reserves) = tokio::try_join!(
        rpc.call_no_args(pool_address, PAIR_TOKEN_0_METHOD),
        rpc.call_no_args(pool_address, PAIR_TOKEN_1_METHOD),
        rpc.call_no_args(pool_address, PAIR_GET_RESERVES_METHOD),
    )?;
    // Older Soroswap pairs may not expose the token interface. Keep the pool
    // visible for analytics; share-based position reads simply stay disabled.
    let total_shares = rpc
        .call_no_args(pool_address, "total_supply")
        .await
        .ok()
        .and_then(|value| scval_to_u128(&value).ok())
        .unwrap_or(0);
    let reserves = parse_reserves(&reserves)?;
    Ok(SharePoolState {
        address: pool_address.to_owned(),
        pool_type: PoolType::ConstantProduct,
        tokens: vec![scval_to_address(&token_a)?, scval_to_address(&token_b)?],
        reserves: vec![reserves.0, reserves.1],
        fee_bps: 30,
        total_shares,
        // Soroswap Pair is also the LP token contract.
        share_token: Some(pool_address.to_owned()),
        amp: None,
    })
}

fn parse_u32(value: &xdr::ScVal) -> Result<u32> {
    match value {
        xdr::ScVal::U32(value) => Ok(*value),
        xdr::ScVal::I32(value) if *value >= 0 => Ok(*value as u32),
        _ => u32::try_from(scval_to_u128(value)?).map_err(|_| anyhow!("Soroswap value exceeds u32")),
    }
}

fn parse_reserves(value: &xdr::ScVal) -> Result<(u128, u128)> {
    let xdr::ScVal::Vec(Some(values)) = value else {
        return Err(anyhow!("Soroswap get_reserves returned no vector"));
    };
    if values.0.len() < 2 {
        return Err(anyhow!("Soroswap get_reserves returned fewer than two values"));
    }
    let reserve = |value: &xdr::ScVal| scval_to_u128(value);
    Ok((reserve(&values.0[0])?, reserve(&values.0[1])?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn soroswap_reserves_require_two_nonnegative_values() {
        let values = xdr::ScVal::Vec(Some(
            vec![
                xdr::ScVal::I128(xdr::Int128Parts { hi: 0, lo: 10 }),
                xdr::ScVal::I128(xdr::Int128Parts { hi: 0, lo: 20 }),
            ]
            .try_into()
            .unwrap(),
        ));
        assert_eq!(parse_reserves(&values).unwrap(), (10, 20));
        assert!(parse_reserves(&xdr::ScVal::Void).is_err());
    }

    #[test]
    fn soroswap_classifies_only_lp_lifecycle_events() {
        assert_eq!(
            classify_liquidity_event(DEPOSIT_EVENT),
            Some(LiquidityEventKind::Deposit)
        );
        assert_eq!(
            classify_liquidity_event(WITHDRAW_EVENT),
            Some(LiquidityEventKind::Withdraw)
        );
        assert_eq!(classify_liquidity_event(SWAP_EVENT), None);
        assert_eq!(classify_liquidity_event(SYNC_EVENT), None);
        assert_eq!(classify_liquidity_event("admin"), None);
    }
}
