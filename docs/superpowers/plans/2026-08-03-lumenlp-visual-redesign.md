# LumenLP Visual Redesign + Strategies (B) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Restyle LumenLP to a grant-ready lpagent.io-inspired look (lime signal palette + Inter + marketing landing) and ship a Tier B Strategies flow (configure → preview → disabled sign CTA) on Stellar/Aquarius.

**Architecture:** Progressive frontend restyle only. Shared CSS tokens first, then landing `/`, Header nav, pools/detail cleanup, then pure-TS strategy persistence/preview + `/strategies` page. No indexer/API schema changes; no unattended on-chain execution.

**Tech Stack:** Next.js 15 App Router, React 19, CSS variables in `globals.css`, Inter (Google Fonts), `localStorage` for strategies, Vitest for pure lib tests.

**Spec:** `docs/superpowers/specs/2026-08-03-lumenlp-visual-redesign-design.md`

---

## File map

| Path | Responsibility |
|------|----------------|
| `apps/web/src/app/globals.css` | Design tokens + shared component chrome |
| `apps/web/src/app/layout.tsx` | Inter font links, metadata, shell |
| `apps/web/src/app/page.tsx` | Marketing landing (replace redirect) |
| `apps/web/src/components/Header.tsx` | Nav: Pools / Strategies / API / wallet |
| `apps/web/src/app/pools/page.tsx` | Visual cleanup only (keep data logic) |
| `apps/web/src/app/pools/view/page.tsx` | Token-aligned detail + optional Apply strategy link |
| `apps/web/src/lib/strategies.ts` | Strategy types, catalog, localStorage, preview builder |
| `apps/web/src/lib/strategies.test.ts` | Vitest unit tests for persistence + preview |
| `apps/web/src/app/strategies/page.tsx` | Strategies hub UI |
| `apps/web/package.json` | Add `vitest` + `test` script |

---

### Task 1: Add Vitest + strategy domain tests (TDD)

**Files:**
- Modify: `apps/web/package.json`
- Create: `apps/web/vitest.config.ts`
- Create: `apps/web/src/lib/strategies.ts` (stubs first via failing tests)
- Create: `apps/web/src/lib/strategies.test.ts`

- [ ] **Step 1: Add vitest dependency and scripts**

```json
// apps/web/package.json — merge into existing
{
  "scripts": {
    "dev": "next dev -p 3000",
    "build": "next build",
    "start": "next start -p 3000",
    "test": "vitest run",
    "test:watch": "vitest"
  },
  "devDependencies": {
    "@types/node": "^22.15.0",
    "@types/react": "^19.1.0",
    "@types/react-dom": "^19.1.0",
    "typescript": "^5.8.0",
    "vitest": "^3.2.0"
  }
}
```

```ts
// apps/web/vitest.config.ts
import { defineConfig } from "vitest/config";
import path from "node:path";

export default defineConfig({
  test: {
    environment: "node",
    include: ["src/**/*.test.ts"],
  },
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
    },
  },
});
```

Run: `cd apps/web && npm install`

- [ ] **Step 2: Write failing tests**

```ts
// apps/web/src/lib/strategies.test.ts
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  STRATEGY_CATALOG,
  buildRebalancePreview,
  normalizeStrategies,
  readStrategies,
  upsertStrategy,
  type SavedStrategy,
} from "./strategies";

describe("normalizeStrategies", () => {
  it("drops malformed entries", () => {
    const raw = [
      {
        id: "s1",
        kind: "stay_in_range",
        poolAddress: "CPOOL",
        status: "idle",
        params: { widthBps: 500 },
        updatedAt: 1,
      },
      { id: 1 },
      null,
    ];
    expect(normalizeStrategies(raw)).toHaveLength(1);
    expect(normalizeStrategies(raw)[0].kind).toBe("stay_in_range");
  });
});

describe("STRATEGY_CATALOG", () => {
  it("exposes the three Tier B strategies", () => {
    expect(STRATEGY_CATALOG.map((s) => s.kind).sort()).toEqual([
      "fee_harvest",
      "fixed_interval",
      "stay_in_range",
    ]);
  });
});

describe("buildRebalancePreview", () => {
  it("returns labeled steps for stay_in_range", () => {
    const preview = buildRebalancePreview({
      kind: "stay_in_range",
      poolAddress: "CPOOL",
      params: { widthBps: 800 },
      inRange: false,
      spotHint: "out of range",
    });
    expect(preview.steps.length).toBeGreaterThanOrEqual(2);
    expect(preview.steps.every((s) => s.label.length > 0)).toBe(true);
    expect(preview.canCompute).toBe(true);
    expect(preview.honestyNote.toLowerCase()).toContain("sign");
  });

  it("marks unknown math as placeholder rather than inventing numbers", () => {
    const preview = buildRebalancePreview({
      kind: "fixed_interval",
      poolAddress: "CPOOL",
      params: { intervalHours: 6, driftBps: 100 },
    });
    expect(preview.steps.some((s) => s.kind === "placeholder" || s.amountLabel == null)).toBe(
      true,
    );
  });
});

describe("readStrategies / upsertStrategy", () => {
  beforeEach(() => {
    const store = new Map<string, string>();
    vi.stubGlobal("localStorage", {
      getItem: (k: string) => store.get(k) ?? null,
      setItem: (k: string, v: string) => {
        store.set(k, v);
      },
      removeItem: (k: string) => {
        store.delete(k);
      },
    });
    vi.stubGlobal("window", {});
  });

  it("persists by address key", () => {
    const base: SavedStrategy = {
      id: "s1",
      kind: "fee_harvest",
      poolAddress: "CPOOL",
      status: "idle",
      params: { feeUsdThreshold: 25, compound: true },
      updatedAt: 42,
    };
    upsertStrategy("GABC", base);
    expect(readStrategies("GABC")).toHaveLength(1);
    expect(readStrategies("GOTHER")).toHaveLength(0);
  });
});
```

