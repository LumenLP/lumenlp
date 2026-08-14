# LP Agent Demo Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship a standalone Aquarius LP analytics demo: portfolio positions with PnL/IL, pool yield terminal from our snapshots — **RPC-first**, metrics computed in Rust.

**Architecture:** Cargo workspace (`metrics`, `dex`, `api-server`, `snapshotter`) + Next.js `apps/web`. Discover pools via Aquarius router on Soroban; hydrate reserves/CL on-chain; snapshotter writes SQLite; API serves JSON; web is thin UI with Freighter + paste address.

**Tech Stack:** Rust 2021, Axum, Tokio, SQLite (`sqlx` or `rusqlite`), `stellar-xdr` / Soroban RPC JSON-RPC, Next.js App Router, Freighter API.

**Spec:** `docs/superpowers/specs/2026-07-22-lpagent-demo-design.md`

---

## File map

| Path | Responsibility |
|------|----------------|
| `Cargo.toml` | Workspace members |
| `crates/metrics/src/lib.rs` | Pure TVL, fee APR, CP/stable/CL value + IL |
| `crates/metrics/tests/*.rs` | Fixture tests |
| `crates/dex/src/rpc.rs` | Soroban simulate + getLedgerEntries client |
| `crates/dex/src/aquarius/router.rs` | Pool discovery from router |
| `crates/dex/src/aquarius/pool.rs` | CP/stable hydrate + share balance |
| `crates/dex/src/aquarius/positions.rs` | CL pool + position reads (phase 2 depth) |
| `crates/dex/src/db.rs` | SQLite schema + snapshot upsert/query |
| `crates/snapshotter/src/main.rs` | One ingest cycle binary |
| `crates/api-server/src/main.rs` | Axum routes |
| `crates/api-server/src/handlers.rs` | `/v1/pools`, `/v1/positions`, … |
| `apps/web/` | Next.js Portfolio + Pools UI |
| `.env.example` | `RPC_URL`, `DATABASE_URL`, `CORS_ORIGIN` |

---

### Task 1: Workspace + `metrics` crate (TDD)

**Files:**
- Create: `Cargo.toml`
- Create: `crates/metrics/Cargo.toml`
- Create: `crates/metrics/src/lib.rs`
- Create: `crates/metrics/tests/apr_il.rs`

- [ ] **Step 1: Scaffold workspace**

```toml
# Cargo.toml
[workspace]
resolver = "2"
members = [
  "crates/metrics",
  "crates/dex",
  "crates/api-server",
  "crates/snapshotter",
]

[workspace.package]
edition = "2021"
license = "Apache-2.0"
```

```toml
# crates/metrics/Cargo.toml
[package]
name = "metrics"
version = "0.1.0"
edition.workspace = true
license.workspace = true

[dependencies]
```

- [ ] **Step 2: Write failing tests for fee APR + CP IL**

```rust
// crates/metrics/tests/apr_il.rs
use metrics::{fee_apr_24h, cp_position_value, cp_il_vs_hodl};

#[test]
fn fee_apr_annualizes_24h_volume() {
    // fee 30 bps, vol 1000, tvl 10_000 → (0.003 * 1000 / 10000) * 365 = 0.1095
    let apr = fee_apr_24h(30, 1_000.0, 10_000.0);
    assert!((apr - 0.1095).abs() < 1e-9);
}

#[test]
fn cp_il_zero_when_price_unchanged() {
    let (a, b) = cp_position_value(100, 200, 100, 200); // shares proportional
    let il = cp_il_vs_hodl(100.0, 200.0, 1.0, 1.0); // prices same as entry ratio
    assert!(il.abs() < 1e-9);
    assert!(a > 0.0 && b > 0.0);
}
```

- [ ] **Step 3: Run tests — expect FAIL**

```bash
cargo test -p metrics
```

Expected: compile error / undefined functions

- [ ] **Step 4: Implement minimal `metrics` API**

