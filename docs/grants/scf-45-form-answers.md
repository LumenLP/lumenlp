# SCF #45 Open Track Form Answers

This document maps the current SCF Build Award form to suggested answers for LumenLP Copy LP Studio.

## Submission Information

### Submission Title

**LumenLP: Automated Copy LP**

### Project Type

**End User Application**

This is a user-facing Stellar DeFi product. It is not only an analytics API and is not a custodial vault.

### Project URL

`https://lumenlp.xyz`

### Technical Architecture Document

Use the public GitHub URL after confirming the repository is accessible:

`https://github.com/LumenLP/lumenlp/blob/main/docs/architecture.md`

The document must be public and should describe the current RPC/indexer/API architecture, Copy Engine, Soroban Policy, relayer, trust boundaries, and the separately authorized DEX aggregator boundary.

### GitHub URL

`https://github.com/LumenLP/lumenlp`

### Video URL

Upload a short 16:9 video, ideally under three minutes, showing:

1. Pool ranking and pool detail.
2. Leader profile with claimed fees and activity.
3. Starting a Copy LP session.
4. Leader event becoming a scaled CopyOp.
5. The planned policy-controlled automation flow.

Do not present the future automatic execution as already live. Label the current flow as the working MVP and the policy execution as the grant milestone.

## Products & Services

LumenLP Copy LP Studio is a non-custodial automation product for Stellar liquidity providers. It connects pool discovery across Stellar DEXes, observable LP behavior, and automated liquidity operations in one workflow. The target coverage includes Aquarius, Phoenix, Sushi V3, Soroswap AMM, and Comet, with a common adapter model for adding further Stellar DEX pool venues. Aquarius is the first production venue today; the remaining venues will be enabled progressively after their pool state, event model, and LP operations are validated.

Users can compare Stellar DEX pools using TVL, liquidity, fees, Fee/TVL, and recent activity. They can then inspect Leader profiles showing claimed fees, deposits, withdrawals, pools touched, current exposure, and source on-chain events. These metrics are presented as observable data signals rather than guaranteed profit or complete PnL.

After selecting a Leader, a user configures a copy coefficient and safety limits. LumenLP creates copy intents for supported LP actions across integrated Stellar DEX pools, initially deposits, withdrawals, and fee claims. The final product will execute these operations through a Soroban policy-controlled account and a LumenLP relayer. The policy limits allowed pools and entrypoints, maximum amount per operation, daily exposure, slippage, expiry, and replay. Users can pause or disarm the policy and LumenLP never stores their private keys or operates a custodial vault.

Leader swaps are not mirrored because a swap may be a fee exit, a position adjustment, or an unrelated trade. Copy LP focuses on verifiable liquidity actions rather than arbitrary wallet activity.

The current mainnet product already includes pool discovery, snapshots, event indexing, Leader profiles, and a user-reviewed scaled Copy LP queue, with Aquarius as the first production venue. The Build Award will turn this working MVP into a testnet-validated and limited-mainnet automated product covering the target Stellar DEX pool venues through reusable adapters.

## Traction Evidence

LumenLP is already deployed and processing Stellar mainnet data.

- Website: https://lumenlp.xyz
- API health: https://api.lumenlp.xyz/health
- API support matrix: https://api.lumenlp.xyz/v1/venues
- Public Aquarius pool ranking with TVL, liquidity, fee, Fee/TVL, and activity windows.
- Aquarius pool detail pages with historical snapshots and recent events.
- Leader ranking and profile pages based on indexed liquidity activity.
- Copy LP sessions with scaled operations tied to source events.
- Production indexer, snapshotter, API, and web deployment.
- Open technical documentation and API specification in the repository.

The current product is intentionally honest about its data coverage. Claimed fees and activity are shown as observable signals, not as complete profitability claims. The existing user-reviewed Copy LP flow provides the validation base for the grant milestone: policy-controlled automatic execution under strict limits.

Current API evidence (checked August 14, 2026):

- `183` distinct pools with indexed events;
- `53` pools with rollup metrics;
- `179,519` indexed events;
- `87,472` indexed swaps;
- `100` Leader profiles returned for the 30-day ranking endpoint;
- latest indexer cursor: ledger `63,945,510`.

These numbers should be refreshed immediately before submission and accompanied by links to the public API endpoints.

## Resubmission Feedback

Leave blank for a first-time submission.

## Ambassador Affiliation

Use the accurate answer:

`[No formal Stellar Ambassador Chapter affiliation at this time.]`

If there is an actual mentor, chapter, or community relationship, name it specifically rather than using a generic affiliation.

