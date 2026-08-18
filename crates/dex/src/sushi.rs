//! Sushi V3 read-only pool adaptor.
//!
//! Sushi V3 is a concentrated-liquidity venue. The initial integration keeps
//! the safe, useful boundary small: discover known mainnet pools and read their
//! token, fee, price, and liquidity state for pool analytics. CL event decoding
//! and Copy LP writes remain disabled until their contract fixtures are tested.

use {
    crate::{
        adaptor::{DexAdaptor, ScaffoldAdaptor, VenueId},
        rpc::{parse_fee_bps_u32, scval_to_address, scval_to_u128, SorobanRpc},
        types::{PoolType, SharePoolState},
    },
    anyhow::{anyhow, Result},
    stellar_xdr::curr as xdr,
};

pub const SUSHI_MAINNET_FACTORY: &str =
    "CD3KRKGDRVWPXVB3VXLUMQKMX6XZ6Q2H334IVZD4XXNAMKSRVQL5GLYF";

const KNOWN_MAINNET_POOLS: &[&str] = &[
    "CCR2CH4GQVCZHG7CHFVMNANCK45CU5DVKXZIIITDZQAU3CEJZ7RQH2MQ",
    "CAKWXQDEVVUF2ABUEM3M2G7QJGJNDZNNVXJZYG4Z4QP6K54QTWV4DW2S",
    "CAWWOFOEGWPPNP6QKVHTJYB7UHRXC6W6EAFMUPGHMJL7K46E6UCOSNDM",
    "CAXJ2FDV6S3L46EFEFRXUBLQ5U5CZLZOG35RPCJRNQVLM5MH2HCK5I7J",
    "CABMZD6BYKKLHRJNS5MURYOBX77NPAH767AI7EVFGWV3WZV55QFN5YNE",
    "CAFLJXGUAURAMBA3AIHC7ZJOAQKGZ7WEFFGMH5XRC35IMNU7PWIBXVTP",
    "CA75VVHLWSM7W6ULNQI7ZJYDFOMQCCPKIDDDHBAL5KOKHWWKWQ5S7MHO",
    "CAUBW4ARD42U2UEIA7GDUB5LNKTRTVYJHXKL3CV27YZRDFADDGKLZWFD",
    "CCRKQ2RHBWB5ZCHOSBSYEC2QNVSU3MGVUF56BWWKJMJIJ3ZF2A6W7KEC",
    "CBVKO35SAF2ZT75FCLCGLYQG3S6B32YZTOJ2G5F7M746UGBRAWZ5BNZ6",
    "CAPT5THGW7WOCX47TICCB5JZZK4Y24CHQIBSM57Y472WFFV6FGTRKJQD",
    "CAWN3BM2ADBMA4CQZLIHTBXA3BQHV4VAPK42LWT5ONAKZW6PH2BBCKLS",
    "CALM7JTAJC7AJ7ZGTQKXZNNILJUCD2AZNN7QA7FVM3YYIJBCJGUABEDH",
];

pub fn scaffold() -> ScaffoldAdaptor {
    ScaffoldAdaptor {
        venue_id: VenueId::SushiV3,
        name: "Sushi V3",
        notes: "Read-only CLMM pool discovery and state; event/write validation pending.",
    }
}

pub fn adaptor() -> impl DexAdaptor {
    scaffold()
}

/// Return the validated mainnet pool catalogue used by the Sushi integration.
/// The upstream factory does not expose a cheap enumerable pool list, so this
/// catalogue is deliberately explicit and can be refreshed from the aggregator
/// discovery tool without changing the reader contract.
pub async fn discover_mainnet_pool_addresses(_rpc: &SorobanRpc) -> Result<Vec<String>> {
    Ok(KNOWN_MAINNET_POOLS.iter().map(|pool| (*pool).to_owned()).collect())
}

