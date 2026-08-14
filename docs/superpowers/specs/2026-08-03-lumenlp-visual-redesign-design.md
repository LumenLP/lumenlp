# LumenLP Visual Redesign + Strategies (B) — Design Spec

**Date:** 2026-08-03  
**Status:** Approved for implementation planning  
**Brand:** LumenLP (`lumenlp.xyz`) — Stellar / Aquarius LP analytics for SCF grant narrative  
**Reference:** Visual language inspired by [lpagent.io](https://lpagent.io), not a pixel clone; accent shifted to lime signal green

## Goal

Upgrade the whole web surface so it looks like a coherent, grant-ready product: marketing landing + pools terminal + pool detail share one visual system. Add a **semi-real Strategies** experience for multi-strategy auto-rebalance (configure → preview → user signs), without unsupervised on-chain execution in this pass.

## Decisions (locked)

| Topic | Choice |
|-------|--------|
| Approach | Design tokens + progressive restyle (not full UI rewrite) |
| Imitation depth | Full-site visual language (landing + product), not 1:1 component clone |
| Palette | Lime signal: primary `#84cc16`, highlight `#d9f99d`, bg `#090B08`, panel `#11140E` |
| Typography | Inter for UI + landing; tabular/mono only where numbers need it |
| Brand name | Keep **LumenLP** (not rebrand to LP Agent) |
| Home | Real marketing landing at `/` (stop redirect-only to pools) |
| Product restyle | Header, `/pools`, pool detail — same tokens, less card/pill noise |
| Auto rebalance | **Tier B** — strategies + rules + preview + user signature; no unattended auto-exec |
| Domain story | `lumenlp.xyz` Stellar-only positioning for grant |

## Visual system

### Tokens

| Token | Value | Usage |
|-------|-------|--------|
| `--bg` | `#090B08` | Page background |
| `--panel` | `#11140E` | Surfaces |
| `--accent` | `#84cc16` | Primary CTA, links, active states |
| `--accent-hi` | `#d9f99d` | Gradient text, secondary CTA border |
| `--text` | `#f5f5f5` | Body |
| `--muted` | `#a3a3a3` | Secondary copy |
| `--line` | `rgba(132,204,22,0.16)` / soft white ~8% | Borders |
| Font | Inter 400/500/600/700 | Replace Space Grotesk + IBM Plex Mono as default |

### Component language (shared)

- Radius ~10–16px; primary buttons solid accent on dark text (`#0a1004`)
- Cards: thin lime border + very light fill; hover glow kept subtle (reference-inspired, not neon overload)
- Hero glow: single soft radial blob, not stacked glows
- Status chips allowed sparingly; avoid rounded-full pill clusters that currently clutter pools

### Motion (landing + light product)

- Fade/slide-in on hero and Act sections (~0.5–0.7s)
- Marquee or static protocol row for Aquarius / Soroban / Stellar
- Product: prefer hover border/background transitions; no decorative animation on dense tables

## Information architecture

```
/                 Marketing landing (new)
/pools            Pool terminal (restyled)
/pools/view       Pool detail (restyled; keep existing query-param route)
/strategies       Strategies hub (new) — list, configure, preview, sign
```

Nav (landing + app shell): **LumenLP · Pools · Strategies · API (`/#api` from app pages) · Launch App / Connect Wallet**

## Landing page (`/`)

Structure mirrors lpagent.io acts, copy adapted to Stellar / LumenLP / grant story:

1. **Nav** — brand, Pools, Strategies, API, Launch App → `/pools`
2. **Hero** — headline + one supporting sentence + dual CTA (Launch App / Explore API) + protocol badges
3. **Act 01 · Discover** — pool comparison, fee/TVL/flow terminal  
4. **Act 02 · Monitor** — wallet or paste G-address portfolio visibility  
5. **Act 03 · Strategies** — multi-strategy rebalance: configure rules, preview, sign (Tier B honesty)  
6. **Act 04 · Build** — reusable analytics API for Stellar wallets/frontends (on-page `#api` section with sample endpoint snippets)  
7. **Closing CTA** — Launch App  

Nav **API** links to `#api` on the landing page (not a separate docs product in this pass).

Homepage must **not** `router.replace("/pools")` as the only experience.

## Product restyle

### Header

- Inter + lime accent CTAs
- Keep Freighter connect + paste G-address behavior
- Add Strategies nav link; active state uses accent

### `/pools`

- Keep table/card modes, filters, watchlist, scoring data paths
- Visual cleanup: collapse redundant status/pill strips; one coherent toolbar; table sticky header on panel tokens
- Pair marks and metrics stay; reduce nested card-in-card chrome

### Pool detail

- Same token pass on hero, charts, health/opportunity blocks
- Optional deep-link: “Apply strategy” → `/strategies?pool=…` (nice-to-have in same pass if cheap)

## Strategies — Tier B

### Intent

Let users pick a rebalance strategy, attach it to a pool/position, set thresholds, see a **preview** of the rebalance path, then **Review & sign** with their wallet. No custody, no bot that signs without the user.

### First strategy catalog

| Strategy | Trigger idea | Preview outcome |
|----------|--------------|-----------------|
| Stay in range | Position out of range | Recenter band ±σ (or fixed width) around spot |
| Fixed interval | Every N hours + drift &gt; threshold | Rebalance to target width |
| Fee harvest + compound | Unclaimed fees &gt; $X | Claim (+ optional re-deposit into range) |

### UX flow

1. `/strategies` — catalog cards  
2. Select strategy → bind pool/position (from watchlist or address positions)  
3. Configure parameters (width, interval, fee threshold, etc.) → **Save** (local and/or API)  
4. **Preview** panel: intended steps (e.g. withdraw → swap adjust → mint) + risk notes  
5. **Review & sign** — wallet transaction(s). If Aquarius rebalance tx wiring is not ready in this pass, the button stays visible but disabled with clear “signing path coming next” copy; preview + saved configs must still work end-to-end for demos  
6. Saved strategies list with status: Idle / Suggested / Awaiting signature

### Backend / honesty bar

- Prefer real preview math from existing pool/position metrics where possible; if a step cannot be computed yet, show a labeled placeholder step rather than fake numbers
- Persistence: **localStorage first** for this pass (keyed by Stellar address when connected); server persistence is out of scope unless it falls out naturally
- Copy and UI must never claim unsupervised auto-execution
- Full unattended execution = future grant milestone (out of scope)

## Architecture (frontend-focused this pass)

```
apps/web
  globals.css     ← token rewrite
  layout.tsx      ← Inter fonts, shell
  page.tsx        ← marketing landing (replace redirect)
  pools/*         ← restyle existing
  strategies/*    ← new route + components
  components/Header.tsx ← nav + CTA restyle
```

Backend changes for Strategies are optional in v1 (local persistence acceptable). No requirement to change pool-indexer for the visual pass.

## Out of scope

- Unattended on-chain auto-rebalance / key custody / keeper bots
- Full backtest engine or ML strategy ranking
- Multi-chain support (Stellar / Aquarius only)
- Pixel-perfect clone of every lpagent.io section/asset
- Rewriting pools data/API logic unless required for Strategies preview bindings

## Success criteria

- Visitor on `lumenlp.xyz` sees a grant-credible landing, not an immediate bare terminal redirect
- Pools and detail feel same family as landing (tokens, type, CTA)
- Strategies page demoable: pick strategy → configure → preview → clear sign/manual path
- Messaging consistent with Tier B (assisted rebalance, user signs)

## Implementation approach reminder

Execute as progressive restyle: tokens → landing → shell/nav → pools/detail → strategies. Do not big-bang rewrite the analytics surface.