## Thumbnail

Upload a 16:9 LumenLP image showing the Copy LP product. Recommended content:

- LumenLP logo;
- headline: `Track LPs. Copy the best. Automate the rest.`;
- small visual showing `Leader event → Policy → Aquarius`;
- dark LumenLP green visual system;
- no browser chrome or personal account details.

## Team Members

Add both confirmed contributors as real team members. Do not add placeholder members.

## Mailing List

Select at least one active team member who will monitor SCF communications.

## Team Description

Lijiao Zhou is the founder and primary builder of LumenLP. He is a full-stack engineer with hands-on experience building automated LP rebalancing systems and working with concentrated-liquidity designs including CLMMs and DLMMs, liquidity ranges, position management, and rebalancing strategies.

He has built the current LumenLP stack end to end: Aquarius and Soroban RPC integration, pool hydration, TVL and fee calculations, event indexing, Leader activity profiles, the Copy LP queue, API services, and the deployed web application. LumenLP is currently running with a production API and indexer on Stellar mainnet.

LinkedIn: https://www.linkedin.com/in/yaransu/

Shilpa Chittara is a Web3 engineering leader and protocol architect specializing in blockchain infrastructure, smart contract systems, and developer platforms. She has more than eight years of experience building production-grade systems across Ethereum, Solana, Polygon, and EDU Chain.

As Head of Engineering at StreamNFT and NotAlone Ventures, she has led the design and delivery of infrastructure for NFT lending, blockchain indexing, governance-driven investment protocols, bonding-curve token systems, and developer SDKs. She also led the on-chain issuance of more than 10 million student credentials on EDU Chain. For LumenLP, she will contribute to protocol architecture, smart contract design, execution safety, and production infrastructure.

LinkedIn: https://www.linkedin.com/in/shilpachittara

Specialized security review and testing will be handled through the SCF-supported review process and qualified external assistance where appropriate.

## Recommended Budget

For a two-person team delivering the first automation release and expanding beyond Aquarius, a reasonable request is **$100,000 USD-equivalent in XLM**, subject to matching the amount already indicated in the Interest Form.

The required payment structure is:

| Tranche | Percentage | Amount on $100,000 request |
|---|---:|---:|
| Tranche #0 | 10% | $10,000 |
| Tranche #1 | 20% | $20,000 |
| Tranche #2 | 30% | $30,000 |
| Tranche #3 | 40% | $40,000 |

Do not include marketing, promotion, or external security audit costs in the budget. Security audit support is handled separately by SCF according to the form guidance.

## Tranche #1 Deliverables

**Target completion date:** `[Choose a date approximately 6–8 weeks after award approval]`

The existing Aquarius pool analytics, event indexer, snapshots, Leader profiles, and user-reviewed Copy LP queue are the starting product and are not counted as new tranche deliverables.

### 1. Unified Stellar DEX adapter coverage — $9,000

Define a common adapter interface for pool discovery, LP state reads, liquidity events, and supported deposit, withdrawal, and claim operations across Stellar DEX pool venues. The target coverage includes Aquarius, Phoenix, Sushi V3, Soroswap AMM, and Comet. Each venue will be enabled only after its pool state, event model, and LP operations are validated, with venue-specific capability reporting and fail-closed behavior for unsupported operations. Add contract fixtures and deterministic compatibility tests across the supported venue set.

**Completion:** all target venues are represented in the adapter registry and support matrix; each validated venue is visible in the pool API with its capabilities and limitations; supported event and operation fixtures pass; unsupported operations fail closed.

### 2. Automated Copy LP policy specification and testnet prototype — $6,000

Turn the current user-reviewed Copy LP flow into an implementation-ready automation specification. Define the Soroban policy account interface, allowed pool and entrypoint model, copy coefficient, per-operation and daily limits, expiry, nonce, replay protection, pause, disarm, and manual fallback. Implement the first testnet policy prototype.

**Completion:** the specification is public; testnet policy state can be created, inspected, paused, and disarmed; out-of-policy intents are rejected in repeatable tests.

### 3. Cross-venue operation and safety test suite — $5,000

Build end-to-end tests covering source-event to copy-intent mapping for Aquarius and the new venue, including coefficient scaling, unsupported pool actions, expired intents, insufficient balance, duplicate events, and manual user-signed fallback.

**Completion:** the test suite runs in CI or a documented reproducible environment and demonstrates that invalid, ambiguous, or duplicated source activity cannot create an executable operation.

**Tranche #1 total: $20,000**

## Tranche #2 Deliverables

