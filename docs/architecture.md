# LumenLP Architecture

LumenLP is a Stellar-native LP discovery, analytics, and Copy LP product. It starts with Aquarius and uses Soroban RPC plus a local event indexer as its data foundation.

The product has two connected surfaces:

1. **LP intelligence:** discover pools and inspect observable liquidity-provider activity.
2. **Copy LP:** select a Leader, scale selected LP actions, and prepare or execute the follower operation without taking custody of user funds.

The current production system is RPC-first and non-custodial. The website and API do not claim complete historical PnL when the indexed data cannot prove it.

## Product Boundary

### Current focus

- Aquarius pool discovery and ranking.
- Pool TVL, liquidity, fee, Fee/TVL, and activity metrics.
- Pool snapshots and historical windows.
- LP event indexing with actor attribution where available.
- Leader discovery from observed LP activity.
- Copy LP sessions and scaled operation queues.
- User-reviewed transaction drafts.

### Deferred capabilities

- Soroban policy-controlled automatic execution.
- LumenLP relayer for policy-approved operations.
- CLMM range copy and rebalance depth.
- Optional fee-token conversion through LumAgg.
- Additional liquidity protocols, only if they directly support the Copy LP product.

LumenLP does not mirror arbitrary Leader swaps. A swap can be a fee exit, a position adjustment, or an unrelated trade. Fee-token conversion is a separate, explicitly authorized LumAgg flow.

## System Overview

```text
                         Stellar Mainnet
                               │
                   Soroban RPC / local RPC node
                               │
             ┌─────────────────┴─────────────────┐
             │                                   │
       Snapshotter                         Pool Indexer
             │                                   │
             ▼                                   ▼
       lpagent.db                         pool-indexer.db
             │                                   │
             └─────────────────┬─────────────────┘
                               │
                         API Server
                               │
                  https://api.lumenlp.xyz
                               │
                         HTTPS / JSON
                               │
                         Web Application
                    https://lumenlp.xyz
                               │
                 Wallet / user-reviewed actions
```

The production services run on `88.198.16.144`. The web application is statically exported and deployed to Cloudflare Pages. Nginx terminates the API domain and proxies requests to the API server on `127.0.0.1:3301`.

## Repository Structure

```text
lumenlp/
├── apps/web/                  Next.js reference web application
├── crates/
│   ├── api-server/             Axum API and application orchestration
│   ├── dex/                    Soroban RPC, Aquarius integration, pool DB
│   ├── metrics/                TVL, fee, pricing, and LP math
│   ├── pool-indexer/           Contract event ingestion and rollups
│   └── snapshotter/            Periodic pool hydration and snapshots
├── deploy/                    systemd, Nginx, and deployment scripts
├── docs/                      Architecture, methodology, and grant material
└── thirdparty/                Local protocol source references
```

## Data Flow

### Pool catalogue and snapshots

The Snapshotter periodically discovers Aquarius pool addresses through the Aquarius router. It hydrates pool state from Soroban RPC and stores pool metadata and snapshots.

```text
Aquarius router
      │ discover pool addresses
      ▼
Snapshotter
      │ get pool type, tokens, reserves, fee, shares
      ▼
Price book
      │ native XLM and supported pool paths
      ▼
TVL / fee metrics
      │
      ▼
lpagent.db
```

The current snapshotter runs as a systemd timer every five minutes and snapshots the configured top-N pools by reserve depth. A pool can remain in the catalogue even when its current price path is incomplete; in that case its snapshot is stored with an unavailable or zero-valued quote metric rather than an invented price.

### Event and swap indexing

The Pool Indexer continuously scans contract events from the configured Soroban RPC. It persists raw and derived events, swap observations, cursor state, and rollup tables.

```text
Soroban RPC getEvents
      │
      ▼
PoolEventScanner
      │ parse Aquarius event topics and payloads
      ▼
pool-indexer.db
      │
      ├── pool events
      ├── swaps
      ├── actor-tagged liquidity activity
      ├── window rollups
      └── indexer status / cursor
```

The indexer is incremental. It stores a ledger cursor and advances it in bounded ledger ranges so a temporary lag does not create an unbounded RPC request. Missing liquidity actors can be re-parsed by the actor backfill pass when the required event data is still available from the RPC retention window.

If the saved cursor falls outside the RPC retention window, the indexer clamps to the oldest available ledger. Older history requires a third-party historical source, an earlier archive, or another archive data provider.

### API read path