- [ ] **Step 3: Run tests — expect FAIL**

Run: `cd apps/web && npm test`

Expected: module not found / exports missing.

- [ ] **Step 4: Implement `strategies.ts` to pass**

```ts
// apps/web/src/lib/strategies.ts
export type StrategyKind = "stay_in_range" | "fixed_interval" | "fee_harvest";
export type StrategyStatus = "idle" | "suggested" | "awaiting_signature";

export type StrategyParams =
  | { widthBps: number }
  | { intervalHours: number; driftBps: number }
  | { feeUsdThreshold: number; compound: boolean };

export type SavedStrategy = {
  id: string;
  kind: StrategyKind;
  poolAddress: string;
  positionId?: string;
  status: StrategyStatus;
  params: StrategyParams;
  updatedAt: number;
};

export type CatalogEntry = {
  kind: StrategyKind;
  title: string;
  blurb: string;
  defaultParams: StrategyParams;
};

export const STRATEGY_CATALOG: CatalogEntry[] = [
  {
    kind: "stay_in_range",
    title: "Stay in range",
    blurb: "When the position exits range, propose recenter around spot.",
    defaultParams: { widthBps: 800 },
  },
  {
    kind: "fixed_interval",
    title: "Fixed interval",
    blurb: "Every N hours, rebalance to target width if drift exceeds threshold.",
    defaultParams: { intervalHours: 6, driftBps: 150 },
  },
  {
    kind: "fee_harvest",
    title: "Fee harvest + compound",
    blurb: "Claim fees above a USD threshold; optionally re-deposit into range.",
    defaultParams: { feeUsdThreshold: 25, compound: true },
  },
];

export type PreviewStep = {
  kind: "action" | "placeholder";
  label: string;
  amountLabel?: string | null;
};

export type RebalancePreview = {
  steps: PreviewStep[];
  canCompute: boolean;
  honestyNote: string;
};

const STORAGE_PREFIX = "lumenlp.strategies.";

function isStrategyKind(value: unknown): value is StrategyKind {
  return value === "stay_in_range" || value === "fixed_interval" || value === "fee_harvest";
}

function isStatus(value: unknown): value is StrategyStatus {
  return value === "idle" || value === "suggested" || value === "awaiting_signature";
}

export function normalizeStrategies(value: unknown): SavedStrategy[] {
  if (!Array.isArray(value)) return [];
  const out: SavedStrategy[] = [];
  for (const item of value) {
    if (!item || typeof item !== "object") continue;
    const row = item as Record<string, unknown>;
    if (typeof row.id !== "string") continue;
    if (!isStrategyKind(row.kind)) continue;
    if (typeof row.poolAddress !== "string") continue;
    if (!isStatus(row.status)) continue;
    if (!row.params || typeof row.params !== "object") continue;
    if (typeof row.updatedAt !== "number") continue;
    out.push({
      id: row.id,
      kind: row.kind,
      poolAddress: row.poolAddress,
      positionId: typeof row.positionId === "string" ? row.positionId : undefined,
      status: row.status,
      params: row.params as StrategyParams,
      updatedAt: row.updatedAt,
    });
  }
  return out;
}

export function storageKey(address: string | null | undefined) {
  const key = (address ?? "anonymous").trim() || "anonymous";
  return `${STORAGE_PREFIX}${key}`;
}

export function readStrategies(address: string | null | undefined): SavedStrategy[] {
  if (typeof window === "undefined") return [];
  const raw = localStorage.getItem(storageKey(address));
  if (!raw) return [];
  try {
    return normalizeStrategies(JSON.parse(raw));
  } catch {
    localStorage.removeItem(storageKey(address));
    return [];
  }
}

export function writeStrategies(address: string | null | undefined, rows: SavedStrategy[]) {
  if (typeof window === "undefined") return;
  localStorage.setItem(storageKey(address), JSON.stringify(rows));
}

export function upsertStrategy(address: string | null | undefined, row: SavedStrategy) {
  const current = readStrategies(address);
  const next = [...current.filter((s) => s.id !== row.id), row].sort(
    (a, b) => b.updatedAt - a.updatedAt,
  );
  writeStrategies(address, next);
  return next;
}

export function deleteStrategy(address: string | null | undefined, id: string) {
  const next = readStrategies(address).filter((s) => s.id !== id);
  writeStrategies(address, next);
  return next;
}

export function buildRebalancePreview(input: {
  kind: StrategyKind;
  poolAddress: string;
  params: StrategyParams;
  inRange?: boolean | null;
  spotHint?: string | null;
}): RebalancePreview {
  const honestyNote =
    "Preview only. Review & sign with your wallet — LumenLP never auto-executes without you.";

  if (input.kind === "stay_in_range") {
    const width = "widthBps" in input.params ? input.params.widthBps : 800;
    return {
      canCompute: true,
      honestyNote,
      steps: [
        {
          kind: "action",
          label: "Withdraw current concentrated position",
          amountLabel: input.inRange === false ? "out of range" : input.spotHint ?? null,
        },
        {
          kind: "action",
          label: `Mint recentered range (±${width} bps around spot)`,
          amountLabel: null,
        },
      ],
    };
  }

  if (input.kind === "fixed_interval") {
    const interval = "intervalHours" in input.params ? input.params.intervalHours : 6;
    const drift = "driftBps" in input.params ? input.params.driftBps : 150;
    return {
      canCompute: false,
      honestyNote,
      steps: [
        {
          kind: "placeholder",
          label: `Check drift every ${interval}h (threshold ${drift} bps)`,
          amountLabel: null,
        },
        {
          kind: "placeholder",
          label: "Rebalance to target width when threshold breached",
          amountLabel: null,
        },
      ],
    };
  }

  const feeUsd = "feeUsdThreshold" in input.params ? input.params.feeUsdThreshold : 25;
  const compound = "compound" in input.params ? input.params.compound : true;
  return {
    canCompute: false,
    honestyNote,
    steps: [
      {
        kind: "placeholder",
        label: `Claim fees when unclaimed ≥ $${feeUsd}`,
        amountLabel: null,
      },
      {
        kind: compound ? "placeholder" : "action",
        label: compound ? "Re-deposit claimed fees into range" : "Leave claimed fees in wallet",
        amountLabel: null,
      },
    ],
  };
}

export function newStrategyId() {
  return `strat_${Date.now().toString(36)}_${Math.random().toString(36).slice(2, 8)}`;
}
```

