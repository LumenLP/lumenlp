use {
    crate::index_db::CopySessionRow,
    dex::{rpc::scval_to_address, support_matrix, DraftOpKind, SorobanRpc, VenueId},
    stellar_xdr::curr as xdr,
};

pub const COEFFICIENT_SCALE: f64 = 1_000_000.0;
pub const MAX_COEFFICIENT_PPM: u32 = 10_000_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OnChainPolicySession {
    pub leader: String,
    pub allowed_pools: Vec<String>,
    pub coefficient_ppm: u32,
    pub follow_claims: bool,
    pub max_per_op_quote: i128,
    pub max_daily_quote: i128,
    pub expires_at: u64,
    pub paused: bool,
}

#[derive(Debug)]
pub enum PolicyBindingError {
    Unavailable(String),
    Mismatch(&'static str),
}

impl PolicyBindingError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Unavailable(_) => "policy_binding_unavailable",
            Self::Mismatch(_) => "policy_binding_mismatch",
        }
    }

    pub fn message(&self) -> String {
        match self {
            Self::Unavailable(error) => format!("could not verify on-chain policy: {error}"),
            Self::Mismatch(field) => format!("on-chain policy does not match local {field}"),
        }
    }
}

pub async fn verify_policy_binding(
    rpc: &SorobanRpc,
    contract_address: &str,
    contract_session_id: u32,
    follower_address: &str,
    leader_address: &str,
    coefficient: f64,
    include_claims: bool,
    allowed_pools: &[String],
    max_per_op_quote_xlm: f64,
    max_daily_quote_xlm: f64,
    expires_at: Option<i64>,
) -> Result<(), PolicyBindingError> {
    let owner = rpc
        .call_no_args(contract_address, "policy_owner")
        .await
        .map_err(|error| PolicyBindingError::Unavailable(error.to_string()))
        .and_then(|value| {
            scval_to_address(&value).map_err(|error| PolicyBindingError::Unavailable(error.to_string()))
        })?;
    if owner != follower_address {
        return Err(PolicyBindingError::Mismatch("owner"));
    }

    let value = rpc
        .simulate_call(contract_address, "session", vec![xdr::ScVal::U32(contract_session_id)])
        .await
        .map_err(|error| PolicyBindingError::Unavailable(error.to_string()))?;
    let session = parse_policy_session(&value).map_err(|error| PolicyBindingError::Unavailable(error.to_string()))?;

    validate_policy_binding(
        &owner,
        &session,
        follower_address,
        leader_address,
        coefficient,
        include_claims,
        allowed_pools,
        max_per_op_quote_xlm,
        max_daily_quote_xlm,
        expires_at,
    )
}

#[allow(clippy::too_many_arguments)]
fn validate_policy_binding(
    owner: &str,
    session: &OnChainPolicySession,
    follower_address: &str,
    leader_address: &str,
    coefficient: f64,
    include_claims: bool,
    allowed_pools: &[String],
    max_per_op_quote_xlm: f64,
    max_daily_quote_xlm: f64,
    expires_at: Option<i64>,
) -> Result<(), PolicyBindingError> {
    if owner != follower_address {
        return Err(PolicyBindingError::Mismatch("owner"));
    }
    if session.leader != leader_address {
        return Err(PolicyBindingError::Mismatch("leader"));
    }
    if session.coefficient_ppm != coefficient_ppm(coefficient).unwrap_or_default() {
        return Err(PolicyBindingError::Mismatch("coefficient"));
    }
    if session.follow_claims != include_claims {
        return Err(PolicyBindingError::Mismatch("claim setting"));
    }
    if allowed_pools.is_empty() {
        return Err(PolicyBindingError::Mismatch("non-empty pool allowlist"));
    }
    let mut expected_pools = allowed_pools.to_vec();
    expected_pools.sort();
    let mut actual_pools = session.allowed_pools.clone();
    actual_pools.sort();
    if actual_pools != expected_pools {
        return Err(PolicyBindingError::Mismatch("pool allowlist"));
    }
    if session.max_per_op_quote != xlm_to_stroops(max_per_op_quote_xlm).unwrap_or_default() {
        return Err(PolicyBindingError::Mismatch("per-operation limit"));
    }
    if session.max_daily_quote != xlm_to_stroops(max_daily_quote_xlm).unwrap_or_default() {
        return Err(PolicyBindingError::Mismatch("daily limit"));
    }
    if Some(session.expires_at) != expires_at.and_then(|value| u64::try_from(value).ok()) {
        return Err(PolicyBindingError::Mismatch("expiry"));
    }
    if session.paused {
        return Err(PolicyBindingError::Mismatch("active state"));
    }
    Ok(())
}

