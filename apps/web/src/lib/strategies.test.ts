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