- [ ] **Step 5: Run tests — expect PASS**

Run: `cd apps/web && npm test`

Expected: all tests pass.

- [ ] **Step 6: Commit**

```bash
git add apps/web/package.json apps/web/package-lock.json apps/web/vitest.config.ts \
  apps/web/src/lib/strategies.ts apps/web/src/lib/strategies.test.ts
git commit -m "$(cat <<'EOF'
Add strategies domain module with Vitest coverage.

Introduce Tier B catalog, localStorage persistence, and honest
rebalance preview builders before wiring the UI.
EOF
)"
```

---

### Task 2: Design tokens + Inter in layout

**Files:**
- Modify: `apps/web/src/app/globals.css` (replace `:root` and base `html, body`, buttons, links)
- Modify: `apps/web/src/app/layout.tsx`

- [ ] **Step 1: Update CSS variables and base surface**

Replace the top of `globals.css` `:root` / body with:

```css
:root {
  --bg: #090b08;
  --panel: #11140e;
  --line: rgba(132, 204, 22, 0.16);
  --line-soft: rgba(245, 245, 245, 0.08);
  --text: #f5f5f5;
  --muted: #a3a3a3;
  --accent: #84cc16;
  --accent-hi: #d9f99d;
  --accent-ink: #0a1004;
  --warn: #e6b35a;
  --danger: #e07a6a;
  --font: "Inter", -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
  --display: "Inter", -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
  --mono: ui-monospace, "SF Mono", Menlo, monospace;
}

html, body {
  margin: 0;
  padding: 0;
  background:
    radial-gradient(900px 480px at 50% -10%, rgba(132, 204, 22, 0.14), transparent 60%),
    var(--bg);
  color: var(--text);
  font-family: var(--font);
  min-height: 100%;
}

a { color: var(--accent); text-decoration: none; }
a:hover { color: var(--accent-hi); text-decoration: none; }

button.primary {
  background: var(--accent);
  border-color: var(--accent);
  color: var(--accent-ink);
  font-weight: 600;
}
button.primary:hover {
  background: var(--accent-hi);
  border-color: var(--accent-hi);
  color: var(--accent-ink);
}

.text-gradient {
  background-image: linear-gradient(90deg, var(--accent), var(--accent-hi));
  -webkit-background-clip: text;
  background-clip: text;
  color: transparent;
}
```

