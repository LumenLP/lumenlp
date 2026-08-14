# LumenLP Stellar Grant Draft

Last updated: 2026-07-24

## One-line summary

LumenLP is an RPC-first liquidity analytics and monitoring layer for Stellar AMMs, starting with Aquarius, that helps LPs understand positions, fees, yield, and impermanent loss using directly computed on-chain data.

## Problem

Liquidity providers on Stellar still lack a reliable LP-native analytics experience.

Current gaps:

- LPs cannot easily see position-level value, unclaimed fees, or impermanent loss in one place.
- Yield figures are often opaque, inconsistent, or tied to third-party APIs rather than reproducible on-chain computation.
- Developers building wallets, dashboards, and DeFi interfaces on Stellar do not have a simple backend they can query for LP analytics.
- Concentrated liquidity makes portfolio visibility harder, especially when positions move in and out of range.

This creates friction for both capital providers and application teams. If users cannot understand LP performance, they are less likely to provide or retain liquidity on Stellar.

## Solution

LumenLP provides a lightweight analytics stack for Stellar LP positions and pools.

The current implementation already includes:

- On-chain pool discovery through Aquarius router contracts.
- Soroban RPC-first data access instead of depending on Aquarius REST as the source of truth.
- Snapshot-based pool analytics stored in SQLite.
- A backend API for pool lists, pool history, position lists, and portfolio summaries.
- A web UI where a user can connect a wallet or paste a Stellar address to inspect LP exposure.
- Local analytics logic for estimated TVL, volume proxy, fee APR, net worth, unclaimed fees, and IL estimates.

Core product direction:

- Portfolio view for Stellar LPs.
- Pool yield terminal for comparing pools over time.
- Reusable analytics API for wallets, explorers, and DeFi frontends.

## Why this matters to Stellar

This project is ecosystem infrastructure, not just a single app feature.

It can improve Stellar in three ways:

- Better LP tooling can increase confidence and retention for liquidity providers.
- RPC-first analytics reduce dependency on fragmented off-chain data pipelines.
- The API and computation layer can be reused by other teams building on Stellar.

In practice, LumenLP can become a public analytics primitive for AMM participation on Stellar, similar to how portfolio and yield tooling helped DeFi participation on other ecosystems.

## What is already built

Repository status today:

- Rust workspace with separate crates for metrics, Aquarius integration, API server, and snapshot ingestion.
- Next.js frontend for portfolio and pool views.
- Deployed domain and API targets referenced in the repository.
- Endpoints:
  - `/v1/pools`
  - `/v1/pools/{address}`
  - `/v1/pools/{address}/history`
  - `/v1/positions`
  - `/v1/positions/summary`

Technical characteristics:

- RPC-first architecture on Soroban.
- Metrics computed locally in Rust.
- Snapshotter computes TVL, estimated volume, and fee APR from observed state changes.
- Position lookup scans discovered pools and derives user exposure from on-chain state.

## Proposed grant scope

The grant should fund converting the current early product into production-grade Stellar ecosystem infrastructure.

### Milestone 1: Productionize Aquarius LP analytics

Deliverables:

- Harden Aquarius pool discovery and hydration.
- Improve position accuracy for constant-product, stable, and concentrated liquidity pools.
- Better error handling, retries, and RPC observability.
- Public hosted API with documentation.
- Methodology notes for APR and IL calculations.

Success metrics:

- Stable indexing of major Aquarius pools.
- Position summaries for real mainnet addresses.
- Public demo usable by external testers.

### Milestone 2: Concentrated liquidity depth and history

Deliverables:

- Better CL range visualization.
- In-range / out-of-range tracking.
- Historical pool and LP performance charts.
- More transparent fee and valuation methodology.

Success metrics:

- Accurate CL position display for sampled LP wallets.
- Historical analytics accessible through API and web UI.

### Milestone 3: Ecosystem-facing API and integrations

Deliverables:

- Developer docs and example integrations.
- Embeddable API patterns for wallets and dashboards.
- Optional webhook / alerting support for LP monitoring.
- Support for additional Stellar liquidity venues beyond Aquarius.

Success metrics:

- At least one external integration or pilot partner.
- Reusable API adopted by another Stellar app or community tool.

## Open-source and public-good value

The strongest grant framing is public infrastructure.

Public-good outputs can include:

- Open-source computation and indexing logic.
- Public API access tier for ecosystem builders.
- Documentation of LP analytics methodology on Stellar.
- Reference implementation for wallet-based LP portfolio analysis.

## Why this team can execute

Even at an early stage, the project already demonstrates:

- Full-stack implementation, not just concept slides.
- Direct protocol integration at the RPC layer.
- Working separation between ingestion, analytics, API, and frontend.
- Fast iteration speed with live deployment targets already defined.

This is important for a grant reviewer: the project is past the idea stage and already has an execution path.

## Suggested positioning in the application

Do not frame this as “just a dashboard.”

Better framing:

- “LP analytics infrastructure for Stellar”
- “RPC-first liquidity intelligence layer”
- “Portfolio and yield primitives for Stellar AMMs”
- “Reusable backend for wallets and DeFi interfaces”

That positioning makes the project more legible as ecosystem infrastructure with leverage across multiple products.

## Risks and how to present them

Main risks:

- Early-stage codebase with limited historical data.
- Metric accuracy needs validation over time.
- Dependence on reliable Soroban RPC access.
- Concentrated liquidity analytics are more complex than standard LP analytics.

How to present them:

- The architecture already isolates ingestion, analytics, and serving layers.
- The first grant goal is production hardening, not speculative research.
- The roadmap is incremental and each milestone creates usable public outputs.

## Short application version

LumenLP is building an RPC-first liquidity analytics layer for Stellar, starting with Aquarius. The product helps liquidity providers understand LP positions, estimated yield, unclaimed fees, and impermanent loss using directly computed on-chain data rather than opaque third-party APIs. The current system already includes pool discovery, snapshot ingestion, portfolio APIs, and a web interface. Grant funding would accelerate production hardening, concentrated liquidity analytics, public API documentation, and ecosystem integrations so Stellar wallets and DeFi apps can reuse this infrastructure.

## Reviewer-facing close

The main argument is simple: better LP tooling improves liquidity quality on Stellar, and LumenLP is being built as reusable infrastructure rather than a closed-end consumer app.
