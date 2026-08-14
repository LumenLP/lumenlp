//! Minimal Soroban JSON-RPC client for read-only simulate calls.

use {
    anyhow::{anyhow, Result},
    reqwest::Client,
    serde_json::{json, Value},
    stellar_xdr::curr as xdr,
};

pub struct SorobanRpc {
    url: String,
    client: Client,
    network_passphrase: String,
}

#[derive(Debug, Clone)]
pub struct LatestLedger {
    pub sequence: u32,
}

#[derive(Debug, Clone)]
pub struct HealthInfo {
    pub latest_ledger: u32,
    pub oldest_ledger: u32,
    pub ledger_retention_window: u32,
}

impl SorobanRpc {
    pub fn new(url: &str, network_passphrase: &str) -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .pool_max_idle_per_host(10)
            .build()
            .unwrap_or_else(|_| Client::new());
        Self {
            url: url.to_string(),
            client,
            network_passphrase: network_passphrase.to_string(),
        }
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    pub fn network_passphrase(&self) -> &str {
        &self.network_passphrase
    }

    async fn post_json_with_retry(&self, body: Value) -> Result<Value> {
        const MAX_ATTEMPTS: usize = 5;
        let mut last_err = anyhow!("RPC request not attempted");
        for attempt in 1..=MAX_ATTEMPTS {
            let resp = match self.client.post(&self.url).json(&body).send().await {
                Ok(r) => r,
                Err(e) => {
                    last_err = anyhow!("RPC request failed: {e}");
                    if attempt < MAX_ATTEMPTS {
                        tokio::time::sleep(std::time::Duration::from_millis(200 * attempt as u64))
                            .await;
                    }
                    continue;
                }
            };
            let text = match resp.text().await {
                Ok(t) => t,
                Err(e) => {
                    last_err = anyhow!("RPC response read failed: {e}");
                    if attempt < MAX_ATTEMPTS {
                        tokio::time::sleep(std::time::Duration::from_millis(200 * attempt as u64))
                            .await;
                    }
                    continue;
                }
            };
            match serde_json::from_str::<Value>(&text) {
                Ok(j) => return Ok(j),
                Err(e) => {
                    last_err = anyhow!("RPC response parse failed: {e}");
                    if attempt < MAX_ATTEMPTS {
                        tokio::time::sleep(std::time::Duration::from_millis(200 * attempt as u64))
                            .await;
                    }
                }
            }
        }
        Err(last_err)
    }

