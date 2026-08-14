# USD Token Pricing Display Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show pool TVL, fees, volume, and event summaries in USD using Freighter (primary) + StellarExpert (fallback) per-token prices, so non-XLM pools no longer display money as `… XLM`.

**Architecture:** Add a pure asset-id + valuation module and an async `PriceService` inside `api-server`. Enrich `/v1/pools`, pool detail, and events with `*_usd` + `quote`. Frontend switches formatters/labels to `$`. Keep existing XLM fields for one release; indexer USD persistence is a follow-up.

**Tech Stack:** Rust (`api-server`, `reqwest`, `tokio`), Freighter + StellarExpert HTTP APIs, Next.js frontend (`fmtUsd`).

**Spec:** `docs/superpowers/specs/2026-08-04-usd-token-pricing-design.md`

---

## File map

| Path | Responsibility |
|------|----------------|
| `crates/api-server/src/pricing/asset_id.rs` | Map contract / meta → Freighter `native` \| `CODE:ISSUER` |
| `crates/api-server/src/pricing/value.rs` | Pure USD valuation helpers |
| `crates/api-server/src/pricing/service.rs` | Cached Freighter + StellarExpert client |
| `crates/api-server/src/pricing/mod.rs` | Module exports |
| `crates/api-server/src/handlers.rs` | Attach `quote` + `*_usd` on list/detail/events |
| `crates/api-server/src/main.rs` | Construct `PriceService`, put on `AppState` |
| `crates/api-server/Cargo.toml` | Add `reqwest` |
| `apps/web/src/lib/api.ts` | Types + `fmtUsd` |
| `apps/web/src/app/pools/page.tsx` | Show USD columns / card metrics |
| `apps/web/src/app/pools/view/page.tsx` | Hero + events use USD, not `XLM` |

---

### Task 1: Asset ID mapping (TDD)

**Files:**
- Create: `crates/api-server/src/pricing/asset_id.rs`
- Create: `crates/api-server/src/pricing/mod.rs`
- Modify: `crates/api-server/src/main.rs` (add `mod pricing;`)

- [ ] **Step 1: Add module stubs and failing tests in `asset_id.rs`**

```rust
//! Map Soroban contract IDs + token meta → Freighter token ids.

pub const NATIVE_SAC_MAINNET: &str =
    "CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FreighterAssetId {
    Native,
    Classic { code: String, issuer: String },
}

impl FreighterAssetId {
    pub fn as_freighter_key(&self) -> String {
        match self {
            Self::Native => "native".to_string(),
            Self::Classic { code, issuer } => format!("{code}:{issuer}"),
        }
    }

    pub fn as_stellar_expert_key(&self) -> Option<String> {
        match self {
            Self::Native => Some("XLM".to_string()),
            Self::Classic { code, issuer } => Some(format!("{code}-{issuer}")),
        }
    }
}

/// Resolve Freighter id from contract address, optional registry issuer, optional on-chain name.
pub fn resolve_freighter_asset_id(
    contract: &str,
    symbol: Option<&str>,
    name: Option<&str>,
    issuer: Option<&str>,
) -> Option<FreighterAssetId> {
    let _ = (contract, symbol, name, issuer);
    None // stub — tests drive implementation
}

fn parse_code_issuer(raw: &str) -> Option<(String, String)> {
    let (code, issuer) = raw.split_once(':')?;
    if code.is_empty() || issuer.len() < 56 {
        return None;
    }
    if !issuer.chars().all(|c| c.is_ascii_alphanumeric()) {
        return None;
    }
    Some((code.to_string(), issuer.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_sac_maps_to_native() {
        let id = resolve_freighter_asset_id(NATIVE_SAC_MAINNET, Some("native"), Some("native"), None)
            .expect("native");
        assert_eq!(id, FreighterAssetId::Native);
        assert_eq!(id.as_freighter_key(), "native");
    }

    #[test]
    fn classic_from_issuer_and_symbol() {
        let id = resolve_freighter_asset_id(
            "CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75",
            Some("USDC"),
            Some("USDC"),
            Some("GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN"),
        )
        .expect("usdc");
        assert_eq!(
            id.as_freighter_key(),
            "USDC:GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN"
        );
    }

    #[test]
    fn classic_from_name_code_issuer() {
        let id = resolve_freighter_asset_id(
            "CAUIKL3IYGMERDRUN6YSCLWVAKIFG5Q4YJHUKM4S4NJZQIA3BAS6OJPK",
            Some("AQUA"),
            Some("AQUA:GBNZILSTVQZ4R7IKQDGHYGY2QXL5QOFJYQMXPKWRRM5PAV7Y4M67AQUA"),
            None,
        )
        .expect("aqua");
        assert_eq!(
            id.as_freighter_key(),
            "AQUA:GBNZILSTVQZ4R7IKQDGHYGY2QXL5QOFJYQMXPKWRRM5PAV7Y4M67AQUA"
        );
    }

    #[test]
    fn unmapped_sep41_without_issuer_is_none() {
        assert!(resolve_freighter_asset_id(
            "CBIJBDNZNF4X35BJ4FFZWCDBSCKOP5NB4PLG4SNENRMLAPYG4P5FM6VN",
            Some("SolvBTC"),
            Some("Solv BTC"),
            None,
        )
        .is_none());
    }
}
```

