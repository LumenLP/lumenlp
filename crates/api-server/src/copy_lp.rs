//! Pure helpers for Copy LP: amount scaling, position keys, and op draft
//! building.

use serde_json::{json, Value};

#[derive(Debug, Clone, PartialEq)]
pub struct CopyOpDraft {
    pub kind: String,
    pub position_key: String,
    pub leader_amounts_json: Value,
    pub scaled_amounts_json: Value,
    pub leader_quote_xlm: Option<f64>,
    pub scaled_quote_xlm: Option<f64>,
}

/// Scale a raw token amount string by `coefficient`.
///
/// Token amounts are non-negative integer strings in base units. Convert the
/// human-facing coefficient to the same fixed-point ppm representation used by
/// the Soroban policy, then multiply with checked integer arithmetic. This
/// preserves truncation toward zero without losing precision for large amounts.
pub fn scale_amount_str(amount: &str, coefficient: f64) -> Option<String> {
    let parsed = amount.parse::<u128>().ok()?;
    if !coefficient.is_finite() || coefficient <= 0.0 {
        return None;
    }
    const SCALE: u128 = 1_000_000;
    let ppm = (coefficient * SCALE as f64).round();
    if !ppm.is_finite() || ppm < 1.0 || ppm > 10_000_000.0 {
        return None;
    }
    let ppm = ppm as u128;
    // Divide first so a valid result does not overflow an intermediate
    // `parsed * ppm` multiplication near the u128 boundary.
    let whole = parsed / SCALE;
    let remainder = parsed % SCALE;
    let scaled = whole
        .checked_mul(ppm)?
        .checked_add(remainder.checked_mul(ppm)?.checked_div(SCALE)?)?;
    Some(scaled.to_string())
}

/// Scale each `{ token, amount }` entry in a token-amounts JSON array.
pub fn scale_token_amounts_json(amounts: &Value, coefficient: f64) -> Option<Value> {
    let rows = amounts.as_array()?;
    let mut scaled = Vec::with_capacity(rows.len());
    for row in rows {
        let token = row.get("token")?.as_str()?;
        let amount = row.get("amount")?.as_str()?;
        scaled.push(json!({
            "token": token,
            "amount": scale_amount_str(amount, coefficient)?,
        }));
    }
    Some(Value::Array(scaled))
}

pub fn position_key_cp(pool_address: &str) -> String {
    format!("cp:{pool_address}")
}

/// Stable key for a concentrated-liquidity position. The current UI does not
/// create CL copy drafts yet, but the key is part of the future draft boundary.
#[allow(dead_code)]
pub fn position_key_cl(pool_address: &str, tick_lower: i64, tick_upper: i64) -> String {
    format!("cl:{pool_address}:{tick_lower}:{tick_upper}")
}

pub fn copy_kind_from_event(event_kind: &str, include_claims: bool) -> Option<&'static str> {
    match event_kind {
        "deposit_liquidity" => Some("deposit"),
        "withdraw_liquidity" => Some("withdraw"),
        "claim_fees" | "claim_protocol_fee" if include_claims => Some("claim"),
        _ => None,
    }
}

fn event_kind_from_body(body: &Value) -> Option<&str> {
    body.get("topic")?.as_array()?.first()?.get("value")?.as_str()
}

fn scale_quote_xlm(quote: f64, coefficient: f64) -> f64 {
    quote * coefficient
}

fn token_amounts_from_claim_fees(derived: &Value) -> Option<Value> {
    let mut rows = Vec::new();
    for (token_key, amount_key) in [("token0", "amount0"), ("token1", "amount1")] {
        let Some(token) = derived.get(token_key).and_then(Value::as_str) else {
            continue;
        };
        let Some(amount) = derived.get(amount_key).and_then(Value::as_str) else {
            continue;
        };
        rows.push(json!({ "token": token, "amount": amount }));
    }
    if rows.is_empty() {
        None
    } else {
        Some(Value::Array(rows))
    }
}

fn token_amounts_from_claim_protocol_fee(derived: &Value) -> Option<Value> {
    let token = derived.get("token").and_then(Value::as_str)?;
    let amount = derived.get("amount").and_then(Value::as_str)?;
    Some(json!([{ "token": token, "amount": amount }]))
}

fn leader_amounts_for_kind(event_kind: &str, derived: &Value) -> Option<Value> {
    match event_kind {
        "deposit_liquidity" | "withdraw_liquidity" => {
            let amounts = derived.get("token_amounts")?;
            if amounts.as_array().is_some_and(|rows| !rows.is_empty()) {
                Some(amounts.clone())
            } else {
                None
            }
        }
        "claim_fees" => token_amounts_from_claim_fees(derived),
        "claim_protocol_fee" => token_amounts_from_claim_protocol_fee(derived),
        _ => None,
    }
}

