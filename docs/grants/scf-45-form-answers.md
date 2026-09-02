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

LumenLP Copy LP Studio is a non-custodial automation product for Stellar liquidity providers. It connects pool discovery, observable LP behavior, and policy-controlled liquidity operations in one workflow. The first funded product focus is reliable automated Copy LP for Aquarius. The architecture also supports additional Stellar DEX pool venues, including Phoenix, Sushi V3, Soroswap AMM, and Comet, but those integrations will be gated by validated contract behavior and demonstrated user demand rather than treated as a coverage target by themselves.

Users can compare Stellar DEX pools using TVL, liquidity, fees, Fee/TVL, and recent activity. They can then inspect Leader profiles showing claimed fees, deposits, withdrawals, pools touched, current exposure, and source on-chain events. These metrics are presented as observable data signals rather than guaranteed profit or complete PnL.

After selecting a Leader, a user configures a copy coefficient and safety limits. LumenLP creates copy intents for supported Aquarius LP actions, initially deposits, withdrawals, and fee claims. The funded automation layer will execute these operations through a Soroban policy-controlled account and a LumenLP relayer. The policy limits allowed pools and entrypoints, maximum amount per operation, daily exposure, slippage, expiry, and replay. Users can pause or disarm the policy and LumenLP never stores their private keys or operates a custodial vault.

Leader swaps are not mirrored because a swap may be a fee exit, a position adjustment, or an unrelated trade. Copy LP focuses on verifiable liquidity actions rather than arbitrary wallet activity.

The deployed foundation is the pool indexer, snapshots, analytics API, Leader profiles, and user-reviewed scaled Copy LP workflow. These components remain under active testing and optimization and are not presented as the funded automation outcome. The Build Award will fund the new policy-controlled execution layer, testnet validation, monitored Aquarius launch, and adoption measurement. Additional DEX execution will proceed only when the core Aquarius flow is used repeatedly by external LPs and the venue-specific safety checks pass.

Existing Stellar DEX tools generally expose swaps, quotes, or pool statistics. LumenLP adds an end-to-end LP action workflow: it attributes observable liquidity events to a Leader, converts them into bounded copy intents, and routes them through a user-controlled Soroban policy with explicit allowlists, limits, replay protection, pause, and disarm. This makes LP strategy following an auditable, non-custodial operation rather than only a data or routing experience.

## Traction Evidence

LumenLP is deployed and processing Stellar mainnet data.

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

Panel-requested revision. We have narrowed the funded scope to Aquarius-first automated Copy LP, reduced the budget, added measurable external-user adoption targets, clarified how policy limits preserve risk boundaries, separated the existing analytics foundation from new grant-funded automation, and added a two-person engineering-day cost breakdown.

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

He has built the current LumenLP foundation end to end: Aquarius and Soroban RPC integration, pool hydration, TVL and fee calculations, event indexing, Leader activity profiles, the Copy LP queue, API services, and the deployed web application. LumenLP is currently running a deployed API and indexer on Stellar mainnet, with the data pipeline still under active testing and optimization.

LinkedIn: https://www.linkedin.com/in/yaransu/

Shilpa Chittara is a Web3 engineering leader and protocol architect specializing in blockchain infrastructure, smart contract systems, and developer platforms. She has more than eight years of experience building production-grade systems across Ethereum, Solana, Polygon, and EDU Chain.

As Head of Engineering at StreamNFT and NotAlone Ventures, she has led the design and delivery of infrastructure for NFT lending, blockchain indexing, governance-driven investment protocols, bonding-curve token systems, and developer SDKs. She also led the on-chain issuance of more than 10 million student credentials on EDU Chain. For LumenLP, she will contribute to protocol architecture, smart contract design, execution safety, and production infrastructure.

LinkedIn: https://www.linkedin.com/in/shilpachittara

Specialized security review and testing will be handled through the SCF-supported review process and qualified external assistance where appropriate.

## Recommended Budget

For a two-person team delivering the Aquarius-first automation release, a reasonable reduced request is **$80,000 USD-equivalent in XLM**. The budget funds the core policy, execution, safety, and adoption validation work first. Phoenix, Sushi V3, Soroswap AMM, and Comet remain documented expansion paths, but their execution work is gated on demonstrated usage and is not priced as if demand were already validated.