Also create `mod.rs`:

```rust
pub mod asset_id;
pub mod value;
// service added in Task 3
```

Wire in `main.rs`: `mod pricing;`

- [ ] **Step 2: Run tests — expect FAIL**

```bash
cargo test -p api-server asset_id -- --nocapture
```

Expected: FAIL (`None` / expect native)

- [ ] **Step 3: Implement `resolve_freighter_asset_id`**

```rust
pub fn resolve_freighter_asset_id(
    contract: &str,
    symbol: Option<&str>,
    name: Option<&str>,
    issuer: Option<&str>,
) -> Option<FreighterAssetId> {
    if contract.eq_ignore_ascii_case(NATIVE_SAC_MAINNET)
        || symbol.is_some_and(|s| s.eq_ignore_ascii_case("native"))
        || name.is_some_and(|n| n.eq_ignore_ascii_case("native"))
    {
        return Some(FreighterAssetId::Native);
    }
    if let Some(iss) = issuer.map(str::trim).filter(|s| !s.is_empty()) {
        let code = symbol
            .map(str::trim)
            .filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case("native"))
            .or_else(|| {
                name.and_then(|n| parse_code_issuer(n).map(|(c, _)| {
                    // keep owned via outer
                    Some(c)
                }).flatten()
            });
        // simpler: prefer symbol, else parse name
        let code = if let Some(s) = symbol.map(str::trim).filter(|s| !s.is_empty()) {
            s.to_string()
        } else if let Some(n) = name {
            parse_code_issuer(n)?.0
        } else {
            return None;
        };
        return Some(FreighterAssetId::Classic {
            code,
            issuer: iss.to_string(),
        });
    }
    name.and_then(parse_code_issuer).map(|(code, issuer)| {
        FreighterAssetId::Classic { code, issuer }
    })
}
```

Clean up the messy `code` block above when implementing — final logic:

1. native SAC / symbol/name `native` → `Native`
2. `issuer` + `symbol` → `Classic`
3. else parse `name` as `CODE:ISSUER`
4. else `None`

- [ ] **Step 4: Run tests — expect PASS**

```bash
cargo test -p api-server asset_id -- --nocapture
```

- [ ] **Step 5: Commit**

```bash
git add crates/api-server/src/pricing crates/api-server/src/main.rs
git commit -m "$(cat <<'EOF'
feat(api): map Soroban tokens to Freighter asset ids

EOF
)"
```

---

### Task 2: Pure USD valuation helpers (TDD)

**Files:**
- Create: `crates/api-server/src/pricing/value.rs`

- [ ] **Step 1: Write failing tests + stubs**