    pub async fn simulate_call(
        &self,
        contract_address: &str,
        function_name: &str,
        args: Vec<xdr::ScVal>,
    ) -> Result<xdr::ScVal> {
        use stellar_xdr::curr::{Limits, ReadXdr, WriteXdr};

        let contract_hash = stellar_strkey::Contract::from_string(contract_address)
            .map_err(|e| anyhow!("Invalid contract address: {e:?}"))?
            .0;

        let invoke_args = xdr::InvokeContractArgs {
            contract_address: xdr::ScAddress::Contract(xdr::ContractId(xdr::Hash(contract_hash))),
            function_name: function_name
                .try_into()
                .map_err(|_| anyhow!("Invalid function name"))?,
            args: args.try_into().map_err(|_| anyhow!("Too many args"))?,
        };

        let op = xdr::Operation {
            source_account: None,
            body: xdr::OperationBody::InvokeHostFunction(xdr::InvokeHostFunctionOp {
                host_function: xdr::HostFunction::InvokeContract(invoke_args),
                auth: xdr::VecM::default(),
            }),
        };

        let tx = xdr::Transaction {
            source_account: xdr::MuxedAccount::Ed25519(xdr::Uint256([0u8; 32])),
            fee: 100,
            seq_num: xdr::SequenceNumber(0),
            cond: xdr::Preconditions::None,
            memo: xdr::Memo::None,
            operations: vec![op].try_into().map_err(|_| anyhow!("ops error"))?,
            ext: xdr::TransactionExt::V0,
        };

        let envelope = xdr::TransactionEnvelope::Tx(xdr::TransactionV1Envelope {
            tx,
            signatures: xdr::VecM::default(),
        });

        let tx_xdr = envelope
            .to_xdr_base64(Limits::none())
            .map_err(|e| anyhow!("XDR encode error: {e:?}"))?;

        let body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "simulateTransaction",
            "params": { "transaction": tx_xdr }
        });

        let resp_json = self.post_json_with_retry(body).await?;
        if let Some(error) = resp_json.get("error") {
            return Err(anyhow!("RPC error: {error}"));
        }
        let result = resp_json
            .get("result")
            .ok_or_else(|| anyhow!("No result in RPC response"))?;
        if let Some(error) = result.get("error") {
            return Err(anyhow!("Simulation error: {error}"));
        }
        let results = result
            .get("results")
            .and_then(|r| r.as_array())
            .ok_or_else(|| anyhow!("No results array"))?;
        if results.is_empty() {
            return Err(anyhow!("Empty results"));
        }
        let xdr_b64 = results[0]
            .get("xdr")
            .and_then(|x| x.as_str())
            .ok_or_else(|| anyhow!("No xdr in result"))?;
        xdr::ScVal::from_xdr_base64(xdr_b64, Limits::none())
            .map_err(|e| anyhow!("ScVal decode error: {e:?}"))
    }

    pub async fn call_no_args(&self, contract: &str, function: &str) -> Result<xdr::ScVal> {
        self.simulate_call(contract, function, vec![]).await
    }

    pub async fn get_latest_ledger(&self) -> Result<LatestLedger> {
        let body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getLatestLedger",
            "params": {}
        });
        let resp_json = self.post_json_with_retry(body).await?;
        if let Some(error) = resp_json.get("error") {
            return Err(anyhow!("RPC error: {error}"));
        }
        let result = resp_json
            .get("result")
            .ok_or_else(|| anyhow!("No result in getLatestLedger response"))?;
        let sequence = result
            .get("sequence")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| anyhow!("Missing sequence in getLatestLedger response"))?;
        Ok(LatestLedger {
            sequence: sequence as u32,
        })
    }

    /// Fetch contract events in `[start_ledger, end_ledger)` (end exclusive, per RPC).
    /// Follows `pagination.cursor` until the page is exhausted so hot pools are not truncated
    /// at the default/page limit.
    pub async fn get_events(
        &self,
        start_ledger: u32,
        end_ledger: Option<u32>,
        contract_ids: &[String],
        limit: u32,
    ) -> Result<Vec<Value>> {
        let limit = limit.clamp(1, 10_000);
        let filter = json!({
            "type": "contract",
            "contractIds": contract_ids,
        });
        let mut all = Vec::new();
        let mut cursor: Option<String> = None;
        // Hard cap guards against a runaway cursor loop on a buggy RPC.
        const MAX_EVENTS: usize = 500_000;
        loop {
            let pagination = match cursor.as_ref() {
                Some(c) => json!({ "cursor": c, "limit": limit }),
                None => json!({ "limit": limit }),
            };
            // Cursor pages must omit start/end ledger (RPC requirement).
            let params = if cursor.is_some() {
                json!({
                    "filters": [filter.clone()],
                    "pagination": pagination,
                })
            } else {
                json!({
                    "startLedger": start_ledger,
                    "endLedger": end_ledger,
                    "filters": [filter.clone()],
                    "pagination": pagination,
                })
            };
            let body = json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "getEvents",
                "params": params,
            });
            let resp_json = self.post_json_with_retry(body).await?;
            if let Some(error) = resp_json.get("error") {
                return Err(anyhow!("RPC error: {error}"));
            }
            let result = resp_json
                .get("result")
                .ok_or_else(|| anyhow!("No result in getEvents response"))?;
            let page = result
                .get("events")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            let next_cursor = result
                .get("cursor")
                .and_then(|v| v.as_str())
                .map(str::to_owned);
            let page_len = page.len();
            all.extend(page);
            let exhausted = page_len == 0
                || page_len < limit as usize
                || next_cursor.is_none()
                || all.len() >= MAX_EVENTS;
            if exhausted {
                break;
            }
            cursor = next_cursor;
        }
        Ok(all)
    }

    pub async fn get_transaction_source(&self, tx_hash: &str) -> Result<Option<String>> {
        let body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getTransaction",
            "params": {
                "hash": tx_hash,
                "xdrFormat": "json"
            }
        });
        let resp_json = self.post_json_with_retry(body).await?;
        if let Some(error) = resp_json.get("error") {
            return Err(anyhow!("RPC error: {error}"));
        }
        let result = resp_json
            .get("result")
            .ok_or_else(|| anyhow!("No result in getTransaction response"))?;
        if result.get("status").and_then(|v| v.as_str()) == Some("NOT_FOUND") {
            return Ok(None);
        }
        Ok(source_account_from_get_transaction(result))
    }

    pub async fn get_health(&self) -> Result<HealthInfo> {
        let body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getHealth",
        });
        let resp_json = self.post_json_with_retry(body).await?;
        if let Some(error) = resp_json.get("error") {
            return Err(anyhow!("RPC error: {error}"));
        }
        let result = resp_json
            .get("result")
            .ok_or_else(|| anyhow!("No result in getHealth response"))?;
        let latest_ledger = result
            .get("latestLedger")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| anyhow!("Missing latestLedger in getHealth response"))?;
        let oldest_ledger = result
            .get("oldestLedger")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| anyhow!("Missing oldestLedger in getHealth response"))?;
        let ledger_retention_window = result
            .get("ledgerRetentionWindow")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| anyhow!("Missing ledgerRetentionWindow in getHealth response"))?;
        Ok(HealthInfo {
            latest_ledger: latest_ledger as u32,
            oldest_ledger: oldest_ledger as u32,
            ledger_retention_window: ledger_retention_window as u32,
        })
    }
}

