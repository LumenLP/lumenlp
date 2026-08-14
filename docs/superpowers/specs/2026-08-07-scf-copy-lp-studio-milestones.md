# LumenLP — SCF Build Tranches (Copy LP Studio / End User)

**Date:** 2026-08-07  
**Supersedes for this application:** `2026-08-05-scf-tooling-milestones.md` (keep as optional multi-DEX appendix / later award)  
**Design:** `2026-08-07-copy-lp-studio-onchain-design.md` (**Path B locked**)  
**Team:** **2 core builders**  
**Category:** **End User Application**  
**Track:** Build / Open  
**Timeline:** ≤6 months  
**Requested budget (example):** **$100,000**  
**Payment shape:** Tranche #0 = 10% ($10k) on award; #1 = 20%; #2 = 30%; #3 = 40%.

> Form needs **3 paid tranches**, each with **N deliverables** (Completion + Budget).  
> Architecture: Soroban **smart-account scoped policy** + **CopyEngine** + **EVENT_RECORDER** + permissionless **execute**; MIT open source.

## Product north star

**Copy LP Studio** on Stellar: discover Aquarius LP leaders → arm non-custodial auto-copy (deposit / withdraw / claim) via smart-account policy → optional idle claim + LumAgg fee-exit (not mirrored swaps).

| Already live (traction) | This award ships |
|-------------------------|------------------|
| lumenlp.xyz Pools / Leaders / Copy queue | CopyEngine + Policy contracts |
| Indexer with actor-tagged LP events | `record_leader_event` + keeper execute |
| User-signed / draft copy (v0) | Auto-exec under policy; disarm; limits |
| LumAgg (sister) | Post-claim sell UX (out of copy policy) |

## One-liner

Non-custodial **LP copy trading** for Stellar: follow Aquarius leaders with coefficient + limits, auto-executed through Soroban smart-account policy — MIT open source, no custody vault.

## Out of scope (this award)

- Custodial copy vault  
- Mirroring leader swaps  
- Full stop-loss Guard product (may appear only as late optional stretch)  
- Five production DEX adaptors as primary deliverable (see old tooling doc for later)

---

## Tranche 1 — Contracts + Arm UX · **$20,000**

1. **CopyEngine + Policy contracts (tested)**  
   - Completion: Session register/pause/stop; `record_leader_event` (recorder-gated); `execute_copy_op` reads recorded amounts × coefficient; unit tests for forge-reject (unrecorded id) and anti-replay.  
   - Budget: **$8,000**

2. **Testnet deploy + web Arm / Disarm**  
   - Completion: Testnet addresses published; `/copy` can arm policy + register session, pause, one-click disarm; Freighter (or documented wallet) path.  
   - Budget: **$7,000**

3. **EVENT_RECORDER pipeline from indexer**  
   - Completion: Indexer/service calls `record_leader_event` for Aquarius deposit/withdraw/claim with actor; idempotent; ops runbook for recorder key.  
   - Budget: **$5,000**

- **Tranche 1 total: $20,000**

---

## Tranche 2 — Auto-copy live loop · **$30,000**

1. **Keeper executor (deposit / withdraw / claim)**  
   - Completion: Permissionless `execute_copy_op` against recorded events; reference keeper binary MIT; protocol-run keeper for availability; fail-closed limits.  
   - Budget: **$10,000**

2. **Multi-session + limits UX**  
   - Completion: One account, many leaders; per-session `max_per_op` / `max_daily`; reject reasons in UI; no silent downscale.  
   - Budget: **$8,000**

3. **Leaders → Copy → executed history E2E**  
   - Completion: Testnet demo: pick leader on Leaders → arm → leader-like event recorded → execute → history with tx links; v0 manual fallback retained.  
   - Budget: **$7,000**

4. **Security writeup**  
   - Completion: Public doc: recorder trust root, keeper cannot forge amounts, disarm, MIT self-host instructions.  
   - Budget: **$5,000**

- **Tranche 2 total: $30,000**

---

## Tranche 3 — Mainnet + polish · **$40,000**

1. **Audit + mainnet deploy**  
   - Completion: External review or equivalent checklist signed off; mainnet CopyEngine/Policy; limited rollout.  
   - Budget: **$14,000**

2. **`auto_claim_idle` + claim hybrid**  
   - Completion: Follow-leader claims + optional idle claim per spec; UI toggles.  
   - Budget: **$10,000**

3. **LumAgg post-claim sell (out of band)**  
   - Completion: Separate user flow to sell claimed tokens via LumAgg; **not** on copy policy whitelist.  
   - Budget: **$8,000**

4. **Launch package**  
   - Completion: Public demo video; docs; MIT repo tagged release; mainnet health runbook.  
   - Budget: **$8,000**

- **Tranche 3 total: $40,000**

---

## Budget rollup (@ $100k)

| Tranche | % | Amount |
|---------|---|--------|
| #0 Award acceptance | 10% | $10,000 |
| #1 Contracts + Arm | 20% | $20,000 |
| #2 Auto-copy loop | 30% | $30,000 |
| #3 Mainnet + polish | 40% | $40,000 |
| **Total** | | **$100,000** |

Adjust ±$10–20k if panel feedback on audit line; keep Completion + Budget per deliverable.

## Application blurb

> **LumenLP** is an End User Application for **non-custodial LP copy trading** on Stellar. Users discover Aquarius leaders (live Leaders board), set a coefficient and limits, and arm a Soroban **smart-account policy** so keepers can auto-mirror deposit/withdraw/claim — without a custody vault and without copying swaps. Leader events are notarized on-chain by a protocol recorder (keepers cannot forge amounts). MIT open source; mainnet analytics + manual Copy queue already live as traction.

## Traction links

- Site: https://lumenlp.xyz · https://lumenlp.xyz/leaders · https://lumenlp.xyz/copy  
- API: https://api.lumenlp.xyz/health · https://api.lumenlp.xyz/v1/lp/leaders  
- Design: `docs/superpowers/specs/2026-08-07-copy-lp-studio-onchain-design.md`  
- Sister: LumAgg (aggregator) for optional fee exit  

## Execution notes

- Path **B only** (smart-account + CopyEngine). No Privy-style delegated copy wallet as primary.  
- Recorder = protocol-operated (or multisig); execute = permissionless on recorded events.  
- Differentiated from stop-loss Guard products: we mirror **LP actions**, not generic exits.
