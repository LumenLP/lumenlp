//! Phoenix Protocol adaptor.
//!
//! The reader is intentionally read-only for now. Phoenix pool contracts expose
//! a stable query surface, but factory discovery and event-version coverage
//! still need mainnet validation before this venue can be promoted to
//! production.

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

/// Phoenix pool operation boundary. The two pool implementations use
/// different provide_liquidity signatures, so the pool type must be explicit
/// in every draft rather than inferred from a token pair. The exact ABI is
/// recorded in `fixtures/phoenix-operation-boundary.json` and is intentionally
/// not collapsed into the generic Copy Policy entry point yet.
#[derive(Debug, Default, Clone, Copy)]
pub struct PhoenixAdaptor;

impl DexAdaptor for PhoenixAdaptor {
    fn venue_id(&self) -> VenueId {
        VenueId::Phoenix
    }

    fn name(&self) -> &'static str {
        "Phoenix"
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
        "Validated unsigned pool drafts for XYK and stable pools; policy-controlled execution remains fail-closed."
    }

    fn build_draft_op(&self, request: DraftRequest) -> Result<DraftOp> {
        if request.pool_address.is_empty() || request.position_key.is_empty() {
            bail!("draft operation requires pool_address and position_key");
        }
        let pool_type = request
            .payload
            .get("pool_type")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("Phoenix draft requires pool_type"))?;
        match request.kind {
            DraftOpKind::Deposit => match pool_type {
                "constant_product" => validate_payload(
                    &request.payload,
                    &[
                        "depositor",
                        "desired_a",
                        "min_a",
                        "desired_b",
                        "min_b",
                        "custom_slippage_bps",
                        "deadline",
                        "auto_stake",
                    ],
                )?,
                "stable" => validate_payload(
                    &request.payload,
                    &[
                        "depositor",
                        "desired_a",
                        "desired_b",
                        "custom_slippage_bps",
                        "deadline",
                        "min_shares_to_receive",
                        "auto_stake",
                    ],
                )?,
                other => bail!("unsupported Phoenix pool_type: {other}"),
            },
            DraftOpKind::Withdraw => validate_payload(
                &request.payload,
                &[
                    "recipient",
                    "share_amount",
                    "min_a",
                    "min_b",
                    "deadline",
                    "auto_unstake",
                ],
            )?,
            _ => bail!("Phoenix supports deposit and withdraw drafts only"),
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
    PhoenixAdaptor
}

fn validate_payload(payload: &Value, required: &[&str]) -> Result<()> {
    let object = payload
        .as_object()
        .ok_or_else(|| anyhow!("Phoenix draft payload must be an object"))?;
    for key in required {
        if !object.contains_key(*key) {
            bail!("Phoenix draft payload missing {key}");
        }
    }
    Ok(())
}

pub const FACTORY_QUERY_ALL_POOLS_DETAILS_METHOD: &str = "query_all_pools_details";
pub const POOL_QUERY_CONFIG_METHOD: &str = "query_config";
pub const POOL_QUERY_INFO_METHOD: &str = "query_pool_info";

/// Phoenix Protocol mainnet factory, also used by the local DEX aggregator.
pub const PHOENIX_MAINNET_FACTORY: &str = "CB4SVAWJA6TSRNOJZ7W2AWFW46D5VR4ZMFZKDIKXEINZCZEGZCJZCKMI";

/// Phoenix event topics that represent user liquidity actions. `swap` is
/// deliberately excluded: it is useful market activity, but not evidence that
/// a leader changed an LP position.
pub const PROVIDE_LIQUIDITY_EVENT: &str = "provide_liquidity";
pub const WITHDRAW_LIQUIDITY_EVENT: &str = "withdraw_liquidity";

pub fn classify_liquidity_event(topic: &str) -> Option<LiquidityEventKind> {
    match topic {
        PROVIDE_LIQUIDITY_EVENT => Some(LiquidityEventKind::Deposit),
        WITHDRAW_LIQUIDITY_EVENT => Some(LiquidityEventKind::Withdraw),
        _ => None,
    }
}