pub fn parse_policy_session(value: &xdr::ScVal) -> anyhow::Result<OnChainPolicySession> {
    let xdr::ScVal::Map(Some(map)) = value else {
        anyhow::bail!("policy session result is not a map");
    };
    let field = |name| {
        map.0
            .iter()
            .find(|entry| matches!(&entry.key, xdr::ScVal::Symbol(symbol) if symbol.to_string() == name))
            .map(|entry| &entry.val)
            .ok_or_else(|| anyhow::anyhow!("policy session missing field {name}"))
    };
    let allowed_pools = match field("allowed_pools")? {
        xdr::ScVal::Vec(Some(values)) => values
            .0
            .iter()
            .map(scval_to_address)
            .collect::<anyhow::Result<Vec<_>>>()?,
        _ => anyhow::bail!("policy session allowed_pools is not a vector"),
    };
    Ok(OnChainPolicySession {
        leader: scval_to_address(field("leader")?)?,
        allowed_pools,
        coefficient_ppm: scval_u32(field("coefficient_ppm")?)?,
        follow_claims: scval_bool(field("follow_claims")?)?,
        max_per_op_quote: scval_i128(field("max_per_op_quote")?)?,
        max_daily_quote: scval_i128(field("max_daily_quote")?)?,
        expires_at: scval_u64(field("expires_at")?)?,
        paused: scval_bool(field("paused")?)?,
    })
}

fn xlm_to_stroops(value: f64) -> Option<i128> {
    if !value.is_finite() || value <= 0.0 {
        return None;
    }
    Some((value * 10_000_000.0).round() as i128)
}

fn scval_u32(value: &xdr::ScVal) -> anyhow::Result<u32> {
    match value {
        xdr::ScVal::U32(value) => Ok(*value),
        _ => anyhow::bail!("expected u32"),
    }
}

fn scval_u64(value: &xdr::ScVal) -> anyhow::Result<u64> {
    match value {
        xdr::ScVal::U64(value) => Ok(*value),
        _ => anyhow::bail!("expected u64"),
    }
}

fn scval_i128(value: &xdr::ScVal) -> anyhow::Result<i128> {
    match value {
        xdr::ScVal::I128(parts) => Ok(((parts.hi as i128) << 64) | parts.lo as i128),
        _ => anyhow::bail!("expected i128"),
    }
}

fn scval_bool(value: &xdr::ScVal) -> anyhow::Result<bool> {
    match value {
        xdr::ScVal::Bool(value) => Ok(*value),
        _ => anyhow::bail!("expected bool"),
    }
}

/// Convert the API's human-friendly coefficient into the fixed-point value
/// expected by the Soroban policy contract.
pub fn coefficient_ppm(coefficient: f64) -> Option<u32> {
    if !coefficient.is_finite() || coefficient <= 0.0 {
        return None;
    }
    let ppm = (coefficient * COEFFICIENT_SCALE).round();
    if !ppm.is_finite() || ppm < 1.0 || ppm > f64::from(MAX_COEFFICIENT_PPM) {
        return None;
    }
    Some(ppm as u32)
}

#[derive(Debug, Clone, PartialEq)]
pub enum PolicyReject {
    Expired,
    VenueNotEnabled,
    OperationNotEnabled,
    PoolNotAllowed,
    OperationLimit,
    DailyLimit,
}

impl PolicyReject {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Expired => "policy_expired",
            Self::VenueNotEnabled => "venue_not_enabled",
            Self::OperationNotEnabled => "operation_not_enabled",
            Self::PoolNotAllowed => "pool_not_allowed",
            Self::OperationLimit => "per_operation_limit",
            Self::DailyLimit => "daily_limit",
        }
    }
}

