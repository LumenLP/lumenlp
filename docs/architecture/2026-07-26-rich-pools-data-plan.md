# LPAgent Rich Pools Data Plan

Last updated: 2026-07-26

## Goal

Make `/pools` evolve from a snapshot-based demo into a product-grade pools data surface closer to `https://app.lpagent.io/pools`.

Target outcomes:

- Real `5m / 1h / 6h / 24h` pool volume and fee metrics
- Sortable pool leaderboard by `fee/tvl`, `volume`, `liquidity`
- More metadata: pool age, holders, token labels, token logos, market cap where feasible
- Reliable ingestion architecture that does not depend on ad hoc recomputation at request time

## Current state in this repo

Current `lpagent` strengths:

- Pool discovery and on-chain hydrate already exist.
- Snapshotter already persists pool state into SQLite.
- API and web already expose pool lists, histories, and windowed metrics.

Current limitations:

- `volume_24h` is estimated from reserve deltas, not true swap flow.
- `fee`, `fee/tvl`, and APR are therefore derived from proxy volume.
- There is no dedicated events indexer.
- There is no token metadata, holder counting, or pool-level rollup table.

This means the UI can look similar to LP Agent, but the data model is not yet rich enough to support the same quality.

## What to reuse from stellar-dex-aggregator

Reference repo:

- `/Users/ligulfzhou/Money/blockchain/stellar/grant/stellar-dex-aggregator`

Most relevant reusable ideas:

### 1. Analytics indexer pattern

Useful reference:

- `docs/analytics-indexer.md`
- `crates/analytics-indexer/src/store.rs`

Key ideas to reuse:

- Persist a cursor ledger in SQLite
- Poll Soroban RPC `getEvents`
- Store raw/parsed swap events in normalized tables
- Build rollups from stored events instead of computing everything live

Important note:

The aggregator indexer tracks events emitted by the LumAgg aggregator contract, not DEX pool swap events directly. The pattern is reusable; the event parser is not directly reusable as-is.

### 2. Event-driven pool freshness

Useful reference:

- `docs/pool-state-architecture.md`
- `crates/market-data-worker/src/ledger_watcher.rs`

Key ideas to reuse:

- Poll `getLatestLedger`
- Fetch events per ledger
- Use touched contracts to decide which pools to refresh
- Separate hot path from slow full discovery

This is the right model for `lpagent` too. The current hourly or 5-minute snapshot timer is enough for coarse snapshots, but not enough for rich pool analytics.

### 3. Token metadata enrichment

Useful reference:

- `crates/market-data-worker/src/worker.rs`
- token metadata handling in the aggregator worker

Key idea:

- Keep topology/state ingest separate from metadata enrichment
- Backfill symbols/logos asynchronously

For `lpagent`, this is the easiest way to upgrade pool labels from raw addresses to product-quality names.

## Recommended architecture for lpagent

Do not replace the current crates. Add one new lane beside them.

### Existing lane

- `snapshotter`: periodic snapshot and coarse pool state
- `api-server`: read models and web API
- `dex`: on-chain discovery and hydrate

### New lane

- `pool-indexer`: event-driven ingest of swaps and touched pools
- `rollup job`: compute windowed metrics from indexed swaps + snapshots

## Proposed new crates

Add these workspace members:

- `crates/pool-indexer`
- `crates/pool-rollups`

Optional later:

- `crates/token-metadata`

## Proposed data model

Keep SQLite for now. It is enough for the current scale.

### 1. Cursor table

```sql
CREATE TABLE IF NOT EXISTS indexer_cursor (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  last_ledger INTEGER NOT NULL,
  updated_at TEXT NOT NULL
);
```

Reuse directly from the aggregator pattern.

### 2. Pool swaps

This is the core table missing today.

```sql
CREATE TABLE IF NOT EXISTS pool_swaps (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  tx_hash TEXT NOT NULL,
  event_id TEXT UNIQUE NOT NULL,
  ledger INTEGER NOT NULL,
  created_at INTEGER NOT NULL,
  pool_address TEXT NOT NULL,
  dex TEXT NOT NULL,
  token_in TEXT,
  token_out TEXT,
  amount_in TEXT,
  amount_out TEXT,
  fee_bps INTEGER,
  volume_quote REAL,
  fee_quote REAL
);
```

Notes:

- `event_id` should be used for idempotency.
- `amount_*` should stay as strings if precision matters.
- `volume_quote` and `fee_quote` can be derived after token pricing is known.

### 3. Pool 5-minute snapshots

```sql
CREATE TABLE IF NOT EXISTS pool_snapshots_5m (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  pool_address TEXT NOT NULL,
  bucket_ts INTEGER NOT NULL,
  tvl REAL NOT NULL,
  reserves_json TEXT NOT NULL,
  fee_bps INTEGER NOT NULL,
  UNIQUE(pool_address, bucket_ts)
);
```