```rust
use std::collections::HashMap;

/// Prices keyed by contract address (C…) → USD per human token unit.
pub type UsdPriceMap = HashMap<String, f64>;

pub fn amount_to_usd(human_amount: f64, usd_price: f64) -> Option<f64> {
    let _ = (human_amount, usd_price);
    None
}

/// Sum reserve_i * price_i. Returns None if any listed token lacks a price.
pub fn tvl_usd(tokens: &[String], human_reserves: &[f64], prices: &UsdPriceMap) -> Option<f64> {
    let _ = (tokens, human_reserves, prices);
    None
}

pub fn xlm_quote_to_usd(xlm_amount: f64, xlm_usd: f64) -> Option<f64> {
    let _ = (xlm_amount, xlm_usd);
    None
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuoteCoverage {
    Full,
    Partial,
    None,
}

pub fn coverage_for(tokens: &[String], prices: &UsdPriceMap) -> QuoteCoverage {
    let _ = (tokens, prices);
    QuoteCoverage::None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn amount_to_usd_basic() {
        assert!((amount_to_usd(100.0, 0.5).unwrap() - 50.0).abs() < 1e-9);
    }

    #[test]
    fn tvl_requires_all_legs() {
        let mut prices = UsdPriceMap::new();
        prices.insert("A".into(), 1.0);
        assert!(tvl_usd(&["A".into(), "B".into()], &[10.0, 20.0], &prices).is_none());
        prices.insert("B".into(), 2.0);
        assert!((tvl_usd(&["A".into(), "B".into()], &[10.0, 20.0], &prices).unwrap() - 50.0).abs() < 1e-9);
    }

    #[test]
    fn bridge_xlm_quote() {
        assert!((xlm_quote_to_usd(10.0, 0.2).unwrap() - 2.0).abs() < 1e-9);
    }

    #[test]
    fn coverage_levels() {
        let mut prices = UsdPriceMap::new();
        prices.insert("A".into(), 1.0);
        assert_eq!(coverage_for(&["A".into(), "B".into()], &prices), QuoteCoverage::Partial);
        prices.insert("B".into(), 1.0);
        assert_eq!(coverage_for(&["A".into(), "B".into()], &prices), QuoteCoverage::Full);
        assert_eq!(coverage_for(&["C".into()], &UsdPriceMap::new()), QuoteCoverage::None);
    }
}
```

- [ ] **Step 2: Run — expect FAIL**

```bash
cargo test -p api-server value -- --nocapture
```

- [ ] **Step 3: Implement**

```rust
pub fn amount_to_usd(human_amount: f64, usd_price: f64) -> Option<f64> {
    if !human_amount.is_finite() || !usd_price.is_finite() || usd_price <= 0.0 {
        return None;
    }
    Some(human_amount * usd_price)
}

pub fn tvl_usd(tokens: &[String], human_reserves: &[f64], prices: &UsdPriceMap) -> Option<f64> {
    if tokens.len() != human_reserves.len() || tokens.is_empty() {
        return None;
    }
    let mut sum = 0.0;
    for (token, amt) in tokens.iter().zip(human_reserves.iter()) {
        let price = prices.get(token).copied().filter(|p| p.is_finite() && *p > 0.0)?;
        if !amt.is_finite() {
            return None;
        }
        sum += amt * price;
    }
    Some(sum)
}

pub fn xlm_quote_to_usd(xlm_amount: f64, xlm_usd: f64) -> Option<f64> {
    amount_to_usd(xlm_amount, xlm_usd)
}

pub fn coverage_for(tokens: &[String], prices: &UsdPriceMap) -> QuoteCoverage {
    if tokens.is_empty() {
        return QuoteCoverage::None;
    }
    let priced = tokens
        .iter()
        .filter(|t| prices.get(t.as_str()).is_some_and(|p| p.is_finite() && *p > 0.0))
        .count();
    if priced == 0 {
        QuoteCoverage::None
    } else if priced == tokens.len() {
        QuoteCoverage::Full
    } else {
        QuoteCoverage::Partial
    }
}
```

- [ ] **Step 4: Run — expect PASS**

```bash
cargo test -p api-server value -- --nocapture
```

- [ ] **Step 5: Commit**

```bash
git add crates/api-server/src/pricing/value.rs
git commit -m "$(cat <<'EOF'
feat(api): add pure USD valuation helpers

EOF
)"
```

---

### Task 3: PriceService (Freighter + StellarExpert + cache)

**Files:**
- Create: `crates/api-server/src/pricing/service.rs`
- Modify: `crates/api-server/src/pricing/mod.rs`
- Modify: `crates/api-server/Cargo.toml` — add `reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }`

- [ ] **Step 1: Add dependency**

```toml
reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }
```

- [ ] **Step 2: Implement `PriceService`**