fn leader_quote_for_kind(event_kind: &str, derived: &Value) -> Option<f64> {
    match event_kind {
        "deposit_liquidity" | "withdraw_liquidity" => derived.get("total_quote_xlm").and_then(Value::as_f64),
        "claim_fees" | "claim_protocol_fee" => derived.get("fee_quote_xlm").and_then(Value::as_f64),
        _ => None,
    }
}

fn is_copyable_pool_type(derived: &Value) -> bool {
    // CL operations require position/range-specific ABI handling. Do not
    // turn an explicitly identified CLMM event into a fungible-share draft.
    !matches!(
        derived.get("pool_type").and_then(Value::as_str),
        Some("concentrated" | "clmm")
    )
}

/// Build a scaled CopyOp draft from an indexed pool event body.
///
/// Actor filtering is the caller's responsibility; events without usable token
/// amounts are skipped.
pub fn build_scaled_op_payload(
    event_body: &Value,
    pool_address: &str,
    coefficient: f64,
    include_claims: bool,
) -> Option<CopyOpDraft> {
    let event_kind = event_kind_from_body(event_body)?;
    let copy_kind = copy_kind_from_event(event_kind, include_claims)?;
    let derived = event_body.get("derived")?;
    if !is_copyable_pool_type(derived) {
        return None;
    }

    let leader_amounts = leader_amounts_for_kind(event_kind, derived)?;
    let scaled_amounts = scale_token_amounts_json(&leader_amounts, coefficient)?;

    let leader_quote_xlm = leader_quote_for_kind(event_kind, derived);
    let scaled_quote_xlm = leader_quote_xlm.map(|quote| scale_quote_xlm(quote, coefficient));

    Some(CopyOpDraft {
        kind: copy_kind.to_string(),
        position_key: position_key_cp(pool_address),
        leader_amounts_json: leader_amounts,
        scaled_amounts_json: scaled_amounts,
        leader_quote_xlm,
        scaled_quote_xlm,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scale_amount_str_by_coefficient() {
        assert_eq!(scale_amount_str("1000", 0.1).as_deref(), Some("100"));
        assert_eq!(scale_amount_str("1000", 2.0).as_deref(), Some("2000"));
        assert_eq!(scale_amount_str("999", 0.3333339).as_deref(), Some("333"));
    }

    #[test]
    fn scale_amount_str_preserves_large_integer_precision_and_fails_on_overflow() {
        let amount = u128::MAX - 1;
        assert_eq!(scale_amount_str(&amount.to_string(), 1.0), Some(amount.to_string()));
        assert!(scale_amount_str(&u128::MAX.to_string(), 10.0).is_none());
    }

    #[test]
    fn position_key_cp_and_cl() {
        assert_eq!(position_key_cp("CPOOL"), "cp:CPOOL");
        assert_eq!(position_key_cl("CPOOL", -100, 200), "cl:CPOOL:-100:200");
    }

    #[test]
    fn kind_from_event() {
        assert_eq!(copy_kind_from_event("deposit_liquidity", false), Some("deposit"));
        assert_eq!(copy_kind_from_event("claim_fees", false), None);
        assert_eq!(copy_kind_from_event("claim_fees", true), Some("claim"));
    }

    #[test]
    fn build_scaled_op_payload_deposit() {
        let body = json!({
            "topic": [{"type":"symbol","value":"deposit_liquidity"}],
            "derived": {
                "token_amounts": [
                    {"token": "CA", "amount": "1000"},
                    {"token": "CB", "amount": "2000"}
                ],
                "total_quote_xlm": 30.0
            }
        });
        let draft = build_scaled_op_payload(&body, "CPOOL", 0.1, false).unwrap();
        assert_eq!(draft.kind, "deposit");
        assert_eq!(draft.position_key, "cp:CPOOL");
        assert_eq!(draft.leader_quote_xlm, Some(30.0));
        assert_eq!(draft.scaled_quote_xlm, Some(3.0));
        assert_eq!(
            draft.scaled_amounts_json,
            json!([
                {"token": "CA", "amount": "100"},
                {"token": "CB", "amount": "200"}
            ])
        );
    }

    #[test]
    fn build_scaled_op_payload_skips_claim_without_flag() {
        let body = json!({
            "topic": [{"type":"symbol","value":"claim_fees"}],
            "derived": {
                "token0": "CA",
                "amount0": "100",
                "fee_quote_xlm": 1.0
            }
        });
        assert!(build_scaled_op_payload(&body, "CPOOL", 0.5, false).is_none());
    }

    #[test]
    fn build_scaled_op_payload_skips_explicit_clmm_event() {
        let body = json!({
            "topic": [{"type":"symbol","value":"deposit_liquidity"}],
            "derived": {
                "pool_type": "concentrated",
                "token_amounts": [{"token": "CA", "amount": "100"}],
                "total_quote_xlm": 1.0
            }
        });
        assert!(build_scaled_op_payload(&body, "CPOOL", 0.1, false).is_none());
    }
}
