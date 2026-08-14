# USD Token Pricing Display — Design Spec

**Date:** 2026-08-04  
**Status:** Approved for implementation planning  
**Product:** LumenLP (`lumenlp.xyz`)  
**Motivation:** Non-XLM pools (e.g. AQUA/USDC) currently show event volume / reserves / fees as `… XLM`, which reads as if the pool contains XLM. Align display with lpagent-style `$` UX using third-party USD token prices.

## Goal

Make **user-facing money amounts** denominated in **USD**, by pricing each token in USD (not by labeling an XLM-valued quote as if it were pool-native).

Internal XLM quote math may remain as a fallback; the default product surface must not show `XLM` as the unit for pool TVL, volume, fees, or event summaries.

## Decisions (locked)

| Topic | Choice |
|-------|--------|
| Approach | **Path 2** — per-token USD prices → revalue display metrics |
| Not chosen | Path 1 only (`value_xlm × xlm_usd`) as the long-term display story |
| Primary price API | Freighter backend `POST /api/v1/token-prices?network=PUBLIC` |
| Fallback price API | StellarExpert `GET /explorer/public/asset/{CODE-ISSUER}` |
| Auth (Freighter) | Anonymous for v1; design must tolerate future JWT without blocking ship |
| Ranking / score | Prefer USD inputs for volume / net-liq components once USD quotes exist; `fee_tvl` stays a unitless ratio |
| Canonical storage | Keep existing XLM-derived fields in DB for compatibility; add USD fields at API / derived layer |
| Missing price | Surface `—` / omit USD for that amount; do not invent prices from symbol alone |

## Problem statement

Today:

