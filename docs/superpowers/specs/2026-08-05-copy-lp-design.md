# Copy LP (read-queue + scaled drafts) — Design Spec

**Date:** 2026-08-05  
**Status:** Approved for implementation planning  
**Product:** LumenLP (`lumenlp.xyz`)  
**Motivation:** Add an lpagent-style **Copy LP** surface: follow a leader wallet’s Aquarius LP actions (all pools), scale each action by a coefficient, and auto-enqueue ops for the follower to draft/sign. No custody and no unsupervised on-chain execution in v1.

## Goal

Let a follower:

1. Start a **copy session** against a leader `G…` address with a **coefficient** (e.g. `0.1` = 10%, `2.0` = 200%).
2. **Ignore** the follower’s existing LP positions at start (do not close/adjust them).
3. **Do not** force-mirror the leader’s already-open positions at start.
4. From `cursor_ts` onward, when the leader **deposits / withdraws / opens / closes / adjusts** Aquarius LP, **immediately** create a **CopyOp** whose amounts = `leader_amounts × coefficient`.
5. Present a live queue; each pending op can **Generate draft** into the existing Strategies Tier-B path (prefill params; signing may remain disabled until a later milestone).

## Decisions (locked)

| Topic | Choice |
|-------|--------|
| Scope tier | Event-driven queue + scaled drafts (lpagent-like follow-on), **not** custodial auto-exec |
| Leader coverage | **All** Aquarius pools for that address (not single-pool only) |
| Existing follower LP | **Untouched** |
| Startup mirror of leader open positions | **No** (follow **future** actions only) |
| Scaling basis | **Per-action amounts × coefficient** (not “share of follower budget”) |
| Queue freshness | **Immediate** on indexed leader events (within indexer poll), not manual Sync-only |
| Execution | Auto-create **CopyOp**; user still **signs** (or drafts only in v1) |
| Coefficient UX | Presets `10%` / `100%` / `200%` + custom; risk hint for `>100%` |
| Insufficient funds | Still enqueue; UI marks `insufficient`; **no silent downscale** |
| Claim fees | **Optional** in schema; default **off** for v1 queue generation |
| Protocols | Aquarius only |

## Non-goals (v1)

- Privy / delegated / dedicated copy wallet
- Keeper / bot that submits txs without the user
- TP / SL, rug filters, min mcap gates (lpagent extras)
- Startup “copy all open positions now”
- Non-Aquarius venues
- Social discovery of “top LPs” leaderboard (may paste any `G…`)

## Problem / current gaps

Today LumenLP has:

- Read-only `GET /v1/positions?address=` (snapshot of open LPs).
- Pool-centric indexer (`pool_events`, swaps) — **deposit/withdraw derived fields do not yet extract `actor`**.
- Strategies page: localStorage catalog + preview; **Review & sign disabled**.

Missing for Copy LP:

1. Per-user actor on liquidity events.
2. Copy session + op queue persistence/API.
3. `/copy` UI + Header nav.
4. Mapping CopyOp → Strategies draft / deep-link.

## Architecture

```
pool-indexer ──(events with actor)──► pool-indexer.db
                                         │
api-server ── match active copy_sessions ──► insert copy_ops
                                         │
Web /copy ◄── GET sessions + ops ────────┘
     │
     └─ Generate draft → Strategies localStorage / ?pool=&copyOp=
```

### Responsibility split

| Layer | Owns |
|-------|------|
| Indexer | Parse & store `actor` on deposit/withdraw/(CL position) events; keep pool scan as today |
| API | CRUD copy sessions; materialize CopyOps from new events after `cursor_ts`; list ops; status transitions |
| Web | Session UI, coefficient, live queue, draft generation, honest “no auto-exec” copy |

Opportunistic note: materializing ops can live in **api-server on read/poll** (scan events since cursor) *or* a small hook after indexer insert. Prefer **API-side reconcile on session poll + background tick** so indexer stays focused on ingest; document either as long as ops appear within ~one indexer poll.

