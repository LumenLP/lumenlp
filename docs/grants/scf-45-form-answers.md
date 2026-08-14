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

`https://github.com/[GITHUB_OWNER]/[GITHUB_REPOSITORY]/blob/main/docs/architecture.md`

The document must be public and should describe the current RPC/indexer/API architecture, Copy Engine, Soroban Policy, relayer, trust boundaries, and LumAgg boundary.

### GitHub URL

`https://github.com/[GITHUB_OWNER]/[GITHUB_REPOSITORY]`

### Video URL

Upload a short 16:9 video, ideally under three minutes, showing:

1. Pool ranking and pool detail.
2. Leader profile with claimed fees and activity.
3. Starting a Copy LP session.
4. Leader event becoming a scaled CopyOp.
5. The planned policy-controlled automation flow.

Do not present the future automatic execution as already live. Label the current flow as the working MVP and the policy execution as the grant milestone.

## Products & Services

LumenLP Copy LP Studio is a non-custodial automation product for Stellar liquidity providers. It starts with Aquarius and connects pool discovery, observable LP behavior, and automated liquidity operations in one workflow.

Users can compare Aquarius pools using TVL, liquidity, fees, Fee/TVL, and recent activity. They can then inspect Leader profiles showing claimed fees, deposits, withdrawals, pools touched, current exposure, and source on-chain events. These metrics are presented as observable data signals rather than guaranteed profit or complete PnL.

After selecting a Leader, a user configures a copy coefficient and safety limits. LumenLP creates copy intents for supported Aquarius LP actions, initially deposits, withdrawals, and fee claims. The final product will execute these operations through a Soroban policy-controlled account and a LumenLP relayer. The policy limits allowed pools and entrypoints, maximum amount per operation, daily exposure, slippage, expiry, and replay. Users can pause or disarm the policy and LumenLP never stores their private keys or operates a custodial vault.

Leader swaps are not mirrored because a swap may be a fee exit, a position adjustment, or an unrelated trade. Optional fee-token conversion is handled as a separate, explicitly authorized LumAgg flow.

The current mainnet product already includes Aquarius pool discovery, snapshots, event indexing, Leader profiles, and a user-reviewed scaled Copy LP queue. The Build Award will turn this working MVP into a testnet-validated and limited-mainnet automated product.

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

If no second person has been confirmed as a real contributor, submit as a one-person team. Do not add a placeholder team member.

## Mailing List

Select at least one active team member who will monitor SCF communications.

## Team Description

Lijiao Zhou is the founder and primary builder of LumenLP. He is a full-stack engineer with hands-on experience building automated LP rebalancing systems and working with concentrated-liquidity designs including CLMMs and DLMMs, liquidity ranges, position management, and rebalancing strategies.

He has built the current LumenLP stack end to end: Aquarius and Soroban RPC integration, pool hydration, TVL and fee calculations, event indexing, Leader activity profiles, the Copy LP queue, API services, and the deployed web application. LumenLP is currently running with a production API and indexer on Stellar mainnet.

LinkedIn: https://www.linkedin.com/in/yaransu/

The project is intentionally scoped around one production venue, Aquarius, for this award. Specialized security review and testing will be handled through the SCF-supported review process and qualified external assistance where appropriate.

## Recommended Budget

For a one-person team with a focused Aquarius-only scope, a reasonable request is **$75,000 USD-equivalent in XLM**, subject to matching the amount already indicated in the Interest Form. Do not increase the scope to multi-DEX support just to justify a larger budget.

The required payment structure is:

| Tranche | Percentage | Amount on $75,000 request |
|---|---:|---:|
| Tranche #0 | 10% | $7,500 |
| Tranche #1 | 20% | $15,000 |
| Tranche #2 | 30% | $22,500 |
| Tranche #3 | 40% | $30,000 |

Do not include marketing, promotion, or external security audit costs in the budget. Security audit support is handled separately by SCF according to the form guidance.

## Tranche #1 Deliverables

**Target completion date:** `[Choose a date approximately 6–8 weeks after award approval]`

### 1. Aquarius event and actor attribution hardening — $5,000

Improve event parsing for Aquarius deposit, withdrawal, and claim events. Persist source ledger, transaction, contract, event identifier, actor, token amounts, and operation kind. Add fixtures and deterministic parser tests.

**Completion:** sampled mainnet events show the correct actor and source metadata; parser tests pass; unresolved actors fail closed instead of generating CopyOps.

### 2. Leader profiles and Copy LP foundation — $5,000

Complete 7-day and 30-day Leader activity profiles and make CopyOps idempotent per session and source event. Show claimed fees, deposits, withdrawals, pools touched, current exposure, and source event links.

