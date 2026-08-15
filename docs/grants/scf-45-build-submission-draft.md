# SCF #45 Build Award Submission Draft

**Project:** LumenLP Copy LP Studio  
**Website:** https://lumenlp.xyz  
**API:** https://api.lumenlp.xyz  
**Status:** Invited after Interest Form approval  
**Deadline:** August 16, 2026

> This is the full-submission working draft. Replace bracketed fields before submitting through the SCF Dashboard.

## Project Summary

LumenLP Copy LP Studio is a non-custodial automation product for Stellar liquidity providers. It helps users discover active Aquarius LPs, inspect observable on-chain activity, select a Leader to follow, and automatically mirror approved liquidity actions under user-defined policy limits.

The product starts with Aquarius and focuses on the complete LP workflow:

```text
Discover pools
    → inspect Leader activity
    → choose copy coefficient and limits
    → policy-controlled automation
    → Aquarius deposit / withdraw / claim
```

LumenLP does not mirror arbitrary swaps. Optional fee-token conversion is handled as a separate, explicitly authorized DEX aggregator flow, using an aggregator such as Soroswap or LumAgg.

## One-Line Description

LumenLP lets Stellar LPs discover and automatically copy observable Aquarius liquidity strategies through user-controlled Soroban policies.

## Problem

Stellar LPs face three practical barriers:

1. It is difficult to identify pools with meaningful liquidity and fee activity.
2. It is difficult to understand which LP accounts are actively providing liquidity and claiming fees.
3. Even after finding a useful strategy, users must manually monitor events and reconstruct deposit, withdrawal, and claim transactions.

This creates friction for both new and experienced LPs. Users may provide liquidity once, but without monitoring and automation they are less likely to maintain, rebalance, or scale their positions.

Existing dashboards solve only the first part of the problem. LumenLP connects discovery to controlled execution.

## Solution

LumenLP combines four components:

### Pool intelligence

An RPC-first Aquarius data layer discovers pools and calculates observable metrics including TVL, liquidity, fees, Fee/TVL, and activity windows.

### Leader discovery

The indexer identifies liquidity actions and actors. Leader profiles show claimed fees, deposits, withdrawals, pools touched, current exposure, and recent activity.

These are presented as observable data signals. LumenLP does not describe claimed fees as complete profit or guarantee future performance.

### Copy LP engine

When a selected Leader performs a supported liquidity action, LumenLP creates a scaled copy intent using the configured coefficient. The initial supported actions are:

- Aquarius deposit;
- Aquarius withdrawal;
- Aquarius fee claim;
- later, concentrated-liquidity position open, close, and adjustment.

Leader swaps are excluded because a swap cannot reliably be classified as a liquidity strategy action.

### Policy-controlled automation

The final product uses a Soroban policy / smart-account layer and a LumenLP relayer. The relayer may submit an operation, but the policy remains the authority for:

- allowed Aquarius pools and entrypoints;
- maximum amount per operation;
- maximum daily amount;
- copy coefficient;
- slippage and expiry;
- nonce and replay protection;
- pause and disarm.

Users retain control of their account and can stop the policy. LumenLP does not hold user private keys or operate a custodial vault.

## Current Progress

The project is already running on Stellar mainnet:

- public web application at `lumenlp.xyz`;
- public API at `api.lumenlp.xyz`;
- Aquarius pool discovery through Soroban RPC;
- periodic pool snapshots;
- event and swap indexer;
- pool ranking with TVL, liquidity, fee, Fee/TVL, and activity windows;
- pool detail pages and event history;
- Leader ranking and profile pages;
- Copy LP sessions;
- scaled CopyOp queue tied to indexed source events;
- user-reviewed Copy LP draft flow;
- production server and monitoring runbook;
- architecture and metric methodology documentation.

The current Copy LP flow is intentionally user-reviewed. The grant will fund the transition from event-driven copy drafts to policy-constrained automation.

## Why Stellar

Stellar has an active but fragmented AMM ecosystem and a fast, low-cost transaction environment that is suitable for frequent liquidity operations. However, LP tooling remains less mature than the user experience available on larger DeFi ecosystems.

LumenLP is built directly around Stellar and Soroban primitives:

- Soroban RPC for on-chain pool and event data;
- Aquarius contracts as the first production venue;
- Soroban policies for scoped automation;
- Stellar wallets for user authorization;
- A DEX aggregator such as Soroswap or LumAgg for separately authorized fee-token conversion.

The product is designed to increase the usability and retention of Stellar liquidity, not merely to display market data.

## Target Users

