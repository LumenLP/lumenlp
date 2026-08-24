# DexAdaptor — multi-DEX LP surface

**Status:** Interface + Aquarius production + multi-venue read/index support for Soroswap AMM, Phoenix, Sushi V3, and Comet.
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
  sushi.rs        # known-pool CLMM reader; event/write support pending
  comet.rs        # weighted-pool reader and discovery; Copy LP writes pending
```

## `venue_id`

| id | Name | Status today |
|----|------|----------------|
| `aquarius` | Aquarius | **production** |
| `sushi_v3` | Sushi V3 | indexed read support; event/Copy LP writes pending |
| `phoenix` | Phoenix | indexed read support; Copy LP writes pending |
| `soroswap_amm` | Soroswap AMM | indexed read support; Copy LP writes pending |
| `comet` | Comet | indexed read + event support; Copy LP writes pending |

## Capabilities

Each row exposes booleans for pool discovery and normalized LP actions:
`list_pools`, `positions`, `liquidity_events`, `quotes`, `draft_ops`,
`deposit`, `withdraw`, `claim`, and `copy_scale`. These operation fields mean
that the adaptor can validate and build an unsigned venue-specific draft; they
do not by themselves authorize an automated transaction.

`copy_execution_enabled` is the separate production gate. It is true only when
the venue is marked `production`, supports coefficient-scaled operations, and
has the required deposit and withdrawal policy path. Today only Aquarius is
enabled. Phoenix, Sushi V3, Soroswap AMM, and Comet can expose analytics and
validated unsigned drafts while remaining fail-closed for automated Copy LP.
Stellar Classic DEX is intentionally outside this pool-LP adapter registry
because its order-book model does not expose the same pool deposit/withdraw
lifecycle.

## Normalized boundary

```text
DexAdaptor
  venue_id() / name() / status() / capabilities() / notes()
  normalize_pool(SharePoolState)       → PoolDescriptor
  normalize_position(UserPosition)     → PositionDescriptor
  normalize_event(LiquidityEvent)      → validated event
  build_draft_op(DraftRequest)          → DraftOp or fail-closed error
  support_row()                         → matrix row for docs/API, including
                                          copy_execution_enabled
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
- no Phoenix write operation is enabled in the production adaptor yet.

The mainnet spot-check fixture records a successful factory and pool read. Phoenix discovery, pool hydration, and swap/activity ingestion are enabled alongside Aquarius and Soroswap in the production indexer and snapshotter. Promotion to Copy LP still requires liquidity-event version validation, LP share accounting, and a monitored write rollout. The factory fixture uses the real address; generic pool entries remain synthetic test data.

Repeatable read-only validation is available at `deploy/validate-phoenix.sh`. It checks factory discovery plus `query_config` and `query_pool_info` without signing or submitting a transaction.

Phoenix execution is deliberately split at the ABI boundary. Its XYK pool
uses optional desired/min token amounts, while its Stable pool uses required
amounts plus an optional minimum share output. Both share the withdrawal
signature, but their configuration structs are not identical. The exact
method argument shapes are captured in
`crates/dex/fixtures/phoenix-operation-boundary.json`; a future policy adapter
must select the pool type from validated on-chain configuration and fail
closed when the type or ABI is unknown.

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

## Sushi V3 first slice

`dex::sushi` covers the initial concentrated-liquidity analytics boundary:

- uses the validated mainnet Sushi V3 pool catalogue from the local aggregator discovery implementation;
- reads `slot0`, `liquidity`, `fee`, `token0`, and `token1` from each pool contract;
- derives virtual reserves from current liquidity and sqrt price for the shared TVL/price pipeline;
- labels pools as `concentrated` and exposes them through the same API DEX filter as Aquarius, Phoenix, and Soroswap;
- does not yet classify Sushi CL liquidity events or build Copy LP writes.

The production snapshotter and indexer now include Sushi pools. The derived reserves are an analytics approximation for CLMM state, not fungible LP-share reserves; event fixtures, tick-range accounting, and policy-controlled Copy LP execution are required before promotion beyond read-only analytics.

## Comet first slice

`dex::comet` provides read-only weighted-pool discovery and hydration from the factory and pool contracts. It reads configured token balances and swap fees, and exposes a normalized weighted-pool state for TVL and activity analytics. Factory discovery is bounded to the RPC's retained ledger range so it does not request unavailable historical ledgers.

The indexer recognizes Comet's `POOL/swap`, `POOL/join_pool`, `POOL/exit_pool`, `POOL/deposit`, and `POOL/withdraw` events. Swap payloads are normalized into token amounts, quote volume, and fee estimates; liquidity payloads retain the caller, token, amount, and quote value. Comet remains read-only for Copy LP: share accounting, policy validation, and safe deposit/withdraw execution must be completed before it is enabled as an execution venue.

## Related

- SCF plan: `docs/superpowers/specs/2026-08-05-scf-tooling-milestones.md`  
- Quality checklist: `docs/architecture/aquarius-quality-checklist.md`