Also update sticky table header backgrounds that hardcode `#121a18` / old panel greens to `var(--panel)`.

Keep existing structural class names (`.panel`, `.toolbar`, `.terminal-table`, etc.) so pools JSX does not need a rewrite — only token + selective cleanup in later tasks.

- [ ] **Step 2: Switch layout fonts to Inter**

```tsx
// apps/web/src/app/layout.tsx — head links
<link
  href="https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700&display=swap"
  rel="stylesheet"
/>
```

Update metadata description to mention Strategies if desired; keep title `LumenLP — Stellar LP analytics`.

- [ ] **Step 3: Smoke check**

Run: `cd apps/web && npm run build`

Expected: build succeeds (pages may still redirect home).

- [ ] **Step 4: Commit**

```bash
git add apps/web/src/app/globals.css apps/web/src/app/layout.tsx
git commit -m "$(cat <<'EOF'
Apply LumenLP lime-signal tokens and Inter typography.

Align the shared CSS surface with the grant redesign spec before
landing and strategies UI land.
EOF
)"
```

---

### Task 3: Marketing landing at `/`

**Files:**
- Modify: `apps/web/src/app/page.tsx`
- Modify: `apps/web/src/app/globals.css` (add landing-only classes)

- [ ] **Step 1: Add landing CSS helpers**

Append to `globals.css`:

```css
.landing { display: grid; gap: 56px; padding-bottom: 48px; }
.landing-hero {
  position: relative;
  text-align: center;
  padding: 48px 12px 24px;
  overflow: hidden;
}
.landing-hero::before {
  content: "";
  position: absolute;
  left: 50%;
  top: -40px;
  transform: translateX(-50%);
  width: 520px;
  height: 320px;
  background: radial-gradient(ellipse, rgba(132, 204, 22, 0.2), transparent 70%);
  pointer-events: none;
}
.landing-eyebrow {
  color: var(--accent);
  text-transform: uppercase;
  letter-spacing: 0.14em;
  font-size: 0.72rem;
  margin-bottom: 14px;
}
.landing-title {
  margin: 0 auto;
  max-width: 16ch;
  font-size: clamp(2.2rem, 5vw, 3.6rem);
  font-weight: 700;
  line-height: 1.05;
}
.landing-lead {
  max-width: 52ch;
  margin: 16px auto 22px;
  color: var(--muted);
  line-height: 1.6;
}
.landing-actions { display: flex; gap: 10px; justify-content: center; flex-wrap: wrap; }
.btn-solid {
  display: inline-flex; align-items: center; justify-content: center;
  min-height: 44px; padding: 0 18px; border-radius: 10px;
  background: var(--accent); color: var(--accent-ink); font-weight: 600;
}
.btn-solid:hover { background: var(--accent-hi); color: var(--accent-ink); text-decoration: none; }
.btn-ghost {
  display: inline-flex; align-items: center; justify-content: center;
  min-height: 44px; padding: 0 18px; border-radius: 10px;
  border: 1px solid rgba(132, 204, 22, 0.35); color: var(--accent-hi);
  background: transparent;
}
.btn-ghost:hover { border-color: var(--accent); color: var(--accent); text-decoration: none; }
.protocol-row {
  display: flex; gap: 10px; justify-content: center; flex-wrap: wrap;
  margin-top: 22px; color: var(--muted); font-size: 0.75rem;
  text-transform: uppercase; letter-spacing: 0.08em;
}
.protocol-pill {
  border: 1px solid var(--line); background: rgba(132, 204, 22, 0.06);
  border-radius: 999px; padding: 6px 12px;
}
.act-grid { display: grid; gap: 14px; }
.act-card {
  border: 1px solid var(--line); border-radius: 16px; padding: 22px;
  background: rgba(132, 204, 22, 0.04);
}
.act-card:hover { border-color: rgba(132, 204, 22, 0.4); box-shadow: 0 0 40px rgba(132, 204, 22, 0.08); }
.act-label {
  color: var(--accent); font-size: 0.72rem; letter-spacing: 0.12em;
  text-transform: uppercase; margin-bottom: 8px;
}
.act-title { font-size: 1.25rem; font-weight: 600; margin: 0 0 8px; }
.act-body { color: var(--muted); line-height: 1.55; margin: 0; }
.landing-api {
  border: 1px solid var(--line); border-radius: 16px; overflow: hidden;
  background: #0d1117;
}
.landing-api pre {
  margin: 0; padding: 18px; overflow-x: auto; font-family: var(--mono);
  font-size: 0.82rem; line-height: 1.7; color: #d7e6df;
}
.landing-close {
  text-align: center; padding: 28px 12px 8px;
  border-top: 1px solid var(--line-soft);
}
@keyframes lumen-fade-up {
  from { opacity: 0; transform: translateY(18px); }
  to { opacity: 1; transform: translateY(0); }
}
.landing-hero, .act-card, .landing-api, .landing-close {
  animation: lumen-fade-up 0.65s ease-out both;
}
```