pub fn scval_to_u128(val: &xdr::ScVal) -> Result<u128> {
    match val {
        xdr::ScVal::U128(parts) => Ok(((parts.hi as u128) << 64) | (parts.lo as u128)),
        xdr::ScVal::I128(parts) => {
            let v = ((parts.hi as i128) << 64) | (parts.lo as u64 as i128);
            u128::try_from(v).map_err(|_| anyhow!("negative i128"))
        }
        _ => Err(anyhow!(
            "Expected U128, got {:?}",
            std::mem::discriminant(val)
        )),
    }
}

pub fn scval_to_i32(val: &xdr::ScVal) -> Result<i32> {
    match val {
        xdr::ScVal::I32(v) => Ok(*v),
        _ => Err(anyhow!("Expected I32")),
    }
}

pub fn parse_fee_bps_u32(val: &xdr::ScVal) -> Option<u32> {
    match val {
        xdr::ScVal::U32(v) => Some(*v),
        xdr::ScVal::I32(v) if *v >= 0 => Some(*v as u32),
        _ => scval_to_u128(val).ok().and_then(|v| u32::try_from(v).ok()),
    }
}

pub fn scval_to_address(val: &xdr::ScVal) -> Result<String> {
    match val {
        xdr::ScVal::Address(xdr::ScAddress::Contract(xdr::ContractId(xdr::Hash(hash)))) => {
            Ok(format!("{}", stellar_strkey::Contract(*hash)))
        }
        xdr::ScVal::Address(xdr::ScAddress::Account(xdr::AccountId(
            xdr::PublicKey::PublicKeyTypeEd25519(xdr::Uint256(key)),
        ))) => Ok(format!("{}", stellar_strkey::ed25519::PublicKey(*key))),
        _ => Err(anyhow!("Expected Address")),
    }
}

