# Copy LP Studio (On-chain auto-copy) — Design Spec

**Date:** 2026-08-07  
**Status:** Approved — **Path B locked** (smart-account + CopyEngine + keeper); use for SCF End User Application  
**Product:** LumenLP (`lumenlp.xyz`) — End User Application  
**SCF milestones:** `2026-08-07-scf-copy-lp-studio-milestones.md`  
**Related:** `2026-08-05-copy-lp-design.md` (v0: queue + user-signed drafts)  
**Related product:** LumAgg (DEX aggregator) for post-claim sells — **not** part of copy mirror  

## Goal

Ship **Copy LP Studio**: discover Aquarius LP leaders, start proportional copy sessions, and **auto-execute** mirrored liquidity actions via Soroban **smart-account policy + keeper**, without custody vaults and without mirroring swaps.

**Path decision:** Long-term and grant path are the same (**B**). Operator-delegated copy wallets (lpagent/Privy-style **A′**) are **not** the primary architecture.

v0 (live today) remains: indexer → copy queue → user signs / draft.  
This spec is **on-chain execution** on top of that UX.

## Decisions (locked)

| Topic | Choice |
|-------|--------|
| SCF category | **End User Application** (not Developer Tooling) |
| Architecture path | **B** — smart-account scoped policy + CopyEngine + EVENT_RECORDER + permissionless execute |
| Auth model | Smart-account **scoped policy** + permissionless **keeper** (Soroban-native; product ≠ stop-loss Guard) |
| Custody | **No vault** — funds stay in follower positions / balances |
| Accounts | **One follower account, many sessions** (many leaders) |
| Mirror actions | Aquarius **deposit / withdraw / claim** |
| Swaps | **Never mirror** leader swaps (cannot tell fee-exit vs normal trade; sells use LumAgg separately) |
| Claim timing | **Hybrid**: follow leader claims by default; optional `auto_claim_idle` |
| Scaling | `leader_amounts × coefficient` (same as v0) |
| Limits | Per-session `max_per_op_quote` + `max_daily_quote`; reject on exceed (**no silent downscale**) |
| Startup mirror | **No** — future actions only |
| Existing follower LP | Untouched at session start |
| Venue v1 | Aquarius only |
| License | **MIT** open source; public instance + self-host docs |

## Non-goals (v1)

- Price stop-loss / health-factor / oracle-deviation **Guard** product (optional later stretch only)
- Custodial copy vault / Privy-style delegated copy wallet as primary
- One smart account per leader
- Multi-DEX production auto-copy (later award / appendix)
- Mirroring leader swaps / aggregator routes
- Force-copy leader’s already-open positions at start
- Letting arbitrary users act as EVENT_RECORDER on the public deployment

## Positioning (one liner)

> **LumenLP Copy LP Studio:** pick an LP leader, set a coefficient and limits, non-custodial auto-copy of deposits/withdraws/claims on Aquarius; optional fee exit via LumAgg.  
> Complementary to stop-loss/auto-exit products — we mirror **LP actions**, not generic position exits.

---

## Architecture

```text
Leader Aquarius LP events
        │
   pool-indexer (existing, actor-tagged)
        │
   CopyEngine (new)  ← sessions: leader, coef, limits, claim flags
        │
   Keeper (permissionless)
        │
   CopyEngine validates → follower Smart Account Policy
        │
   Aquarius deposit | withdraw | claim only
        │
   Optional separate flow: LumAgg sell of claimed tokens
   (own auth / not on copy policy whitelist)
```

| Component | Responsibility |
|-----------|------------------|
| **Web** | Leaders → Start copy → Arm policy + register session → Active queue / history → Pause / Disarm; optional “sell via LumAgg” |
| **CopyEngine** | Canonical sessions + execute gates; anti-replay on `source_event_id`; limit accounting |
| **Policy** | Scoped auth: only Engine-approved Aquarius LP entrypoints; owner can always full-control / disarm |
| **Keeper** | Watch intents/events; submit `execute_*`; pays fees; cannot invent authority |
| **Indexer + API** | Discovery, off-chain queue cache, UX status (authority = chain for armed sessions) |
| **LumAgg** | Post-claim token exit — **out of band** from copy |

### Multi-leader

```text
Follower account (one)
  └── Policy: allow CopyEngine-approved Aquarius deposit/withdraw/claim
  └── Session A: leader=…, coef=0.1, limits=…
  └── Session B: leader=…, coef=1.0, paused
  └── Session C: …
```

Limits are **per session** in v1. Optional account-wide cap is later.

---

## Session state machine

```text
draft ──arm──► active ──pause──► paused
                  │                 │
                  │◄──── resume ────┘
                  ├──stop / disarm──► stopped
                  └──(policy revoked)──► stopped
```

| State | Keeper |
|-------|--------|
| `draft` | No |
| `active` | Yes (allowed op kinds) |
| `paused` | No |
| `stopped` | No (terminal; open a new session) |

Arming = configure/enable policy **and** `register_session`.  
**Disarm** = revoke policy scope + `stop_session` (primary safety control).

---

## Web flow

1. Connect wallet  
2. Pick leader (Leaders page / paste address)  
3. Configure: coefficient, `follow_claims` (default on), `auto_claim_idle` (default off), per-op / daily limits  
4. Arm (policy + register)  
5. Active panel: pending / executed / rejected / skipped; tx links; failure reasons  
6. Pause / Resume / Disarm  
7. Optional: “Sell claimed fees via LumAgg” (separate consent)

**Fallback:** if auto-exec fails or user prefers, keep v0 “generate draft / sign manually”.

---

## Contract sketch

### CopyEngine