- Existing Stellar LPs who want to monitor and automate positions.
- Users who want to follow observable LP behavior without delegating unrestricted custody.
- Advanced LPs who need configurable limits and execution history.
- Stellar DeFi applications that need pool, position, and LP activity data.
- Protocol teams that want to direct users toward safer liquidity strategies.

## Product Workflow

### 1. Discover

Users compare Aquarius pools by TVL, liquidity, fee, Fee/TVL, and recent activity.

### 2. Inspect

Users open a Leader profile and inspect claimed fees, LP actions, pools touched, current exposure, and source transactions.

### 3. Configure

Users choose a Leader and configure:

- copy coefficient;
- supported action types;
- pool allowlist;
- maximum amount per operation;
- maximum daily amount;
- slippage and expiry limits;
- pause / disarm controls.

### 4. Automate

The Copy Engine creates an intent from an indexed Leader event. The policy checks the intent, and the LumenLP relayer submits only operations within the policy scope.

### 5. Verify

The user sees the source event, target pool, scaled amounts, execution status, transaction hash, and any failure reason.

## Technical Architecture

```text
Aquarius contracts
        │
        ▼
Soroban RPC / local RPC node
        │
        ├── Snapshotter → pool snapshots and TVL metrics
        └── Pool Indexer → events, actors, swaps, rollups
                                  │
                                  ▼
                              API Server
                                  │
                         Web / Copy Studio
                                  │
                         Copy Engine intents
                                  │
                    Soroban Policy / Smart Account
                                  │
                         LumenLP Relayer
                                  │
                     Aquarius LP operations
```

The data layer is authoritative for observed history and metrics. The Soroban policy is authoritative for follower permissions and execution limits. The relayer is an execution service, not a custodian.

The system fails closed when actor attribution, source event metadata, token pricing, or position mapping is unavailable.

## Scope of This Award

The current Aquarius analytics, event indexer, snapshots, Leader profiles, and user-reviewed Copy LP queue are the working MVP and are not counted as new grant deliverables. The milestones below describe the work that remains to turn that MVP into an automated, multi-venue Stellar product.

### Milestone 1: DEX expansion and automation foundation

**Deliverables**

- Define a reusable adapter boundary for pool discovery, LP state, liquidity events, and deposit, withdrawal, and claim operations.
- Add the first additional Stellar DEX venue while retaining Aquarius as the reference implementation.
- Define the Soroban policy account interface, limits, pause/disarm behavior, and manual fallback for automated Copy LP.
- Add cross-venue fixtures and safety tests for scaling, unsupported operations, expired intents, duplicate events, and ambiguous source activity.

**Completion criteria**

- Both venues are represented through the same adapter boundary and the additional venue is visible in the pool API.
- The policy prototype can be created, inspected, paused, and disarmed on testnet.
- Invalid, ambiguous, expired, or duplicated source activity cannot create an executable operation.
- The adapter and policy specifications, fixtures, and test results are public.

### Milestone 2: Policy-controlled automation

**Deliverables**

- Implement Soroban Copy Policy / Smart Account integration for supported DEX adapters.
- Add venue, pool, and entrypoint allowlists.
- Add per-operation and daily limits.
- Add nonce, expiry, and replay protection.
- Add pause, resume, and disarm.
- Implement LumenLP Relayer for policy-approved operations across supported venues.
- Deploy and validate the cross-venue flow on testnet.

**Completion criteria**

- A user can configure and arm a copy policy on testnet.
- A permitted Leader deposit or withdrawal is executed automatically within configured limits on supported venues.
- An operation exceeding a limit is rejected on-chain.
- A user can pause or disarm the policy and prevent new execution.
- The manual user-signed path remains available as a fallback.

### Milestone 3: Multi-venue mainnet launch and aggregator boundary

**Deliverables**

- Limited mainnet deployment for Aquarius and at least one additional Stellar DEX.
- Execution history with transaction links and failure reasons.
- Concentrated-liquidity action support where event and position mapping are reliable.
- Optional, separately authorized fee-token conversion through a DEX aggregator such as Soroswap or LumAgg.
- Security review, operational runbook, and public documentation.
- External user walkthrough and feedback cycle.

**Completion criteria**

- Mainnet copy flow works for supported actions on both venues.
- No unrestricted server-side wallet custody is introduced.
- Every automatic action is policy-checked and traceable.
- Aggregator-based fee-token conversion cannot expand Copy LP permissions into arbitrary swaps.
- Public documentation explains limitations, data coverage, and trust boundaries.

## Success Metrics

The following metrics should be finalized before submission:

