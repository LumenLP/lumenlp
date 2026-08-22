//! Sushi V3 read-only pool adaptor.
//!
//! Sushi V3 is a concentrated-liquidity venue. The initial integration keeps
//! the safe, useful boundary small: discover known mainnet pools and read their
//! token, fee, price, and liquidity state for pool analytics. LP lifecycle
//! writes remain disabled until operation fixtures are validated.

use {
    crate::{
        adaptor::{
            DexAdaptor, DraftOp, DraftOpKind, DraftRequest, ScaffoldAdaptor, VenueCapabilities, VenueId, VenueStatus,
        },
        rpc::{
            account_address_scval, parse_fee_bps_u32, scval_to_address, scval_to_i32, scval_to_symbol_string,
            scval_to_u128, SorobanRpc,
        },
        types::{ClPositionRange, PoolType, SharePoolState, UserPosition},
    },
    anyhow::{anyhow, bail, Result},
    metrics::{cl_position_amounts, value_xlm},
    serde_json::Value,
    stellar_xdr::curr as xdr,
};

pub const SUSHI_MAINNET_FACTORY: &str = "CD3KRKGDRVWPXVB3VXLUMQKMX6XZ6Q2H334IVZD4XXNAMKSRVQL5GLYF";
pub const SUSHI_MAINNET_POSITION_MANAGER: &str = "CC5CQHSGZEVKPDLMYTJYGUBDL5UW4NBMTRQ5Y43YDBJTJZKMZMKCEEDU";

/// Sushi V3 position-manager operation boundary. These drafts mirror the
/// deployed mint/increase/decrease/collect parameter structs, but remain
/// unsigned until the policy contract supports this venue explicitly.
#[derive(Debug, Default, Clone, Copy)]
pub struct SushiAdaptor;

impl DexAdaptor for SushiAdaptor {
    fn venue_id(&self) -> VenueId {
        VenueId::SushiV3
    }