```rust
use crate::pricing::asset_id::{resolve_freighter_asset_id, FreighterAssetId};
use crate::pricing::value::UsdPriceMap;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const FREIGHTER_URL: &str =
    "https://freighter-backend-v2.stellar.org/api/v1/token-prices?network=PUBLIC";
const EXPERT_ASSET_URL: &str = "https://api.stellar.expert/explorer/public/asset";
const CACHE_TTL: Duration = Duration::from_secs(90);

#[derive(Debug, Clone)]
pub struct QuoteMeta {
    pub currency: &'static str, // "USD"
    pub as_of: String,          // RFC3339
    pub source: String,         // freighter | stellar_expert | mixed | none
    pub xlm_usd: Option<f64>,
    pub coverage: String, // full | partial | none — set by caller often
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
        // 1) resolve freighter keys
        // 2) if cache fresh and covers keys, use it
        // 3) else POST Freighter with unique keys (always include "native")
        // 4) for missing keys, GET StellarExpert per asset
        // 5) map freighter key → contract addresses in UsdPriceMap
        // 6) return QuoteMeta with xlm_usd from "native"
        todo!()
    }
}

#[derive(Deserialize)]
struct FreighterEnvelope {
    data: HashMap<String, Option<FreighterPrice>>,
}

#[derive(Deserialize)]
struct FreighterPrice {
    #[serde(rename = "currentPrice")]
    current_price: String,
}

#[derive(Deserialize)]
struct ExpertAsset {
    price: Option<f64>,
}
```

Implementation details for the agent:

- Batch Freighter keys; on HTTP error, keep stale cache if any
- Parse `current_price` with `str::parse::<f64>()`
- Expert fallback URL: `{EXPERT_ASSET_URL}/{CODE}-{ISSUER}` (no trailing `-1` required; Expert accepts)
- Native expert key `XLM` if Freighter native missing
- `as_of`: `chrono::Utc::now().to_rfc3339()`

- [ ] **Step 3: Manual smoke (optional while implementing)**

```bash
# After wiring a tiny bin or temporary test with #[tokio::test]:
# fetch native + USDC + AQUA and print prices
```

Add one `#[tokio::test]` marked `#[ignore]` for live Freighter, plus a unit test that parses a fixture JSON string for Freighter envelope.

```rust
#[test]
fn parse_freighter_fixture() {
    let raw = r#"{"data":{"native":{"currentPrice":"0.17","percentagePriceChange24h":"0"}}}"#;
    let env: FreighterEnvelope = serde_json::from_str(raw).unwrap();
    assert!((env.data["native"].as_ref().unwrap().current_price.parse::<f64>().unwrap() - 0.17).abs() < 1e-9);
}
```

- [ ] **Step 4: `cargo test -p api-server`**

Expected: unit tests pass; ignored live test skipped

- [ ] **Step 5: Commit**

```bash
git add crates/api-server/Cargo.toml crates/api-server/src/pricing
git commit -m "$(cat <<'EOF'
feat(api): add Freighter/StellarExpert price service with cache

EOF
)"
```

---

### Task 4: Wire USD fields into pool list + detail

**Files:**
- Modify: `crates/api-server/src/main.rs`
- Modify: `crates/api-server/src/handlers.rs`

- [ ] **Step 1: Put `PriceService` on `AppState`**

```rust
pub struct AppState {
    // existing fields…
    pub prices: Arc<PriceService>,
}
```

In `main.rs`: `let prices = Arc::new(PriceService::new());`

- [ ] **Step 2: Helper to build quote JSON**

```rust
fn quote_json(meta: &QuoteMeta, coverage: &str) -> serde_json::Value {
    json!({
        "currency": "USD",
        "as_of": meta.as_of,
        "source": meta.source,
        "xlm_usd": meta.xlm_usd,
        "coverage": coverage,
    })
}
```

- [ ] **Step 3: In `list_pools`, after resolving `token_meta_map`:**

1. Collect unique tokens across pools with (contract, symbol, name, issuer)
2. `let (price_map, quote_meta) = state.prices.prices_for_tokens(&wanted).await;`
3. For each pool object:
   - Parse latest snapshot `reserves` into human units if available (existing reserves in snapshot JSON — check `reserves_json` / pool row fields already exposed). If human reserves + full coverage: `tvl_usd = tvl_usd(tokens, reserves, &price_map)`
   - Else if `tvl` (XLM) and `xlm_usd`: `tvl_usd = xlm_quote_to_usd(tvl, xlm_usd)` and note source may be `xlm_bridge` when token coverage incomplete
   - For each window in `window_metrics`: set `volume_usd` / `fee_usd` via `xlm_quote_to_usd` when XLM metrics exist and `xlm_usd` known (v1 rollup bridge — document in quote.source as `mixed` if TVL is token-priced but volume bridged)
4. Insert top-level `"quote"` once per response: `json!({ "pools": …, "quote": quote_json(...), …})` **and** optionally per-pool `"tvl_usd"`

Per-pool fields to add:

```json
"tvl_usd": 12345.6,
"window_metrics": {
  "24h": {
    "volume": …,
    "volume_usd": …,
    "fee": …,
    "fee_usd": …,
    …
  }
}
```