/// Validate an indexed copy draft before it becomes eligible for execution.
/// A zero limit means "not configured" for backwards compatibility with v0
/// sessions; new automated sessions should always set explicit limits.
pub fn validate_copy_op(
    session: &CopySessionRow,
    venue: &str,
    operation: &str,
    pool_address: &str,
    scaled_quote_xlm: Option<f64>,
    now: i64,
    daily_used_xlm: f64,
) -> Result<(), PolicyReject> {
    if session.expires_at.is_some_and(|expires_at| now >= expires_at) {
        return Err(PolicyReject::Expired);
    }

    // Keep this decision driven by the shared venue matrix. A venue may expose
    // analytics and unsigned drafts without being eligible for policy-driven
    // execution; unknown and scaffold venues must fail closed.
    let Some(row) = execution_row(venue) else {
        return Err(PolicyReject::VenueNotEnabled);
    };
    let Some(kind) = draft_kind(operation) else {
        return Err(PolicyReject::OperationNotEnabled);
    };
    if !row.capabilities.supports(kind) {
        return Err(PolicyReject::OperationNotEnabled);
    }

    if !session.allowed_pools.is_empty() && !session.allowed_pools.iter().any(|pool| pool == pool_address) {
        return Err(PolicyReject::PoolNotAllowed);
    }

    let quote = scaled_quote_xlm.unwrap_or(0.0);
    if session.max_per_op_quote_xlm > 0.0 && quote > session.max_per_op_quote_xlm {
        return Err(PolicyReject::OperationLimit);
    }
    if session.max_daily_quote_xlm > 0.0 && daily_used_xlm + quote > session.max_daily_quote_xlm {
        return Err(PolicyReject::DailyLimit);
    }
    Ok(())
}

fn execution_row(venue: &str) -> Option<dex::VenueSupportRow> {
    let Some(venue_id) = VenueId::parse(venue) else {
        return None;
    };
    support_matrix()
        .into_iter()
        .find(|row| row.venue_id == venue_id)
        .filter(|row| row.copy_execution_enabled)
}