    fn name(&self) -> &'static str {
        "Sushi V3"
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
            claim: true,
            copy_scale: false,
        }
    }

    fn notes(&self) -> &'static str {
        "Validated unsigned Position Manager drafts; policy-controlled execution remains fail-closed."
    }

    fn build_draft_op(&self, request: DraftRequest) -> Result<DraftOp> {
        if request.pool_address.is_empty() || request.position_key.is_empty() {
            bail!("draft operation requires pool_address and position_key");
        }
        match request.kind {
            DraftOpKind::Deposit => validate_payload(
                &request.payload,
                &[
                    "token0",
                    "token1",
                    "fee",
                    "recipient",
                    "sender",
                    "tick_lower",
                    "tick_upper",
                    "amount0_desired",
                    "amount0_min",
                    "amount1_desired",
                    "amount1_min",
                    "deadline",
                ],
            )?,
            DraftOpKind::Withdraw => validate_payload(
                &request.payload,
                &[
                    "token_id",
                    "operator",
                    "liquidity",
                    "amount0_min",
                    "amount1_min",
                    "deadline",
                ],
            )?,
            DraftOpKind::Claim => validate_payload(
                &request.payload,
                &["token_id", "operator", "recipient", "amount0_max", "amount1_max"],
            )?,
            _ => bail!("Sushi V3 supports deposit, withdraw, and claim drafts only"),
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

/// Current state for one Sushi V3 CL position range.
///
/// The pool contract can read a range once its lower and upper ticks are
/// known. Range discovery is deliberately kept outside this primitive because
/// the contract does not expose an enumerable owner-position list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SushiPositionState {
    pub liquidity: u128,
    pub tokens_owed_0: u128,
    pub tokens_owed_1: u128,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SushiPositionRangeCandidate {
    pub pool_address: String,
    pub tick_lower: i32,
    pub tick_upper: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SushiManagedPosition {
    fee: u32,
    liquidity: u128,
    tokens_owed_0: u128,
    tokens_owed_1: u128,
    tick_lower: i32,
    tick_upper: i32,
    token0: String,
    token1: String,
}

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
        notes: "Read-only CLMM pool discovery, state, and indexed LP events; writes remain gated.",
    }
}

pub fn adaptor() -> impl DexAdaptor {
    SushiAdaptor
}

fn validate_payload(payload: &Value, required: &[&str]) -> Result<()> {
    let object = payload
        .as_object()
        .ok_or_else(|| anyhow!("Sushi Position Manager payload must be an object"))?;
    for key in required {
        if !object.contains_key(*key) {
            bail!("Sushi Position Manager payload missing {key}");
        }
    }
    Ok(())
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

/// Read a known Sushi V3 position range from the pool contract.
///
/// This mirrors the deployed CL pool interface used by Aquarius, but keeps
/// Sushi-specific ownership/range discovery out of the read primitive. A
/// zeroed position is returned as `None` so callers can safely probe ranges
/// discovered from indexed `mint` events.
pub async fn read_position(
    rpc: &SorobanRpc,
    pool_address: &str,
    owner: &str,
    tick_lower: i32,
    tick_upper: i32,
) -> Result<Option<SushiPositionState>> {
    let owner = account_address_scval(owner)?;
    let value = rpc
        .simulate_call(
            pool_address,
            "get_position",
            vec![owner, xdr::ScVal::I32(tick_lower), xdr::ScVal::I32(tick_upper)],
        )
        .await?;
    let position = parse_position(&value)?;
    if position.liquidity == 0 && position.tokens_owed_0 == 0 && position.tokens_owed_1 == 0 {
        Ok(None)
    } else {
        Ok(Some(position))
    }
}

/// Read the canonical Sushi V3 position list. Sushi positions are NFTs held
/// by the position manager, so querying the pool with the wallet as owner can
/// return an empty or unrelated range.
async fn read_managed_positions(rpc: &SorobanRpc, owner: &str) -> Result<Vec<SushiManagedPosition>> {
    let value = rpc
        .simulate_call(
            SUSHI_MAINNET_POSITION_MANAGER,
            "get_user_positions_with_fees",
            vec![account_address_scval(owner)?, xdr::ScVal::U32(0), xdr::ScVal::U32(100)],
        )
        .await?;
    parse_managed_positions(&value)
}

/// Resolve currently owned Sushi positions against concrete known pools.
/// This path does not depend on the event index, but only emits a position
/// when token pair, fee tier, and range identify a concrete Sushi pool.
pub async fn positions_for_managed_pools(
    rpc: &SorobanRpc,
    user: &str,
    pool_addresses: &[String],
    pricing_pools: &[SharePoolState],
) -> Vec<UserPosition> {
    let managed = read_managed_positions(rpc, user).await.unwrap_or_default();
    let mut candidates = Vec::new();
    for pool_address in pool_addresses {
        let state = if let Some(state) = pricing_pools
            .iter()
            .find(|state| state.address == *pool_address)
            .cloned()
        {
            state
        } else {
            let Ok(state) = hydrate_pool(rpc, pool_address).await else {
                continue;
            };
            state
        };
        for position in managed.iter().filter(|position| {
            position.token0 == state.tokens[0]
                && position.token1 == state.tokens[1]
                && (position.fee / 100 == state.fee_bps || position.fee == state.fee_bps)
        }) {
            candidates.push(SushiPositionRangeCandidate {
                pool_address: pool_address.clone(),
                tick_lower: position.tick_lower,
                tick_upper: position.tick_upper,
            });
        }
    }
    positions_for_candidates(rpc, user, &candidates, pricing_pools).await
}

/// Resolve event-derived range candidates against current Sushi pool state.
/// This is intentionally a point-read workflow: Sushi does not expose an
/// enumerable owner-position method, so candidates come from indexed mint /
/// burn events and are then verified on-chain.
pub async fn positions_for_candidates(
    rpc: &SorobanRpc,
    user: &str,
    candidates: &[SushiPositionRangeCandidate],
    pricing_pools: &[SharePoolState],
) -> Vec<UserPosition> {
    let book = crate::aquarius::pricing::price_book_from_pools(pricing_pools);
    let managed = read_managed_positions(rpc, user).await.unwrap_or_default();
    let mut out = Vec::new();
    for candidate in candidates {
        let Ok(state) = hydrate_pool(rpc, &candidate.pool_address).await else {
            continue;
        };
        let mut position = managed
            .iter()
            .find(|position| {
                position.tick_lower == candidate.tick_lower
                    && position.tick_upper == candidate.tick_upper
                    && position.token0 == state.tokens[0]
                    && position.token1 == state.tokens[1]
            })
            .map(|position| SushiPositionState {
                liquidity: position.liquidity,
                tokens_owed_0: position.tokens_owed_0,
                tokens_owed_1: position.tokens_owed_1,
            });
        if position.is_none() {
            position = read_position(
                rpc,
                &candidate.pool_address,
                user,
                candidate.tick_lower,
                candidate.tick_upper,
            )
            .await
            .ok()
            .flatten();
        }
        let Some(position) = position else { continue };
        let current_tick = rpc
            .call_no_args(&candidate.pool_address, "slot0")
            .await
            .ok()
            .and_then(|value| parse_slot0(&value).ok().map(|(_, tick)| tick))
            .unwrap_or(0);
        let (amount0, amount1) = cl_position_amounts(
            position.liquidity,
            candidate.tick_lower,
            candidate.tick_upper,
            current_tick,
        );
        let amounts = vec![
            amount0 + position.tokens_owed_0 as f64,
            amount1 + position.tokens_owed_1 as f64,
        ];
        let prices = book.required(&state.tokens);
        let value_quote = prices.as_ref().and_then(|p| value_xlm(&amounts, p));
        let fees_unclaimed_quote = prices
            .as_ref()
            .map(|p| position.tokens_owed_0 as f64 * p[0] + position.tokens_owed_1 as f64 * p[1]);
        out.push(UserPosition {
            pool_address: state.address,
            venue: "sushi".into(),
            pool_type: PoolType::Concentrated,
            tokens: state.tokens,
            fee_bps: state.fee_bps,
            amounts,
            value_quote,
            il_est: None,
            pnl: None,
            fees_unclaimed_quote,
            status: "ok".into(),
            shares: None,
            cl_ranges: Some(vec![ClPositionRange {
                tick_lower: candidate.tick_lower,
                tick_upper: candidate.tick_upper,
                liquidity: position.liquidity,
                tokens_owed_0: position.tokens_owed_0,
                tokens_owed_1: position.tokens_owed_1,
                in_range: current_tick >= candidate.tick_lower && current_tick < candidate.tick_upper,
            }]),
            note: Some("Sushi V3 range verified by Position Manager or pool fallback; il/pnl=n/a".into()),
        });
    }
    out
}

fn parse_position(value: &xdr::ScVal) -> Result<SushiPositionState> {
    let xdr::ScVal::Map(Some(map)) = value else {
        return Err(anyhow!("Sushi get_position returned a non-map value"));
    };
    let mut position = SushiPositionState {
        liquidity: 0,
        tokens_owed_0: 0,
        tokens_owed_1: 0,
    };
    for entry in map.0.iter() {
        let xdr::ScVal::Symbol(key) = &entry.key else {
            continue;
        };
        match key.to_string().as_str() {
            "liquidity" => position.liquidity = scval_to_u128(&entry.val).unwrap_or(0),
            "tokens_owed_0" => position.tokens_owed_0 = scval_to_u128(&entry.val).unwrap_or(0),
            "tokens_owed_1" => position.tokens_owed_1 = scval_to_u128(&entry.val).unwrap_or(0),
            _ => {}
        }
    }
    Ok(position)
}

fn parse_managed_positions(value: &xdr::ScVal) -> Result<Vec<SushiManagedPosition>> {
    let value = unwrap_contract_result(value)?;
    let xdr::ScVal::Vec(Some(values)) = value else {
        return Err(anyhow!("Sushi position manager returned a non-vector value"));
    };
    values.0.iter().map(parse_managed_position).collect()
}

fn parse_managed_position(value: &xdr::ScVal) -> Result<SushiManagedPosition> {
    let xdr::ScVal::Map(Some(map)) = value else {
        return Err(anyhow!("Sushi managed position is not a map"));
    };
    let mut position = SushiManagedPosition {
        fee: 0,
        liquidity: 0,
        tokens_owed_0: 0,
        tokens_owed_1: 0,
        tick_lower: 0,
        tick_upper: 0,
        token0: String::new(),
        token1: String::new(),
    };
    for entry in map.0.iter() {
        let key = scval_to_symbol_string(&entry.key).unwrap_or_default();
        match key.as_str() {
            "fee" => position.fee = parse_fee_bps_u32(&entry.val).unwrap_or(0),
            "liquidity" => position.liquidity = scval_to_u128(&entry.val)?,
            "tokens_owed0" | "tokens_owed_0" => position.tokens_owed_0 = scval_to_u128(&entry.val)?,
            "tokens_owed1" | "tokens_owed_1" => position.tokens_owed_1 = scval_to_u128(&entry.val)?,
            "tick_lower" => position.tick_lower = scval_to_i32(&entry.val)?,
            "tick_upper" => position.tick_upper = scval_to_i32(&entry.val)?,
            "token0" => position.token0 = scval_to_address(&entry.val)?,
            "token1" => position.token1 = scval_to_address(&entry.val)?,
            _ => {}
        }
    }
    if position.token0.is_empty() || position.token1.is_empty() {
        return Err(anyhow!("Sushi managed position is missing token addresses"));
    }
    Ok(position)
}

fn unwrap_contract_result(value: &xdr::ScVal) -> Result<&xdr::ScVal> {
    let xdr::ScVal::Map(Some(map)) = value else {
        return Ok(value);
    };
    for entry in map.0.iter() {
        let key = scval_to_symbol_string(&entry.key).unwrap_or_default();
        if key == "Ok" {
            return Ok(&entry.val);
        }
        if key == "Err" {
            return Err(anyhow!("Sushi position manager returned Err"));
        }
    }
    Ok(value)
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
            "tick" => {
                tick = match entry.val {
                    xdr::ScVal::I32(value) => Some(value),
                    _ => None,
                }
            }
            _ => {}
        }
    }
    Ok((
        price.ok_or_else(|| anyhow!("Sushi slot0 missing sqrt_price_x96"))?,
        tick.unwrap_or(0),
    ))
}

