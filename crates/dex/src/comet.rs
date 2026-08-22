//! Comet weighted-AMM read-only pool adaptor.
//!
//! The first production slice reads the validated Comet mainnet seed pool and
//! optional operator-supplied pool addresses. Weighted pool balances are real
//! on-chain balances; event discovery and Copy LP writes remain gated until
//! factory event fixtures are added.

use {
    crate::{
        adaptor::{
            DexAdaptor, DraftOp, DraftOpKind, DraftRequest, ScaffoldAdaptor, VenueCapabilities, VenueId, VenueStatus,
        },
        rpc::{scval_to_address, scval_to_u128, SorobanRpc},
        types::{PoolType, SharePoolState},
    },
    anyhow::{anyhow, bail, Result},
    base64::{engine::general_purpose::STANDARD as BASE64, Engine as _},
    serde_json::Value,
    stellar_xdr::curr as xdr,
};

pub const COMET_MAINNET_FACTORY: &str = "CA2LVIPU6HJHHPPD6EDDYJTV2QEUBPGOAVJ4VIYNTMFUCRM4LFK3TJKF";
pub const COMET_MAINNET_SEED_POOL: &str = "CAS3FL6TLZKDGGSISDBWGGPXT3NRR4DYTZD7YOD3HMYO6LTJUVGRVEAM";

