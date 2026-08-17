# DexAdaptor — multi-DEX LP surface

**Status:** Interface + Aquarius production + scaffolds for Sushi V3, Phoenix, Soroswap AMM, and Comet.  
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
  phoenix.rs      # read-only query reader; factory/event validation pending
  sushi.rs / soroswap.rs / comet.rs                # scaffolds
```

## `venue_id`

| id | Name | Status today |
|----|------|----------------|
| `aquarius` | Aquarius | **production** |
| `sushi_v3` | Sushi V3 | scaffold |
| `phoenix` | Phoenix | scaffold; read-only reader in progress |
| `soroswap_amm` | Soroswap AMM | scaffold |
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

`dex::phoenix::hydrate_pool` reads the Phoenix XYK pool query surface:

- `query_config` for token addresses and `total_fee_bps`;
- `query_pool_info` for token reserves and total shares;
- signed Soroban integer fields are normalized to non-negative `u128` values;
- token order is checked between both query responses;
- no write operation or factory discovery is enabled yet.

Promotion to production requires a validated factory address set, event topic fixtures, and mainnet spot checks before enabling indexing or Copy LP operations.

## Related

- SCF plan: `docs/superpowers/specs/2026-08-05-scf-tooling-milestones.md`  
- Quality checklist: `docs/architecture/aquarius-quality-checklist.md`
