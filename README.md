# LumenLP

**Stellar LP strategy toolchain** — multi-DEX adaptors, indexed pool analytics, copy-scale & rebalance strategies, SDK/CLI path.  
**No custody.** Drafts and analytics; you keep the keys and sign.

**Live:** [lumenlp.xyz](https://lumenlp.xyz) · **API:** [api.lumenlp.xyz](https://api.lumenlp.xyz) · **Venues:** `GET /v1/venues`

## Positioning

LumenLP is **developer / LP infrastructure**, not a custodial vault:

| Layer | What ships |
|-------|------------|
| **DexAdaptor** | Stable `venue_id` + capability matrix ([docs](docs/architecture/dex-adaptor.md)) |
| **Aquarius (production)** | Router discovery, pool metrics, event indexer, Copy LP dry-run |
| **Phoenix** | Mainnet factory/pool read smoke-tested; event and Copy LP integration remain scaffolded |
| **Other DEXes** | Soroswap AMM has a read-only mainnet reader; Sushi V3 and Comet remain scaffolds (+ Classic deferred) |
| **Strategies** | Copy-scale, stay-in-range / fee-harvest previews (reference UI) |
| **Reference client** | Web `/pools`, `/copy`, `/strategies` — demo, not the product definition |

## Traction (mainnet)

- Public site + API on Stellar mainnet data  
- Aquarius pool ranking (TVL, fee/TVL, activity windows) via Soroban RPC + indexer  
- **Copy LP:** follow a leader’s Aquarius LP actions at a coefficient → scaled draft queue (you still sign)  
- Support matrix: [https://api.lumenlp.xyz/v1/venues](https://api.lumenlp.xyz/v1/venues)  
- Quality checklist: [docs/architecture/aquarius-quality-checklist.md](docs/architecture/aquarius-quality-checklist.md)  
- OpenAPI draft: [docs/openapi.yaml](docs/openapi.yaml)

## Repo layout

| Path | Role |
|------|------|
| `apps/web` | Next.js reference client (Cloudflare Pages) |
| `crates/api-server` | Axum HTTP API |
| `crates/snapshotter` | Pool snapshot cycle → SQLite |
| `crates/pool-indexer` | Event ingest + rollups |
| `crates/dex` | Multi-DEX clients + **`DexAdaptor`** (`aquarius` production; others scaffold) |
| `crates/metrics` | TVL / fee / CL helpers |
| `deploy/` | systemd, nginx, deploy scripts |

## Quick start (local)

### Prerequisites

- Rust toolchain, Node 20+, Soroban RPC (mainnet)

```bash
cp .env.example .env
mkdir -p data
```

### Backend

```bash
RPC_URL=... DATABASE_PATH=./data/lpagent.db cargo run -p snapshotter --release
RPC_URL=... INDEXER_DB_PATH=./data/pool-indexer.db cargo run -p pool-indexer --release -- run
RPC_URL=... DATABASE_PATH=./data/lpagent.db INDEXER_DB_PATH=./data/pool-indexer.db \
  BIND=0.0.0.0:3301 cargo run -p api-server --release
```

### Phoenix read-only validation

The Phoenix mainnet factory and a sample pool can be checked without signing or
submitting a transaction:

```bash
RPC_URL=https://mainnet.sorobanrpc.com ./deploy/validate-phoenix.sh
```

### Soroswap read-only validation

The Soroswap AMM factory and a sample pair can be checked without signing or
submitting a transaction:

```bash
RPC_URL=https://mainnet.sorobanrpc.com ./deploy/validate-soroswap.sh
```

The reader currently covers factory pair discovery and `token_0`, `token_1`,
and `get_reserves`. LP event indexing and Copy LP operations remain disabled
until their venue-specific semantics are validated.

### Frontend

```bash
cd apps/web && npm ci
NEXT_PUBLIC_API_BASE=http://127.0.0.1:3301 npm run dev
```

## API (selected)

| Method | Path | Notes |
|--------|------|-------|
| `GET` | `/health` | Liveness |
| `GET` | `/v1/venues` | Multi-DEX support matrix |
| `GET` | `/v1/indexer/status` | Indexer cursor |
| `GET` | `/v1/pools` | Ranked pools |
| `GET` | `/v1/pools/{address}` | Pool detail |
| `GET` | `/v1/positions` | Positions for account |
| `POST`/`GET` | `/v1/copy/sessions` | Copy LP sessions |
| `GET` | `/v1/copy/sessions/{id}/ops` | Scaled op queue |
| `GET` | `/v1/copy/ops/{id}` | Single op (scaled amounts) |

Full draft: [`docs/openapi.yaml`](docs/openapi.yaml).

## Deploy

```bash
./deploy/deploy.sh
NEXT_PUBLIC_API_BASE=https://api.lumenlp.xyz ./deploy/deploy_site.sh
```

## Docs / grants

- Architecture: [`docs/architecture.md`](docs/architecture.md)
- DexAdaptor: `docs/architecture/dex-adaptor.md`  
- SCF tranches: `docs/superpowers/specs/2026-08-05-scf-tooling-milestones.md`  
- Grant draft: `docs/grants/stellar-grant-draft.md`

## Third-party references (optional)

Upstream DEX repos for **local adapter work only** — not Cargo dependencies.

```bash
mkdir -p thirdparty
git clone https://github.com/AquaToken/soroban-amm thirdparty/aquarius-amm
git clone https://github.com/soroswap/core thirdparty/soroswap
git clone https://github.com/Phoenix-Protocol-Group/phoenix-contracts thirdparty/phoenix-contracts
git clone https://github.com/CometDEX/comet-contracts-v1 thirdparty/comet-contracts-v1
git clone https://github.com/hyplabs/sushiswap-stellar-interface-fork thirdparty/sushiswap-stellar-interface-fork
```

## License

[MIT](LICENSE)