- [ ] **Step 2: Replace homepage redirect with landing**

```tsx
// apps/web/src/app/page.tsx
import Link from "next/link";

export default function HomePage() {
  return (
    <main className="landing">
      <section className="landing-hero">
        <div className="landing-eyebrow">Stellar LP analytics</div>
        <h1 className="landing-title">
          Track Stellar LP.{" "}
          <span className="text-gradient">RPC-first analytics.</span>
        </h1>
        <p className="landing-lead">
          Pool discovery, fee/TVL signals, portfolio visibility, and assisted
          rebalance strategies for Aquarius LPs — built as Stellar ecosystem
          infrastructure.
        </p>
        <div className="landing-actions">
          <Link className="btn-solid" href="/pools">
            Launch App
          </Link>
          <a className="btn-ghost" href="#api">
            Explore the API
          </a>
        </div>
        <div className="protocol-row">
          <span className="protocol-pill">Aquarius</span>
          <span className="protocol-pill">Soroban</span>
          <span className="protocol-pill">Stellar</span>
        </div>
      </section>

      <section className="act-grid" aria-label="Product acts">
        <article className="act-card">
          <div className="act-label">Act 01 · Discover</div>
          <h2 className="act-title">Compare pools by fee, TVL, and flow</h2>
          <p className="act-body">
            A terminal for Aquarius pools with windowed metrics, scores, and
            watchlists — no spreadsheet gymnastics.
          </p>
        </article>
        <article className="act-card">
          <div className="act-label">Act 02 · Monitor</div>
          <h2 className="act-title">See positions from a Stellar address</h2>
          <p className="act-body">
            Connect a wallet or paste a G… address to inspect LP exposure and
            portfolio summary in one place.
          </p>
        </article>
        <article className="act-card">
          <div className="act-label">Act 03 · Strategies</div>
          <h2 className="act-title">Rebalance with different strategies</h2>
          <p className="act-body">
            Stay in range, fixed interval, or fee harvest + compound. Configure
            rules, preview the path, then sign yourself — no unsupervised bots.
          </p>
        </article>
        <article className="act-card" id="api">
          <div className="act-label">Act 04 · Build</div>
          <h2 className="act-title">Reuse the analytics API</h2>
          <p className="act-body">
            Pools, history, and positions endpoints for wallets and Stellar DeFi
            frontends. RPC-first math you can trust.
          </p>
        </article>
      </section>

      <section className="landing-api" aria-label="API sample">
        <div className="panel-head">GET /v1/pools</div>
        <pre>{`const res = await fetch("https://api.lumenlp.xyz/v1/pools");
const { pools } = await res.json();
// fee/TVL, flow windows, scores — ready for your bot or UI`}</pre>
      </section>

      <section className="landing-close">
        <h2 className="act-title">Stop flying blind on Stellar LP.</h2>
        <p className="act-body" style={{ marginBottom: 16 }}>
          Launch the terminal or start with a strategy preview.
        </p>
        <div className="landing-actions">
          <Link className="btn-solid" href="/pools">
            Launch App
          </Link>
          <Link className="btn-ghost" href="/strategies">
            Browse strategies
          </Link>
        </div>
      </section>
    </main>
  );
}
```

- [ ] **Step 3: Verify**

Run: `cd apps/web && npm run build`

Expected: `/` no longer client-redirects; HTML includes “RPC-first analytics”.

Manual: `npm run dev` → open `/` → Launch App goes to `/pools`, Explore API scrolls to `#api`.

- [ ] **Step 4: Commit**

```bash
git add apps/web/src/app/page.tsx apps/web/src/app/globals.css
git commit -m "$(cat <<'EOF'
Replace home redirect with LumenLP marketing landing.

Add Discover/Monitor/Strategies/Build acts and API anchor for
grant-facing lumenlp.xyz demos.
EOF
)"
```

---

### Task 4: Header nav restyle