- [ ] **Step 4: Same enrichment in `pool_detail`**

Also map `activity_summary` fields:

- `volume_quote_24h` → `volume_usd_24h` (bridge via xlm_usd in v1 if only XLM summary exists)
- `fee_quote_24h` → `fee_usd_24h`
- `net_liquidity_delta_quote_24h` → `net_liquidity_delta_usd_24h`
- `claim_quote_24h` → `claim_usd_24h`

- [ ] **Step 5: Score — when `coverage == full` and `tvl_usd` / `volume_usd` available, pass USD into `pool_score_json` by temporarily using USD numbers as `tvl` / window volume inputs** (same formula). If not full, keep existing XLM score path.

Minimal change: compute score using `tvl_usd.unwrap_or(tvl)` and window volume_usd when present.

- [ ] **Step 6: Build + smoke**

```bash
cargo build -p api-server --release
# run against local DB if available, curl /v1/pools | jq '.[0] | {tvl,tvl_usd,quote}' 
# or jq '.pools[0] | {tvl,tvl_usd}, .quote'
```

- [ ] **Step 7: Commit**

```bash
git add crates/api-server/src/main.rs crates/api-server/src/handlers.rs
git commit -m "$(cat <<'EOF'
feat(api): expose tvl_usd and window fee/volume USD on pools

EOF
)"
```

---

### Task 5: Event USD enrichment

**Files:**
- Modify: `crates/api-server/src/handlers.rs` (`pool_events` + event JSON builder)

- [ ] **Step 1: When serving events for a pool, resolve token prices for that pool’s tokens**

- [ ] **Step 2: For each event `body.derived`:**

If raw amounts exist (`amount_in`, `fee_amount`, reserve amounts) + token addresses + decimals known:

- Prefer `*_quote_usd = human(amount) * usd_price(token)`

Else if only `*_quote_xlm` exists and `xlm_usd` known:

- `*_quote_usd = xlm * xlm_usd` with understanding this is bridge

Always attach when possible:

```json
"derived": {
  "volume_quote_xlm": …,
  "volume_quote_usd": …,
  "fee_quote_xlm": …,
  "fee_quote_usd": …,
  "reserves_quote_xlm": …,
  "reserves_quote_usd": …,
  "total_quote_xlm": …,
  "total_quote_usd": …
}
```

- [ ] **Step 3: Commit**

```bash
git add crates/api-server/src/handlers.rs
git commit -m "$(cat <<'EOF'
feat(api): add derived *_quote_usd on pool events

EOF
)"
```

---

### Task 6: Frontend `fmtUsd` + types

**Files:**
- Modify: `apps/web/src/lib/api.ts`

- [ ] **Step 1: Extend types**

```ts
export type QuoteInfo = {
  currency: string;
  as_of?: string;
  source?: string;
  xlm_usd?: number | null;
  coverage?: "full" | "partial" | "none" | string;
};

// On PoolRow / PoolDetailResponse:
tvl_usd?: number | null;
quote?: QuoteInfo;
// window_metrics values:
volume_usd?: number;
fee_usd?: number;
```

Activity summary optional `*_usd_24h` fields.

- [ ] **Step 2: Add formatter**

```ts
export function fmtUsd(n: number | null | undefined, digits = 2) {
  if (n == null || Number.isNaN(n)) return "—";
  return n.toLocaleString(undefined, {
    style: "currency",
    currency: "USD",
    maximumFractionDigits: digits,
  });
}
```

- [ ] **Step 3: Prefer USD helper**

```ts
export function pickUsd(usd: number | null | undefined, xlm: number | null | undefined, xlmUsd?: number | null) {
  if (usd != null && Number.isFinite(usd)) return { value: usd, kind: "usd" as const };
  if (xlm != null && xlmUsd != null && Number.isFinite(xlm) && xlmUsd > 0) {
    return { value: xlm * xlmUsd, kind: "usd" as const };
  }
  return { value: null, kind: "none" as const };
}
```

- [ ] **Step 4: Commit**

```bash
git add apps/web/src/lib/api.ts
git commit -m "$(cat <<'EOF'
feat(web): add fmtUsd and quote types for USD display

EOF
)"
```

---

### Task 7: Pools list UI → USD

**Files:**
- Modify: `apps/web/src/app/pools/page.tsx`

- [ ] **Step 1: Store response-level `quote` from `fetchPools` if returned**

Update `fetchPools` in `api.ts` to retain `quote` on the list response type.

- [ ] **Step 2: Replace labels**

