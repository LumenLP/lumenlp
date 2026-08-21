use crate::index_db::CopySessionRow;

pub const COEFFICIENT_SCALE: f64 = 1_000_000.0;
pub const MAX_COEFFICIENT_PPM: u32 = 10_000_000;

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
    PoolNotAllowed,
    OperationLimit,
    DailyLimit,
}

impl PolicyReject {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Expired => "policy_expired",
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
    pool_address: &str,
    scaled_quote_xlm: Option<f64>,
    now: i64,
    daily_used_xlm: f64,
) -> Result<(), PolicyReject> {
    if session.expires_at.is_some_and(|expires_at| now >= expires_at) {
        return Err(PolicyReject::Expired);
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

#[cfg(test)]
mod tests {
    use {super::*, crate::index_db::CopySessionRow};

    fn session() -> CopySessionRow {
        CopySessionRow {
            id: "s".into(),
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
            validate_copy_op(&session(), "COTHER", Some(1.0), 1_000, 0.0),
            Err(PolicyReject::PoolNotAllowed)
        );
        assert_eq!(
            validate_copy_op(&session(), "CPOOL", Some(11.0), 1_000, 0.0),
            Err(PolicyReject::OperationLimit)
        );
    }

    #[test]
    fn rejects_daily_limit_and_expiry() {
        assert_eq!(
            validate_copy_op(&session(), "CPOOL", Some(5.0), 1_000, 16.0),
            Err(PolicyReject::DailyLimit)
        );
        assert_eq!(
            validate_copy_op(&session(), "CPOOL", Some(1.0), 2_000, 0.0),
            Err(PolicyReject::Expired)
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
}