The required payment structure is:

| Tranche | Percentage | Amount on $80,000 request |
|---|---:|---:|
| Tranche #0 | 10% | $8,000 |
| Tranche #1 | 20% | $16,000 |
| Tranche #2 | 30% | $24,000 |
| Tranche #3 | 40% | $32,000 |

Do not include marketing, promotion, or external security audit costs in the budget. Security audit support is handled separately by SCF according to the form guidance.

The two-person team allocation is based on approximately 144-160 combined engineering days across the paid tranches, covering protocol architecture, Soroban policy work, backend execution, data reliability, testing, and release operations. The tranche sections provide a per-deliverable budget and estimated engineering effort rather than a request for general operating costs.

## Tranche #1 Deliverables

**Target completion date:** `06/10/2026` (5 weeks after the planned project start; shift this date if the award start date changes)

The existing Aquarius pool analytics, event indexer, snapshots, Leader profiles, and user-reviewed Copy LP queue are the starting product and are not counted as new tranche deliverables.

### 1. Aquarius automation boundary and policy specification — $6,000 (approximately 24 combined engineering days)

Define the reusable adapter and policy boundary for the first funded venue, Aquarius. Specify pool discovery, LP state reads, liquidity events, deposit, withdrawal, and claim operations, together with pool and entrypoint allowlists, copy coefficient, amount limits, expiry, nonce, replay protection, pause, disarm, and manual fallback. Additional venues remain behind the same boundary and are enabled only after validation and usage gates.

**Completion:** the Aquarius capability model and policy specification are public; unsupported or unverified operations fail closed; deterministic contract and event fixtures pass.

### 2. Aquarius Copy Engine and testnet prototype — $6,000 (approximately 24 combined engineering days)

Turn the current user-reviewed Copy LP flow into an implementation-ready Aquarius automation prototype. Convert an attributed Leader deposit, withdrawal, or fee claim into a scaled intent and submit it only through the policy boundary.

**Completion:** a testnet policy can be created, inspected, paused, and disarmed; permitted Aquarius intents are scaled and submitted; out-of-policy intents are rejected in repeatable tests.

### 3. Adoption instrumentation and user validation — $4,000 (approximately 16 combined engineering days)

Add privacy-preserving product instrumentation for policy arms, repeated automated Copy LP executions, failures, pauses, and manual fallback. Use the existing public application and an external pilot group to validate whether users understand the policy and repeat the workflow.

**Completion:** a public or reproducible usage report defines the adoption funnel and records external pilot feedback; the test suite demonstrates that invalid, ambiguous, expired, or duplicated source activity cannot create an executable operation.

**Tranche #1 total: $16,000**

## Tranche #2 Deliverables

