"use client";

import { Suspense, useEffect, useMemo, useState } from "react";
import { useSearchParams } from "next/navigation";
import { useIdentity } from "@/lib/identity";
import { fetchPoolDetail } from "@/lib/api";
import {
  STRATEGY_CATALOG,
  buildRebalancePreview,
  deleteStrategy,
  formatCopyDraftAmounts,
  newStrategyId,
  readStrategies,
  upsertStrategy,
  type CopyDraftSnapshot,
  type SavedStrategy,
  type StrategyKind,
  type StrategyParams,
} from "@/lib/strategies";
import { copyOpToDraftSnapshot, getCopyOp } from "@/lib/copyLp";

function venueLabel(venue: string | null | undefined) {
  if (venue === "aquarius") return "Aquarius";
  if (venue === "phoenix") return "Phoenix";
  if (venue === "soroswap" || venue === "soroswap_amm") return "Soroswap";
  if (venue === "sushi" || venue === "sushi_v3") return "Sushi V3";
  if (venue === "comet") return "Comet";
  return !venue || venue === "unknown" ? "Unknown DEX" : venue;
}

function StrategiesInner() {
  const searchParams = useSearchParams();
  const poolFromQuery = searchParams.get("pool") ?? "";
  const copyOpFromQuery = searchParams.get("copyOp") ?? "";
  const poolLocked = Boolean(poolFromQuery);
  const { address } = useIdentity();
  const [kind, setKind] = useState<StrategyKind>("stay_in_range");
  const [poolAddress, setPoolAddress] = useState(poolFromQuery);
  const [saved, setSaved] = useState<SavedStrategy[]>([]);
  const [copyDraft, setCopyDraft] = useState<CopyDraftSnapshot | null>(null);
  const [poolVenue, setPoolVenue] = useState<string | null>(null);
  const catalog = STRATEGY_CATALOG.find((c) => c.kind === kind)!;
  const [params, setParams] = useState<StrategyParams>(catalog.defaultParams);

  useEffect(() => {
    setSaved(readStrategies(address));
  }, [address]);

  useEffect(() => {
    if (poolFromQuery) setPoolAddress(poolFromQuery);
  }, [poolFromQuery]);

  useEffect(() => {
    const candidate = poolAddress.trim();
    if (!candidate || !candidate.startsWith("C") || candidate.length < 50) {
      setPoolVenue(null);
      return;
    }
    let cancelled = false;
    void fetchPoolDetail(candidate)
      .then((detail) => {
        if (!cancelled) setPoolVenue(detail.venue ?? null);
      })
      .catch(() => {
        if (!cancelled) setPoolVenue(null);
      });
    return () => {
      cancelled = true;
    };
  }, [poolAddress]);

  useEffect(() => {
    const entry = STRATEGY_CATALOG.find((c) => c.kind === kind)!;
    setParams(entry.defaultParams);
  }, [kind]);

  useEffect(() => {
    if (!copyOpFromQuery) {
      setCopyDraft(null);
      return;
    }
    const fromSaved = readStrategies(address).find((s) => s.copyOpId === copyOpFromQuery);
    if (fromSaved?.copyDraft) {
      setCopyDraft(fromSaved.copyDraft);
      return;
    }
    let cancelled = false;
    void getCopyOp(copyOpFromQuery)
      .then((op) => {
        if (!cancelled) setCopyDraft(copyOpToDraftSnapshot(op));
      })
      .catch(() => {
        if (!cancelled) setCopyDraft(null);
      });
    return () => {
      cancelled = true;
    };
  }, [address, copyOpFromQuery]);

  const preview = useMemo(
    () =>
      buildRebalancePreview({
        kind,
        poolAddress: poolAddress || "—",
        params,
        inRange: null,
        copyDraft,
      }),
    [kind, poolAddress, params, copyDraft],
  );

  function onSave() {
    if (!poolAddress.trim()) return;
    const existingDraft = copyOpFromQuery
      ? saved.find((s) => s.copyOpId === copyOpFromQuery)
      : undefined;
    const row: SavedStrategy = {
      id: newStrategyId(),
      kind,
      poolAddress: poolAddress.trim(),
      copyOpId: copyOpFromQuery || undefined,
      positionKey: existingDraft?.positionKey,
      copyDraft: copyDraft ?? existingDraft?.copyDraft,
      status: "idle",
      params,
      updatedAt: Date.now(),
    };
    setSaved(upsertStrategy(address, row));
  }

  return (
    <div className="strategies-layout">
      {copyOpFromQuery ? (
        <div className="copy-banner">
          Draft from CopyOp <strong>{copyOpFromQuery}</strong>
          {poolLocked ? (
            <span>
              {" "}
              · pool locked to <span title={poolFromQuery}>{poolFromQuery}</span>
            </span>
          ) : null}
          {copyDraft ? (
            <div className="copy-banner-amounts">
              Scaled {copyDraft.kind}: {formatCopyDraftAmounts(copyDraft)}
            </div>
          ) : (
            <div className="copy-banner-amounts">Loading scaled amounts…</div>
          )}
        </div>
      ) : null}

      <div className="panel">
        <div className="panel-head">Strategy catalog · auto-rebalance</div>
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
            <span className="filter-label">
              Pool contract {poolVenue ? <span className="badge">{venueLabel(poolVenue)}</span> : null}
            </span>
            <input
              className="filter-input"
              value={poolAddress}
              onChange={(e) => setPoolAddress(e.target.value)}
              placeholder="C… pool address"
              spellCheck={false}
              readOnly={poolLocked}
            />
          </label>
          {poolVenue && poolVenue !== "aquarius" ? (
            <p className="sign-disabled-note">
              {venueLabel(poolVenue)} analytics and strategy previews are available. Live Copy LP
              execution is not enabled for this venue yet.
            </p>
          ) : null}

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
                {row.copyDraft ? (
                  <span className="muted">{formatCopyDraftAmounts(row.copyDraft)}</span>
                ) : null}
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
    <Suspense
      fallback={
        <div className="panel">
          <div className="empty">Loading strategies…</div>
        </div>
      }
    >
      <StrategiesInner />
    </Suspense>
  );
}