The API Server combines both databases with live RPC reads and pricing helpers:

```text
HTTP request
    │
    ▼
Axum handlers
    │
    ├── lpagent.db          pool metadata and snapshots
    ├── pool-indexer.db     events, swaps, rollups, Leader activity
    ├── Soroban RPC         live positions and contract state
    └── PriceService         token metadata and quote conversion
    │
    ▼
Normalized JSON response
```

Important API surfaces:

| Endpoint | Purpose |
|---|---|
| `GET /health` | Service health |
| `GET /v1/indexer/status` | Indexer cursor and counts |
| `GET /v1/pools` | Ranked pool list and window metrics |
| `GET /v1/pools/{address}` | Pool detail and latest state |
| `GET /v1/pools/{address}/history` | Snapshot history |
| `GET /v1/pools/{address}/events` | Recent pool events |
| `GET /v1/lp/leaders` | Leader ranking from indexed activity |
| `GET /v1/lp/profile` | One Leader's activity and exposure |
| `GET /v1/positions` | Positions for an account |
| `GET /v1/copy/sessions` | Copy sessions |
| `GET /v1/copy/sessions/{id}/ops` | Scaled Copy LP operations |
| `GET /v1/venues` | Current protocol support status |

## Metric Methodology

### TVL

TVL is computed from current pool reserves and a quote price book:

```text
TVL = sum(reserve_i × price_i_in_quote_asset)
```

The current quote basis is XLM where a usable price path exists. User-facing fields must state the quote unit and must not present XLM estimates as USD.

### Fee and Fee/TVL

Fee values are derived from observed swap activity and the pool fee configuration. Fee/TVL is a windowed efficiency signal:

```text
Fee/TVL(window) = estimated fees in window / current TVL
```

It is not a guaranteed APR and should be interpreted together with data coverage, liquidity changes, and pool activity.

### Leader activity

Leader rankings use observable indexed activity such as:

- claimed fees;
- deposits and withdrawals;
- net liquidity change;
- pools touched;
- current open exposure;
- event frequency and recency.

These are data signals, not a promise of profit. Unless cost basis and complete history are available, the UI must not label them as complete PnL, win rate, or guaranteed earnings.

## Planned Stellar Integration

LumenLP is a Stellar-specific application. Its core workflow integrates directly with Stellar mainnet contracts and Soroban infrastructure rather than importing generic DeFi data from an external chain.

### Aquarius AMM integration

Aquarius is the first and current production venue. LumenLP integrates with Aquarius in four ways:

1. **Pool discovery**
   - Read the Aquarius router through Soroban RPC.
   - Discover pool addresses from the router's token-set catalogue.
   - Read each pool's token list, pool type, fee configuration, reserves, and total shares.
   - Support the current Aquarius pool families: constant-product AMM, stable pools, and concentrated liquidity pools where the required state is available.

2. **Pool analytics**
   - Hydrate pool state from contract simulation and RPC reads.
   - Build an XLM-normalized price book from native-XLM and connected pool paths.
   - Compute TVL from on-chain reserves and the price book.
   - Persist periodic snapshots for historical TVL, fee, Fee/TVL, and activity windows.

3. **Liquidity event indexing**
   - Scan Aquarius contract events with Soroban RPC `getEvents`.
   - Parse deposit, withdrawal, claim, reserve-update, and trade events.
   - Persist source ledger, transaction hash, contract, event topics, actor, token amounts, and event kind.
   - Use the indexed actor field to build Leader profiles and Copy LP source events.

4. **LP operation execution**
   - Generate or execute only declared Aquarius LP entrypoints: deposit, withdrawal, and fee claim in the initial automation scope.
   - Add concentrated-liquidity open, close, and adjust operations only after reliable range and position identifiers are available.
   - Do not mirror arbitrary Leader swaps.

### Soroban RPC integration

Soroban RPC is the source of truth for the current Stellar implementation:

- `getHealth` determines the available ledger range and RPC retention boundary.
- `getLatestLedger` advances the indexer cursor.
- `getEvents` supplies Aquarius contract events in bounded ledger ranges.
- Contract simulation and read calls hydrate pool state, reserves, fees, shares, and positions.
- Transaction simulation and submission are used by the future policy-controlled execution path.

The production server uses the local RPC endpoint on `88.198.16.144`. The indexer stores its cursor and never assumes that public RPC can provide history outside its retention window. If an event is older than the available RPC range, LumenLP marks the data as unavailable rather than inventing history.

