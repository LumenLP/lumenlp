//! Canonical source-event payloads for the future on-chain EVENT_RECORDER.
//!
//! This module deliberately stops at a durable, idempotent payload boundary.
//! It never holds a signing key or submits a transaction.

use {crate::index_db::PoolEventRow, serde_json::Value};

const STROOPS_PER_XLM: f64 = 10_000_000.0;

#[derive(Debug, Clone, PartialEq)]
pub struct RecorderEvent {
    pub source_event_id: String,
    pub leader_address: String,
    pub pool_address: String,
    pub kind: String,
    pub claim_token: Option<String>,
    pub amounts: Vec<u128>,
    pub quote_stroops: i128,
    pub ledger: u32,
    pub created_at: i64,
}

/// Encode an indexed event ID for the Soroban `BytesN<32>` replay key.
/// Stellar ledger-operation IDs currently fit in 32 bytes; zero padding keeps
/// the mapping deterministic without changing the human-readable DB key.
pub fn source_event_id_bytes(source_event_id: &str) -> Option<[u8; 32]> {
    if source_event_id.len() > 32 || !source_event_id.is_ascii() {
        return None;
    }
    let mut encoded = [0u8; 32];
    encoded[..source_event_id.len()].copy_from_slice(source_event_id.as_bytes());
    Some(encoded)
}

/// Convert an indexed LP event into the exact payload shape expected by the
/// Soroban recorder. Events without complete integer amounts or quote data are
/// withheld rather than recorded with invented values.
pub fn canonical_event(event: &PoolEventRow, leader_address: &str) -> Option<RecorderEvent> {
    // The contract recorder uses BytesN<32> as its replay key. Reject an
    // event before building a payload that cannot be represented on-chain.
    source_event_id_bytes(&event.event_id)?;
    let kind = match event.kind.as_str() {
        "deposit_liquidity" => "deposit",
        "withdraw_liquidity" => "withdraw",
        "claim_fees" | "claim_protocol_fee" => "claim",
        _ => return None,
    };
    let derived = event.body.get("derived")?;
    let amounts = derived
        .get("token_amounts")
        .and_then(parse_token_amounts)
        .or_else(|| claim_amounts(&event.kind, derived))?;
    let claim_token = if kind == "claim" {
        Some(claim_token(&event.kind, derived)?)
    } else {
        None
    };
    let quote = derived
        .get("total_quote_xlm")
        .or_else(|| derived.get("fee_quote_xlm"))
        .and_then(Value::as_f64)?;
    if !quote.is_finite() || quote <= 0.0 || quote > i128::MAX as f64 / STROOPS_PER_XLM {
        return None;
    }
    let quote_stroops = (quote * STROOPS_PER_XLM).floor() as i128;
    if quote_stroops <= 0 {
        return None;
    }
    Some(RecorderEvent {
        source_event_id: event.event_id.clone(),
        leader_address: leader_address.to_string(),
        pool_address: event.pool_address.clone(),
        kind: kind.to_string(),
        claim_token,
        amounts,
        quote_stroops,
        ledger: event.ledger,
        created_at: event.created_at,
    })
}

fn parse_token_amounts(value: &Value) -> Option<Vec<u128>> {
    let rows = value.as_array()?;
    if rows.is_empty() {
        return None;
    }
    rows.iter()
        .map(|row| row.get("amount")?.as_str()?.parse::<u128>().ok())
        .collect()
}

fn claim_amounts(kind: &str, derived: &Value) -> Option<Vec<u128>> {
    if kind == "claim_protocol_fee" {
        return Some(vec![derived.get("amount")?.as_str()?.parse().ok()?]);
    }
    let mut amounts = Vec::new();
    for key in ["amount0", "amount1"] {
        if let Some(amount) = derived.get(key).and_then(Value::as_str) {
            amounts.push(amount.parse::<u128>().ok()?);
        }
    }
    (!amounts.is_empty()).then_some(amounts)
}

fn claim_token(kind: &str, derived: &Value) -> Option<String> {
    if kind == "claim_protocol_fee" {
        return derived.get("token").and_then(Value::as_str).map(str::to_owned);
    }
    let mut tokens = Vec::new();
    for (token_key, amount_key) in [("token0", "amount0"), ("token1", "amount1")] {
        let amount = derived.get(amount_key).and_then(Value::as_str)?.parse::<u128>().ok()?;
        if amount > 0 {
            tokens.push(derived.get(token_key).and_then(Value::as_str)?.to_owned());
        }
    }
    (tokens.len() == 1).then(|| tokens.remove(0))
}

#[cfg(test)]
mod tests {
    use {super::*, serde_json::json};

    fn event(kind: &str, derived: Value) -> PoolEventRow {
        PoolEventRow {
            event_id: "evt-1".into(),
            tx_hash: Some("tx-1".into()),
            ledger: 123,
            created_at: 456,
            pool_address: "CPOOL".into(),
            kind: kind.into(),
            body: json!({"derived": derived}),
        }
    }

    #[test]
    fn canonicalizes_deposit_amounts_and_quote() {
        let row = canonical_event(
            &event(
                "deposit_liquidity",
                json!({
                    "token_amounts": [
                        {"token": "CA", "amount": "100"},
                        {"token": "CB", "amount": "200"}
                    ],
                    "total_quote_xlm": 12.9
                }),
            ),
            "GLEADER",
        )
        .unwrap();
        assert_eq!(row.kind, "deposit");
        assert_eq!(row.amounts, vec![100, 200]);
        assert_eq!(row.claim_token, None);
        assert_eq!(row.quote_stroops, 129_000_000);
        assert_eq!(row.ledger, 123);
    }

    #[test]
    fn rejects_missing_or_non_integer_amounts() {
        assert!(canonical_event(&event("deposit_liquidity", json!({"total_quote_xlm": 1.0})), "GLEADER").is_none());
        assert!(canonical_event(
            &event(
                "deposit_liquidity",
                json!({
                    "token_amounts": [{"token": "CA", "amount": "not-an-int"}],
                    "total_quote_xlm": 1.0
                }),
            ),
            "GLEADER"
        )
        .is_none());
    }

    #[test]
    fn rejects_event_ids_that_cannot_be_recorded_on_chain() {
        let mut row = event(
            "deposit_liquidity",
            json!({
                "token_amounts": [{"token": "CA", "amount": "100"}],
                "total_quote_xlm": 1.0
            }),
        );
        row.event_id = "x".repeat(33);
        assert!(canonical_event(&row, "GLEADER").is_none());
    }

    #[test]
    fn carries_a_single_claim_reward_token() {
        let row = canonical_event(
            &event(
                "claim_protocol_fee",
                json!({"token": "CREWARD", "amount": "12", "fee_quote_xlm": 1.0}),
            ),
            "GLEADER",
        )
        .unwrap();
        assert_eq!(row.claim_token.as_deref(), Some("CREWARD"));
    }

    #[test]
    fn rejects_multi_token_claims_without_a_single_reward_token() {
        let row = event(
            "claim_fees",
            json!({
                "token0": "CA", "amount0": "10",
                "token1": "CB", "amount1": "20",
                "fee_quote_xlm": 1.0
            }),
        );
        assert!(canonical_event(&row, "GLEADER").is_none());
    }

    #[test]
    fn source_event_id_encoding_is_deterministic_and_padded() {
        let encoded = source_event_id_bytes("ledger-op-1").unwrap();
        assert_eq!(&encoded[..11], b"ledger-op-1");
        assert!(encoded[11..].iter().all(|byte| *byte == 0));
        assert!(source_event_id_bytes(&"x".repeat(33)).is_none());
    }
}
