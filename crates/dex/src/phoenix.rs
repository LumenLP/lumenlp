//! Phoenix Protocol adaptor.
//!
//! The reader is intentionally read-only for now. Phoenix pool contracts expose
//! a stable query surface, but factory discovery and event-version coverage still
//! need mainnet validation before this venue can be promoted to production.

use {
    crate::{
        adaptor::{DexAdaptor, ScaffoldAdaptor, VenueId},
        rpc::{scval_to_address, scval_to_u128, SorobanRpc},
        types::{PoolType, SharePoolState},
    },
    anyhow::{anyhow, Result},
    stellar_xdr::curr as xdr,
};

pub fn scaffold() -> ScaffoldAdaptor {
    ScaffoldAdaptor {
        venue_id: VenueId::Phoenix,
        name: "Phoenix",
        notes: "Scaffold. CP AMM read+draft planned.",
    }
}

pub fn adaptor() -> impl DexAdaptor {
    scaffold()
}

/// Read a Phoenix XYK pool through its public query methods.
///
/// This does not discover pool addresses and does not build write operations.
/// Callers must obtain a pool address from a validated Phoenix factory source.
pub async fn hydrate_pool(rpc: &SorobanRpc, pool_address: &str) -> Result<SharePoolState> {
    let config = rpc.call_no_args(pool_address, "query_config").await?;
    let pool_info = rpc.call_no_args(pool_address, "query_pool_info").await?;

    let config_map = scval_map(&config)?;
    let tokens = vec![
        scval_to_address(map_value(config_map, "token_a")?)?,
        scval_to_address(map_value(config_map, "token_b")?)?,
    ];
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
        pool_type: PoolType::ConstantProduct,
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
}