**Files:**
- Modify: `apps/web/src/components/Header.tsx`
- Modify: `apps/web/src/app/globals.css` (`.brand`, `.nav-links a.active` if needed)

- [ ] **Step 1: Update Header links**

```tsx
// apps/web/src/components/Header.tsx — nav portion
<header className="nav">
  <div>
    <Link href="/" className="brand">
      LumenLP
    </Link>
    <div className="brand-subline">Stellar LP analytics</div>
    <div className="nav-links" style={{ marginTop: 6 }}>
      <Link href="/pools" className={pathname?.startsWith("/pools") ? "active" : ""}>
        Pools
      </Link>
      <Link
        href="/strategies"
        className={pathname?.startsWith("/strategies") ? "active" : ""}
      >
        Strategies
      </Link>
      <Link href="/#api">API</Link>
    </div>
  </div>
  {/* keep existing identity / connect wallet block */}
</header>
```

Ensure `Link` is imported from `next/link`. Brand should link home. Keep connect/disconnect behavior unchanged.

- [ ] **Step 2: Active link color**

```css
.nav-links a.active { color: var(--accent); }
.brand { color: var(--text); }
.brand:hover { color: var(--accent-hi); text-decoration: none; }
```

- [ ] **Step 3: Verify**

Run: `cd apps/web && npm run build`

Manual: active state on `/pools` and `/strategies`; API jumps to landing `#api`.

- [ ] **Step 4: Commit**

```bash
git add apps/web/src/components/Header.tsx apps/web/src/app/globals.css
git commit -m "$(cat <<'EOF'
Extend app header with Strategies and API navigation.

Keep wallet connect behavior while aligning chrome to the new
landing information architecture.
EOF
)"
```

---

### Task 5: Pools + detail visual cleanup

**Files:**
- Modify: `apps/web/src/app/pools/page.tsx`
- Modify: `apps/web/src/app/pools/view/page.tsx`
- Modify: `apps/web/src/app/globals.css` (only if hardcoded old greens remain)

- [ ] **Step 1: Pools page chrome reduction**

Do **not** change fetch/filter/sort logic. UI-only:

1. Remove or hide redundant decorative strips that duplicate toolbar info (e.g. dense `.status-strip` pill rows if both meta line and chips say the same thing — keep one).
2. Ensure primary panel uses existing `.panel` / `.toolbar` without extra nested “card on card” wrappers where a single panel suffices.
3. Prefer `button.primary` solid accent for the main mode toggle currently selected.
4. Leave WatchlistPanel / table / card grid behavior intact.

Concrete check: after edit, `fetchPools`, `watchlistOnly`, and sort keys still compile and behave.

- [ ] **Step 2: Detail page token pass + Apply strategy link**

Near the detail hero actions / meta row, add:

```tsx
<Link
  className="btn-ghost"
  href={`/strategies?pool=${encodeURIComponent(poolAddress)}`}
>
  Apply strategy
</Link>
```

Use the same pool address variable already used for detail fetch. Soften any remaining `#121a18` / old mint hardcoded colors via CSS variables if present inline.

- [ ] **Step 3: Verify**

Run: `cd apps/web && npm run build`

Manual: `/pools` filters still work; detail “Apply strategy” lands on `/strategies?pool=…`.

- [ ] **Step 4: Commit**

```bash
git add apps/web/src/app/pools/page.tsx apps/web/src/app/pools/view/page.tsx apps/web/src/app/globals.css
git commit -m "$(cat <<'EOF'
Restyle pools terminal and detail to match landing tokens.

Reduce chrome noise and deep-link detail into Strategies without
changing analytics data paths.
EOF
)"
```

---

### Task 6: Strategies page UI

**Files:**
- Create: `apps/web/src/app/strategies/page.tsx`
- Modify: `apps/web/src/app/globals.css` (strategies layout classes)

- [ ] **Step 1: Add strategies CSS**

```css
.strategies-layout { display: grid; gap: 16px; }
.strategies-grid {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 12px;
  padding: 16px;
}
@media (max-width: 960px) {
  .strategies-grid { grid-template-columns: 1fr; }
}
.strategy-card {
  text-align: left;
  display: grid;
  gap: 8px;
  padding: 16px;
  border-radius: 14px;
  border: 1px solid var(--line);
  background: rgba(132, 204, 22, 0.04);
  color: var(--text);
}
.strategy-card.active {
  border-color: rgba(132, 204, 22, 0.45);
  background: rgba(132, 204, 22, 0.1);
}
.strategy-config {
  display: grid;
  gap: 12px;
  padding: 16px;
}
.strategy-preview {
  display: grid;
  gap: 10px;
  padding: 16px;
  border-top: 1px solid var(--line-soft);
}
.strategy-step {
  border: 1px solid var(--line-soft);
  border-radius: 12px;
  padding: 12px;
  background: rgba(255, 255, 255, 0.02);
}
.strategy-step.placeholder { border-style: dashed; color: var(--muted); }
.sign-disabled-note { color: var(--muted); font-size: 0.8rem; line-height: 1.45; }
```