```rust
// crates/metrics/src/lib.rs
//! Pure LP math — no I/O.

/// Annualized fee APR from 24h volume and TVL. `fee_bps` e.g. 30 = 0.3%.
pub fn fee_apr_24h(fee_bps: u32, volume_24h: f64, tvl: f64) -> f64 {
    if tvl <= 0.0 || volume_24h <= 0.0 {
        return 0.0;
    }
    let fee_rate = fee_bps as f64 / 10_000.0;
    fee_rate * volume_24h / tvl * 365.0
}

/// Share of reserves: `user_shares / total_shares * reserve`.
pub fn cp_position_amounts(
    user_shares: u128,
    total_shares: u128,
    reserve_a: u128,
    reserve_b: u128,
) -> (f64, f64) {
    if total_shares == 0 {
        return (0.0, 0.0);
    }
    let s = user_shares as f64 / total_shares as f64;
    (s * reserve_a as f64, s * reserve_b as f64)
}

pub fn cp_position_value(
    user_shares: u128,
    total_shares: u128,
    reserve_a: u128,
    reserve_b: u128,
) -> (f64, f64) {
    cp_position_amounts(user_shares, total_shares, reserve_a, reserve_b)
}

/// IL fraction vs HODL: `value_lp / value_hodl - 1` (negative = loss).
/// `amount_a0`, `amount_b0` at entry; `price_a`, `price_b` current (same quote).
pub fn cp_il_vs_hodl(amount_a0: f64, amount_b0: f64, price_a: f64, price_b: f64) -> f64 {
    if amount_a0 <= 0.0 || amount_b0 <= 0.0 || price_a <= 0.0 || price_b <= 0.0 {
        return 0.0;
    }
    let p0 = amount_b0 / amount_a0; // B per A at entry (in token units)
    let p1 = price_a / price_b; // current A priced in B… use consistent quote:
    // Prefer quote both in price units:
    let hodl = amount_a0 * price_a + amount_b0 * price_b;
    // Constant-product LP value for same liquidity: 2 * sqrt(a0*b0*pa*pb) style
    let k = amount_a0 * amount_b0;
    let lp = 2.0 * (k * price_a * price_b).sqrt();
    if hodl <= 0.0 {
        return 0.0;
    }
    lp / hodl - 1.0
}

/// TVL in quote units given reserve amounts and prices.
pub fn tvl_from_reserves(reserves: &[f64], prices: &[f64]) -> f64 {
    reserves.iter().zip(prices.iter()).map(|(r, p)| r * p).sum()
}
```

Fix the CP IL test to match the chosen formula (entry amounts + current prices → `lp/hodl - 1`).

- [ ] **Step 5: Run tests — expect PASS**

```bash
cargo test -p metrics
```

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml crates/metrics
git commit -m "feat: add metrics crate with fee APR and CP IL helpers"
```

---

### Task 2: `dex` RPC client + router discovery

**Files:**
- Create: `crates/dex/Cargo.toml`
- Create: `crates/dex/src/lib.rs`
- Create: `crates/dex/src/rpc.rs`
- Create: `crates/dex/src/aquarius/router.rs`
- Create: `crates/dex/src/types.rs`

Reference patterns (do **not** import LumAgg as dependency — reimplement slim copies):

- Router: `CBQDHNBFBZYE4MKPWBSJOPIYLW4SFSXAXUTSXJN76GNKYVYPCKWC6QUK`
- `get_tokens_sets_count` + `get_pools_for_tokens_range`
- Per pool: `pool_type`, `get_tokens`, `get_reserves`, `get_fee_fraction` / fee, `get_total_shares`, share balance for account

- [ ] **Step 1: Add crate deps**

```toml
# crates/dex/Cargo.toml
[package]
name = "dex"
version = "0.1.0"
edition.workspace = true
license.workspace = true

