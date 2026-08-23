//! Soroswap AMM adaptor.
//!
//! The reader is intentionally read-only. This module covers the factory and
//! pair query boundary; liquidity event and Copy LP operation support remain
//! behind the scaffold capability matrix.

use {
    crate::{
        adaptor::{
            DexAdaptor, DraftOp, DraftOpKind, DraftRequest, LiquidityEventKind, VenueCapabilities, VenueId, VenueStatus,
        },
        rpc::{scval_to_address, scval_to_u128, SorobanRpc},
        types::{PoolType, SharePoolState},
    },
    anyhow::{anyhow, bail, Result},
    serde_json::Value,
    stellar_xdr::curr as xdr,
};

pub const FACTORY_ALL_PAIRS_LENGTH_METHOD: &str = "all_pairs_length";
pub const FACTORY_ALL_PAIRS_METHOD: &str = "all_pairs";
pub const PAIR_TOKEN_0_METHOD: &str = "token_0";
pub const PAIR_TOKEN_1_METHOD: &str = "token_1";
pub const PAIR_GET_RESERVES_METHOD: &str = "get_reserves";
pub const ROUTER_ADD_LIQUIDITY_METHOD: &str = "add_liquidity";
pub const ROUTER_REMOVE_LIQUIDITY_METHOD: &str = "remove_liquidity";
pub const DEPOSIT_EVENT: &str = "deposit";
pub const WITHDRAW_EVENT: &str = "withdraw";
pub const SWAP_EVENT: &str = "swap";
pub const SYNC_EVENT: &str = "sync";
pub const SOROSWAP_MAINNET_FACTORY: &str = "CA4HEQTL2WPEUYKYKCDOHCDNIV4QHNJ7EL4J4NQ6VADP7SYHVRYZ7AW2";

/// Soroswap's Router operation boundary. This only creates a validated,
/// unsigned operation draft; the policy contract and relayer do not execute
/// it until the venue-specific authorization path is implemented.
#[derive(Debug, Default, Clone, Copy)]
pub struct SoroswapAdaptor;

impl DexAdaptor for SoroswapAdaptor {
    fn venue_id(&self) -> VenueId {
        VenueId::SoroswapAmm
    }

    fn name(&self) -> &'static str {
        "Soroswap AMM"
    }

    fn status(&self) -> VenueStatus {
        VenueStatus::Scaffold
    }

    fn capabilities(&self) -> VenueCapabilities {
        VenueCapabilities {
            list_pools: true,
            positions: true,
            unclaimed_fees: false,
            liquidity_events: true,
            quotes: true,
            draft_ops: true,
            deposit: true,
            withdraw: true,
            claim: false,
            copy_scale: false,
        }
    }

    fn notes(&self) -> &'static str {
        "Validated unsigned Router drafts; policy-controlled execution remains fail-closed."
    }

    fn build_draft_op(&self, request: DraftRequest) -> Result<DraftOp> {
        if request.pool_address.is_empty() || request.position_key.is_empty() {
            bail!("draft operation requires pool_address and position_key");
        }
        match request.kind {
            DraftOpKind::Deposit => validate_router_payload(
                &request.payload,
                &[
                    "token_a",
                    "token_b",
                    "amount_a_desired",
                    "amount_b_desired",
                    "amount_a_min",
                    "amount_b_min",
                    "to",
                    "deadline",
                ],
            )?,
            DraftOpKind::Withdraw => validate_router_payload(
                &request.payload,
                &[
                    "token_a",
                    "token_b",
                    "liquidity",
                    "amount_a_min",
                    "amount_b_min",
                    "to",
                    "deadline",
                ],
            )?,
            _ => bail!("Soroswap AMM supports deposit and withdraw drafts only"),
        }
        Ok(DraftOp {
            venue_id: self.venue_id(),
            pool_address: request.pool_address,
            kind: request.kind,
            position_key: request.position_key,
            amounts: request.payload,
            quote_xlm: request.quote_xlm,
        })
    }
}

pub fn adaptor() -> impl DexAdaptor {
    SoroswapAdaptor
}

fn validate_router_payload(payload: &Value, required: &[&str]) -> Result<()> {
    let object = payload
        .as_object()
        .ok_or_else(|| anyhow!("Soroswap Router payload must be an object"))?;
    for key in required {
        let value = object
            .get(*key)
            .ok_or_else(|| anyhow!("Soroswap Router payload missing {key}"))?;
        if value.as_str().is_none_or(str::is_empty) {
            bail!("Soroswap Router payload field {key} must be a non-empty string");
        }
    }
    Ok(())
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

    #[test]
    fn soroswap_builds_deposit_draft_only_with_router_safety_fields() {
        let request = DraftRequest {
            pool_address: "CPOOL".into(),
            kind: DraftOpKind::Deposit,
            position_key: "shares:100".into(),
            payload: serde_json::json!({
                "token_a": "CTOKENA",
                "token_b": "CTOKENB",
                "amount_a_desired": "100",
                "amount_b_desired": "200",
                "amount_a_min": "99",
                "amount_b_min": "198",
                "to": "GUSER",
                "deadline": "12345"
            }),
            quote_xlm: Some(1.0),
        };
        let draft = SoroswapAdaptor.build_draft_op(request).unwrap();
        assert_eq!(draft.venue_id, VenueId::SoroswapAmm);
        assert!(SoroswapAdaptor
            .build_draft_op(DraftRequest {
                pool_address: "CPOOL".into(),
                kind: DraftOpKind::Deposit,
                position_key: "shares:100".into(),
                payload: serde_json::json!({"token_a": "CTOKENA"}),
                quote_xlm: None,
            })
            .is_err());
    }

    #[test]
    fn soroswap_rejects_claim_and_malformed_router_payloads() {
        let base = DraftRequest {
            pool_address: "CPOOL".into(),
            kind: DraftOpKind::Claim,
            position_key: "shares:100".into(),
            payload: serde_json::json!({}),
            quote_xlm: None,
        };
        assert!(SoroswapAdaptor.build_draft_op(base).is_err());
    }
}