/// Read the Sushi V3 pool state. Virtual reserves are derived from current
/// liquidity and sqrt price so existing TVL/price infrastructure can represent
/// CLMM pools without pretending they have constant-product reserves.
pub async fn hydrate_pool(rpc: &SorobanRpc, pool_address: &str) -> Result<SharePoolState> {
    let slot0 = rpc.call_no_args(pool_address, "slot0").await?;
    let (sqrt_price_x96, _) = parse_slot0(&slot0)?;
    let liquidity = scval_to_u128(&rpc.call_no_args(pool_address, "liquidity").await?)?;
    let fee_bps = parse_fee_bps_u32(&rpc.call_no_args(pool_address, "fee").await?)
        .map(|ppm| ppm / 100)
        .unwrap_or(30);
    let token0 = scval_to_address(&rpc.call_no_args(pool_address, "token0").await?)?;
    let token1 = scval_to_address(&rpc.call_no_args(pool_address, "token1").await?)?;

    let sqrt_price = u256_to_f64(&sqrt_price_x96) / 2f64.powi(96);
    if !sqrt_price.is_finite() || sqrt_price <= 0.0 {
        return Err(anyhow!("Sushi slot0 returned invalid sqrt price"));
    }
    let l = liquidity as f64;
    let reserve0 = (l / sqrt_price).clamp(0.0, u128::MAX as f64) as u128;
    let reserve1 = (l * sqrt_price).clamp(0.0, u128::MAX as f64) as u128;

    Ok(SharePoolState {
        address: pool_address.to_owned(),
        pool_type: PoolType::Concentrated,
        tokens: vec![token0, token1],
        reserves: vec![reserve0, reserve1],
        fee_bps,
        total_shares: 0,
        share_token: None,
        amp: None,
    })
}

fn parse_slot0(value: &xdr::ScVal) -> Result<(xdr::UInt256Parts, i32)> {
    let xdr::ScVal::Map(Some(map)) = value else {
        return Err(anyhow!("Sushi slot0 is not a map"));
    };
    let mut price = None;
    let mut tick = None;
    for entry in map.0.iter() {
        let name = match &entry.key {
            xdr::ScVal::Symbol(symbol) => symbol.to_string(),
            xdr::ScVal::String(string) => string.to_string(),
            _ => continue,
        };
        match name.as_str() {
            "sqrt_price_x96" => price = parse_u256(&entry.val),
            "tick" => tick = match entry.val {
                xdr::ScVal::I32(value) => Some(value),
                _ => None,
            },
            _ => {}
        }
    }
    Ok((price.ok_or_else(|| anyhow!("Sushi slot0 missing sqrt_price_x96"))?, tick.unwrap_or(0)))
}

fn parse_u256(value: &xdr::ScVal) -> Option<xdr::UInt256Parts> {
    match value {
        xdr::ScVal::U256(parts) => Some(parts.clone()),
        _ => None,
    }
}

fn u256_to_f64(value: &xdr::UInt256Parts) -> f64 {
    (((value.hi_hi as f64 * 2f64.powi(64) + value.hi_lo as f64) * 2f64.powi(64)
        + value.lo_hi as f64)
        * 2f64.powi(64))
        + value.lo_lo as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sushi_catalogue_is_deterministic() {
        let mut pools = KNOWN_MAINNET_POOLS.to_vec();
        pools.sort_unstable();
        pools.dedup();
        assert_eq!(pools.len(), KNOWN_MAINNET_POOLS.len());
        assert!(!pools.is_empty());
    }

    #[test]
    fn parses_sushi_slot0_map() {
        let value = xdr::ScVal::Map(Some(
            vec![
                xdr::ScMapEntry {
                    key: xdr::ScVal::Symbol("sqrt_price_x96".try_into().unwrap()),
                    val: xdr::ScVal::U256(xdr::UInt256Parts {
                        hi_hi: 0,
                        hi_lo: 0,
                        lo_hi: 1,
                        lo_lo: 0,
                    }),
                },
                xdr::ScMapEntry {
                    key: xdr::ScVal::Symbol("tick".try_into().unwrap()),
                    val: xdr::ScVal::I32(10),
                },
            ]
            .try_into()
            .unwrap(),
        ));
        let (_, tick) = parse_slot0(&value).unwrap();
        assert_eq!(tick, 10);
    }
}