```text
register_session({
  leader,
  coefficient,           // fixed-point
  follow_claims,
  auto_claim_idle,
  idle_claim_after_secs,
  idle_min_fee_quote,    // optional
  max_per_op_quote,
  max_daily_quote,
  venue,                 // aquarius
}) -> session_id

pause_session / resume_session / stop_session

# Leader-event authenticity (required — do NOT trust keeper-supplied amounts alone)
record_leader_event(event)   // only EVENT_RECORDER role / multisig in v1
  // event: source_event_id, leader, pool, kind, amounts, ledger/tx metadata
  // idempotent on source_event_id; stores canonical amounts on-chain

execute_copy_op(session_id, source_event_id)  // amounts loaded from recorded event × coef
execute_idle_claim(session_id, pool, …)       // only if auto_claim_idle; separate authenticity path
```

**`execute_copy_op` checks (fail closed):**

1. Session `active` and `session.leader` matches recorded event’s leader  
2. Op kind allowed (`claim` requires `follow_claims`)  
3. `source_event_id` **exists in on-chain event store** (recorded) and not already consumed for this session  
4. Scaled amounts = `recorded.amounts × coefficient` (computed in-contract; keeper does not supply authority amounts)  
5. Under `max_per_op` and `max_daily` (UTC calendar day)  
6. Invoke Aquarius via follower policy  

### Leader event authenticity (answers forge risk)

**Problem:** If a permissionless keeper could pass arbitrary `source_event_id` + amounts into `execute`, they could fake leader deposits and force the follower into unwanted LPs (spending the follower’s tokens under policy). Anti-replay alone only prevents *reusing* an id — it does **not** prove the id was a real leader action.

**v1 rule:** `execute_copy_op` **never** takes leader amounts from the keeper. It only executes against an event already stored by `record_leader_event`.

| Path | Who | Trust |
|------|-----|--------|
| `record_leader_event` | **EVENT_RECORDER** role (protocol key or small multisig; fed by the same indexer that already observes Aquarius) | Trusted ingest (explicit, auditable) |
| `execute_copy_op` | Any keeper | Permissionless; can only replay *recorded* events under session rules |
| Later | Inclusion proofs / watcher set / bonded recorders | Reduce recorder trust |

**Defense in depth (still required):**

- Policy: Aquarius LP entrypoints only (no swap)  
- Per-op / daily limits  
- Optional later: session **deposit budget** escrow so even a compromised recorder cannot spend more than the user pre-committed  
- One-click **disarm**  
- Monitoring: web shows every recorded event + execution  

**Attack notes:**

- Forged **deposit** = highest risk (moves user capital into pools) → blocked unless recorder first notarizes a fake event (recorder compromise), not by random keepers  
- Forged **withdraw/claim** without a real position generally fails at Aquarius; still must be recorded  
- Compromised **recorder** is the residual trust root in v1 — document honestly in SCF materials; T3 can add multisig recorders / proofs  

Idle claim does not use a leader `source_event_id`; authenticity is “on-chain unclaimed fees + idle rules” (adapter or Engine re-read), not keeper storytelling.

### Policy (smart account)

| Allow | Deny |
|-------|------|
| Whitelisted Aquarius deposit / withdraw / claim entrypoints when CopyEngine attests the op | Any swap / router / LumAgg |
| Owner signature = full authority | Unrelated transfers / policy self-widening by keeper |

Claimed-token sells use a **separate** user authorization path through LumAgg.

---

## Claim hybrid policy

| Flag | Behavior |
|------|----------|
| `follow_claims=true` (default) | On leader `claim_*` event → scaled claim |
| `auto_claim_idle=true` (default off) | If idle longer than `idle_claim_after_secs` and unclaimed fees ≥ `idle_min_fee_quote` → `execute_idle_claim` |
| Sell tokens | Never follow leader; optional LumAgg after |

Idle claim still respects per-op / daily limits.

---

## Relation to v0 Copy LP

| v0 (shipped) | v1 (this spec) |
|--------------|----------------|
| Off-chain session + ops queue | On-chain session authority |
| User signs each op / draft | Keeper auto-exec under policy |
| Honesty: no custody | Same; plus disarm |
| Traction for SCF | Auto-exec is the funded capability |

Indexer actor tagging and Leaders remain shared infrastructure.

---

## SCF tranche sketch (End User Application)

Canonical budget table: **`2026-08-07-scf-copy-lp-studio-milestones.md`** (~$100k example).

| Tranche | Focus |
|---------|--------|
| **T1** | CopyEngine + Policy tests; testnet; Arm/Disarm; recorder pipeline |
| **T2** | Keeper auto deposit/withdraw/claim; multi-session limits; Leaders→executed E2E |
| **T3** | Audit + mainnet; `auto_claim_idle`; LumAgg fee-exit UX; launch demo |

---

## Security notes

- No custody vault → reduces “rug the pool” surface; residual risk is **policy mis-scope**, **Engine bugs**, and **v1 EVENT_RECORDER honesty**  
- **Keepers cannot forge leader events** under v1: execute only consumes on-chain `record_leader_event` rows  
- Fail closed on limits; scaled amounts computed in-contract from recorded events  
- Disarm must be one click and owner-only  
- Permissionless execute: invalid/unrecorded ids → reject (keeper wastes own fee)  
- Audit before mainnet capital  

## Open implementation details (plan phase)

- Exact Aquarius entrypoint whitelist (CP vs CL function names)  
- Coefficient fixed-point scale and CL tick/liquidity scaling rules  
- EVENT_RECORDER key management / multisig threshold; indexer → `record_leader_event` pipeline  
- Idle claim: on-chain fee readout adapter vs attested proof with Engine re-check  
- Optional session deposit budget escrow  
- Policy upgrade / versioning UX for Freighter smart accounts  

These do not unblock writing the implementation plan; they are resolved in planning/TDD.