### Soroban Policy and automated execution

The grant-funded automation layer will use Soroban authorization to constrain what LumenLP can execute for a follower account.

The policy will restrict:

- Aquarius contract addresses and allowed pool addresses;
- allowed entrypoints: deposit, withdrawal, and claim;
- copy coefficient and integer amount scaling rules;
- maximum quote value per operation;
- maximum quote value per UTC day;
- slippage, nonce, and intent expiry;
- pause, resume, and disarm state.

The execution sequence is:

```text
Aquarius event on Stellar
        │ observed by LumenLP Indexer
        ▼
Copy Engine creates source-linked intent
        │ leader amounts × configured coefficient
        ▼
Soroban Policy checks permissions and limits
        │ fail closed if any check fails
        ▼
LumenLP Relayer submits the approved transaction
        ▼
Aquarius contract executes the LP operation
        ▼
LumenLP records ledger, transaction hash, and final status
```

The relayer is not a custodian and cannot widen the policy. The user can pause or disarm the policy and retains the ultimate account authority. The current production fallback is a user-reviewed transaction draft while this policy layer is developed and tested.

The API also applies a fail-closed boundary to the off-chain CopyOp queue. A session can declare allowed pools,
per-operation quote limits, a UTC daily quote limit, and an expiry. Events outside that scope are retained as
`rejected` with a machine-readable reason rather than being presented as executable drafts. This is a queue-level
guardrail for the testnet vertical slice; Soroban remains the final authority once the policy contract and relayer are
enabled.

### LumAgg integration boundary

LumAgg is the separate Stellar DEX aggregator used for optional conversion of claimed fee tokens. It is not used to mirror Leader swaps.

The planned flow is:

```text
Aquarius fee claim
        ▼
Claimed fee tokens in follower account
        ▼
User explicitly authorizes LumAgg quote and swap
        ▼
LumAgg route executes with independent amount and slippage limits
```

The Copy LP policy will not automatically grant unrestricted LumAgg or swap-router permissions. This separation prevents a Leader's unrelated swap activity from becoming an automatic follower trade.

### Stellar integration deliverables

The planned integration will be delivered in this order:

1. Mainnet Aquarius event and actor attribution hardening.
2. User-reviewed Aquarius Copy LP operations linked to source events.
3. Soroban Policy implementation and testnet registration / disarm flow.
4. LumenLP Relayer for policy-approved deposit, withdrawal, and claim operations on testnet.
5. On-chain monitoring plan and threat model covering relayer, policy, replay, actor attribution, RPC, and Aquarius contract risks.
6. Limited mainnet launch with conservative limits and public transaction history.
7. Optional, separately authorized LumAgg fee-token conversion.

This integration plan is specific to Stellar contracts, Soroban RPC, Aquarius pool behavior, Stellar account authorization, and LumAgg. It is not a generic multi-chain or off-chain analytics integration.

## Copy LP Architecture

### Current implementation: user-reviewed flow

```text
Leader LP event
      │ indexed with actor and source event id
      ▼
Copy session reconciliation
      │ leader amount × coefficient
      ▼
Copy operation queue
      │ pending / drafted / skipped / failed
      ▼
Copy Preview
      │
      ▼
Strategies / transaction draft
      │
      ▼
User reviews and signs
```

The current Copy LP implementation is not custodial and does not automatically submit transactions. A CopyOp must be tied to a source event and remain idempotent for a given session and source event.

For concentrated liquidity, the target range should be copied as an explicit range value, while liquidity or token amounts are scaled. A failed or unrecognized position mapping must remain visible as pending or unsupported; it must not silently guess a target position.

### Planned execution flow: policy-constrained relayer

The next execution model is a centralized LumenLP relayer operating under a user-authorized Soroban policy:

```text
Indexer observes Leader event
      │
      ▼
LumenLP Copy Engine creates intent
      │ coefficient, pool, operation, limits
      ▼
Soroban policy validates
      │ allowlist, amount cap, daily cap, slippage, nonce, expiry
      ▼
LumenLP relayer submits approved operation
      │
      ▼
Aquarius deposit / withdraw / claim
```

The relayer is an implementation detail and does not receive unrestricted authority. The contract policy must be the final authority for what can execute. The user must be able to pause or disarm the policy and retain full control of the account.

The initial relayer can be operated by LumenLP. The interface should remain compatible with a future permissionless keeper without making a keeper network a prerequisite for the product.

### Event authenticity boundary