- Table header `TVL XLM est.` → `TVL`
- `Fee XLM est.` → `Fee`
- Cell values: `fmtUsd(p.tvl_usd ?? pickUsd(...).value)` and fee from `window_metrics[w].fee_usd`

- [ ] **Step 3: Card metrics**

Liquidity / Fee rows use `fmtUsd`.

- [ ] **Step 4: Manual check in browser against AQUA/USDC pool cards

- [ ] **Step 5: Commit**

```bash
git add apps/web/src/app/pools/page.tsx apps/web/src/lib/api.ts
git commit -m "$(cat <<'EOF'
feat(web): show pool list liquidity and fees in USD

EOF
)"
```

---

### Task 8: Pool detail + event summaries → USD

**Files:**
- Modify: `apps/web/src/app/pools/view/page.tsx`

- [ ] **Step 1: Hero / stats use `tvl_usd`, `fee_usd`, activity `*_usd_24h`**

- [ ] **Step 2: Replace event summary builders**

Change helpers that currently append `XLM`:

```ts
// before
`· vol ${fmtNum(…, 2)} XLM`
// after — prefer usd
derived.volume_quote_usd != null
  ? `· vol ${fmtUsd(numValue(derived.volume_quote_usd))}`
  : derived.volume_quote_xlm != null && xlmUsd
    ? `· vol ${fmtUsd(numValue(derived.volume_quote_xlm) * xlmUsd)}`
    : null;
```

Same for reserves / claim fees / `fmtMaybeQuote` → `fmtMaybeUsd` using `*_quote_usd` first.

- [ ] **Step 3: Table columns that show `… XLM` switch to USD**

- [ ] **Step 4: Verify on**

`https://lumenlp.xyz/pools/view?address=CBRUQ7I6C6OGHMDYWD6XQUZFB6KJ3LLPNE34EPKSPFZ2YMBJ2GIWYYZ7`  
(after deploy) — event feed must not say `XLM` for money amounts when USD resolves.

- [ ] **Step 5: Commit**

```bash
git add apps/web/src/app/pools/view/page.tsx
git commit -m "$(cat <<'EOF'
feat(web): show pool detail and events in USD

EOF
)"
```

---

### Task 9: Deploy + verify

- [ ] **Step 1: Deploy API**

```bash
./deploy/deploy.sh
```

- [ ] **Step 2: Deploy site**

```bash
NEXT_PUBLIC_API_BASE=https://api.lumenlp.xyz ./deploy/deploy_site.sh
```

- [ ] **Step 3: Curl checks**

```bash
curl -sS 'https://api.lumenlp.xyz/v1/pools' | jq '.quote, .pools[0] | {pair: .token_meta, tvl, tvl_usd, fee_usd: .window_metrics["24h"].fee_usd}'
curl -sS 'https://api.lumenlp.xyz/v1/pools/CBRUQ7I6C6OGHMDYWD6XQUZFB6KJ3LLPNE34EPKSPFZ2YMBJ2GIWYYZ7/events?limit=3' \
  | jq '.events[0].body.derived | {volume_quote_xlm, volume_quote_usd, reserves_quote_usd}'
```

- [ ] **Step 4: Final commit only if deploy scripts / env example updated**

If env knobs added (`FREIGHTER_URL`, `PRICE_CACHE_SECS`), update `.env.example` and README pricing note.

```bash
git add .env.example README.md
git commit -m "$(cat <<'EOF'
docs: note USD pricing via Freighter/StellarExpert

EOF
)"
```

---

## Spec coverage checklist

| Spec item | Task |
|-----------|------|
| Freighter primary | Task 3 |
| StellarExpert fallback | Task 3 |
| `C…` → `CODE:ISSUER` / native | Task 1 |
| No symbol-only pricing | Task 1 tests |
| `tvl_usd` / window USD | Task 4 |
| Event `*_quote_usd` | Task 5 |
| UI `$` not `XLM` | Tasks 7–8 |
| Score USD when full coverage | Task 4 step 5 |
| Cache + degrade | Task 3 |
| Indexer persist USD | Out of scope (follow-up) |

## Notes for implementers

- v1 **window volume/fee USD** may bridge from existing XLM rollups (`× xlm_usd`) while **TVL USD** should prefer reserves × token prices when available. Event rows with token amounts should prefer true per-token USD.
- Native SAC constant must be mainnet `CAS3J7…OWMA` (not the older registry test id if mismatched).
- Do not remove `*_quote_xlm` fields yet.
