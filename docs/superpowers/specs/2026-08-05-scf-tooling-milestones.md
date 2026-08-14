# LumenLP — SCF Build Tranches & Deliverables

**Date:** 2026-08-05 (revised 2026-08-06)  
**Team:** **2 core builders** (+ optional short-term help).  
**Track fit:** Build / tooling (multi-DEX LP strategy infra). No custody contracts.  
**Timeline:** 3–5 months (≤6 months SCF cap).  
**Requested budget (example):** **$110,000**  
**Payment shape:** Tranche #0 = 10% ($11k) on award; #1 = 20%; #2 = 30%; #3 = 40%.

> Form needs **3 paid tranches**, each with **N deliverables** (Completion + Budget).  
> Pre-submit traction: live Aquarius index + Copy LP + `DexAdaptor` / `GET /v1/venues` (scaffolds for other DEXes).

## Product north star

Five **production** LP adaptors under one `DexAdaptor` + venue-agnostic strategy runtime.

| Venue | This award |
|-------|------------|
| Aquarius | Production (reference; largely live at submit) |
| Sushi V3 | Production CL read + draft |
| Phoenix | Production CP read + draft |
| Soroswap AMM | Production CP (not aggregator) |
| Comet | Production or testnet-gated + checklist |
| Classic | Thin adaptor **or** ADR (not a 6th production count) |

## One-liner

Two-builder Stellar LP strategy toolchain: Aquarius, Sushi V3, Phoenix, Soroswap AMM, Comet adaptors; copy-scale + rebalance runtime; SDK/CLI + reference bot — no custody.

## Out of scope

Custodial vaults / auto-submit contracts · social copy as primary product · marketing / audits

---

## Tranche 1 (Deliverable Roadmap) — MVP · **$22,000**

1. **`DexAdaptor` interface + docs + `GET /v1/venues`**  
   - Completion: Public docs (`docs/architecture/dex-adaptor.md`); trait/`venue_id` in repo; API returns support matrix; Aquarius production, others scaffold.  
   - Budget: **$4,000**

2. **Aquarius production quality bar**  
   - Completion: Written spot-check ≥3 pools volume/TVL vs Aquarius UI; sampled deposit/withdraw show `derived.actor`; checklist in repo.  
   - Budget: **$6,000**

3. **Copy-scale MVP (Aquarius) hardened for reviewers**  
   - Completion: Session → scaled ops → `leader → scaled` in UI; statuses pending/drafted/skipped; no custody copy visible.  
   - Budget: **$8,000**

4. **Demo artifact**  
   - Completion: Live site walkthrough or recorded 2–3 min demo covering pools + copy + venues matrix.  
   - Budget: **$4,000**

- **Tranche 1 total: $22,000**

---

## Tranche 2 (Deliverable Roadmap) — Expansion · **$33,000**

1. **Sushi V3 adaptor (production)**  
   - Completion: Mainnet pools/positions, CL liquidity events, draft subset; smoke + fixtures in repo.  
   - Budget: **$10,000**

2. **Phoenix adaptor (production)**  
   - Completion: Mainnet pools, shares, liquidity events, deposit/withdraw draft; smoke + fixtures.  
   - Budget: **$8,000**

3. **Soroswap AMM adaptor (production)**  
   - Completion: Same CP bar as Phoenix (AMM only).  
   - Budget: **$7,000**

4. **Venue-agnostic runtime + OpenAPI**  
   - Completion: observe → draft with `venue_id`; ≥1 non-copy strategy dry-run on ≥2 venues; OpenAPI published; watermark + scale-math tests green.  
   - Budget: **$8,000**

- **Tranche 2 total: $33,000**

---

## Tranche 3 (Deliverable Roadmap) — Mainnet launch · **$44,000**

1. **Comet adaptor (production / gated)**  
   - Completion: Join/exit + shares + dry-run draft; **or** testnet E2E + mainnet readiness checklist if mainnet liquidity absent (code complete either way).  
   - Budget: **$12,000**

2. **Five-venue support matrix (live)**  
   - Completion: Aquarius, Sushi, Phoenix, Soroswap AMM, Comet each documented with list/position/event/draft status; gaps explicit.  
   - Budget: **$6,000**

3. **SDK/CLI + reference bot**  
   - Completion: Multi-`venue_id` examples; third party can dry-run ≥3 venues from docs; optional user-key submit documented.  
   - Budget: **$16,000**

4. **Classic decision + launch package**  
   - Completion: Thin Classic adaptor **or** ADR; production deploy health + runbook + launch demo; no-custody UX.  
   - Budget: **$10,000**

- **Tranche 3 total: $44,000**

---

## Budget rollup (@ $110k)

| Tranche | % | Amount |
|---------|---|--------|
| #0 Award acceptance | 10% | $11,000 |
| #1 MVP | 20% | $22,000 |
| #2 Expansion | 30% | $33,000 |
| #3 Mainnet | 40% | $44,000 |
| **Total** | | **$110,000** |

Engineering + infra only (no marketing/audit/legal).

## Application blurb

> LumenLP is **multi-DEX LP strategy infrastructure** for Stellar. A two-person core team will ship production `DexAdaptor` implementations for Aquarius, Sushi V3, Phoenix, Soroswap AMM, and Comet, a venue-agnostic strategy runtime (including copy-scale), and SDK/CLI/reference bots — **without custodial contracts**. Mainnet Aquarius analytics and Copy LP dry-run are already live as traction.

## Traction links (update at submit)

- Site: https://lumenlp.xyz  
- API: https://api.lumenlp.xyz/health · https://api.lumenlp.xyz/v1/venues  
- Adaptor docs: `docs/architecture/dex-adaptor.md`  
- OpenAPI: `docs/openapi.yaml`  
- Quality checklist: `docs/architecture/aquarius-quality-checklist.md`

## Execution notes

- Split: Builder A = CL (Aquarius patterns → Sushi); Builder B = CP (Phoenix ↔ Soroswap); Comet last.  
- Identical feature parity not required — ship an honest matrix.  
- Comet: keep testnet-gate wording if mainnet pools are thin.