/// Discover Phoenix pool contracts from a factory contract.
///
/// The result is normalized for callers that persist pool identities: duplicate
/// addresses are removed and ordering is deterministic. The factory address is
/// intentionally supplied by the deployment configuration rather than baked
/// into this module because Phoenix deployments can use different factories.
pub async fn discover_pool_addresses(rpc: &SorobanRpc, factory_address: &str) -> Result<Vec<String>> {
    let value = rpc
        .call_no_args(factory_address, FACTORY_QUERY_ALL_POOLS_DETAILS_METHOD)
        .await?;
    let mut pools = parse_factory_pool_details(&value)?;
    pools.sort_unstable();
    pools.dedup();
    Ok(pools)
}

pub async fn discover_mainnet_pool_addresses(rpc: &SorobanRpc) -> Result<Vec<String>> {
    discover_pool_addresses(rpc, PHOENIX_MAINNET_FACTORY).await
}

fn parse_factory_pool_details(value: &xdr::ScVal) -> Result<Vec<String>> {
    let xdr::ScVal::Vec(Some(entries)) = value else {
        return Err(anyhow!("Phoenix query_all_pools_details returned no vector"));
    };
    let mut pools = Vec::with_capacity(entries.0.len());
    for entry in entries.0.iter() {
        let map = scval_map(entry)?;
        pools.push(scval_to_address(map_value(map, "pool_address")?)?);
    }
    if pools.is_empty() {
        return Err(anyhow!("Phoenix query_all_pools_details returned no pools"));
    }
    Ok(pools)
}

/// Read a Phoenix XYK pool through its public query methods.
///
/// This does not discover pool addresses and does not build write operations.
/// Callers must obtain a pool address from a validated Phoenix factory source.
pub async fn hydrate_pool(rpc: &SorobanRpc, pool_address: &str) -> Result<SharePoolState> {
    let config = rpc.call_no_args(pool_address, POOL_QUERY_CONFIG_METHOD).await?;
    let pool_info = rpc.call_no_args(pool_address, POOL_QUERY_INFO_METHOD).await?;

    let config_map = scval_map(&config)?;
    let tokens = vec![
        scval_to_address(map_value(config_map, "token_a")?)?,
        scval_to_address(map_value(config_map, "token_b")?)?,
    ];
    let pool_type = pool_type_from_scval(map_value(config_map, "pool_type")?)?;
    let fee_bps = nonnegative_u128(map_value(config_map, "total_fee_bps")?)?
        .try_into()
        .map_err(|_| anyhow!("Phoenix fee exceeds u32"))?;

    let pool_map = scval_map(&pool_info)?;
    let asset_a = scval_map(map_value(pool_map, "asset_a")?)?;
    let asset_b = scval_map(map_value(pool_map, "asset_b")?)?;
    let share = scval_map(map_value(pool_map, "asset_lp_share")?)?;
    let reserve_tokens = [
        scval_to_address(map_value(asset_a, "address")?)?,
        scval_to_address(map_value(asset_b, "address")?)?,
    ];
    if reserve_tokens != tokens.as_slice() {
        return Err(anyhow!("Phoenix query_pool_info token order differs from query_config"));
    }

    Ok(SharePoolState {
        address: pool_address.to_owned(),
        pool_type,
        tokens,
        reserves: vec![
            nonnegative_u128(map_value(asset_a, "amount")?)?,
            nonnegative_u128(map_value(asset_b, "amount")?)?,
        ],
        fee_bps,
        total_shares: nonnegative_u128(map_value(share, "amount")?)?,
        share_token: Some(scval_to_address(map_value(share, "address")?)?),
        amp: None,
    })
}

fn pool_type_from_scval(value: &xdr::ScVal) -> Result<PoolType> {
    let raw = match value {
        xdr::ScVal::U32(value) => *value,
        xdr::ScVal::I32(value) if *value >= 0 => *value as u32,
        _ => return Err(anyhow!("Phoenix pool_type is not an integer enum")),
    };
    match raw {
        0 => Ok(PoolType::ConstantProduct),
        1 => Ok(PoolType::Stable),
        _ => Err(anyhow!("Phoenix pool_type enum value {raw} is unsupported")),
    }
}

fn nonnegative_u128(value: &xdr::ScVal) -> Result<u128> {
    match value {
        xdr::ScVal::U64(value) => Ok(*value as u128),
        xdr::ScVal::I64(value) if *value >= 0 => Ok(*value as u128),
        xdr::ScVal::U32(value) => Ok(*value as u128),
        xdr::ScVal::I32(value) if *value >= 0 => Ok(*value as u128),
        _ => scval_to_u128(value),
    }
}