## Prerequisite: actor on liquidity events

Trade events already expose a user address in topics (see indexer trade parsing). Liquidity events must do the same.

### Required derived field

For `deposit_liquidity` / `withdraw_liquidity` (and CL open/close/adjust if distinct kinds exist):

```json
"derived": {
  "actor": "G…",
  "share_amount": "…",
  "token_amounts": [{ "token": "C…", "amount": "…" }],
  "total_quote_xlm": 123.4
}
```

### Implementation notes

- Confirm Aquarius event topic layout against `thirdparty/aquarius-amm` / live samples; extract the LP owner / caller address into `actor`.
- Backfill: optional best-effort re-parse of recent `body_json` for active sessions; not required for sessions that start after deploy.
- If an event cannot resolve `actor`, **do not** generate a CopyOp (log/metric); never guess.

## Data model

### `copy_sessions`

| Column | Type | Notes |
|--------|------|-------|
| `id` | text PK | uuid |
| `follower_address` | text | `G…` (wallet / pasted identity) |
| `leader_address` | text | `G…` |
| `coefficient` | real | `> 0`; e.g. `0.1`, `2.0` |
| `status` | text | `active` \| `paused` \| `stopped` |
| `include_claims` | int | default `0` |
| `cursor_ts` | i64 | unix; only events with `created_at > cursor_ts` |
| `created_at` | i64 | |
| `updated_at` | i64 | |

Constraints: one **active** session per `(follower, leader)` (or replace/pause previous).

### `copy_ops`

| Column | Type | Notes |
|--------|------|-------|
| `id` | text PK | uuid |
| `session_id` | text FK | |
| `source_event_id` | text | idempotent with session |
| `pool_address` | text | |
| `kind` | text | see below |
| `position_key` | text | CP: `cp:{pool}` or share class; CL: `cl:{pool}:{tick_lower}:{tick_upper}` |
| `leader_amounts_json` | text | raw token amounts (+ optional quote) |
| `scaled_amounts_json` | text | leader × coefficient |
| `leader_quote_xlm` | real? | |
| `scaled_quote_xlm` | real? | |
| `status` | text | `pending` \| `drafted` \| `signed` \| `skipped` \| `failed` \| `insufficient` |
| `note` | text? | |
| `created_at` | i64 | |
| `updated_at` | i64 | |

Unique: `(session_id, source_event_id)`.

### `kind` enum (v1)

| kind | Trigger |
|------|---------|
| `deposit` | `deposit_liquidity` |
| `withdraw` | `withdraw_liquidity` |
| `open_cl` | CL position open (when event kind available) |
| `close_cl` | CL position close |
| `adjust` | CL range/liquidity change that is not pure deposit/withdraw |
| `claim` | fee claim — only if `include_claims` |

## Scaling rules

For each token amount `a_i` on the leader action:

\[
a'_i = a_i \times \mathrm{coefficient}
\]

- Prefer **raw token amounts × coefficient** (integer/base units as strings where possible; document rounding: floor toward zero for withdraw safety).
- Also store scaled quote when `total_quote_xlm` exists: `quote' = quote × coefficient`.
- **CL ranges**: copy leader `tick_lower` / `tick_upper` **unchanged**; scale liquidity / deposited amounts only.
- **Coefficient > 1**: allow; UI warning that capital and inventory risk scale up.
- **Insufficient balances**: keep op `pending` or set status `insufficient` after a balance check helper; never auto-shrink coefficient for that op.

## Position mapping (follower ↔ leader)

Copy LP needs a stable key so later withdraw/close targets the follower’s mirrored position:

1. On follower **draft/open** from a CopyOp, Strategies draft (or future signed tx) should store `position_key` + optional `positionId`.
2. Subsequent leader withdraw/close/adjust with the same `position_key` generate ops that reference that key.
3. If follower never drafted/opened the matching position, later withdraw/close ops stay `pending` with note `no_local_position` (user can skip).