**Target completion date:** `[Choose a date approximately 3 months after award approval]`

### 1. Soroban Copy Policy and account authorization — $10,000

Implement the policy-controlled execution boundary for supported Stellar DEX LP operations. Support venue, pool, and entrypoint allowlists, copy coefficients, maximum amount per operation, daily limits, expiry, nonce, replay protection, pause, and disarm.

**Completion:** testnet registration, authorization, pause, disarm, and rejection of out-of-policy operations work in repeatable tests.

### 2. LumenLP Copy Engine and relayer — $8,000

Implement the service that converts indexed Leader events into validated copy intents and submits only policy-approved operations through the supported DEX adapters. Keep the interface compatible with a future independent keeper, while using a LumenLP-operated relayer for the initial product.

**Completion:** a permitted testnet deposit, withdrawal, and claim can be executed automatically; duplicate events and over-limit operations are rejected.

### 3. On-chain monitoring plan and threat model — $6,000

Document and implement monitoring for policy state, relayer actions, source event identifiers, failed operations, amount limits, daily exposure, pause/disarm state, and suspicious execution patterns. Produce a threat model covering forged Leader events, compromised relayer, replay, policy widening, incorrect actor attribution, RPC failure, and supported venue contract or adapter changes.

**Completion:** monitoring dashboards or alert outputs are available on testnet; threat model and incident response runbook are published; each high-severity threat has a mitigation or fail-closed behavior.

### 4. Testnet end-to-end validation — $6,000

Run end-to-end scenarios with real testnet accounts, including permitted actions, insufficient balances, expired intents, daily-limit exhaustion, pause, disarm, and relayer downtime with manual fallback.

**Completion:** testnet walkthrough is reproducible and evidence includes transaction hashes, rejected-operation examples, and execution logs.

**Tranche #2 total: $30,000**

## Tranche #3 Deliverables

**Target completion date:** `[Choose a date approximately 4 months after award approval]`

### 1. Limited multi-venue mainnet launch — $14,000

Deploy the policy-controlled Copy LP flow to mainnet with conservative limits and a staged rollout. Support Aquarius and at least one additional Stellar DEX for deposit, withdrawal, and claim operations where event and position mapping are reliable.

**Completion:** limited external users can arm a policy, observe a supported Leader event, and complete a policy-approved mainnet Copy LP operation on each production-enabled venue with public transaction links.

### 2. Execution history and operational reliability — $8,000

Add complete execution history, transaction links, failure reasons, relayer health, indexer lag visibility, and safe recovery paths. Preserve the manual user-signed fallback.

**Completion:** every automatic operation is traceable from source event to intent, policy decision, submitted transaction, and final status.

### 3. Separately authorized DEX aggregator integration — $8,000

Add a separate user-authorized path to quote and swap claimed fee tokens through a DEX aggregator such as Soroswap or LumAgg. This flow must not inherit unrestricted authority from the Copy LP policy.

**Completion:** the user can explicitly authorize a fee-token swap with the selected aggregator, route, amount, slippage, and expiry shown before execution; the Copy LP policy cannot authorize arbitrary swaps.

### 4. Public documentation and launch package — $10,200

Publish the architecture, policy model, data methodology, API usage, limitations, operational runbook, and a short public walkthrough. Include evidence of external testing and known limitations.

**Completion:** documentation is accessible from the website and repository; the mainnet demo can be independently followed; all grant deliverables and links are public.

**Tranche #3 total: $40,000**

## Completion Dates

The form requires dates in `DD/MM/YYYY` format. These dates assume the award project starts on **01/09/2026**, after SCF approval and Tranche #0. If the official award start date is different, shift all three dates by the same number of days.

- **Tranche #1:** `06/10/2026` — unified adapter coverage for the target Stellar DEX venues, policy prototype, and safety tests (5 weeks).
- **Tranche #2:** `10/11/2026` — testnet Policy, relayer, monitoring plan, threat model, and cross-venue validation (5 weeks).
- **Tranche #3:** `22/12/2026` — limited multi-venue mainnet launch, DEX aggregator boundary, reliability, and documentation (6 weeks).

## Important Submission Decisions

- Select **End User Application**.
- Submit one real team member unless a second contributor is confirmed and can create an SCF account.
- Treat Aquarius as the first supported venue and build toward broader Stellar DEX pool coverage through reusable adapters.
- Present automatic execution as the grant outcome, not as an already-live feature.
- Include the required on-chain monitoring plan and threat model in Tranche #2.
- Keep the manual user-signed path as a fallback.
