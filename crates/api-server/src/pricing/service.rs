use crate::pricing::asset_id::{resolve_freighter_asset_id, FreighterAssetId, NATIVE_SAC_MAINNET};
use crate::pricing::value::{coverage_for, QuoteCoverage, UsdPriceMap};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::time::{Duration, Instant};

const FREIGHTER_URL: &str =
    "https://freighter-backend-v2.stellar.org/api/v1/token-prices?network=PUBLIC";
const EXPERT_ASSET_URL: &str = "https://api.stellar.expert/explorer/public/asset";
const CACHE_TTL: Duration = Duration::from_secs(90);
const USER_AGENT: &str = "lumenlp-api/0.1";

#[derive(Debug, Clone)]
pub struct QuoteMeta {
    pub currency: &'static str,
    pub as_of: String,
    pub source: String,
    pub xlm_usd: Option<f64>,
    pub coverage: String,
}

struct CacheEntry {
    prices_by_freighter_key: HashMap<String, f64>,
    fetched_at: Instant,
}

pub struct PriceService {
    client: reqwest::Client,
    cache: Mutex<Option<CacheEntry>>,
}

impl PriceService {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(8))
                .user_agent(USER_AGENT)
                .build()
                .expect("reqwest"),
            cache: Mutex::new(None),
        }
    }

    /// `wanted`: list of (contract, symbol, name, issuer)
    pub async fn prices_for_tokens(
        &self,
        wanted: &[(String, Option<String>, Option<String>, Option<String>)],
    ) -> (UsdPriceMap, QuoteMeta) {
        let mut resolved: Vec<(String, FreighterAssetId)> = Vec::new();
        for (contract, symbol, name, issuer) in wanted {
            if let Some(id) = resolve_freighter_asset_id(
                contract,
                symbol.as_deref(),
                name.as_deref(),
                issuer.as_deref(),
            ) {
                resolved.push((contract.clone(), id));
            }
        }

        let needed_keys: HashSet<String> = resolved
            .iter()
            .map(|(_, id)| id.as_freighter_key())
            .collect();

        let mut freighter_sourced: HashSet<String> = HashSet::new();
        let mut expert_sourced: HashSet<String> = HashSet::new();

        let cache_snapshot = {
            let guard = self.cache.lock().expect("price cache lock");
            guard.as_ref().map(|e| {
                (
                    e.prices_by_freighter_key.clone(),
                    e.fetched_at.elapsed() < CACHE_TTL,
                )
            })
        };

        let cache_covers = cache_snapshot
            .as_ref()
            .map(|(prices, fresh)| *fresh && needed_keys.iter().all(|k| prices.contains_key(k)))
            .unwrap_or(false);

        let mut prices_by_key: HashMap<String, f64> = if cache_covers {
            let prices = cache_snapshot.expect("checked").0;
            freighter_sourced.extend(needed_keys.iter().cloned());
            prices
        } else {
            let mut tokens: Vec<String> = needed_keys.iter().cloned().collect();
            if !tokens.iter().any(|t| t == "native") {
                tokens.push("native".to_string());
            }
            tokens.sort();
            tokens.dedup();

            match self.fetch_freighter(&tokens).await {
                Ok(fetched) => {
                    let mut guard = self.cache.lock().expect("price cache lock");
                    let mut merged = guard
                        .as_ref()
                        .map(|e| e.prices_by_freighter_key.clone())
                        .unwrap_or_default();
                    for (k, v) in &fetched {
                        merged.insert(k.clone(), *v);
                        freighter_sourced.insert(k.clone());
                    }
                    *guard = Some(CacheEntry {
                        prices_by_freighter_key: merged.clone(),
                        fetched_at: Instant::now(),
                    });
                    merged
                }
                Err(_) => {
                    // Keep stale Freighter cache on HTTP failure when available.
                    let guard = self.cache.lock().expect("price cache lock");
                    let stale = guard
                        .as_ref()
                        .map(|e| e.prices_by_freighter_key.clone())
                        .unwrap_or_default();
                    freighter_sourced.extend(stale.keys().cloned());
                    stale
                }
            }
        };

        // Keep pool-list requests bounded: the bulk price response is enough for
        // the current XLM-quote valuation path. Per-asset Expert fallbacks turn
        // one request into dozens of serial network calls and make `/v1/pools`
        // slow whenever the token set is large.
        if !prices_by_key
            .get("native")
            .is_some_and(|p| p.is_finite() && *p > 0.0)
        {
            if let Some(price) = self.fetch_expert("XLM").await {
                prices_by_key.insert("native".to_string(), price);
                expert_sourced.insert("native".to_string());
                freighter_sourced.remove("native");
            }
        }

        let mut map = UsdPriceMap::new();
        let mut used_freighter = false;
        let mut used_expert = false;
        for (contract, id) in &resolved {
            let key = id.as_freighter_key();
            let Some(&price) = prices_by_key.get(&key) else {
                continue;
            };
            if !price.is_finite() || price <= 0.0 {
                continue;
            }
            map.insert(contract.clone(), price);
            if expert_sourced.contains(&key) {
                used_expert = true;
            } else {
                used_freighter = true;
            }
        }

        let source = match (used_freighter, used_expert) {
            (true, true) => "mixed",
            (true, false) => "freighter",
            (false, true) => "stellar_expert",
            (false, false) => "none",
        }
        .to_string();

        let contracts: Vec<String> = wanted.iter().map(|(c, _, _, _)| c.clone()).collect();
        let coverage = match coverage_for(&contracts, &map) {
            QuoteCoverage::Full => "full",
            QuoteCoverage::Partial => "partial",
            QuoteCoverage::None => "none",
        }
        .to_string();

        let xlm_usd = prices_by_key
            .get("native")
            .copied()
            .filter(|p| p.is_finite() && *p > 0.0);

        let meta = QuoteMeta {
            currency: "USD",
            as_of: chrono::Utc::now().to_rfc3339(),
            source,
            xlm_usd,
            coverage,
        };
        (map, meta)
    }

    async fn fetch_freighter(&self, tokens: &[String]) -> Result<HashMap<String, f64>, ()> {
        let body = serde_json::json!({ "tokens": tokens });
        let resp = self
            .client
            .post(FREIGHTER_URL)
            .json(&body)
            .send()
            .await
            .map_err(|_| ())?;
        if !resp.status().is_success() {
            return Err(());
        }
        let env: FreighterEnvelope = resp.json().await.map_err(|_| ())?;
        Ok(parse_freighter_prices(&env))
    }

    async fn fetch_expert(&self, expert_key: &str) -> Option<f64> {
        let url = format!("{EXPERT_ASSET_URL}/{expert_key}");
        let resp = self.client.get(&url).send().await.ok()?;
        if !resp.status().is_success() {
            return None;
        }
        let asset: ExpertAsset = resp.json().await.ok()?;
        asset.price.filter(|p| p.is_finite() && *p > 0.0)
    }
}