v1 may keep this mapping in **localStorage** keyed by `session_id` if on-chain position ids are not yet wired; API still carries `position_key` on every op.

## API (sketch)

| Method | Path | Purpose |
|--------|------|---------|
| `POST` | `/v1/copy/sessions` | `{ follower, leader, coefficient, include_claims? }` → session; sets `cursor_ts = now` |
| `GET` | `/v1/copy/sessions?follower=` | list sessions |
| `PATCH` | `/v1/copy/sessions/{id}` | pause/stop/update coefficient |
| `GET` | `/v1/copy/sessions/{id}/ops?status=` | list ops (reconcile new events before respond) |
| `POST` | `/v1/copy/ops/{id}/status` | `{ status: drafted\|skipped\|signed\|failed }` |

Auth: v1 same as rest of site — **address trust from client** (paste/connect). Document spoofing risk; no server wallet custody.

Reconcile algorithm (on `GET …/ops` or internal tick):

1. Load session; if not `active`, return stored ops.
2. Query `pool_events` where `derived.actor = leader` and `created_at > cursor_ts` (and `> last_processed` watermark).
3. For each new event: insert CopyOp with scaled amounts; advance watermark.
4. Return ops newest-first.

## Frontend

### Route `/copy`

1. Form: leader address, coefficient (presets + custom), Start.
2. Active session panel: status, coefficient, pause/stop.
3. **Queue**: cards with time, pool pair, kind, leader amounts → scaled amounts, status, actions:
   - **Generate draft** → Strategies draft + navigate `/strategies?pool=…&copyOp=…`
   - **Skip**
4. Banner: *LumenLP does not submit transactions for you. You review and sign.*

### Nav

Add **Copy** link in `Header` next to Pools / Strategies.

### Identity

Use existing Header identity (`G…` connected or pasted) as `follower_address`. If missing, prompt before Start.

## Strategies integration

- Extend draft payload / localStorage optional fields: `copyOpId`, `position_key`, `scaled_amounts`.
- Prefill pool address and honesty preview steps from CopyOp (deposit/withdraw labels + scaled amount labels).
- Signing button may remain disabled with existing “Signing path coming next” until tx-build lands; Copy LP v1 success does **not** require live Aquarius invoke.

## Success criteria

- [ ] Liquidity events expose reliable `derived.actor` for deposit/withdraw on sampled mainnet pools.
- [ ] Start session with coefficient `0.1`; leader deposit appears as CopyOp with amounts ≈ 10% within one indexer poll after event ingest.
- [ ] Coefficient `2.0` doubles amounts; UI shows risk hint.
- [ ] Follower’s pre-existing positions unchanged when session starts.
- [ ] Leader actions before `cursor_ts` do not enqueue.
- [ ] Idempotent: re-poll does not duplicate ops for same `source_event_id`.
- [ ] Generate draft deep-links to Strategies with pool + copy metadata.
- [ ] Copy clearly states no custodial auto-exec.

## Risks

| Risk | Mitigation |
|------|------------|
| Actor missing / wrong topic index | Spike on live events before queue feature; gate Copy behind actor coverage metric |
| CLMM event shapes differ from CP | Separate `kind`s; skip unknown with note |
| Indexer lag | Queue timestamp + “as of ledger” in UI |
| Address spoofing on session API | Document; later add signed challenge if needed |
| User expects full auto like lpagent | Explicit banner + grant narrative: “assisted copy queue” |

## Implementation order (for planning)

1. Actor extraction on liquidity events (+ tests / live sample fixtures).
2. DB tables + copy session/ops API + reconcile.
3. `/copy` UI + Header.
4. Strategies draft bridge.
5. Deploy API + site; verify with a known active leader wallet.

## Open questions (non-blocking for plan)

- Exact CLMM event symbol names for open/close/adjust on Aquarius concentrated pools — resolve during actor/event spike.
- Whether `claim` defaults stay off permanently or become a session toggle in UI v1.1.