pub fn scval_to_symbol_string(val: &xdr::ScVal) -> Result<String> {
    match val {
        xdr::ScVal::Symbol(s) => Ok(s.to_string()),
        xdr::ScVal::String(s) => Ok(s.to_string()),
        _ => Err(anyhow!("Expected Symbol/String")),
    }
}

pub fn account_address_scval(g_address: &str) -> Result<xdr::ScVal> {
    let pk = stellar_strkey::ed25519::PublicKey::from_string(g_address)
        .map_err(|e| anyhow!("Invalid G-address: {e:?}"))?;
    Ok(xdr::ScVal::Address(xdr::ScAddress::Account(
        xdr::AccountId(xdr::PublicKey::PublicKeyTypeEd25519(xdr::Uint256(pk.0))),
    )))
}

pub fn parse_address_vec(val: &xdr::ScVal) -> Option<Vec<String>> {
    let xdr::ScVal::Vec(Some(vec)) = val else {
        return None;
    };
    let mut addrs = Vec::new();
    for item in vec.0.iter() {
        if let Ok(addr) = scval_to_address(item) {
            addrs.push(addr);
        }
    }
    if addrs.is_empty() {
        None
    } else {
        Some(addrs)
    }
}

/// Extract the Stellar account (`G…`) that submitted the tx from a
/// `getTransaction` JSON-RPC result (or Horizon-compatible envelope summary).
pub fn source_account_from_get_transaction(result: &Value) -> Option<String> {
    for key in ["account", "accountId", "sourceAccount", "envelopeSource"] {
        if let Some(addr) = result.get(key).and_then(g_address_from_json) {
            return Some(addr);
        }
    }

    if let Some(envelope) = result.get("envelopeJson") {
        for path in [
            "/tx/sourceAccount",
            "/tx/source_account",
            "/tx/tx/sourceAccount",
            "/tx/tx/source_account",
            "/tx_fee_bump/tx/inner_tx/tx/tx/sourceAccount",
            "/tx_fee_bump/tx/inner_tx/tx/tx/source_account",
            "/tx_fee_bump/tx/feeSource",
            "/tx_fee_bump/tx/fee_source",
        ] {
            if let Some(addr) = envelope.pointer(path).and_then(g_address_from_json) {
                return Some(addr);
            }
        }
    }

    result
        .get("envelopeXdr")
        .and_then(|v| v.as_str())
        .and_then(source_account_from_envelope_xdr)
}

fn g_address_from_json(value: &Value) -> Option<String> {
    value
        .as_str()
        .filter(|s| s.starts_with('G'))
        .map(str::to_owned)
        .or_else(|| {
            value
                .get("account_id")
                .or_else(|| value.get("accountId"))
                .and_then(|v| v.as_str())
                .filter(|s| s.starts_with('G'))
                .map(str::to_owned)
        })
}

fn source_account_from_envelope_xdr(xdr_b64: &str) -> Option<String> {
    use stellar_xdr::curr::{Limits, ReadXdr};

    let envelope = xdr::TransactionEnvelope::from_xdr_base64(xdr_b64, Limits::none()).ok()?;
    match envelope {
        xdr::TransactionEnvelope::Tx(v1) => muxed_account_to_g(&v1.tx.source_account),
        xdr::TransactionEnvelope::TxFeeBump(fb) => match &fb.tx.inner_tx {
            xdr::FeeBumpTransactionInnerTx::Tx(v1) => muxed_account_to_g(&v1.tx.source_account),
        },
        xdr::TransactionEnvelope::TxV0(v0) => Some(format!(
            "{}",
            stellar_strkey::ed25519::PublicKey(v0.tx.source_account_ed25519.0)
        )),
    }
}

fn muxed_account_to_g(account: &xdr::MuxedAccount) -> Option<String> {
    match account {
        xdr::MuxedAccount::Ed25519(key) => {
            Some(format!("{}", stellar_strkey::ed25519::PublicKey(key.0)))
        }
        xdr::MuxedAccount::MuxedEd25519(muxed) => Some(format!(
            "{}",
            stellar_strkey::ed25519::PublicKey(muxed.ed25519.0)
        )),
    }
}

