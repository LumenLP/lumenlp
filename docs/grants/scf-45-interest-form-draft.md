# SCF #45 Interest Form Draft

> Working draft for the SCF #45 Interest Form. Replace bracketed fields before submission.

## Project

- **Project name:** LumenLP
- **Website:** https://lumenlp.xyz
- **API:** https://api.lumenlp.xyz
- **Repository:** https://github.com/LumenLP/lumenlp
- **Primary contact:** [NAME]
- **Contact email:** [EMAIL]
- **Team / legal entity:** [TEAM_OR_ENTITY]
- **Requested amount:** [REQUESTED_AMOUNT]
- **Track:** Build / Open Track [CONFIRM IN FORM]

## One-line description

LumenLP is a non-custodial LP automation product for Stellar that helps users discover active liquidity providers, inspect on-chain LP behavior, and safely copy Aquarius liquidity actions with configurable limits.

## Problem

Stellar LPs currently lack a clear way to answer three practical questions: which pools are active, which liquidity providers are consistently generating fees, and how to reproduce a liquidity strategy without manually monitoring contracts and rebuilding every transaction.

Existing pool statistics are useful for discovery but do not complete the workflow. Users still need to interpret LP activity, follow deposits and withdrawals, and execute or rebalance positions themselves. This limits the amount of capital that can be confidently deployed and retained in Stellar liquidity pools.

## Solution

LumenLP combines an RPC-first Aquarius indexer, pool and LP activity analytics, Leader profiles, and Copy LP execution tooling.

The user flow is:

1. Discover Aquarius pools using TVL, liquidity, fees, Fee/TVL, and recent activity.
2. Inspect a Leader account using observable on-chain data such as claimed fees, deposits, withdrawals, pools touched, and current exposure.
3. Select a Leader and configure a copy coefficient and safety limits.
4. Preview the Leader's deposit, withdrawal, or fee-claim action and the scaled follower action.
5. Execute the approved Aquarius operation through a non-custodial, policy-constrained account. Fee-token conversion can be separately authorized through a DEX aggregator such as Soroswap or LumAgg.

Leader rankings are presented as data and activity signals, not as guaranteed profit or investment advice. Claimed fees are not labeled as complete PnL.

## What is already live

- Aquarius pool discovery from Soroban RPC.
- Mainnet pool ranking and snapshots for TVL, liquidity, fees, Fee/TVL, and activity windows.
- Event indexer and public API.
- Leader ranking and profile pages based on indexed LP activity.
- Copy LP sessions and scaled operation queue.
- Deployed web application at https://lumenlp.xyz.
- Deployed API at https://api.lumenlp.xyz.

## Proposed build scope

The grant will turn the current Aquarius prototype into a production-ready Copy LP Studio:

- Harden Aquarius event parsing, actor attribution, and historical coverage.
- Support deposit, withdrawal, and fee-claim copy flows with idempotent execution records.
- Add a Soroban policy layer with pool allowlists, per-operation limits, daily limits, slippage limits, pause, and disarm controls.
- Run a LumenLP relayer for the initial production flow; the contract remains the authority for permissions and limits.
- Keep all user funds non-custodial and keep Leader swaps outside the copy flow.
- Add optional, separately authorized DEX aggregator swaps for claimed fee tokens, using an aggregator such as Soroswap or LumAgg.
- Publish the data methodology, API documentation, and execution history so users can verify every action.

## Initial milestones

### Milestone 1: Reliable Aquarius data and Copy LP preview

- Verified actor attribution for sampled Aquarius LP events.
- Clear 7d / 30d Leader activity profiles.
- Copy preview showing Leader amounts, scaled amounts, pool, operation type, and source transaction.
- Manual user-signed execution path.

### Milestone 2: Policy-constrained execution

- Soroban policy / smart-account integration.
- Pool and entrypoint allowlists.
- Per-operation and daily amount limits.
- Pause / resume / disarm controls.
- LumenLP relayer triggering only policy-approved operations.

### Milestone 3: Mainnet product and ecosystem reuse

- Production Aquarius Copy LP Studio.
- Execution history with transaction links and failure reasons.
- Optional DEX aggregator fee-token conversion flow.
- Public API and integration documentation.
- Testnet and mainnet walkthrough for external users.

## Stellar ecosystem impact

LumenLP is focused on increasing the usability and retention of Stellar liquidity. It turns on-chain liquidity activity into an actionable, non-custodial workflow for LPs, while exposing reusable APIs and methodology for Stellar wallets and DeFi applications.

The project is Stellar-native, starting with Aquarius and Soroban RPC. The initial product does not require custody, does not mirror arbitrary swaps, and does not make unsupported claims about user profitability.

## Risks and mitigation

- **Incomplete historical data:** clearly display indexer coverage and begin accumulating a durable event history from deployment.
- **Incorrect event attribution:** fail closed when the actor or action cannot be resolved; retain source ledger and transaction metadata.
- **Automated execution risk:** constrain execution through contract policy, pool allowlists, amount limits, slippage limits, and one-click disarm.
- **Misleading performance claims:** show observed fees and activity as labeled data, not guaranteed profit or win rate.

## Information still required

- [ ] Primary contact name and email
- [ ] Team members and roles
- [ ] Legal entity / jurisdiction, if applicable
- [ ] Public repository URL
- [ ] Requested XLM/USD budget
- [ ] Whether the project will be submitted as an individual or entity
- [ ] Any existing Stellar ecosystem users, partners, or testers
- [ ] Preferred Build track selection after checking the form requirements
