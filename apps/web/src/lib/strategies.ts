export type StrategyKind = "stay_in_range" | "fixed_interval" | "fee_harvest";
export type StrategyStatus = "idle" | "suggested" | "awaiting_signature";

export type StrategyParams =
  | { widthBps: number }
  | { intervalHours: number; driftBps: number }
  | { feeUsdThreshold: number; compound: boolean };

export type CopyDraftSnapshot = {
  kind: string;
  leaderAmounts: unknown;
  scaledAmounts: unknown;
  leaderQuoteXlm?: number | null;
  scaledQuoteXlm?: number | null;
};

export type SavedStrategy = {
  id: string;
  kind: StrategyKind;
  poolAddress: string;
  positionId?: string;
  copyOpId?: string;
  positionKey?: string;
  copyDraft?: CopyDraftSnapshot;
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
      copyOpId: typeof row.copyOpId === "string" ? row.copyOpId : undefined,
      positionKey: typeof row.positionKey === "string" ? row.positionKey : undefined,
      copyDraft: normalizeCopyDraft(row.copyDraft),
      status: row.status,
      params: row.params as StrategyParams,
      updatedAt: row.updatedAt,
    });
  }
  return out;
}

function normalizeCopyDraft(value: unknown): CopyDraftSnapshot | undefined {
  if (!value || typeof value !== "object" || Array.isArray(value)) return undefined;
  const row = value as Record<string, unknown>;
  if (typeof row.kind !== "string") return undefined;
  return {
    kind: row.kind,
    leaderAmounts: row.leaderAmounts,
    scaledAmounts: row.scaledAmounts,
    leaderQuoteXlm:
      typeof row.leaderQuoteXlm === "number" || row.leaderQuoteXlm == null
        ? (row.leaderQuoteXlm as number | null | undefined)
        : undefined,
    scaledQuoteXlm:
      typeof row.scaledQuoteXlm === "number" || row.scaledQuoteXlm == null
        ? (row.scaledQuoteXlm as number | null | undefined)
        : undefined,
  };
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

export function formatCopyDraftAmounts(draft: CopyDraftSnapshot): string {
  if (draft.leaderQuoteXlm != null && draft.scaledQuoteXlm != null) {
    const leader = Number(draft.leaderQuoteXlm).toFixed(2);
    const scaled = Number(draft.scaledQuoteXlm).toFixed(2);
    return `${leader} → ${scaled} XLM`;
  }
  if (draft.leaderAmounts != null && draft.scaledAmounts != null) {
    return `${JSON.stringify(draft.leaderAmounts)} → ${JSON.stringify(draft.scaledAmounts)}`;
  }
  return "scaled amounts unavailable";
}

export function buildRebalancePreview(input: {
  kind: StrategyKind;
  poolAddress: string;
  params: StrategyParams;
  inRange?: boolean | null;
  spotHint?: string | null;
  copyDraft?: CopyDraftSnapshot | null;
}): RebalancePreview {
  const honestyNote =
    "Preview only. Review & sign with your wallet — LumenLP never auto-executes without you.";
  const copyStep: PreviewStep | null = input.copyDraft
    ? {
        kind: "action",
        label: `Copy LP ${input.copyDraft.kind} (scaled)`,
        amountLabel: formatCopyDraftAmounts(input.copyDraft),
      }
    : null;

  if (input.kind === "stay_in_range") {
    const width = "widthBps" in input.params ? input.params.widthBps : 800;
    return {
      canCompute: true,
      honestyNote,
      steps: [
        ...(copyStep ? [copyStep] : []),
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
        ...(copyStep ? [copyStep] : []),
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
      ...(copyStep ? [copyStep] : []),
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