[dependencies]
metrics = { path = "../metrics" }
anyhow = "1"
reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
stellar-strkey = "0.0.9"
stellar-xdr = { version = "23", features = ["curr", "serde"] }
tokio = { version = "1", features = ["rt-multi-thread", "macros", "time"] }
tracing = "0.1"
base64 = "0.22"
```

- [ ] **Step 2: Implement `SorobanRpc::simulate_call` + `call_no_args`**

Minimal port of LumAgg `dex-adapters` RPC simulate path: build invoke host function tx → `simulateTransaction` → decode `retval` ScVal.

- [ ] **Step 3: Implement `discover_pool_addresses() -> Vec<String>`**

Batch `get_pools_for_tokens_range`; parse map values to contract addresses (same shape as LumAgg).

- [ ] **Step 4: Implement `hydrate_share_pool(addr) -> SharePoolState`**

Fields: `address`, `pool_type` (`constant_product` | `stable` | `concentrated`), `tokens: Vec<String>`, `reserves: Vec<u128>`, `fee_bps: u32`, `total_shares: u128`, `amp: Option<u128>`.

Skip full CL hydrate in this step if `pool_type == concentrated` — return type tag only; Task 4 fills CL.

- [ ] **Step 5: Manual smoke (optional if RPC reachable)**

```bash
RPC_URL=http://127.0.0.1:8003 cargo test -p dex -- --ignored
```

Or a `examples/discover.rs` later. If RPC not local, document SSH tunnel: `ssh -L 8003:127.0.0.1:8003 root@178.63.81.216`.

- [ ] **Step 6: Commit**

```bash
git add crates/dex
git commit -m "feat: dex crate with Soroban RPC and router pool discovery"
```

---

### Task 3: SQLite + snapshotter

**Files:**
- Create: `crates/dex/src/db.rs`
- Create: `crates/snapshotter/Cargo.toml`
- Create: `crates/snapshotter/src/main.rs`
- Create: `.env.example`

- [ ] **Step 1: Schema**

```sql
CREATE TABLE IF NOT EXISTS pools (
  address TEXT PRIMARY KEY,
  pool_type TEXT NOT NULL,
  tokens_json TEXT NOT NULL,
  fee_bps INTEGER NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS pool_snapshots (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  pool_address TEXT NOT NULL,
  ts TEXT NOT NULL,
  tvl REAL NOT NULL,
  volume_24h REAL NOT NULL,
  est_apr REAL NOT NULL,
  reserves_json TEXT NOT NULL,
  UNIQUE(pool_address, ts)
);

CREATE INDEX IF NOT EXISTS idx_snapshots_pool_ts ON pool_snapshots(pool_address, ts);
```

- [ ] **Step 2: Snapshot cycle**

1. `discover_pool_addresses`
2. Hydrate share pools (limit `SNAPSHOT_TOP_N` default 50 by reserve depth heuristic)
3. Price: for demo, price token vs XLM using pool that pairs with native (`CAS3J7…` or native address convention used on Aquarius); if no path, TVL in “raw reserve units” flagged
4. `volume_24h`: sum of positive TVL-normalized reserve turnover vs previous snapshot within 24h window; if no previous row, `volume_24h = 0`, `est_apr = 0`
5. Upsert

- [ ] **Step 3: Binary**

```bash
RPC_URL=... DATABASE_PATH=./data/lpagent.db cargo run -p snapshotter
```

Expected: exits 0, DB has rows in `pools` and `pool_snapshots`.

- [ ] **Step 4: Commit**

```bash
git commit -am "feat: snapshotter writes on-chain pool TVL snapshots to SQLite"
```

---

### Task 4: Positions API (share pools + CL stub → full)

**Files:**
- Create: `crates/api-server/Cargo.toml`
- Create: `crates/api-server/src/main.rs`
- Create: `crates/api-server/src/handlers.rs`
- Create: `crates/dex/src/aquarius/positions.rs`

- [ ] **Step 1: Share positions**

For address `G…`: iterate cached pool list from DB (from snapshotter); call on-chain share balance; if `> 0`, compute amounts via `metrics::cp_position_amounts`, value via prices, IL via `cp_il_vs_hodl` with **current amounts as proxy entry** labeled `il_basis: "current_composition_proxy"` until deposit history exists; `pnl: null`.

- [ ] **Step 2: CL positions**

Read Aquarius CL position APIs / ledger keys (mirror LumAgg `aquarius_clmm` where useful). Return: tick range, in-range bool, token amounts, unclaimed fees if available.

- [ ] **Step 3: Axum routes**

| Route | Behavior |
|-------|----------|
| `GET /health` | `{ ok: true }` |
| `GET /v1/pools` | DB pools + latest snapshot |
| `GET /v1/pools/:address` | Detail |
| `GET /v1/pools/:address/history` | Snapshot series |
| `GET /v1/positions?address=` | Position rows |
| `GET /v1/positions/summary?address=` | Aggregates |

CORS from `CORS_ORIGIN`.

- [ ] **Step 4: Smoke**

```bash
curl -s localhost:8080/health
curl -s "localhost:8080/v1/pools" | head
curl -s "localhost:8080/v1/positions?address=G..." 
```

- [ ] **Step 5: Commit**

```bash
git commit -am "feat: api-server serves pools history and Aquarius positions"
```

---

### Task 5: Next.js web UI

**Files:**
- Create: `apps/web/` (Next.js TS)
- Create: `apps/web/src/app/page.tsx` (Portfolio)
- Create: `apps/web/src/app/pools/page.tsx`
- Create: `apps/web/src/app/pools/[address]/page.tsx`
- Create: `apps/web/src/lib/api.ts`
- Create: `apps/web/src/lib/identity.tsx` (Freighter + paste)

- [ ] **Step 1: Scaffold**

```bash
cd apps && npx create-next-app@latest web --typescript --eslint --app --src-dir --no-tailwind --import-alias "@/*"
```

Use a simple CSS module / global CSS (terminal-leaning, not purple SaaS). Prefer fonts already decided in UI pass — dark terminal for `/pools` is fine.

- [ ] **Step 2: Identity context**

- Paste G-address validation (`G` + length)
- Freighter `getAddress` / `isConnected` if available
- Persist active address in `localStorage`

- [ ] **Step 3: Portfolio page**

Fetch `/v1/positions/summary` + `/v1/positions`; empty/CTA/error states per spec.

- [ ] **Step 4: Pools terminal**

Table + simple SVG/canvas chart from `/v1/pools/:id/history`.

- [ ] **Step 5: Commit**

```bash
git add apps/web
git commit -m "feat: web portfolio and pools terminal UI"
```

---

### Task 6: Wiring, README, verification

- [ ] **Step 1: Root README** — how to tunnel RPC, run snapshotter, api-server, web
- [ ] **Step 2: `.env.example`**
- [ ] **Step 3: End-to-end checklist** against spec success criteria
- [ ] **Step 4: Commit docs**

```bash
git commit -am "docs: README and env example for LP Agent demo"
```

---

## Spec coverage checklist

| Spec item | Task |
|-----------|------|
| RPC-first discovery | 2, 3 |
| Self-computed TVL / APR | 1, 3 |
| Positions CP/stable/CL | 4 |
| PnL N/A + IL est. | 1, 4 |
| Snapshot history terminal | 3, 5 |
| Freighter + paste | 5 |
| REST optional | 2–4 (no hard dep) |
| Standalone from LumAgg | all |

## Notes for implementers

- Prefer copying **ideas** from LumAgg `aquarius.rs` / `rpc.rs` / `aquarius_clmm.rs`, not adding a path dependency on that repo.
- Volume quality improves over time as snapshots accumulate; first snapshot APR=0 is expected.
- Keep `metrics` free of async/I/O so formulas stay unit-tested.