fn parse_u256(value: &xdr::ScVal) -> Option<xdr::UInt256Parts> {
    match value {
        xdr::ScVal::U256(parts) => Some(parts.clone()),
        _ => None,
    }
}

fn u256_to_f64(value: &xdr::UInt256Parts) -> f64 {
    (((value.hi_hi as f64 * 2f64.powi(64) + value.hi_lo as f64) * 2f64.powi(64) + value.lo_hi as f64) * 2f64.powi(64))
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

    #[test]
    fn parses_sushi_position_state() {
        let value = xdr::ScVal::Map(Some(
            vec![
                xdr::ScMapEntry {
                    key: xdr::ScVal::Symbol("liquidity".try_into().unwrap()),
                    val: xdr::ScVal::U128(xdr::UInt128Parts { hi: 1, lo: 2 }),
                },
                xdr::ScMapEntry {
                    key: xdr::ScVal::Symbol("tokens_owed_0".try_into().unwrap()),
                    val: xdr::ScVal::U128(xdr::UInt128Parts { hi: 0, lo: 3 }),
                },
                xdr::ScMapEntry {
                    key: xdr::ScVal::Symbol("tokens_owed_1".try_into().unwrap()),
                    val: xdr::ScVal::U128(xdr::UInt128Parts { hi: 0, lo: 4 }),
                },
            ]
            .try_into()
            .unwrap(),
        ));
        assert_eq!(
            parse_position(&value).unwrap(),
            SushiPositionState {
                liquidity: (1u128 << 64) + 2,
                tokens_owed_0: 3,
                tokens_owed_1: 4,
            }
        );
    }

    #[test]
    fn parses_sushi_managed_position_info() {
        let token0 = xdr::ScVal::Address(xdr::ScAddress::Contract(xdr::ContractId(xdr::Hash([1; 32]))));
        let token1 = xdr::ScVal::Address(xdr::ScAddress::Contract(xdr::ContractId(xdr::Hash([2; 32]))));
        let value = xdr::ScVal::Vec(Some(
            vec![xdr::ScVal::Map(Some(
                vec![
                    xdr::ScMapEntry {
                        key: xdr::ScVal::Symbol("fee".try_into().unwrap()),
                        val: xdr::ScVal::U32(3000),
                    },
                    xdr::ScMapEntry {
                        key: xdr::ScVal::Symbol("liquidity".try_into().unwrap()),
                        val: xdr::ScVal::U128(xdr::UInt128Parts { hi: 0, lo: 11 }),
                    },
                    xdr::ScMapEntry {
                        key: xdr::ScVal::Symbol("tick_lower".try_into().unwrap()),
                        val: xdr::ScVal::I32(-120),
                    },
                    xdr::ScMapEntry {
                        key: xdr::ScVal::Symbol("tick_upper".try_into().unwrap()),
                        val: xdr::ScVal::I32(120),
                    },
                    xdr::ScMapEntry {
                        key: xdr::ScVal::Symbol("token0".try_into().unwrap()),
                        val: token0,
                    },
                    xdr::ScMapEntry {
                        key: xdr::ScVal::Symbol("token1".try_into().unwrap()),
                        val: token1,
                    },
                ]
                .try_into()
                .unwrap(),
            ))]
            .try_into()
            .unwrap(),
        ));
        let positions = parse_managed_positions(&value).unwrap();
        assert_eq!(positions.len(), 1);
        assert_eq!(positions[0].fee, 3000);
        assert_eq!(positions[0].liquidity, 11);
        assert_eq!(positions[0].tick_lower, -120);
        assert_eq!(positions[0].tick_upper, 120);
        assert!(!positions[0].token0.is_empty());
        assert!(!positions[0].token1.is_empty());
    }
}