Soroban contracts cannot independently query arbitrary historical Leader activity. The system therefore has an explicit trust boundary:

- The indexer observes and stores source ledger / transaction / event metadata.
- The Copy Engine creates an intent from the indexed event.
- The policy contract enforces the follower's limits and allowed operations.
- The relayer submits the operation but cannot widen policy authority.

For automatic execution, the system must not trust keeper-supplied token amounts alone. A future on-chain execution design needs a recorder, signed attestation, multisig recorder, or inclusion-proof mechanism. Until that is implemented and tested, the manual user-signed path remains the safe default.

## LumAgg Boundary

LumAgg is a separate product and service used for optional token conversion after a fee claim. It is not part of Leader action mirroring.

```text
Copy LP claim
      │ separate user consent / policy permission
      ▼
Claimed fee tokens
      │
      ▼
LumAgg quote and swap
```

Any future automated LumAgg flow must have independent permissions, route validation, slippage limits, and amount caps. A Copy LP policy must not implicitly grant arbitrary swap or router authority.

## Frontend Architecture

The web application is a thin client. It does not calculate authoritative chain state and does not hold user private keys.

```text
apps/web
  ├── /pools       pool discovery and ranking
  ├── /pools/view  pool detail and event history
  ├── /leaders     Leader discovery and profiles
  ├── /copy        Copy sessions and operation queue
  └── /strategies  transaction / strategy preview surface
```

The frontend reads `NEXT_PUBLIC_API_BASE`, which is `https://api.lumenlp.xyz` in production. Wallet integration is used for identity and, when enabled, user authorization/signing. Connected accounts are not treated as a server-side custody account.

## Persistence

### `lpagent.db`

Owned by the Snapshotter and read by the API:

- pool catalogue;
- pool type and token metadata;
- pool snapshots;
- reserves;
- TVL and fee estimates;
- snapshot timestamps.

### `pool-indexer.db`

Owned by the Pool Indexer and read by the API:

- indexer cursor;
- raw / derived pool events;
- swaps;
- actor-tagged liquidity activity;
- rollups and window metrics;
- Copy LP sessions and operations where enabled by the API schema.

The deployment process must preserve both databases. In particular, an existing `pool-indexer.db` must never be replaced during a normal code deployment because it contains event history and Copy LP state.

## Production Deployment

```text
88.198.16.144
  ├── local Stellar RPC       127.0.0.1:8003
  ├── pool-indexer             systemd, 30s polling
  ├── snapshotter              systemd timer, 5m interval
  ├── api-server               127.0.0.1:3301
  └── nginx                    api.lumenlp.xyz → API server

Cloudflare Pages
  └── lumenlp.xyz              static Next.js export
```

Deployment entry points:

- `deploy/deploy.sh` syncs source, builds Rust services, installs systemd/Nginx configuration, and restarts API, indexer, and snapshotter services.
- `deploy/deploy_site.sh` builds the static web export with `NEXT_PUBLIC_API_BASE` and deploys it to Cloudflare Pages.

The API and indexer use the local RPC endpoint on the server. The public API is exposed through `api.lumenlp.xyz`; the frontend must never use `http://127.0.0.1:3301` in production because that address refers to the visitor's own machine.

## Reliability and Safety Rules

- Persist and monitor the indexer ledger cursor.
- Bound RPC event scans and retain source ledger / transaction metadata.
- Fail closed when actor attribution, token pricing, or position mapping is unavailable.
- Label estimates and proxy metrics explicitly.
- Preserve indexer and snapshot databases across deploys.
- Do not store user private keys on the server.
- Keep Copy LP limited to declared Aquarius LP entrypoints.
- Never silently downscale a user-selected copy coefficient.
- Enforce pool allowlists, per-operation caps, daily caps, slippage, nonce, and expiry before automatic execution.
- Keep LumAgg swaps separate from Leader-event mirroring.

## Planned Evolution

```text
Current
  RPC + snapshots + event indexer
      ↓
  Pools + Leaders + manual Copy LP drafts

Next
  Aquarius Copy Engine
      ↓
  Soroban policy + LumenLP relayer
      ↓
  Automatic deposit / withdraw / claim under limits

Later
  CLMM rebalance and fee automation
      ↓
  Optional LumAgg fee conversion
      ↓
  Reusable policy-triggered LP strategies
```

The project should not expand into a generic multi-DEX framework or a SoroGuard clone before the Aquarius Copy LP workflow is reliable, measurable, and understandable to users.