pub fn parse_u128_vec(val: &xdr::ScVal) -> Option<Vec<u128>> {
    let xdr::ScVal::Vec(Some(vec)) = val else {
        return None;
    };
    let out: Vec<u128> = vec.0.iter().filter_map(|v| scval_to_u128(v).ok()).collect();
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Trimmed from mainnet `getTransaction` for tx
    /// `9993ec7870cec37db262eea109696cddb87526b6ebd3858f03a46ef5ab9b2972`
    /// (Aquarius router swap, `xdrFormat: json`).
    const MAINNET_GET_TX_FIXTURE: &str = r#"{
        "status": "SUCCESS",
        "txHash": "9993ec7870cec37db262eea109696cddb87526b6ebd3858f03a46ef5ab9b2972",
        "feeBump": false,
        "envelopeJson": {
            "tx": {
                "tx": {
                    "source_account": "GBS3LFM2PIMRGZUC65G2GNMSWTQIX3FYSKB7ZF62ZLPVLG7MDXFUHQ64"
                }
            }
        }
    }"#;

    #[test]
    fn source_account_from_mainnet_get_transaction_fixture() {
        let result: Value = serde_json::from_str(MAINNET_GET_TX_FIXTURE).unwrap();
        let account = source_account_from_get_transaction(&result).unwrap();
        assert!(account.starts_with('G'));
        assert_eq!(account, "GBS3LFM2PIMRGZUC65G2GNMSWTQIX3FYSKB7ZF62ZLPVLG7MDXFUHQ64");
    }

    #[test]
    fn source_account_prefers_top_level_account_field() {
        let result = json!({
            "account": "GCL5ZDPP4YWKBLFAYIQHZSHP63KHWPI6L4O2F7TQ5V27UKQDEWWKHZIU",
            "envelopeJson": {
                "tx": { "tx": { "source_account": "GBS3LFM2PIMRGZUC65G2GNMSWTQIX3FYSKB7ZF62ZLPVLG7MDXFUHQ64" } }
            }
        });
        assert_eq!(
            source_account_from_get_transaction(&result).as_deref(),
            Some("GCL5ZDPP4YWKBLFAYIQHZSHP63KHWPI6L4O2F7TQ5V27UKQDEWWKHZIU")
        );
    }

    #[test]
    fn source_account_from_fee_bump_inner_tx() {
        let result = json!({
            "status": "SUCCESS",
            "feeBump": true,
            "envelopeJson": {
                "tx_fee_bump": {
                    "tx": {
                        "fee_source": "GCMPFUGGGUEPSWP5ZI6NE7HOPVM2RYQHPM3KLDDP4RRBUI7BRL4IS77O",
                        "inner_tx": {
                            "tx": {
                                "tx": {
                                    "source_account": "GDM22NKY7E67YQ25HDWZFQCNN3XUITQDXAWC5RAIUTCEJUQMJRNW5DHK"
                                }
                            }
                        }
                    }
                }
            }
        });
        assert_eq!(
            source_account_from_get_transaction(&result).as_deref(),
            Some("GDM22NKY7E67YQ25HDWZFQCNN3XUITQDXAWC5RAIUTCEJUQMJRNW5DHK")
        );
    }

    /// envelopeXdr from the same mainnet tx as `MAINNET_GET_TX_FIXTURE`.
    const MAINNET_ENVELOPE_XDR: &str = "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";

    #[test]
    fn source_account_from_envelope_xdr_mainnet() {
        let result = json!({ "envelopeXdr": MAINNET_ENVELOPE_XDR });
        let account = source_account_from_get_transaction(&result).unwrap();
        assert!(account.starts_with('G'));
        assert_eq!(account, "GBS3LFM2PIMRGZUC65G2GNMSWTQIX3FYSKB7ZF62ZLPVLG7MDXFUHQ64");
    }
}