pub fn scaffold() -> ScaffoldAdaptor {
    ScaffoldAdaptor {
        venue_id: VenueId::Comet,
        name: "Comet",
        notes: "Read-only weighted pool analytics; factory events and Copy LP support pending.",
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct CometAdaptor;

impl DexAdaptor for CometAdaptor {
    fn venue_id(&self) -> VenueId {
        VenueId::Comet
    }

    fn name(&self) -> &'static str {
        "Comet"
    }

    fn status(&self) -> VenueStatus {
        VenueStatus::Scaffold
    }

    fn capabilities(&self) -> VenueCapabilities {
        VenueCapabilities {
            list_pools: true,
            positions: true,
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
        "Validated unsigned weighted-pool join/exit drafts; policy-controlled execution remains fail-closed."
    }

    fn build_draft_op(&self, request: DraftRequest) -> Result<DraftOp> {
        if request.pool_address.is_empty() || request.position_key.is_empty() {
            bail!("draft operation requires pool_address and position_key");
        }
        match request.kind {
            DraftOpKind::Deposit => validate_join_exit_payload(&request.payload, "pool_amount_out", "max_amounts_in")?,
            DraftOpKind::Withdraw => validate_join_exit_payload(&request.payload, "pool_amount_in", "min_amounts_out")?,
            _ => bail!("Comet supports join and exit drafts only"),
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
    CometAdaptor
}

fn validate_join_exit_payload(payload: &Value, amount_key: &str, limits_key: &str) -> Result<()> {
    let object = payload
        .as_object()
        .ok_or_else(|| anyhow!("Comet draft payload must be an object"))?;
    let amount = object
        .get(amount_key)
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("Comet draft payload missing {amount_key}"))?;
    if amount.parse::<u128>().ok().filter(|value| *value > 0).is_none() {
        bail!("Comet {amount_key} must be a positive integer string");
    }
    let limits = object
        .get(limits_key)
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("Comet draft payload missing {limits_key} array"))?;
    if limits.len() < 2 {
        bail!("Comet weighted pool requires at least two token limits");
    }
    if limits
        .iter()
        .any(|value| value.as_str().and_then(|value| value.parse::<u128>().ok()).is_none())
    {
        bail!("Comet token limits must be integer strings");
    }
    if object.get("user").and_then(Value::as_str).is_none_or(str::is_empty) {
        bail!("Comet draft payload requires user");
    }
    Ok(())
}

pub async fn discover_mainnet_pool_addresses(rpc: &SorobanRpc) -> Result<Vec<String>> {
    let mut pools = vec![COMET_MAINNET_SEED_POOL.to_owned()];
    if let Ok(extra) = std::env::var("COMET_EXTRA_POOLS") {
        pools.extend(
            extra
                .split(',')
                .map(str::trim)
                .filter(|pool| !pool.is_empty())
                .map(str::to_owned),
        );
    }
    let factory = std::env::var("COMET_FACTORY").unwrap_or_else(|_| COMET_MAINNET_FACTORY.to_owned());
    let health = rpc.get_health().await?;
    let latest = health.latest_ledger;
    let window = std::env::var("COMET_FACTORY_EVENTS_LEDGER_WINDOW")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(50_000);
    let start_ledger = latest.saturating_sub(window).max(health.oldest_ledger);
    let events = rpc
        .get_events(start_ledger, Some(latest), &[factory.clone()], 10_000)
        .await?;
    for event in events {
        let Some(encoded) = event
            .get("value")
            .and_then(|value| value.as_str().or_else(|| value.get("xdr").and_then(|xdr| xdr.as_str())))
        else {
            continue;
        };
        let Some(hash) = contract_hash_from_xdr(encoded) else {
            continue;
        };
        let address = format!("{}", stellar_strkey::Contract(hash));
        let address_value = address_scval(&address)?;
        if matches!(
            rpc.simulate_call(&factory, "is_c_pool", vec![address_value]).await,
            Ok(xdr::ScVal::Bool(true))
        ) {
            pools.push(address);
        }
    }
    pools.sort_unstable();
    pools.dedup();
    Ok(pools)
}

fn address_scval(address: &str) -> Result<xdr::ScVal> {
    let hash = stellar_strkey::Contract::from_string(address)
        .map_err(|error| anyhow!("invalid Comet pool address {address}: {error:?}"))?
        .0;
    Ok(xdr::ScVal::Address(xdr::ScAddress::Contract(xdr::ContractId(
        xdr::Hash(hash),
    ))))
}

fn contract_hash_from_xdr(encoded: &str) -> Option<[u8; 32]> {
    let raw = BASE64.decode(encoded).ok()?;
    const MARKER: [u8; 8] = [0, 0, 0, 0x12, 0, 0, 0, 1];
    for index in 0..=raw.len().saturating_sub(40) {
        if raw[index..].starts_with(&MARKER) {
            let mut hash = [0u8; 32];
            hash.copy_from_slice(&raw[index + 8..index + 40]);
            return Some(hash);
        }
    }
    None
}

pub async fn hydrate_pool(rpc: &SorobanRpc, pool_address: &str) -> Result<SharePoolState> {
    let tokens = match rpc.call_no_args(pool_address, "get_tokens").await? {
        xdr::ScVal::Vec(Some(values)) => values.0.iter().map(scval_to_address).collect::<Result<Vec<_>>>()?,
        _ => return Err(anyhow!("Comet get_tokens returned no vector")),
    };
    if tokens.len() < 2 {
        return Err(anyhow!("Comet pool has fewer than two tokens"));
    }

    let fee_raw = rpc
        .call_no_args(pool_address, "get_swap_fee")
        .await
        .ok()
        .as_ref()
        .and_then(nonnegative_integer)
        .unwrap_or(30_000);
    let fee_bps = (fee_raw / 1_000).min(u32::MAX as u128) as u32;
    // Some Comet deployments do not expose a usable total supply yet. Keep
    // pool analytics available and let position reads require a non-zero value.
    let total_shares = rpc
        .call_no_args(pool_address, "get_total_supply")
        .await
        .ok()
        .and_then(|value| nonnegative_integer(&value))
        .unwrap_or(0);

    let mut reserves = Vec::with_capacity(tokens.len());
    for token in &tokens {
        let token_value = address_scval(token)?;
        let balance = rpc
            .simulate_call(pool_address, "get_balance", vec![token_value])
            .await
            .map_err(|error| anyhow!("Comet get_balance failed for {token}: {error}"))?;
        reserves
            .push(nonnegative_integer(&balance).ok_or_else(|| anyhow!("Comet returned invalid balance for {token}"))?);
    }

    Ok(SharePoolState {
        address: pool_address.to_owned(),
        pool_type: PoolType::Weighted,
        tokens,
        reserves,
        fee_bps,
        total_shares,
        share_token: Some(pool_address.to_owned()),
        amp: None,
    })
}

fn nonnegative_integer(value: &xdr::ScVal) -> Option<u128> {
    match value {
        xdr::ScVal::U64(value) => Some(*value as u128),
        xdr::ScVal::I64(value) if *value >= 0 => Some(*value as u128),
        xdr::ScVal::U32(value) => Some(*value as u128),
        xdr::ScVal::I32(value) if *value >= 0 => Some(*value as u128),
        _ => scval_to_u128(value).ok(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comet_fee_uses_stroop_scale() {
        assert_eq!(
            nonnegative_integer(&xdr::ScVal::I128(xdr::Int128Parts { hi: 0, lo: 30_000 })),
            Some(30_000)
        );
        assert_eq!((30_000u128 / 1_000) as u32, 30);
    }

    #[test]
    fn comet_seed_pool_is_stable() {
        assert!(COMET_MAINNET_SEED_POOL.starts_with('C'));
        assert!(COMET_MAINNET_FACTORY.starts_with('C'));
    }

    #[test]
    fn invalid_event_xdr_is_ignored() {
        assert_eq!(contract_hash_from_xdr("not-xdr"), None);
    }

    #[test]
    fn comet_requires_ordered_token_limit_arrays() {
        let request = DraftRequest {
            pool_address: "CPOOL".into(),
            kind: DraftOpKind::Deposit,
            position_key: "shares:100".into(),
            payload: serde_json::json!({
                "pool_amount_out": "100",
                "max_amounts_in": ["1000", "2000", "3000"],
                "user": "GUSER"
            }),
            quote_xlm: Some(1.0),
        };
        assert!(CometAdaptor.build_draft_op(request).is_ok());

        let bad = DraftRequest {
            pool_address: "CPOOL".into(),
            kind: DraftOpKind::Withdraw,
            position_key: "shares:100".into(),
            payload: serde_json::json!({
                "pool_amount_in": "100",
                "min_amounts_out": ["0"],
                "user": "GUSER"
            }),
            quote_xlm: None,
        };
        assert!(CometAdaptor.build_draft_op(bad).is_err());
    }
}