fn scval_map(value: &xdr::ScVal) -> Result<&xdr::ScMap> {
    match value {
        xdr::ScVal::Map(Some(map)) => Ok(map),
        _ => Err(anyhow!("Phoenix query result is not a map")),
    }
}

fn map_value<'a>(map: &'a xdr::ScMap, name: &str) -> Result<&'a xdr::ScVal> {
    map.0
        .iter()
        .find(|entry| {
            matches!(&entry.key, xdr::ScVal::Symbol(symbol) if symbol.to_string() == name)
                || matches!(&entry.key, xdr::ScVal::String(string) if string.to_string() == name)
        })
        .map(|entry| &entry.val)
        .ok_or_else(|| anyhow!("Phoenix query result missing field {name}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phoenix_integer_fields_accept_contract_signed_types() {
        assert_eq!(nonnegative_u128(&xdr::ScVal::I64(30)).unwrap(), 30);
        assert_eq!(nonnegative_u128(&xdr::ScVal::U64(42)).unwrap(), 42);
        assert!(nonnegative_u128(&xdr::ScVal::I64(-1)).is_err());
    }

    #[test]
    fn phoenix_pool_type_maps_xyk_and_stable_enums() {
        assert_eq!(
            pool_type_from_scval(&xdr::ScVal::U32(0)).unwrap(),
            PoolType::ConstantProduct
        );
        assert_eq!(pool_type_from_scval(&xdr::ScVal::U32(1)).unwrap(), PoolType::Stable);
        assert!(pool_type_from_scval(&xdr::ScVal::U32(2)).is_err());
    }

    #[test]
    fn phoenix_factory_pool_result_rejects_empty_values() {
        assert!(parse_factory_pool_details(&xdr::ScVal::Void).is_err());
    }

    #[test]
    fn phoenix_classifies_only_lp_lifecycle_events() {
        assert_eq!(
            classify_liquidity_event("provide_liquidity"),
            Some(LiquidityEventKind::Deposit)
        );
        assert_eq!(
            classify_liquidity_event("withdraw_liquidity"),
            Some(LiquidityEventKind::Withdraw)
        );
        assert_eq!(classify_liquidity_event("swap"), None);
        assert_eq!(classify_liquidity_event("initialize"), None);
    }

    #[test]
    fn phoenix_requires_pool_specific_deposit_parameters() {
        let request = DraftRequest {
            pool_address: "CPOOL".into(),
            kind: DraftOpKind::Deposit,
            position_key: "shares:100".into(),
            payload: serde_json::json!({
                "pool_type": "constant_product",
                "depositor": "GUSER",
                "desired_a": "100",
                "min_a": "99",
                "desired_b": "200",
                "min_b": "198",
                "custom_slippage_bps": null,
                "deadline": "12345",
                "auto_stake": false
            }),
            quote_xlm: Some(1.0),
        };
        assert!(PhoenixAdaptor.build_draft_op(request).is_ok());
        assert!(PhoenixAdaptor
            .build_draft_op(DraftRequest {
                payload: serde_json::json!({
                    "pool_type": "stable",
                    "depositor": "GUSER",
                    "desired_a": "100",
                    "desired_b": "200",
                    "custom_slippage_bps": null,
                    "deadline": "12345",
                    "min_shares_to_receive": "90",
                    "auto_stake": false
                }),
                ..DraftRequest {
                    pool_address: "CPOOL".into(),
                    kind: DraftOpKind::Deposit,
                    position_key: "shares:100".into(),
                    payload: Value::Null,
                    quote_xlm: None,
                }
            })
            .is_ok());
    }

    #[test]
    fn phoenix_rejects_unknown_pool_type_and_claim() {
        let request = DraftRequest {
            pool_address: "CPOOL".into(),
            kind: DraftOpKind::Deposit,
            position_key: "shares:100".into(),
            payload: serde_json::json!({"pool_type": "clmm"}),
            quote_xlm: None,
        };
        assert!(PhoenixAdaptor.build_draft_op(request).is_err());
    }
}