#[derive(Debug, Deserialize)]
struct FreighterEnvelope {
    data: HashMap<String, Option<FreighterPrice>>,
}

#[derive(Debug, Deserialize)]
struct FreighterPrice {
    #[serde(rename = "currentPrice")]
    current_price: String,
}

#[derive(Debug, Deserialize)]
struct ExpertAsset {
    price: Option<f64>,
}

fn parse_freighter_prices(env: &FreighterEnvelope) -> HashMap<String, f64> {
    let mut out = HashMap::new();
    for (key, maybe) in &env.data {
        let Some(entry) = maybe else {
            continue;
        };
        let Ok(price) = entry.current_price.parse::<f64>() else {
            continue;
        };
        if price.is_finite() && price > 0.0 {
            out.insert(key.clone(), price);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_freighter_fixture() {
        let raw = r#"{"data":{"native":{"currentPrice":"0.17","percentagePriceChange24h":"0"}}}"#;
        let env: FreighterEnvelope = serde_json::from_str(raw).unwrap();
        assert!(
            (env.data["native"]
                .as_ref()
                .unwrap()
                .current_price
                .parse::<f64>()
                .unwrap()
                - 0.17)
                .abs()
                < 1e-9
        );
        let prices = parse_freighter_prices(&env);
        assert!((prices["native"] - 0.17).abs() < 1e-9);
    }

    #[tokio::test]
    #[ignore]
    async fn live_freighter_smoke() {
        let svc = PriceService::new();
        let wanted = vec![
            (
                NATIVE_SAC_MAINNET.to_string(),
                Some("native".into()),
                Some("native".into()),
                None,
            ),
            (
                "CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75".into(),
                Some("USDC".into()),
                Some("USDC".into()),
                Some("GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN".into()),
            ),
        ];
        let (map, meta) = svc.prices_for_tokens(&wanted).await;
        assert!(
            meta.xlm_usd.is_some_and(|p| p > 0.0),
            "xlm_usd={:?}",
            meta.xlm_usd
        );
        assert!(
            map.contains_key(NATIVE_SAC_MAINNET),
            "map keys={:?}",
            map.keys().collect::<Vec<_>>()
        );
        assert!(
            map.values().any(|p| *p > 0.0),
            "expected at least one positive price"
        );
    }
}
