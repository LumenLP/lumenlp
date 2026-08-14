# LP Agent Demo — Design Spec

**Date:** 2026-07-22  
**Status:** Approved for implementation planning  
**Goal:** Standalone Stellar / Aquarius LP analytics demo (portfolio PnL/IL, pool yield terminal, user positions) suitable for SCF narrative — not a LumAgg feature.

## Context

Build a product analogous in *intent* to [LP Agent](https://app.lpagent.io/) (portfolio + pool analytics), but **Aquarius-native on Stellar**, as its own codebase under `lpagent`. **Source of truth is Soroban RPC + our own math/snapshots**; Aquarius REST is optional bootstrap only. No zap / copy-LP / multi-protocol in this phase.

## Decisions (locked)

| Topic | Choice |
|-------|--------|
| Data | **RPC-first** (router + pool contracts); compute TVL/APR/PnL/IL ourselves |
| Aquarius REST | Optional convenience only — never required for core metrics |
| Identity | Freighter **and** paste G-address (read-only) |
| Repo | New standalone monorepo (`lpagent`), not inside LumAgg |
| Protocol | Aquarius only |
| Pool types | `constant_product`, stable, **and** concentrated liquidity |
| History | Lightweight snapshotter → SQLite (volume from our deltas / events, not Aquarius stats) |
| Stack | Next.js frontend + **Rust** backend (api-server + snapshotter) |
| Architecture | Thin web; Rust owns positions, PnL/IL, history |

## Architecture

```
apps/web (Next.js)
  Freighter + paste address
  Routes: / | /pools | /pools/[address]
        │ REST
        ▼
crates/api-server (Axum)
  Soroban RPC (primary) → compute TVL / positions / PnL / IL
  Aquarius REST optional (pool label hints only)
        │
        ▼
SQLite
  pool_snapshots (tvl, volume_delta, est_apr, …)
        ▲
crates/snapshotter (cron / systemd)
  Discover pools via router → read reserves on-chain →
  compute TVL + volume from snapshot deltas → upsert
```

### Repository layout (target)

```
lpagent/
  apps/web/           # Next.js App Router
  crates/
    api-server/       # HTTP API
    dex/             # Router discovery + pool/CL reads via Soroban RPC
    metrics/          # PnL / IL / APR / TVL pure functions
    snapshotter/      # binary: ingest cycle
  docs/superpowers/
  Cargo.toml          # workspace
```

### External dependencies

- **Soroban RPC (required):** operator node (e.g. `178.63.81.216`); `RPC_URL` env
- **Aquarius router (mainnet):** `CBQDHNBFBZYE4MKPWBSJOPIYLW4SFSXAXUTSXJN76GNKYVYPCKWC6QUK` — discover pools via `get_tokens_sets_count` + `get_pools_for_tokens_range`
- **Aquarius REST (optional):** may seed human-readable token labels; core numbers must work with REST disabled
- **Wallet:** Freighter for connect; no signing required for demo reads

### Computation ownership

| Metric | Computed by us from | Not taken as-is from Aquarius REST |
|--------|---------------------|-------------------------------------|
| Pool list | Router catalogue + `pool_type` / `get_tokens` / fee | REST `/pools/` only as optional cache |
| Reserves / TVL | `get_reserves` (or CL slot0 + liquidity) + token decimals/prices | — |
| Volume 24h | Snapshot deltas and/or RPC `getEvents` for swaps | REST `total_volume` not trusted as live truth |
| Fee APR | `fee_bps × volume_24h / tvl × 365` in `metrics` | — |
| Share LP amounts | On-chain `share_balance` / total shares × reserves | REST user pools only as discovery hint |
| CL positions | Pool contract position getters / ledger entries | — |
| PnL / IL | `metrics` crate formulas | — |

## Product surface

### `/` — Portfolio

- Header identity: Freighter or pasted address
- Summary cards: net worth, fees earned (see metric rules), estimated IL
- Positions table: all Aquarius LP for that address (CP / stable / CL), per-row type, range status (CL), value, PnL, IL

### `/pools` — Yield terminal

- Searchable pool list (pair, type, TVL, volume, est. APR)
- Chart for selected pool from `pool_snapshots` history

### `/pools/[address]` — Pool detail

- Metadata: tokens, pool type, fee tier, current est. APR, TVL, reserves
- History chart from snapshots
- If identity set: “Your positions in this pool”

### Out of scope (this phase)

- Deposit / withdraw / zap / claim UI
- Copy-LP / automation
- Phoenix, Soroswap, SDEX LP
- AQUA/ICE incentive APR in v1 estimates
- Full swap-event indexer

## Data flow

### Positions (on demand)

1. Resolve active address (wallet or paste)
2. Discover candidate pools from **on-chain** (router catalogue cached by snapshotter; optional REST hint for “pools this user touched”)
3. For each candidate: Soroban reads — share balance / CL positions, reserves, unclaimed fees
4. Skip pools with zero position
5. `metrics` crate computes mark-to-market value, IL, PnL when cost basis exists
6. Return aggregated JSON to web

### Snapshots (scheduled)

1. Discover all pool addresses from Aquarius **router** (RPC)
2. Hydrate each (or top-N by TVL): `get_tokens`, `get_reserves` / CL state, `fee`, `pool_type`
3. Compute TVL in quote asset (start with XLM-normalized using pool prices)
4. Volume: difference in cumulative swap counters if available on-chain; else estimate from reserve/price changes between snapshots; else ingest swap events via RPC `getEvents` for that pool
5. `est_apr = fee_bps/10000 × volume_24h / tvl × 365` in `metrics`
6. Upsert `pool_snapshots(...)`; API serves history for charts

## Metric definitions

| Metric | Definition | Honesty rule |
|--------|------------|--------------|
| Position value | MTM of underlying at current pool price; CL from liquidity + tick range | Always show |
| Fees | Prefer **unclaimed** fees from contract | If claimed history unavailable, label “unclaimed only” |
| PnL | `value + unclaimed_fees − cost_basis` | If no cost basis → show N/A; do not invent |
| IL (est.) | vs HODL of same initial amounts at current prices (CP/stable/CL formulas) | Label estimate |
| Pool est. APR | Fee APR: `fee_rate × volume_24h / tvl × 365` | **Exclude** AQUA incentives in v1; label “fee APR 24h (est.)” |

Cost basis: best-effort from deposit-related data if cheaply available; otherwise omit absolute PnL and still show value + fees + IL vs HODL where initial amounts are known from current share/position math only as appropriate.

## API sketch (api-server)

| Method | Path | Purpose |
|--------|------|---------|
| GET | `/health` | Liveness |
| GET | `/v1/pools` | List/search pools + latest snapshot fields |
| GET | `/v1/pools/:address` | Pool detail + latest metrics |
| GET | `/v1/pools/:address/history` | Snapshot time series |
| GET | `/v1/positions?address=` | All positions for account |
| GET | `/v1/positions/summary?address=` | Net worth / fees / IL aggregates |

Errors: structured JSON `{ error, code }`; partial success allowed at position-row level via per-item `status`.

## Error & empty states

- No address → Portfolio CTA (connect or paste)
- Address, zero LP → empty state (not error)
- Aquarius / RPC timeout → retry affordance; render successful rows; badge failed rows
- Single CL decode failure → skip that position; server log
- Snapshot lag → show `last_snapshot_at`; never fabricate chart points

## Deploy (demo)

- **web:** Vercel or same VPS
- **api-server + snapshotter:** VPS with `RPC_URL` to local Soroban RPC; `AQUA_API_BASE` optional
- **SQLite:** file on disk beside services
- **CORS:** allow web origin only
- Secrets: RPC URL / any keys in env only; never commit
- Demo must remain usable with `AQUA_API_BASE` unset

## Testing bar

- Unit tests in `metrics`: fixed fixtures for CP, stable, CL value + IL + fee APR
- Integration: router discovery returns ≥1 pool via RPC (requires `RPC_URL`)
- Snapshotter dry-run: one cycle inserts ≥1 row without panic
- Manual: Freighter and pasted address produce the same Portfolio for the same G-address
- Regression: with REST disabled, positions + pool TVL still populate from RPC

## Non-goals / follow-ups

- AQUA reward APR and ICE boost
- Multi-protocol portfolio
- Write paths (LP mutate)
- Postgres / event indexer upgrade path (compatible later; not required for demo)

## Success criteria

1. Paste or Freighter → see real Aquarius positions including CL where present
2. Positions show value, fee, IL (est.), and PnL or explicit N/A
3. `/pools` terminal shows list + historical est. APR/TVL from our snapshots
4. Stack and boundaries are clearly separate from LumAgg for grant storytelling
