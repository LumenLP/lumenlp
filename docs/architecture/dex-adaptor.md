# DexAdaptor — multi-DEX LP surface

**Status:** Interface + Aquarius production + scaffolds for Sushi V3, Phoenix, Soroswap AMM, and Comet.  
**Code:** `crates/dex/src/adaptor.rs`  
**API:** `GET /v1/venues`

## Why

LP strategies (copy-scale, stay-in-range, fee harvest) must not hardcode Aquarius types. Each venue implements the same identity and capability matrix; Aquarius is the reference production adaptor under `dex::aquarius`.

## Layout

```text
crates/dex/
  adaptor.rs      # DexAdaptor trait + support matrix
  rpc.rs          # shared Soroban RPC
  types.rs / db.rs
  aquarius/       # production (pool, router, positions, pricing)
  sushi.rs / phoenix.rs / soroswap.rs / comet.rs   # scaffolds
```

## `venue_id`

| id | Name | Status today |
|----|------|----------------|
| `aquarius` | Aquarius | **production** |
| `sushi_v3` | Sushi V3 | scaffold |
| `phoenix` | Phoenix | scaffold |
| `soroswap_amm` | Soroswap AMM | scaffold |
| `comet` | Comet | scaffold |

## Capabilities

Each row exposes booleans for pool discovery and normalized LP actions: `list_pools`, `positions`, `liquidity_events`, `quotes`, `draft_ops`, `deposit`, `withdraw`, `claim`, and `copy_scale`.

Aquarius currently sets all to `true`. Scaffolds set all to `false` until a production adaptor lands. Stellar Classic DEX is intentionally outside this pool-LP adapter registry because its order-book model does not expose the same pool deposit/withdraw lifecycle.

## Trait (conceptual)

```text
DexAdaptor
  venue_id() / name() / status() / capabilities() / notes()
  support_row()  → matrix row for docs/API
```

Heavy RPC (hydrate pool, scan events, build XDR) stays in venue modules (`dex::aquarius`, `pool-indexer`, …). The trait is the **stable binding** for strategy config and the public support matrix.

## Draft ops

Normalized kinds: `deposit`, `withdraw`, `claim`, `open_range`, `close_range`, `adjust_range`.  
Payload amounts remain venue-specific JSON so CP shares and CL ticks can coexist.

## Adding a venue

1. Implement `DexAdaptor` (or promote `ScaffoldAdaptor` → production impl).  
2. Wire indexer parsers + draft builders.  
3. Flip `VenueStatus::Production` and capabilities.  
4. Extend copy/runtime to accept that `venue_id`.  
5. Update `GET /v1/venues` via `default_venue_registry()`.

## Related

- SCF plan: `docs/superpowers/specs/2026-08-05-scf-tooling-milestones.md`  
- Quality checklist: `docs/architecture/aquarius-quality-checklist.md`