- [ ] **Step 2: Implement page**

```tsx
// apps/web/src/app/strategies/page.tsx
"use client";

import { Suspense, useEffect, useMemo, useState } from "react";
import { useSearchParams } from "next/navigation";
import { useIdentity } from "@/lib/identity";
import {
  STRATEGY_CATALOG,
  buildRebalancePreview,
  deleteStrategy,
  newStrategyId,
  readStrategies,
  upsertStrategy,
  type SavedStrategy,
  type StrategyKind,
  type StrategyParams,
} from "@/lib/strategies";

function StrategiesInner() {
  const searchParams = useSearchParams();
  const poolFromQuery = searchParams.get("pool") ?? "";
  const { address } = useIdentity();
  const [kind, setKind] = useState<StrategyKind>("stay_in_range");
  const [poolAddress, setPoolAddress] = useState(poolFromQuery);
  const [saved, setSaved] = useState<SavedStrategy[]>([]);
  const catalog = STRATEGY_CATALOG.find((c) => c.kind === kind)!;
  const [params, setParams] = useState<StrategyParams>(catalog.defaultParams);

  useEffect(() => {
    setSaved(readStrategies(address));
  }, [address]);

  useEffect(() => {
    if (poolFromQuery) setPoolAddress(poolFromQuery);
  }, [poolFromQuery]);

  useEffect(() => {
    const entry = STRATEGY_CATALOG.find((c) => c.kind === kind)!;
    setParams(entry.defaultParams);
  }, [kind]);

  const preview = useMemo(
    () =>
      buildRebalancePreview({
        kind,
        poolAddress: poolAddress || "—",
        params,
        inRange: null,
      }),
    [kind, poolAddress, params],
  );

  function onSave() {
    if (!poolAddress.trim()) return;
    const row: SavedStrategy = {
      id: newStrategyId(),
      kind,
      poolAddress: poolAddress.trim(),
      status: "idle",
      params,
      updatedAt: Date.now(),
    };
    setSaved(upsertStrategy(address, row));
  }

  return (
    <div className="strategies-layout">
      <div className="panel">
        <div className="panel-head">Strategy catalog</div>
        <div className="strategies-grid">
          {STRATEGY_CATALOG.map((entry) => (
            <button
              key={entry.kind}
              type="button"
              className={`strategy-card ${kind === entry.kind ? "active" : ""}`}
              onClick={() => setKind(entry.kind)}
            >
              <strong>{entry.title}</strong>
              <span className="muted">{entry.blurb}</span>
            </button>
          ))}
        </div>
      </div>

      <div className="panel">
        <div className="panel-head">Configure · {catalog.title}</div>
        <div className="strategy-config">
          <label className="filter-field">
            <span className="filter-label">Pool contract</span>
            <input
              className="filter-input"
              value={poolAddress}
              onChange={(e) => setPoolAddress(e.target.value)}
              placeholder="C… pool address"
              spellCheck={false}
            />
          </label>

          {kind === "stay_in_range" && "widthBps" in params ? (
            <label className="filter-field">
              <span className="filter-label">Width (bps)</span>
              <input
                className="filter-input"
                type="number"
                value={params.widthBps}
                onChange={(e) => setParams({ widthBps: Number(e.target.value) || 0 })}
              />
            </label>
          ) : null}

          {kind === "fixed_interval" && "intervalHours" in params ? (
            <>
              <label className="filter-field">
                <span className="filter-label">Interval (hours)</span>
                <input
                  className="filter-input"
                  type="number"
                  value={params.intervalHours}
                  onChange={(e) =>
                    setParams({
                      intervalHours: Number(e.target.value) || 0,
                      driftBps: params.driftBps,
                    })
                  }
                />
              </label>
              <label className="filter-field">
                <span className="filter-label">Drift threshold (bps)</span>
                <input
                  className="filter-input"
                  type="number"
                  value={params.driftBps}
                  onChange={(e) =>
                    setParams({
                      intervalHours: params.intervalHours,
                      driftBps: Number(e.target.value) || 0,
                    })
                  }
                />
              </label>
            </>
          ) : null}

          {kind === "fee_harvest" && "feeUsdThreshold" in params ? (
            <>
              <label className="filter-field">
                <span className="filter-label">Fee threshold (USD)</span>
                <input
                  className="filter-input"
                  type="number"
                  value={params.feeUsdThreshold}
                  onChange={(e) =>
                    setParams({
                      feeUsdThreshold: Number(e.target.value) || 0,
                      compound: params.compound,
                    })
                  }
                />
              </label>
              <label className="filter-field filter-field-inline">
                <input
                  type="checkbox"
                  checked={params.compound}
                  onChange={(e) =>
                    setParams({
                      feeUsdThreshold: params.feeUsdThreshold,
                      compound: e.target.checked,
                    })
                  }
                />
                <span className="filter-label">Compound back into range</span>
              </label>
            </>
          ) : null}

          <div className="landing-actions" style={{ justifyContent: "flex-start" }}>
            <button type="button" className="primary" onClick={onSave} disabled={!poolAddress.trim()}>
              Save strategy
            </button>
          </div>
        </div>

        <div className="strategy-preview">
          <div className="filter-label">Preview</div>
          {preview.steps.map((step, idx) => (
            <div
              key={`${step.label}-${idx}`}
              className={`strategy-step ${step.kind === "placeholder" ? "placeholder" : ""}`}
            >
              <div>{step.label}</div>
              {step.amountLabel ? <div className="muted">{step.amountLabel}</div> : null}
            </div>
          ))}
          <p className="sign-disabled-note">{preview.honestyNote}</p>
          <button type="button" className="primary" disabled title="Signing path coming next">
            Review &amp; sign
          </button>
          <p className="sign-disabled-note">
            Signing path coming next — preview and saved configs work for demos now.
          </p>
        </div>
      </div>

      <div className="panel">
        <div className="panel-head">Saved ({address ? "wallet" : "anonymous"})</div>
        {saved.length === 0 ? (
          <div className="empty">No saved strategies yet.</div>
        ) : (
          <div className="strategies-grid">
            {saved.map((row) => (
              <div key={row.id} className="strategy-card">
                <strong>{STRATEGY_CATALOG.find((c) => c.kind === row.kind)?.title}</strong>
                <span className="muted">{row.poolAddress}</span>
                <span className="badge">{row.status}</span>
                <button type="button" onClick={() => setSaved(deleteStrategy(address, row.id))}>
                  Delete
                </button>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

export default function StrategiesPage() {
  return (
    <Suspense fallback={<div className="panel"><div className="empty">Loading strategies…</div></div>}>
      <StrategiesInner />
    </Suspense>
  );
}
```