**Target completion date:** `10/11/2026` (5 weeks after Tranche #1; shift this date if the award start date changes)

### 1. Soroban Copy Policy and account authorization — $8,000 (approximately 32 combined engineering days)

Implement the policy-controlled execution boundary for supported Stellar DEX LP operations. Support venue, pool, and entrypoint allowlists, copy coefficients, maximum amount per operation, daily limits, expiry, nonce, replay protection, pause, and disarm.

**Completion:** testnet registration, authorization, pause, disarm, and rejection of out-of-policy operations work in repeatable tests.

### 2. LumenLP Copy Engine and relayer — $8,000 (approximately 32 combined engineering days)

Implement the service that converts indexed Leader events into validated copy intents and submits only policy-approved operations through the supported DEX adapters. Keep the interface compatible with a future independent keeper, while using a LumenLP-operated relayer for the initial product.

**Completion:** a permitted testnet deposit, withdrawal, and claim can be executed automatically; duplicate events and over-limit operations are rejected.

### 3. On-chain monitoring plan and threat model — $4,000 (approximately 16 combined engineering days)

Document and implement monitoring for policy state, relayer actions, source event identifiers, failed operations, amount limits, daily exposure, pause/disarm state, and suspicious execution patterns. Produce a threat model covering forged Leader events, compromised relayer, replay, policy widening, incorrect actor attribution, RPC failure, and supported venue contract or adapter changes.

**Completion:** monitoring dashboards or alert outputs are available on testnet; threat model and incident response runbook are published; each high-severity threat has a mitigation or fail-closed behavior.

### 4. Testnet end-to-end validation and external pilot — $4,000 (approximately 16 combined engineering days)

Run end-to-end scenarios with real testnet accounts, including permitted actions, insufficient balances, expired intents, daily-limit exhaustion, pause, disarm, and relayer downtime with manual fallback. Recruit and support the first external pilot users without counting speculative future demand as traction.

**Completion:** testnet walkthrough is reproducible and evidence includes transaction hashes, rejected-operation examples, execution logs, and pilot feedback. The adoption target is at least **10 external users who arm an Aquarius policy, with at least 5 of them completing two or more automated Copy LP operations**.

**Tranche #2 total: $24,000**

## Tranche #3 Deliverables

**Target completion date:** `22/12/2026` (6 weeks after Tranche #2; shift this date if the award start date changes)

### 1. Limited Aquarius mainnet launch — $12,000 (approximately 48 combined engineering days)

Deploy the policy-controlled Copy LP flow to Aquarius mainnet with conservative limits and a staged rollout. Keep additional DEX execution disabled until the adoption and safety gates are met.

**Completion:** limited external users can arm a policy, observe a supported Aquarius Leader event, and complete a policy-approved mainnet Copy LP operation with public transaction links and a manual fallback.

### 2. Execution history and operational reliability — $8,000 (approximately 32 combined engineering days)

Add complete execution history, transaction links, failure reasons, relayer health, indexer lag visibility, and safe recovery paths. Preserve the manual user-signed fallback.

**Completion:** every automatic operation is traceable from source event to intent, policy decision, submitted transaction, and final status.

### 3. Demand-gated venue expansion design — $4,000 (approximately 16 combined engineering days)

Prepare the next venue adapter using the common boundary only if the Aquarius pilot demonstrates repeated usage. Candidate venues include Phoenix, Sushi V3, Soroswap AMM, and Comet. This work does not promise automatic execution across all candidates in this award and does not include unrestricted swap authority.

**Completion:** the decision gate is public and based on external usage, contract/event validation, and safety evidence; if the gate is met, one additional venue has a tested adapter and capability report. If it is not met, the deliverable is a documented expansion plan and validation backlog rather than an unsafe or unused production integration.

### 4. Adoption and public launch package — $8,000 (approximately 32 combined engineering days)

Publish the architecture, policy model, data methodology, API usage, limitations, operational runbook, and a short public walkthrough. Include evidence of external testing and known limitations.

**Completion:** documentation is accessible from the website and repository; the mainnet demo can be independently followed; adoption results, limitations, execution history, and all grant deliverables and links are public. The project reports the target of 10 armed external users and 5 repeat users, whether achieved or not.

**Tranche #3 total: $32,000**

## Completion Dates

The form requires dates in `DD/MM/YYYY` format. These dates assume the award project starts on **01/09/2026**, after SCF approval and Tranche #0. If the official award start date is different, shift all three dates by the same number of days.

- **Tranche #1:** `06/10/2026` — Aquarius automation boundary, policy prototype, safety tests, and adoption instrumentation (5 weeks).
- **Tranche #2:** `10/11/2026` — Aquarius testnet automation, monitoring, threat model, and external pilot validation (5 weeks).
- **Tranche #3:** `22/12/2026` — limited Aquarius mainnet launch, reliability, adoption reporting, and demand-gated venue expansion (6 weeks).

## Important Submission Decisions

- Select **End User Application**.
- Submit both confirmed team members, with their roles and relevant experience stated separately.
- Treat Aquarius as the first funded venue; gate Phoenix, Sushi V3, Soroswap AMM, and Comet expansion on demonstrated external usage and venue-specific validation.
- Commit to an adoption target of 10 external users arming an Aquarius policy, with at least 5 completing two or more automated Copy LP operations.
- Present automatic execution as the grant outcome, not as an already-live feature.
- Include the required on-chain monitoring plan and threat model in Tranche #2.
- Keep the manual user-signed path as a fallback.
