# Aquarius quality / traction checklist

Use before SCF demos and tranche reviews. Spot-check mainnet via [lumenlp.xyz](https://lumenlp.xyz) + [api.lumenlp.xyz](https://api.lumenlp.xyz).

## Indexer / pools

- [ ] `GET /health` → `{ "ok": true }`
- [ ] `GET /v1/indexer/status` shows advancing `cursor_ledger` and non-zero events
- [ ] Pick ≥3 liquid Aquarius pools on `/pools`; compare **24h volume** and **TVL** to Aquarius UI (record % gap; target “same order of magnitude / within agreed band”)
- [ ] Pool detail with recent swaps shows non-zero volume when Aquarius shows activity

## Liquidity actor (Copy LP prerequisite)

- [ ] Sample recent `deposit_liquidity` / `withdraw_liquidity` events include `derived.actor` (G…)
- [ ] Indexer logs show actor backfill progressing when historical rows lack actor

## Copy LP

- [ ] Create session on `/copy` with coefficient (e.g. 0.1)
- [ ] Leader deposit enqueues op; UI shows **leader → scaled** quote/amounts
- [ ] Generate draft → Strategies banner shows scaled amounts
- [ ] Status transitions: pending → drafted / skipped work; invalid transitions return a conflict and rejected operations remain terminal; **no custody / no auto-submit** copy visible in UI

## Multi-DEX surface

- [ ] `GET /v1/venues` lists Aquarius as `production` and others as `scaffold` / deferred
- [ ] Docs: `docs/architecture/dex-adaptor.md`

## Demo script (2–3 min)

1. Pools ranked list + one pool detail  
2. Copy session + scaled op  
3. `/v1/venues` support matrix (architecture for five DEX roadmap)