- `[N]` indexed Aquarius pools with current snapshots;
- `[N]` indexed LP events with actor attribution;
- `[N]` Leader profiles with 7-day and 30-day activity;
- `[N]` Copy LP sessions created during the grant;
- `[N]` successful testnet Copy LP operations;
- `[N]` successful mainnet Copy LP operations after limited rollout;
- `>= [N]` external testers;
- `>= [N]` Stellar ecosystem integrations or pilot users;
- `>= [99.x]%` API / relayer availability after mainnet launch;
- `0` incidents involving unrestricted custody or policy bypass.

The product will not use guaranteed-profit, win-rate, or unsupported PnL claims as success metrics.

## Open Source and Ecosystem Value

The project will publish reusable components and documentation where practical:

- Aquarius event parsing and actor attribution;
- TVL and Fee/TVL methodology;
- Copy operation schemas and scaling rules;
- Soroban policy interfaces;
- API and integration documentation;
- deployment and operational runbooks;
- examples for wallets and Stellar DeFi applications.

The public web application provides an accessible reference implementation, while the API and event schemas allow other ecosystem projects to reuse the data layer.

## Security and Trust Model

LumenLP will not store user private keys or operate a custodial vault.

The main residual risks are handled as follows:

### False Leader event or actor attribution

The indexer stores source ledger and transaction metadata and fails closed when it cannot resolve the actor. Automatic execution will not trust arbitrary keeper-supplied amounts.

### Compromised relayer

The relayer cannot widen policy authority. It can only submit operations accepted by the Soroban policy. Users can pause or disarm the policy.

### Excessive capital exposure

Per-operation, daily, pool, coefficient, slippage, and expiry limits are enforced before execution.

### Replay or duplicate execution

Copy operations use source event identifiers, session identifiers, nonces, and consumed-operation tracking.

### Incorrect performance interpretation

Leader pages show observable fees and activity as labeled signals. They do not claim complete PnL when cost basis or historical coverage is incomplete.

## Risks and Mitigation

| Risk | Mitigation |
|---|---|
| Limited historical RPC retention | Start durable indexing immediately; document coverage; use external history only where necessary |
| Aquarius event format changes | Contract fixtures, parser tests, and sampled mainnet checks |
| Incorrect CL position mapping | Support CL automation only after reliable position identifiers are available |
| Relayer downtime | Keep manual signing fallback; design relayer interface for future independent keepers |
| Policy or contract bug | Testnet rollout, fail-closed limits, disarm control, internal security review, limited mainnet launch |
| Misleading Leader ranking | Show raw observable data and explicit methodology instead of guaranteed profitability claims |

## Budget and Tranches

**Requested amount:** `[USD/XLM AMOUNT TO CONFIRM]`

Suggested allocation structure:

| Tranche | Scope | Amount |
|---|---|---:|
| Tranche 1 | Reusable DEX adapter boundary, first additional venue, policy prototype, and safety tests | `[AMOUNT]` |
| Tranche 2 | Soroban Policy, testnet automation, relayer, safety limits | `[AMOUNT]` |
| Tranche 3 | Limited multi-venue mainnet launch, DEX aggregator boundary, reliability, security, and docs | `[AMOUNT]` |
| **Total** |  | **`[TOTAL]`** |

The budget should be tied to completed, verifiable deliverables rather than general operating costs.

## Team

- **Project lead:** `[NAME / ROLE]`
- **Engineering:** `[NAME / ROLE]`
- **Relevant experience:** hands-on experience building automated LP rebalancing systems and working with concentrated-liquidity designs including CLMMs and DLMMs.
- **Stellar experience:** Aquarius and Soroban RPC integration, pool hydration, event indexing, LP metrics, and deployed mainnet services.

## Current Links

- Website: https://lumenlp.xyz
- API health: https://api.lumenlp.xyz/health
- API venues: https://api.lumenlp.xyz/v1/venues
- Repository: https://github.com/LumenLP/lumenlp
- Architecture: `docs/architecture.md`
- API specification: `docs/openapi.yaml`

## Final Positioning

LumenLP is not only a pool dashboard and not a custodial copy-trading vault. It is a Stellar-native LP automation product that connects verifiable LP activity with policy-controlled execution.

The first production venue is Aquarius. The immediate grant outcome is a safe and usable Copy LP Studio. The long-term product is automated LP strategy execution where users define the authority and limits, while LumenLP handles monitoring, intent creation, and execution within those constraints.

## Submission Checklist

- [ ] Confirm Build Award track in the SCF Dashboard.
- [ ] Confirm requested amount and tranche budgets.
- [ ] Add team names, roles, and contact information.
- [ ] Add public repository URL.
- [ ] Add current usage / tester numbers.
- [ ] Confirm whether the project is an individual or entity submission.
- [ ] Verify website, API, and demo links.
- [ ] Attach a short Copy LP walkthrough or screen recording if allowed.
- [ ] Submit before August 16, 2026.