- Snapshotter / indexer value amounts via an **XLM price book** (RPC pool reserves).
- API / UI suffix many numbers with `XLM` (e.g. `vol 286.34 XLM`, `reserves ≈ 529,947 XLM`).
- Pool [CBRUQ7…YYZ7](https://lumenlp.xyz/pools/view?address=CBRUQ7I6C6OGHMDYWD6XQUZFB6KJ3LLPNE34EPKSPFZ2YMBJ2GIWYYZ7) is **AQUA/USDC** — no XLM in the pair — so XLM units are confusing.

Desired:

- Same pool shows `$` (or `USD`) for TVL, volume, fees, event summaries.
- Pricing comes from external Stellar-aware APIs keyed by classic asset identity where possible.

## External price sources

### Freighter (primary)

- URL: `https://freighter-backend-v2.stellar.org/api/v1/token-prices?network=PUBLIC`
- Body: `{ "tokens": ["native", "AQUA:GBNZ…", "USDC:GA5Z…"] }`
- Response: `{ "data": { "<tokenId>": { "currentPrice": "<usd string>", "percentagePriceChange24h": "…" } } }`
- Token ID rules:
  - `native` for XLM
  - `CODE:ISSUER` for classic / SAC-backed assets
  - **Soroban `C…` addresses are rejected** (`invalid token id`)
- Auth: currently anonymous; Freighter is rolling out per-request JWT — treat as soft dependency

### StellarExpert (fallback / enrichment)

- List: `GET https://api.stellar.expert/explorer/public/asset/?…`
- Detail: `GET https://api.stellar.expert/explorer/public/asset/{CODE}-{ISSUER}`
- Field: `price` (USD), plus rating / toml / logos
- ID format uses **hyphen** (`AQUA-GBNZ…`), optional trailing `-N`
- Better as single-asset fallback + metadata than as primary batch pricer

### Out of scope for v1

- Ankr `stellar_getTokenPrice` (needs API key; keep as optional later)
- CoinGecko / DexScreener as primary Stellar sources
- Fully rewriting snapshotter to store only USD

## Asset identity mapping

Soroban pool tokens are `C…` contract IDs. Freighter needs classic IDs.

Resolution order per token address:

1. `token_registry` entry with `symbol` + `issuer` → `CODE:ISSUER` (or `native` for XLM SAC)
2. Parse `token_meta.name` when it matches `CODE:ISSUER` (already true for many Aquarius SAC names)
3. Treat known native SAC as `native`
4. Else: **unpriced** for Freighter; try StellarExpert search only if we have CODE+issuer; else leave USD null

Never price by bare symbol (`AQUA` alone) — issuer collisions are unacceptable.

## Valuation rules

### Token amount → USD

```text
amount_usd = human_amount(token) × usd_price(token)
```

`human_amount` uses token decimals (from meta / defaults).

### Pool TVL (display)

```text
tvl_usd = Σ_i reserve_i_human × usd_price(token_i)
```

If any reserve token is unpriced:

- Prefer partial TVL only if policy is explicit; **v1 default:** mark `tvl_usd` null when any leg is missing (avoid silently understating TVL). Document in API `quote.coverage`.

### Swap volume / fee (events & rollups)

For a trade with `token_in` / `amount_in` (and fee in `token_in` units today):

```text
volume_usd ≈ amount_in_human × usd_price(token_in)
fee_usd    ≈ fee_amount_human × usd_price(token_in)
```

Same spirit as current XLM estimation, but USD book instead of XLM book.

For `update_reserves` / claims: sum legs with available prices; if incomplete, null or show raw token amounts without fake USD.

### Conversion of existing XLM fields (compatibility)

Where raw reserves are not available at API time but `*_quote_xlm` exists:

- **Do not** present `× xlm_usd` as the primary story for non-XLM pools (that is Path 1).
- Allowed only as **emergency fallback** when token USD prices are unavailable *and* XLM quote exists, with `quote.source = "xlm_bridge"` so UI can avoid implying pool-native XLM.

Primary path must revalue from token amounts + USD prices whenever amounts exist.

## API shape (additive)

Introduce a shared quote block on list/detail (and optionally events):

```json
"quote": {
  "currency": "USD",
  "as_of": "2026-08-04T12:00:00Z",
  "source": "freighter" | "stellar_expert" | "mixed" | "xlm_bridge" | "none",
  "xlm_usd": 0.17,
  "coverage": "full" | "partial" | "none"
}
```

Add parallel fields (names illustrative):

| Existing | Add |
|----------|-----|
| `tvl` (XLM est.) | `tvl_usd` |
| `window_metrics.*.volume` | `volume_usd` (or nested under same window) |
| `window_metrics.*.fee` | `fee_usd` |
| `activity_summary.*_quote_24h` | `*_usd_24h` counterparts |
| event `derived.*_quote_xlm` | `derived.*_quote_usd` |

Keep XLM fields through one release for debugging; UI stops labeling them as the primary unit.

Optional: `GET /v1/prices?tokens=…` for debugging / frontend cache — nice-to-have, not required if pools responses embed prices.

## Backend architecture

```text
┌─────────────┐     ┌──────────────────┐     ┌─────────────┐
│ Freighter   │────▶│ PriceService     │◀────│ StellarExpert│
└─────────────┘     │ (api-server)     │     └─────────────┘
                    │ cache TTL 60–120s│
                    └────────┬─────────┘
                             │
                    list/detail/events
                    attach *_usd + quote
```

Responsibilities:

1. **PriceService** — fetch, cache, map contract→Freighter id, return `usd_price` map
2. **API handlers** — when building pool/event JSON, compute USD fields from reserves / derived amounts
3. **Indexer (phase)** — optionally persist `volume_quote_usd` / `fee_quote_usd` at ingest time once PriceService is shared; **v1 may compute USD only in api-server** from existing amount fields + live prices to ship faster

Recommended v1 split:

- **Ship:** api-server PriceService + USD on `/v1/pools`, `/v1/pools/{addr}`, event list enrichment
- **Follow-up:** indexer writes USD at ingest for historical consistency when prices move

## Frontend

- Replace user-visible `XLM` suffixes with `$` / `USD` using `*_usd` fields
- Formatters: `fmtUsd(n)` → `$1,234.56` (compact for large TVL optional)
- Event summary strings: `vol $48.90`, `reserves ≈ $90,450`, `claim fees ≈ $2.11`
- If `*_usd` null: show `—` or token-raw summary, **not** `… XLM` as default
- Pool cards / table: Liquidity, Fee columns use USD
- Detail hero stats same

## Score

Current score uses `fee_tvl`, `volume/tvl`, net liq ratios, cadence. Ratios are mostly unit-invariant if numerator and denominator share a currency.

v1:

- Keep formula weights
- Feed volume / net-liq / tvl from **USD** when `coverage == full`
- If coverage incomplete, fall back to existing XLM-based score (unchanged behavior)

## Failure modes

| Case | Behavior |
|------|----------|
| Freighter 4xx/5xx | Use cache if fresh; else StellarExpert for needed assets; else `quote.source=none` |
| Freighter rate limit / 403 | Back off; serve stale cache |
| Token unmapped to CODE:ISSUER | Unpriced; coverage partial/none |
| Dust / absurd fee_tvl | Orthogonal; still apply existing min-TVL / filter hygiene if present |
| Auth required later | Config flag + env for future token; document operational risk |

## Non-goals (v1)

- Removing XLM price book from snapshotter
- Guaranteeing USD history charts match live reprice (chart may still use stored XLM series × bridge until follow-up)
- Multi-currency UI toggle (USD only for display)
- Pricing pure SEP-41 tokens with no classic issuer (show unpriced)

## Success criteria

1. AQUA/USDC pool detail event feed does **not** show `XLM` as the money unit for vol / reserves / claims when USD prices resolve
2. Pool list Liquidity / Fee read as dollars for fully covered pools
3. Freighter outage does not crash API; degrades via cache / Expert / none
4. No pricing by symbol-only

## Implementation phases

1. **PriceService + mapping + `/v1/pools` USD fields**
2. **Detail + events USD summaries in UI**
3. **Score USD inputs when coverage full**
4. **Follow-up:** indexer persist USD; chart series; optional Ankr

## Open questions (non-blocking)

- Exact JSON nesting (`volume_usd` sibling vs `window_metrics.24h.volume_usd`) — prefer sibling-under-window for locality
- Whether to hide raw XLM fields from public JSON immediately or keep one release — prefer keep for debug, UI ignores