- [ ] **Step 3: Verify**

Run:

```bash
cd apps/web && npm test && npm run build
```

Expected: tests pass; `/strategies` builds; query `?pool=` pre-fills; Save persists; Review & sign disabled.

Manual demo path: Landing → Strategies → pick Stay in range → paste pool → Save → confirm localStorage key `lumenlp.strategies.*`.

- [ ] **Step 4: Commit**

```bash
git add apps/web/src/app/strategies/page.tsx apps/web/src/app/globals.css
git commit -m "$(cat <<'EOF'
Ship Tier B Strategies hub with preview and local saves.

Catalog, configure, and honest disabled sign CTA — no unsupervised
execution in this pass.
EOF
)"
```

---

### Task 7: Final verification pass

**Files:** none required unless fixes

- [ ] **Step 1: Full frontend check**

```bash
cd apps/web && npm test && npm run build
```

Expected: PASS + production build OK.

- [ ] **Step 2: Manual checklist**

- [ ] `/` shows landing (no instant redirect)
- [ ] Launch App → `/pools`
- [ ] `#api` / Header API works
- [ ] Pools + detail readable under lime tokens
- [ ] Detail Apply strategy → `/strategies?pool=…`
- [ ] Strategies save/load; Review & sign disabled with honesty copy
- [ ] Copy never claims auto-exec without user

- [ ] **Step 3: Fix any regressions found, then commit if needed**

```bash
git add -A apps/web
git commit -m "$(cat <<'EOF'
Polish LumenLP redesign after verification pass.

Fix residual token or Strategies UX issues found in smoke checks.
EOF
)"
```

(Skip commit if nothing to fix.)

---

## Spec coverage check

| Spec item | Task |
|-----------|------|
| Lime tokens + Inter | Task 2 |
| Marketing landing `/` + 4 acts + `#api` | Task 3 |
| Header Pools / Strategies / API | Task 4 |
| Pools + detail restyle | Task 5 |
| Apply strategy deep-link | Task 5 |
| Strategy catalog (3 kinds) | Task 1 + 6 |
| Configure → preview → disabled sign | Task 6 |
| localStorage persistence | Task 1 + 6 |
| No unattended execution | Task 6 honesty copy |
| No backend/indexer required | (none) |

## Out of scope (do not implement in this plan)

- Keeper bots / custody / auto-sign
- Server-side strategy persistence
- Real Aquarius rebalance transaction construction
- Multi-chain
- Pixel clone of lpagent.io assets
