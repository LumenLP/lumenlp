# DexAdaptor — multi-DEX LP surface

**Status:** Interface + Aquarius production + multi-venue read/index support for Soroswap AMM; Phoenix has a validated read-only boundary; Sushi V3 and Comet remain scaffolds.
**Code:** `crates/dex/src/adaptor.rs`  
**API:** `GET /v1/venues`

## Why

LP strategies (copy-scale, stay-in-range, fee harvest) must not hardcode Aquarius types. Each venue implements the same identity and capability matrix; Aquarius is the reference production adaptor under `dex::aquarius`.

## Layout

```text
crates/dex/
  adaptor.rs      # DexAdaptor trait + normalized boundary + support matrix
  rpc.rs          # shared Soroban RPC
  types.rs / db.rs
  aquarius/       # production (pool, router, positions, pricing)
  phoenix.rs      # factory/pool query reader; write/copy support pending
  soroswap.rs     # factory/pair query reader; write/copy support pending
  sushi.rs / comet.rs                             # scaffolds
```

## `venue_id`

| id | Name | Status today |
|----|------|----------------|
| `aquarius` | Aquarius | **production** |
| `sushi_v3` | Sushi V3 | scaffold |
| `phoenix` | Phoenix | scaffold; read-only reader validated on mainnet |
| `soroswap_amm` | Soroswap AMM | indexed read support; Copy LP writes pending |
| `comet` | Comet | scaffold |

## Capabilities

Each row exposes booleans for pool discovery and normalized LP actions: `list_pools`, `positions`, `liquidity_events`, `quotes`, `draft_ops`, `deposit`, `withdraw`, `claim`, and `copy_scale`.

Aquarius currently sets all to `true`. Scaffolds set all to `false` until a production adaptor lands. Stellar Classic DEX is intentionally outside this pool-LP adapter registry because its order-book model does not expose the same pool deposit/withdraw lifecycle.

## Normalized boundary

```text
DexAdaptor
  venue_id() / name() / status() / capabilities() / notes()
  normalize_pool(SharePoolState)       → PoolDescriptor
  normalize_position(UserPosition)     → PositionDescriptor
  normalize_event(LiquidityEvent)      → validated event
  build_draft_op(DraftRequest)          → DraftOp or fail-closed error
  support_row()                         → matrix row for docs/API
```

`PoolDescriptor`, `PositionDescriptor`, `LiquidityEvent`, and `DraftRequest` are shared strategy-facing types. Their payload fields intentionally retain venue-specific JSON where CP shares and CL ticks cannot be represented by one fixed schema.

Heavy RPC (hydrate pool, scan events, build XDR) stays in venue modules (`dex::aquarius`, `pool-indexer`, …). The trait is the **stable binding** for strategy config and the public support matrix. Every draft operation is checked against the venue capability matrix; unsupported operations fail closed before reaching a relayer or wallet.

## Draft ops

Normalized kinds: `deposit`, `withdraw`, `claim`, `open_range`, `close_range`, `adjust_range`.  
Payload amounts remain venue-specific JSON so CP shares and CL ticks can coexist.

## Adding a venue

1. Implement `DexAdaptor` (or promote `ScaffoldAdaptor` → production impl).  
2. Wire indexer parsers + draft builders.  
3. Flip `VenueStatus::Production` and capabilities.  
4. Extend copy/runtime to accept that `venue_id`.  
5. Add contract/event fixtures and compatibility tests for pool, position, event, and draft normalization.  
6. Update `GET /v1/venues` via `default_venue_registry()`.

## Phoenix first slice

`dex::phoenix` now covers the first read-only Phoenix boundary:

- `discover_pool_addresses` calls the configured factory's `query_all_pools_details` method;
- the mainnet factory configuration is `CB4SVAWJA6TSRNOJZ7W2AWFW46D5VR4ZMFZKDIKXEINZCZEGZCJZCKMI`;
- `discover_mainnet_pool_addresses` provides the production-network convenience path;
- discovered addresses are sorted and de-duplicated before persistence;
- `hydrate_pool` reads the Phoenix XYK pool query surface:

- `query_config` for token addresses and `total_fee_bps`;
- `pool_type` is decoded as XYK (`0`) or Stable (`1`) instead of assuming every pool is XYK;
- `query_pool_info` for token reserves and total shares;
- signed Soroban integer fields are normalized to non-negative `u128` values;
- token order is checked between both query responses;
- `provide_liquidity` and `withdraw_liquidity` are the supported LP lifecycle event topics;
- `swap`, `initialize`, and admin topics are not Copy LP position events;
- no write operation is enabled yet.

The mainnet spot-check fixture records a successful factory and pool read. Promotion to production still requires event-version validation and a monitored indexing rollout before enabling Copy LP operations. The factory fixture uses the real address; generic pool entries remain synthetic test data.

Repeatable read-only validation is available at `deploy/validate-phoenix.sh`. It checks factory discovery plus `query_config` and `query_pool_info` without signing or submitting a transaction.

## Soroswap first slice

`dex::soroswap` covers the factory, constant-product pair, and first production indexing boundary:

- `discover_pool_addresses` calls `all_pairs_length` and `all_pairs`;
- the mainnet factory configuration is `CA4HEQTL2WPEUYKYKCDOHCDNIV4QHNJ7EL4J4NQ6VADP7SYHVRYZ7AW2`;
- `hydrate_pool` reads `token_0`, `token_1`, and `get_reserves` concurrently;
- the current Soroswap AMM fee is normalized as 30 bps;
- reserves are decoded as non-negative signed Soroban integers;
- `deposit` and `withdraw` are recognized as LP lifecycle events; `swap`, `sync`, and `skim` are excluded;
- no write operation is enabled yet.

The mainnet spot-check fixture records a successful factory and pair read. Soroswap discovery, pool hydration, and event ingestion are enabled alongside Aquarius in the production indexer and snapshotter. Repeatable validation is available at `deploy/validate-soroswap.sh`. Copy LP promotion still requires pair event-version validation, LP share accounting, policy integration, and a monitored write rollout.

## Related

- SCF plan: `docs/superpowers/specs/2026-08-05-scf-tooling-milestones.md`  
- Quality checklist: `docs/architecture/aquarius-quality-checklist.md`