fn draft_kind(operation: &str) -> Option<DraftOpKind> {
    match operation {
        "deposit" => Some(DraftOpKind::Deposit),
        "withdraw" => Some(DraftOpKind::Withdraw),
        "claim" => Some(DraftOpKind::Claim),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use {super::*, crate::index_db::CopySessionRow};

    fn map_entry(name: &str, value: xdr::ScVal) -> xdr::ScMapEntry {
        xdr::ScMapEntry {
            key: xdr::ScVal::Symbol(name.try_into().unwrap()),
            val: value,
        }
    }

    fn session() -> CopySessionRow {
        CopySessionRow {
            id: "s".into(),
            contract_address: None,
            contract_session_id: None,
            follower_address: "GFOLLOWER".into(),
            leader_address: "GLEADER".into(),
            coefficient: 0.1,
            status: "active".into(),
            include_claims: true,
            allowed_pools: vec!["CPOOL".into()],
            max_per_op_quote_xlm: 10.0,
            max_daily_quote_xlm: 20.0,
            expires_at: Some(2_000),
            cursor_ts: 0,
            watermark_ts: 0,
            watermark_event_id: String::new(),
            created_at: 0,
            updated_at: 0,
        }
    }

    #[test]
    fn rejects_outside_policy_scope() {
        assert_eq!(
            validate_copy_op(&session(), "aquarius", "deposit", "COTHER", Some(1.0), 1_000, 0.0),
            Err(PolicyReject::PoolNotAllowed)
        );
        assert_eq!(
            validate_copy_op(&session(), "aquarius", "deposit", "CPOOL", Some(11.0), 1_000, 0.0),
            Err(PolicyReject::OperationLimit)
        );
    }

    #[test]
    fn rejects_daily_limit_and_expiry() {
        assert_eq!(
            validate_copy_op(&session(), "aquarius", "deposit", "CPOOL", Some(5.0), 1_000, 16.0),
            Err(PolicyReject::DailyLimit)
        );
        assert_eq!(
            validate_copy_op(&session(), "aquarius", "deposit", "CPOOL", Some(1.0), 2_000, 0.0),
            Err(PolicyReject::Expired)
        );
    }

    #[test]
    fn rejects_non_aquarius_until_execution_adapter_is_enabled() {
        assert_eq!(
            validate_copy_op(&session(), "soroswap_amm", "deposit", "CPOOL", Some(1.0), 1_000, 0.0),
            Err(PolicyReject::VenueNotEnabled)
        );
        assert_eq!(
            validate_copy_op(&session(), "unknown_dex", "deposit", "CPOOL", Some(1.0), 1_000, 0.0),
            Err(PolicyReject::VenueNotEnabled)
        );
    }

    #[test]
    fn execution_gate_accepts_only_matrix_enabled_venue_aliases() {
        assert!(execution_row("aquarius").is_some());
        assert!(execution_row("soroswap").is_none());
        assert!(execution_row("sushi_v3").is_none());
        assert!(execution_row("unknown_dex").is_none());
    }

    #[test]
    fn operation_gate_rejects_unknown_operation() {
        assert_eq!(
            validate_copy_op(&session(), "aquarius", "swap", "CPOOL", Some(1.0), 1_000, 0.0),
            Err(PolicyReject::OperationNotEnabled)
        );
    }

    #[test]
    fn coefficient_ppm_matches_contract_scale_and_bounds() {
        assert_eq!(coefficient_ppm(0.1), Some(100_000));
        assert_eq!(coefficient_ppm(1.0), Some(1_000_000));
        assert_eq!(coefficient_ppm(10.0), Some(10_000_000));
        assert_eq!(coefficient_ppm(0.0), None);
        assert_eq!(coefficient_ppm(10.000_001), None);
    }

    #[test]
    fn parses_copy_policy_session_contract_value() {
        let leader_value = xdr::ScVal::Address(xdr::ScAddress::Account(xdr::AccountId(
            xdr::PublicKey::PublicKeyTypeEd25519(xdr::Uint256([7; 32])),
        )));
        let pool_value = xdr::ScVal::Address(xdr::ScAddress::Contract(xdr::ContractId(xdr::Hash([8; 32]))));
        let leader = scval_to_address(&leader_value).unwrap();
        let pool = scval_to_address(&pool_value).unwrap();
        let value = xdr::ScVal::Map(Some(xdr::ScMap(
            vec![
                map_entry(
                    "allowed_pools",
                    xdr::ScVal::Vec(Some(xdr::ScVec(vec![pool_value].try_into().unwrap()))),
                ),
                map_entry("coefficient_ppm", xdr::ScVal::U32(100_000)),
                map_entry("daily_day", xdr::ScVal::U64(1)),
                map_entry("daily_used_quote", xdr::ScVal::I128(xdr::Int128Parts { hi: 0, lo: 0 })),
                map_entry("expires_at", xdr::ScVal::U64(2_000)),
                map_entry("follow_claims", xdr::ScVal::Bool(true)),
                map_entry("leader", leader_value),
                map_entry(
                    "max_daily_quote",
                    xdr::ScVal::I128(xdr::Int128Parts { hi: 0, lo: 20_000_000 }),
                ),
                map_entry(
                    "max_per_op_quote",
                    xdr::ScVal::I128(xdr::Int128Parts { hi: 0, lo: 10_000_000 }),
                ),
                map_entry("paused", xdr::ScVal::Bool(false)),
            ]
            .try_into()
            .unwrap(),
        )));

        assert_eq!(
            parse_policy_session(&value).unwrap(),
            OnChainPolicySession {
                leader,
                allowed_pools: vec![pool],
                coefficient_ppm: 100_000,
                follow_claims: true,
                max_per_op_quote: 10_000_000,
                max_daily_quote: 20_000_000,
                expires_at: 2_000,
                paused: false,
            }
        );
    }

    #[test]
    fn binding_validation_rejects_policy_drift_and_pause() {
        let mut actual = OnChainPolicySession {
            leader: "GLEADER".into(),
            allowed_pools: vec!["CPOOL".into()],
            coefficient_ppm: 100_000,
            follow_claims: true,
            max_per_op_quote: 100_000_000,
            max_daily_quote: 200_000_000,
            expires_at: 2_000,
            paused: false,
        };
        let validate = |session: &OnChainPolicySession| {
            validate_policy_binding(
                "GFOLLOWER",
                session,
                "GFOLLOWER",
                "GLEADER",
                0.1,
                true,
                &["CPOOL".into()],
                10.0,
                20.0,
                Some(2_000),
            )
        };

        assert!(validate(&actual).is_ok());
        actual.coefficient_ppm = 200_000;
        assert!(matches!(
            validate(&actual),
            Err(PolicyBindingError::Mismatch("coefficient"))
        ));
        actual.coefficient_ppm = 100_000;
        actual.paused = true;
        assert!(matches!(
            validate(&actual),
            Err(PolicyBindingError::Mismatch("active state"))
        ));
    }
}