**Completion:** a Leader event creates one correctly scaled CopyOp; the UI shows the original event, scaled amounts, pool, operation type, and status.

### 3. User-reviewed Aquarius Copy LP flow — $5,000

Complete the manual fallback path for deposit, withdrawal, and claim drafts. Keep user signing explicit while the automatic policy layer is developed.

**Completion:** an external tester can create a session, receive a scaled operation, generate a transaction draft, and verify the source transaction and operation status.

**Tranche #1 total: $15,000**

## Tranche #2 Deliverables

**Target completion date:** `[Choose a date approximately 3 months after award approval]`

### 1. Soroban Copy Policy and account authorization — $8,000

Implement the policy-controlled execution boundary for Aquarius LP operations. Support pool and entrypoint allowlists, copy coefficients, maximum amount per operation, daily limits, expiry, nonce, replay protection, pause, and disarm.

**Completion:** testnet registration, authorization, pause, disarm, and rejection of out-of-policy operations work in repeatable tests.

### 2. LumenLP Copy Engine and relayer — $6,000

Implement the service that converts indexed Leader events into validated copy intents and submits only policy-approved operations. Keep the interface compatible with a future independent keeper, while using a LumenLP-operated relayer for the initial product.

**Completion:** a permitted testnet deposit, withdrawal, and claim can be executed automatically; duplicate events and over-limit operations are rejected.

### 3. On-chain monitoring plan and threat model — $4,000

Document and implement monitoring for policy state, relayer actions, source event identifiers, failed operations, amount limits, daily exposure, pause/disarm state, and suspicious execution patterns. Produce a threat model covering forged Leader events, compromised relayer, replay, policy widening, incorrect actor attribution, RPC failure, and Aquarius contract changes.

**Completion:** monitoring dashboards or alert outputs are available on testnet; threat model and incident response runbook are published; each high-severity threat has a mitigation or fail-closed behavior.

### 4. Testnet end-to-end validation — $4,500

Run end-to-end scenarios with real testnet accounts, including permitted actions, insufficient balances, expired intents, daily-limit exhaustion, pause, disarm, and relayer downtime with manual fallback.

**Completion:** testnet walkthrough is reproducible and evidence includes transaction hashes, rejected-operation examples, and execution logs.

**Tranche #2 total: $22,500**

## Tranche #3 Deliverables

**Target completion date:** `[Choose a date approximately 4 months after award approval]`

### 1. Limited Aquarius mainnet launch — $12,000

Deploy the policy-controlled Copy LP flow to mainnet with conservative limits and a staged rollout. Support Aquarius deposit, withdrawal, and claim operations where event and position mapping are reliable.

**Completion:** limited external users can arm a policy, observe a Leader event, and complete a policy-approved mainnet Copy LP operation with a public transaction link.

### 2. Execution history and operational reliability — $6,000

Add complete execution history, transaction links, failure reasons, relayer health, indexer lag visibility, and safe recovery paths. Preserve the manual user-signed fallback.

**Completion:** every automatic operation is traceable from source event to intent, policy decision, submitted transaction, and final status.

### 3. Optional LumAgg fee-token conversion — $4,000

Add a separate user-authorized path to quote and swap claimed fee tokens through LumAgg. This flow must not inherit unrestricted authority from the Copy LP policy.

**Completion:** the user can explicitly authorize a fee-token swap with route, amount, slippage, and expiry shown before execution.

### 4. Public documentation and launch package — $8,000

Publish the architecture, policy model, data methodology, API usage, limitations, operational runbook, and a short public walkthrough. Include evidence of external testing and known limitations.

**Completion:** documentation is accessible from the website and repository; the mainnet demo can be independently followed; all grant deliverables and links are public.

**Tranche #3 total: $30,000**

## Completion Dates

The form requires dates in `DD/MM/YYYY` format. Use dates consistent with the actual award start date. The suggested sequence is:

- **Tranche #1:** `[DD/MM/YYYY]` — data, Leader profiles, manual Copy LP MVP.
- **Tranche #2:** `[DD/MM/YYYY]` — testnet Policy, relayer, monitoring plan, threat model.
- **Tranche #3:** `[DD/MM/YYYY]` — limited mainnet launch, LumAgg boundary, documentation.

## Important Submission Decisions

- Select **End User Application**.
- Submit one real team member unless a second contributor is confirmed and can create an SCF account.
- Keep the product scope Aquarius Copy LP, not multi-DEX infrastructure.
- Present automatic execution as the grant outcome, not as an already-live feature.
- Include the required on-chain monitoring plan and threat model in Tranche #2.
- Keep the manual user-signed path as a fallback.