Why 5-minute buckets:

- They are the base grain needed for `5m / 1h / 6h / 24h`.
- Larger windows should be derived from these, not stored independently first.

### 4. Pool rollups

```sql
CREATE TABLE IF NOT EXISTS pool_rollups (
  pool_address TEXT NOT NULL,
  window TEXT NOT NULL,
  as_of_ts INTEGER NOT NULL,
  sample_count INTEGER NOT NULL,
  volume_quote REAL NOT NULL,
  fee_quote REAL NOT NULL,
  avg_tvl REAL NOT NULL,
  fee_tvl REAL NOT NULL,
  tx_count INTEGER NOT NULL,
  PRIMARY KEY (pool_address, window)
);
```

Windows:

- `5m`
- `1h`
- `6h`
- `24h`

### 5. Token metadata

```sql
CREATE TABLE IF NOT EXISTS token_metadata (
  token_address TEXT PRIMARY KEY,
  symbol TEXT,
  name TEXT,
  decimals INTEGER,
  logo_url TEXT,
  price_quote REAL,
  market_cap_quote REAL,
  holders_count INTEGER,
  updated_at TEXT NOT NULL
);
```

## Ingestion flow

### A. Fast path: event indexer

`pool-indexer` loop:

1. Poll `getLatestLedger`
2. For each new ledger, call `getEvents`
3. Filter to known pool contracts
4. Parse swap-related events
5. Insert into `pool_swaps`
6. Mark touched pools
7. Refresh touched pool state only
8. Write/update `pool_snapshots_5m`

This is the main upgrade over the current `snapshotter`.

### B. Slow path: periodic discovery

Every 5-10 minutes:

1. Re-run pool discovery
2. Reconcile added/removed pools
3. Refresh metadata for new pools
4. Backfill any missing state

This follows the aggregator worker model closely.

## Where the real volume should come from

Not from reserve delta.

Preferred order:

1. Parse explicit swap contract events from each DEX pool
2. If explicit swap events are missing on some venue, inspect transaction meta / state diff as fallback
3. Only use reserve delta as a last-resort temporary approximation

This is the most important architectural correction.

## What parts can be copied vs rewritten

### Copy the pattern

- Cursor handling
- Ledger polling loop
- Event-driven touched-pool refresh model
- Separation of discovery vs hot refresh
- Rollup table idea

### Rewrite for lpagent

- Event parser for Aquarius pool swaps
- Pool-level event attribution
- TVL + fee/tvl rollup logic
- API schema for pools leaderboard

The aggregator contract indexer is invocation-centric. `lpagent` needs pool-centric indexing.

## API shape to target

### List endpoint

`GET /v1/pools?window=24h&sort=fee_tvl&min_tvl=1000&min_samples=3&q=xlm`

Response should include:

- `address`
- `pool_type`
- `tokens`
- `fee_bps`
- `tvl`
- `last_snapshot_at`
- `window_metrics[5m|1h|6h|24h]`
- `holders_count`
- `created_at`

### Detail endpoint

`GET /v1/pools/{address}`

Should include:

- current pool metadata
- latest state
- `window_metrics`
- token metadata
- historical chart points

### History endpoint

`GET /v1/pools/{address}/history?metric=tvl&interval=5m&limit=288`

This avoids reusing a single coarse chart for all use cases.

## Delivery phases

### Phase 1: Trustworthy volume and fee

Deliver:

- `pool-indexer`
- `pool_swaps`
- cursor persistence
- real `5m/1h/6h/24h` `volume / fee / fee_tvl`

Do first:

- Aquarius only

### Phase 2: Better list quality

Deliver:

- token symbol/name/logo enrichment
- `min_samples` and `min_tvl` filtering
- better pair labels
- stable leaderboard sorting

### Phase 3: Rich metadata

Deliver:

- pool age
- holders count
- token price and market cap
- better chart history

## Practical recommendation

Do not try to reach full LP Agent richness in one pass.

The shortest path to a much better product is:

1. Build a dedicated swap event indexer for Aquarius pools
2. Write 5-minute state buckets
3. Materialize window rollups
4. Upgrade the existing `/v1/pools` API to read those rollups

That gives you the most visible product improvement for the least architectural risk.

## Immediate next implementation step

Start Phase 1 by creating:

- `crates/pool-indexer/Cargo.toml`
- `crates/pool-indexer/src/main.rs`
- `crates/pool-indexer/src/db.rs`
- `crates/pool-indexer/src/events.rs`
- `crates/pool-indexer/src/rollups.rs`

And update the workspace in `Cargo.toml`.

The first working target should be:

- ingest Aquarius swap events
- populate `pool_swaps`
- produce one correct `24h volume / fee / fee_tvl` per pool

Once that is stable, add `5m/1h/6h`.
