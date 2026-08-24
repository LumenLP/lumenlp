"use client";

import { Suspense, useEffect, useState } from "react";
import { useSearchParams } from "next/navigation";
import Link from "next/link";
import { TokenPairMark, tokenVisualsFromMeta } from "@/components/TokenIdentity";
import {
  fetchPoolDetail,
  fetchPoolEvents,
  fetchPoolHistory,
  fmtNum,
  fmtPct,
  fmtTs,
  fmtUnixTs,
  fmtUsd,
  pickUsd,
  shortAddr,
  type HistoryPoint,
  type PoolDetailResponse,
  type PoolEventRow,
} from "@/lib/api";
function normalizedTokenLabel(label: string | null | undefined) {
  if (!label) return "";
  return label.toLowerCase() === "native" ? "XLM" : label;
}

function poolTypeLabel(type: string | null | undefined) {
  if (!type || type === "unknown") return "Unknown";
  if (type === "constant_product") return "AMM";
  if (type === "concentrated") return "CLMM";
  if (type === "weighted") return "Weighted";
  if (type === "stable") return "Stable";
  return type;
}

function venueLabel(venue: string | null | undefined) {
  if (venue === "aquarius") return "Aquarius";
  if (venue === "phoenix") return "Phoenix";
  if (venue === "soroswap" || venue === "soroswap_amm") return "Soroswap";
  if (venue === "sushi" || venue === "sushi_v3") return "Sushi V3";
  if (venue === "comet") return "Comet";
  return !venue || venue === "unknown" ? "Unknown DEX" : venue;
}

function venueCopyEnabled(venue: string | null | undefined) {
  return venue === "aquarius";
}

function detailTokenPairLabel(detail: PoolDetailResponse | null, address: string) {
  const labels =
    detail?.token_meta
      ?.map((token) => normalizedTokenLabel(token.symbol?.trim()))
      .filter(Boolean) ?? [];
  if (labels.length >= 2) return `${labels[0]} / ${labels[1]}`;
  if (detail?.tokens?.length) return detail.tokens.slice(0, 2).map(shortAddr).join(" / ");
  return shortAddr(address);
}

function detailTokenSubtitle(detail: PoolDetailResponse | null) {
  const labels =
    detail?.token_meta
      ?.map(
        (token) =>
          token.name?.trim() ||
          normalizedTokenLabel(token.symbol?.trim()) ||
          shortAddr(token.address),
      )
      .filter(Boolean) ?? [];
  if (labels.length >= 2) return labels.join(" / ");
  return detail?.tokens?.map(shortAddr).join(" / ") ?? "";
}

function detailTokenVisuals(detail: PoolDetailResponse | null) {
  return tokenVisualsFromMeta(detail?.token_meta, detail?.tokens);
}

function tokenDisplayLabel(
  token: NonNullable<PoolDetailResponse["token_meta"]>[number] | undefined,
  fallback: string | undefined,
) {
  if (token) {
    return (
      normalizedTokenLabel(token.symbol?.trim()) ||
      token.name?.trim() ||
      shortAddr(token.address)
    );
  }
  return fallback ? shortAddr(fallback) : "Unknown";
}

function compactTokenId(value: string | null | undefined) {
  if (!value) return "Token metadata pending";
  const separator = value.indexOf(":");
  if (separator <= 0) return value;
  const code = value.slice(0, separator);
  const issuer = value.slice(separator + 1);
  return `${code}:${shortAddr(issuer)}`;
}

function findPointHoursBack(points: HistoryPoint[], hoursBack: number) {
  if (points.length === 0) return null;
  const latestTs = Date.parse(points[points.length - 1].ts);
  if (Number.isNaN(latestTs)) return null;
  const target = latestTs - hoursBack * 3600 * 1000;
  for (let index = points.length - 1; index >= 0; index -= 1) {
    const ts = Date.parse(points[index].ts);
    if (!Number.isNaN(ts) && ts <= target) {
      return points[index];
    }
  }
  return points[0] ?? null;
}

function pctMove(current: number | null | undefined, previous: number | null | undefined) {
  if (current == null || previous == null || previous === 0) return null;
  return (current - previous) / previous;
}

function absMove(current: number | null | undefined, previous: number | null | undefined) {
  if (current == null || previous == null) return null;
  return current - previous;
}

function trendTone(value: number | null | undefined, positiveIsGood = true) {
  if (value == null || Number.isNaN(value) || value === 0) return "flat";
  if (positiveIsGood) return value > 0 ? "up" : "down";
  return value > 0 ? "down" : "up";
}

function trendLabel(value: number | null | undefined, format: "pct" | "num" = "pct") {
  if (value == null || Number.isNaN(value)) return "—";
  if (format === "pct") return `${value > 0 ? "+" : ""}${fmtPct(value)}`;
  return `${value > 0 ? "+" : ""}${fmtNum(value, 2)}`;
}

function PoolDetailInner() {
  const windows = ["5m", "1h", "6h", "24h"] as const;
  const eventTabs = ["all", "swaps", "liquidity", "claims", "updates"] as const;
  const chartModes = ["tvl", "volume", "apr"] as const;
  const search = useSearchParams();
  const address = (search.get("address") ?? "").trim();
  const [points, setPoints] = useState<HistoryPoint[]>([]);
  const [detail, setDetail] = useState<PoolDetailResponse | null>(null);
  const [events, setEvents] = useState<PoolEventRow[]>([]);
  const [windowKey, setWindowKey] = useState<(typeof windows)[number]>("24h");
  const [eventTab, setEventTab] = useState<(typeof eventTabs)[number]>("all");
  const [chartMode, setChartMode] = useState<(typeof chartModes)[number]>("tvl");
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!address) return;
    fetchPoolDetail(address)
      .then((r) => setDetail(r))
      .catch((e: Error) => setError(e.message));
    fetchPoolHistory(address)
      .then((r) => setPoints(r.points ?? []))
      .catch((e: Error) => setError(e.message));
    fetchPoolEvents(address, 48)
      .then((r) => setEvents(r.events ?? []))
      .catch((e: Error) => setError(e.message));
  }, [address]);

  if (!address) {
    return <div className="empty">Missing pool address. Pick one from the pools list.</div>;
  }

  const latest = points[points.length - 1];
  const currentWindow = detail?.window_metrics?.[windowKey];
  const activity = detail?.activity;
  const activitySummary = detail?.activity_summary;
  const liquidityEvents = events.filter(
    (event) =>
      event.kind === "deposit_liquidity" || event.kind === "withdraw_liquidity",
  );
  const claimEvents = events.filter(
    (event) =>
      event.kind === "claim_fees" || event.kind === "claim_protocol_fee",
  );
  const reserveEvents = events.filter(
    (event) =>
      event.kind === "update_reserves" || event.kind === "reserves_sync",
  );
  const swapEvents = events.filter((event) => event.kind === "trade");
  const visibleEvents =
    eventTab === "swaps"
      ? swapEvents
      : eventTab === "liquidity"
        ? liquidityEvents
      : eventTab === "claims"
        ? claimEvents
      : eventTab === "updates"
      ? reserveEvents
      : events;
  const eventTabCounts = {
    all: events.length,
    swaps: swapEvents.length,
    liquidity: liquidityEvents.length,
    claims: claimEvents.length,
    updates: reserveEvents.length,
  };
  const visibleTabTitle =
    eventTab === "all"
      ? "All recent events"
      : eventTab === "swaps"
        ? "Recent swaps"
      : eventTab === "liquidity"
        ? "Recent liquidity changes"
      : eventTab === "claims"
        ? "Recent fee claims"
      : "Recent reserve updates";
  const pairLabel = detailTokenPairLabel(detail, address);
  const pairSubtitle = detailTokenSubtitle(detail);
  const pairVisuals = detailTokenVisuals(detail);
  const latestTvl = detail?.tvl ?? detail?.latest?.tvl ?? latest?.tvl;
  const score = detail?.score;
  const eventMix = activitySummary ? eventMixSegments(activitySummary) : [];
  const scoreBreakdown = detail?.score_breakdown;
  const tokenCards = (detail?.tokens ?? []).slice(0, 2).map((tokenAddress, index) => ({
    address: tokenAddress,
    meta: detail?.token_meta?.[index],
    label: tokenDisplayLabel(detail?.token_meta?.[index], tokenAddress),
    name: compactTokenId(detail?.token_meta?.[index]?.name?.trim()),
    issuer: detail?.token_meta?.[index]?.issuer?.trim() || null,
    domain: detail?.token_meta?.[index]?.domain?.trim() || null,
    icon: detail?.token_meta?.[index]?.icon?.trim() || null,
  }));
  const xlmUsd = detail?.quote?.xlm_usd;
  // Treat 0 as missing so unsapped pools don't flash `$0.00` before/without fallbacks.
  const positiveUsd = (n: number | null | undefined) =>
    n != null && Number.isFinite(n) && n > 0 ? n : null;
  const displayTvlUsd = positiveUsd(
    pickUsd(positiveUsd(detail?.tvl_usd), latestTvl, xlmUsd).value,
  );
  const windowVolumeUsd = pickUsd(
    currentWindow?.volume_usd,
    currentWindow?.volume,
    xlmUsd,
  ).value;
  const displayVolumeUsd = positiveUsd(
    positiveUsd(windowVolumeUsd) ??
      (windowKey === "24h"
        ? pickUsd(
            activitySummary?.volume_usd_24h,
            activitySummary?.volume_quote_24h,
            xlmUsd,
          ).value
        : null),
  );
  const windowFeeUsd = pickUsd(currentWindow?.fee_usd, currentWindow?.fee, xlmUsd).value;
  const displayFeeUsd = positiveUsd(
    positiveUsd(windowFeeUsd) ??
      (windowKey === "24h"
        ? pickUsd(activitySummary?.fee_usd_24h, activitySummary?.fee_quote_24h, xlmUsd)
            .value
        : null),
  );
  const displayAvgTvlUsd = positiveUsd(
    pickUsd(null, currentWindow?.avg_tvl ?? latestTvl, xlmUsd).value,
  );
  const netFlowUsd = pickUsd(
    activitySummary?.net_liquidity_delta_usd_24h,
    activitySummary?.net_liquidity_delta_quote_24h,
    xlmUsd,
  ).value;
  const claimUsd = pickUsd(
    activitySummary?.claim_usd_24h,
    activitySummary?.claim_quote_24h,
    xlmUsd,
  ).value;
  const volumeUsd24h = pickUsd(
    activitySummary?.volume_usd_24h,
    activitySummary?.volume_quote_24h,
    xlmUsd,
  ).value;
  const feeUsd24h = pickUsd(
    activitySummary?.fee_usd_24h,
    activitySummary?.fee_quote_24h,
    xlmUsd,
  ).value;
  const eventBurst = [
    {
      label: "Swaps",
      value: activitySummary?.swap_count_24h ?? 0,
      tone: "good",
      hint: `${fmtNum(currentWindow?.tx_count ?? 0, 0)} total txs in ${windowKey}`,
      format: "count" as const,
    },
    {
      label: "Net Flow",
      value: netFlowUsd,
      tone:
        (netFlowUsd ?? 0) > 0 ? "good" : (netFlowUsd ?? 0) < 0 ? "warn" : "muted",
      hint: "24h liquidity delta",
      format: "usd" as const,
    },
    {
      label: "Claims",
      value: claimUsd,
      tone: "warm",
      hint: `${fmtNum(activitySummary?.claim_count_24h ?? 0, 0)} claim events`,
      format: "usd" as const,
    },
    {
      label: "Cadence",
      value: activitySummary?.avg_update_interval_secs_24h ?? 0,
      tone: "muted",
      hint: "avg reserve update gap",
      format: "cadence" as const,
    },
  ];
  const scoreCards = scoreBreakdown
    ? [
        {
          label: "Fee yield",
          value: scoreBreakdown.fee_tvl_component,
          hint: fmtPct(scoreBreakdown.inputs?.fee_tvl_24h),
        },
        {
          label: "Volume",
          value: scoreBreakdown.volume_component,
          hint: fmtNum(scoreBreakdown.inputs?.volume_24h, 2),
        },
        {
          label: "Net liq",
          value: scoreBreakdown.net_liq_component,
          hint: fmtNum(scoreBreakdown.inputs?.net_liquidity_delta_quote_24h, 2),
        },
        {
          label: "Cadence",
          value: scoreBreakdown.cadence_component,
          hint: fmtCadence(scoreBreakdown.inputs?.avg_update_interval_secs_24h),
        },
      ]
    : [];
  const chartSeries = points.map((point) =>
    chartMode === "tvl"
      ? point.tvl
      : chartMode === "volume"
        ? point.volume_24h
        : point.est_apr,
  );
  const chartPeak = Math.max(...chartSeries, 1e-9);
  const latestChartValue = chartSeries[chartSeries.length - 1] ?? null;
  const chartTitle =
    chartMode === "tvl"
      ? "TVL history"
      : chartMode === "volume"
        ? "Volume history"
        : "Estimated APR history";
  const chartValueLabel =
    chartMode === "tvl"
      ? "Latest TVL"
      : chartMode === "volume"
        ? "Latest volume"
        : "Latest APR";
  const chartPeakLabel =
    chartMode === "tvl"
      ? "Peak TVL"
      : chartMode === "volume"
        ? "Peak volume"
        : "Peak APR";
  const point6h = findPointHoursBack(points, 6);
  const point24h = findPointHoursBack(points, 24);
  const aprNow = latest?.est_apr ?? null;
  const tvlNow = latestTvl ?? null;
  const volumeNow = latest?.volume_24h ?? currentWindow?.volume ?? null;
  const aprDelta6h = absMove(aprNow, point6h?.est_apr);
  const aprDelta24h = absMove(aprNow, point24h?.est_apr);
  const tvlDelta24h = pctMove(tvlNow, point24h?.tvl);
  const volumeDelta24h = pctMove(volumeNow, point24h?.volume_24h);
  const feeEfficiency = currentWindow?.avg_tvl
    ? (currentWindow?.volume ?? 0) / Math.max(currentWindow.avg_tvl, 1)
    : null;
  const realizedFeeRate =
    currentWindow?.volume && currentWindow.volume > 0
      ? (currentWindow.fee ?? 0) / currentWindow.volume
      : null;
  const opportunityCards = [
    {
      label: "APR vs 6h",
      value: trendLabel(aprDelta6h, "num"),
      tone: trendTone(aprDelta6h, true),
      hint: "change in estimated fee APR",
    },
    {
      label: "APR vs 24h",
      value: trendLabel(aprDelta24h, "num"),
      tone: trendTone(aprDelta24h, true),
      hint: "longer trend in fee yield",
    },
    {
      label: "TVL vs 24h",
      value: trendLabel(tvlDelta24h, "pct"),
      tone: trendTone(tvlDelta24h, true),
      hint: "capital moving in or out",
    },
    {
      label: "Volume vs 24h",
      value: trendLabel(volumeDelta24h, "pct"),
      tone: trendTone(volumeDelta24h, true),
      hint: "flow acceleration or slowdown",
    },
  ];
  const efficiencyCards = [
    {
      label: "Vol / Avg TVL",
      value: feeEfficiency == null ? "—" : fmtPct(feeEfficiency),
      hint: `${windowKey} volume efficiency`,
    },
    {
      label: "Fee / Volume",
      value: realizedFeeRate == null ? "—" : fmtPct(realizedFeeRate),
      hint: "realized fee capture rate",
    },
    {
      label: "Fee / TVL",
      value: fmtPct(currentWindow?.fee_tvl),
      hint: `${windowKey} fee return on capital`,
    },
    {
      label: "Samples",
      value: fmtNum(currentWindow?.samples ?? 0, 0),
      hint: "snapshot count in window",
    },
  ];
  const opportunitySummary =
    score != null && (currentWindow?.fee_tvl ?? 0) > 0.01
      ? "Pool currently combines measurable fee yield with usable recent activity."
      : (activitySummary?.swap_count_24h ?? 0) > 0 && (currentWindow?.volume ?? 0) > 0
        ? "Flow exists, but yield quality still needs inspection."
        : "Pool has limited recent flow, so treat surface metrics cautiously.";

  return (
    <>
      <section className="detail-hero">
        <div className="detail-hero-main">
          <p className="muted">
            <Link href="/pools">← Back to pools</Link>
          </p>
          <div className="eyebrow">Pool overview</div>
          <div className="detail-title-row">
            <TokenPairMark tokens={pairVisuals} size="lg" />
            <h1 className="detail-title">{pairLabel}</h1>
          </div>
          <p className="detail-subtitle">{pairSubtitle || address}</p>
          <div className="detail-meta-row">
            <span className="badge">{venueLabel(detail?.venue)}</span>
            <span className="badge">{poolTypeLabel(detail?.pool_type)}</span>
            <span className="badge">{detail?.fee_bps ?? 0} bps</span>
            {score != null ? <span className="badge">score {fmtNum(score, 2)}</span> : null}
            <Link
              className="btn-ghost"
              href={`/strategies?pool=${encodeURIComponent(address)}`}
            >
              {venueCopyEnabled(detail?.venue) ? "Apply strategy" : "View strategy preview"}
            </Link>
            <span className="muted">latest {fmtTs(detail?.last_snapshot_at)}</span>
          </div>
          <p className="muted detail-address">{address}</p>
          <div className="hero-stats detail-stats">
            <div className="hero-stat">
              <span className="hero-stat-label">TVL</span>
              <strong>{fmtUsd(displayTvlUsd)}</strong>
            </div>
            <div className="hero-stat">
              <span className="hero-stat-label">Volume ({windowKey})</span>
              <strong>{fmtUsd(displayVolumeUsd)}</strong>
            </div>
            <div className="hero-stat">
              <span className="hero-stat-label">Fee / TVL</span>
              <strong>{fmtPct(currentWindow?.fee_tvl)}</strong>
            </div>
            <div className="hero-stat">
              <span className="hero-stat-label">24h swaps</span>
              <strong>{fmtNum(activitySummary?.swap_count_24h ?? activity?.swap_count ?? 0, 0)}</strong>
            </div>
          </div>
          {detail?.tvl_status === "missing_price" ? (
            <p className="muted">TVL unavailable: token price data is not available yet.</p>
          ) : detail?.tvl_status === "empty_reserves" ? (
            <p className="muted">TVL unavailable: the pool currently reports empty reserves.</p>
          ) : null}
        </div>
        <div className="detail-hero-side">
          <div className="panel-head">{chartTitle}</div>
          <div className="detail-chart-toolbar">
            <div className="segmented">
              {chartModes.map((mode) => (
                <button
                  key={mode}
                  type="button"
                  className={mode === chartMode ? "primary" : undefined}
                  onClick={() => setChartMode(mode)}
                >
                  {mode === "tvl" ? "TVL" : mode === "volume" ? "Volume" : "APR"}
                </button>
              ))}
            </div>
            <div className="detail-chart-stats">
              <span className="status-pill">
                {chartValueLabel}{" "}
                {chartMode === "apr" ? fmtPct(latestChartValue) : fmtNum(latestChartValue, 2)}
              </span>
              <span className="status-pill">
                {chartPeakLabel}{" "}
                {chartMode === "apr" ? fmtPct(chartPeak) : fmtNum(chartPeak, 2)}
              </span>
            </div>
          </div>
          <div className="chart detail-chart">
            {points.length === 0 ? (
              <div className="empty" style={{ width: "100%" }}>
                No snapshots yet
              </div>
            ) : (
              points.map((p) => (
                <div
                  key={p.ts}
                  className="bar"
                  style={{
                    height: `${Math.max(
                      4,
                      ((chartMode === "tvl"
                        ? p.tvl
                        : chartMode === "volume"
                          ? p.volume_24h
                          : p.est_apr) /
                        chartPeak) *
                        100,
                    )}%`,
                    background:
                      chartMode === "tvl"
                        ? "linear-gradient(180deg, var(--accent), transparent)"
                        : chartMode === "volume"
                          ? "linear-gradient(180deg, #4ea8de, transparent)"
                          : "linear-gradient(180deg, #e9c46a, transparent)",
                  }}
                  title={`${p.ts} ${
                    chartMode === "tvl"
                      ? `tvl=${p.tvl}`
                      : chartMode === "volume"
                        ? `volume=${p.volume_24h}`
                        : `apr=${p.est_apr}`
                  }`}
                />
              ))
            )}
          </div>
        </div>
      </section>

      <div className="detail-section-grid" style={{ marginBottom: 16 }}>
        <div className="panel">
          <div className="panel-head">Token profile</div>
          <div className="token-profile-grid">
            {tokenCards.map((token) => (
              <div key={token.address} className="token-profile-card">
                <div className="token-profile-header">
                  <TokenPairMark
                    tokens={[
                      {
                        key: token.address,
                        label: token.label,
                        name: token.name,
                        issuer: token.issuer,
                        domain: token.domain,
                        icon: token.icon,
                        seed: token.address,
                      },
                    ]}
                    size="sm"
                  />
                  <div className="token-profile-label">{token.label}</div>
                </div>
                <div className="token-profile-name">{token.name || "Token metadata pending"}</div>
                <div className="token-profile-meta">
                  {token.domain ? <span className="badge">{token.domain}</span> : <span className="badge">domain pending</span>}
                  {token.issuer ? <span className="badge">{shortAddr(token.issuer)}</span> : null}
                </div>
                <div
                  className="token-profile-address"
                  title={token.address}
                  aria-label={`Token address ${token.address}`}
                >
                  {shortAddr(token.address)}
                </div>
              </div>
            ))}
            {tokenCards.length === 0 ? (
              <div className="empty" style={{ width: "100%" }}>
                Token metadata not indexed for this pool yet
              </div>
            ) : null}
          </div>
        </div>
        <div className="panel">
          <div className="panel-head">24h health strip</div>
          <div className="health-strip">
            {eventBurst.map((item) => (
              <div key={item.label} className={`health-card tone-${item.tone}`}>
                <div className="health-label">{item.label}</div>
                <div className="health-value">
                  {item.format === "cadence"
                    ? fmtCadence(item.value as number)
                    : item.format === "usd"
                      ? fmtUsd(item.value as number | null)
                      : fmtNum(item.value as number, 0)}
                </div>
                <div className="health-hint">{item.hint}</div>
              </div>
            ))}
          </div>
        </div>
      </div>

      <div className="detail-section-grid" style={{ marginBottom: 16 }}>
        <div className="panel">
          <div className="panel-head">Opportunity read</div>
          <div className="opportunity-summary">
            <div className="opportunity-summary-copy">
              <div className="opportunity-summary-title">{opportunitySummary}</div>
              <div className="opportunity-summary-text">
                Read this as a triage layer before digging into event rows and raw swaps.
              </div>
            </div>
            <div className="opportunity-grid">
              {opportunityCards.map((item) => (
                <div key={item.label} className={`opportunity-card tone-${item.tone}`}>
                  <div className="opportunity-label">{item.label}</div>
                  <div className="opportunity-value">{item.value}</div>
                  <div className="opportunity-hint">{item.hint}</div>
                </div>
              ))}
            </div>
          </div>
        </div>
        <div className="panel">
          <div className="panel-head">Efficiency panel</div>
          <div className="score-breakdown-grid">
            {efficiencyCards.map((item) => (
              <div key={item.label} className="score-breakdown-card">
                <div className="score-breakdown-label">{item.label}</div>
                <div className="score-breakdown-value">{item.value}</div>
                <div className="score-breakdown-hint">{item.hint}</div>
              </div>
            ))}
          </div>
        </div>
      </div>

      <div className="cards" style={{ marginTop: 16 }}>
        <div className="card">
          <div className="label">Fee ({windowKey})</div>
          <div className="value">{fmtUsd(displayFeeUsd)}</div>
        </div>
        <div className="card">
          <div className="label">Avg TVL ({windowKey})</div>
          <div className="value">{fmtUsd(displayAvgTvlUsd)}</div>
        </div>
        <div className="card">
          <div className="label">Txs ({windowKey})</div>
          <div className="value">{fmtNum(currentWindow?.tx_count ?? 0, 0)}</div>
        </div>
        <div className="card">
          <div className="label">Events</div>
          <div className="value">{fmtNum(activity?.event_count ?? 0, 0)}</div>
        </div>
        <div className="card">
          <div className="label">Swaps</div>
          <div className="value">{fmtNum(activity?.swap_count ?? 0, 0)}</div>
        </div>
        <div className="card">
          <div className="label">Last event</div>
          <div className="value">{fmtUnixTs(activity?.last_event_at)}</div>
        </div>
      </div>

      {error ? <div className="panel error">{error}</div> : null}
      {detail?.note ? <div className="panel warning">{detail.note}</div> : null}

      <div className="panel" style={{ marginBottom: 16 }}>
        <div className="panel-head">Analysis window</div>
        <div className="toolbar">
          <div className="identity">
            <div className="segmented">
              {windows.map((window) => (
                <button
                  key={window}
                  type="button"
                  className={window === windowKey ? "primary" : undefined}
                  onClick={() => setWindowKey(window)}
                >
                  {window}
                </button>
              ))}
            </div>
            <span className="status-pill">
              {currentWindow?.samples ?? 0} samples · rollup{" "}
              {currentWindow?.as_of_ts
                ? fmtTs(new Date(currentWindow.as_of_ts * 1000).toISOString())
                : "—"}
            </span>
          </div>
        </div>
      </div>

      {scoreCards.length > 0 ? (
        <div className="panel" style={{ marginBottom: 16 }}>
          <div className="panel-head">Score breakdown</div>
          <div className="score-breakdown-grid">
            {scoreCards.map((item) => (
              <div key={item.label} className="score-breakdown-card">
                <div className="score-breakdown-label">{item.label}</div>
                <div className="score-breakdown-value">{fmtNum(item.value, 2)}</div>
                <div className="score-breakdown-hint">{item.hint}</div>
              </div>
            ))}
          </div>
        </div>
      ) : null}

      {activitySummary ? (
        <>
          <div className="detail-section-grid" style={{ marginTop: 16 }}>
            <div className="panel">
              <div className="panel-head">24h event mix</div>
              <div style={{ padding: 16 }}>
                <div className="mix-track">
                  {eventMix.map((segment) => (
                    <div
                      key={segment.label}
                      className="mix-segment"
                      style={{ width: `${segment.ratio * 100}%`, background: segment.color }}
                      title={`${segment.label} ${segment.count} (${(segment.ratio * 100).toFixed(1)}%)`}
                    />
                  ))}
                </div>
                <div className="mix-grid">
                  {eventMix.map((segment) => (
                    <div key={segment.label} className="mix-item">
                      <div className="mix-label">
                        <span className="mix-dot" style={{ background: segment.color }} />
                        {segment.label}
                      </div>
                      <div>{fmtNum(segment.count, 0)}</div>
                      <div className="muted">{(segment.ratio * 100).toFixed(1)}%</div>
                    </div>
                  ))}
                </div>
              </div>
            </div>
            <div className="cards detail-mini-cards" style={{ marginTop: 0, marginBottom: 0 }}>
              <div className="card">
                <div className="label">Events (24h)</div>
                <div className="value">{fmtNum(activitySummary.event_count_24h, 0)}</div>
              </div>
              <div className="card">
                <div className="label">Swaps (24h)</div>
                <div className="value">{fmtNum(activitySummary.swap_count_24h, 0)}</div>
              </div>
              <div className="card">
                <div className="label">Volume (24h)</div>
                <div className="value">{fmtUsd(volumeUsd24h)}</div>
              </div>
              <div className="card">
                <div className="label">Fees (24h)</div>
                <div className="value">{fmtUsd(feeUsd24h)}</div>
              </div>
              <div className="card">
                <div className="label">Net Liq Delta (24h)</div>
                <div className="value">{fmtUsd(netFlowUsd)}</div>
              </div>
              <div className="card">
                <div className="label">Claim Quote (24h)</div>
                <div className="value">{fmtUsd(claimUsd)}</div>
              </div>
            </div>
          </div>

          <div className="cards" style={{ marginTop: 16 }}>
            <div className="card">
              <div className="label">Avg Update Interval (24h)</div>
              <div className="value">{fmtCadence(activitySummary.avg_update_interval_secs_24h)}</div>
            </div>
            <div className="card">
              <div className="label">Latest Update (24h)</div>
              <div className="value">{fmtUnixTs(activitySummary.latest_update_at_24h)}</div>
            </div>
            <div className="card">
              <div className="label">Deposits / Withdraws</div>
              <div className="value">
                {fmtNum(activitySummary.deposit_count_24h, 0)} / {fmtNum(activitySummary.withdraw_count_24h, 0)}
              </div>
            </div>
            <div className="card">
              <div className="label">Claims / Updates</div>
              <div className="value">
                {fmtNum(activitySummary.claim_count_24h, 0)} / {fmtNum(activitySummary.update_count_24h, 0)}
              </div>
            </div>
          </div>
        </>
      ) : null}

      <div className="panel" style={{ marginTop: 16 }}>
        <div className="panel-head">{visibleTabTitle}</div>
        <div className="panel-tabs">
          {eventTabs.map((tab) => (
            <button
              key={tab}
              type="button"
              className={tab === eventTab ? "primary" : undefined}
              onClick={() => setEventTab(tab)}
            >
              {tab === "all"
                ? "All"
                : tab === "swaps"
                  ? "Swaps"
                  : tab === "liquidity"
                    ? "Liquidity"
                  : tab === "claims"
                    ? "Claims"
                    : "Updates"}{" "}
              {fmtNum(eventTabCounts[tab], 0)}
            </button>
          ))}
        </div>
        <div className="event-digest-row">
          <div className="event-digest-card">
            <div className="event-digest-label">Recent swap route</div>
            <div className="event-digest-value">
              {swapEvents[0]
                ? `${shortAsset(swapEvents[0].body?.derived?.token_in)} → ${shortAsset(
                    swapEvents[0].body?.derived?.token_out,
                  )}`
                : "No recent swaps"}
            </div>
            <div className="event-digest-hint">
              {swapEvents[0] ? fmtUnixTs(swapEvents[0].created_at) : "Waiting for more flow"}
            </div>
          </div>
          <div className="event-digest-card">
            <div className="event-digest-label">Latest liquidity action</div>
            <div className="event-digest-value">
              {liquidityEvents[0] ? liquidityEvents[0].kind.replaceAll("_", " ") : "No recent LP moves"}
            </div>
            <div className="event-digest-hint">
              {liquidityEvents[0]
                ? fmtUnixTs(liquidityEvents[0].created_at)
                : "No deposit or withdraw events indexed"}
            </div>
          </div>
          <div className="event-digest-card">
            <div className="event-digest-label">Latest reserve sync</div>
            <div className="event-digest-value">
              {reserveEvents[0] ? reserveEvents[0].kind.replaceAll("_", " ") : "No recent sync"}
            </div>
            <div className="event-digest-hint">
              {reserveEvents[0]
                ? fmtUnixTs(reserveEvents[0].created_at)
                : "Cadence will improve as indexer runs"}
            </div>
          </div>
        </div>
        {events.length === 0 ? (
          <div className="empty" style={{ width: "100%" }}>
            No indexed events for this pool yet
          </div>
        ) : (
          renderEventTable(eventTab, visibleEvents, xlmUsd)
        )}
        {events.length > 0 && visibleEvents.length === 0 ? (
          <div className="empty" style={{ width: "100%" }}>
            No {eventTab} events in the recent indexed window
          </div>
        ) : null}
      </div>
    </>
  );
}

function renderEventTable(
  eventTab: "all" | "swaps" | "liquidity" | "claims" | "updates",
  events: PoolEventRow[],
  xlmUsd?: number | null,
) {
  if (eventTab === "swaps") {
    return (
      <div className="table-scroll">
      <table>
        <thead>
          <tr>
            <th>Time</th>
            <th>Route</th>
            <th>Amount In</th>
            <th>Amount Out</th>
            <th>Volume</th>
            <th>Fee</th>
          </tr>
        </thead>
        <tbody>
          {events.map((event) => (
            <tr key={event.event_id}>
              <td>{fmtUnixTs(event.created_at)}</td>
              <td className="muted">
                {shortAsset(event.body?.derived?.token_in)} → {shortAsset(event.body?.derived?.token_out)}
              </td>
              <td className="muted">{valueText(event.body?.derived?.amount_in) ?? "—"}</td>
              <td className="muted">{valueText(event.body?.derived?.amount_out) ?? "—"}</td>
              <td className="muted">
                {fmtMaybeUsd(
                  event.body?.derived?.volume_quote_usd,
                  event.body?.derived?.volume_quote_xlm,
                  xlmUsd,
                )}
              </td>
              <td className="muted">
                {fmtMaybeUsd(
                  event.body?.derived?.fee_quote_usd,
                  event.body?.derived?.fee_quote_xlm,
                  xlmUsd,
                )}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      </div>
    );
  }
  if (eventTab === "liquidity") {
    return (
      <div className="table-scroll">
      <table>
        <thead>
          <tr>
            <th>Time</th>
            <th>Kind</th>
            <th>Shares</th>
            <th>Quote</th>
            <th>Tx</th>
          </tr>
        </thead>
        <tbody>
          {events.map((event) => (
            <tr key={event.event_id}>
              <td>{fmtUnixTs(event.created_at)}</td>
              <td>
                <span className="badge">{event.kind}</span>
              </td>
              <td className="muted">{valueText(event.body?.derived?.share_amount) ?? "—"}</td>
              <td className="muted">
                {fmtMaybeUsd(
                  event.body?.derived?.total_quote_usd,
                  event.body?.derived?.total_quote_xlm,
                  xlmUsd,
                )}
              </td>
              <td className="muted">{event.tx_hash ? shortAddr(event.tx_hash) : "—"}</td>
            </tr>
          ))}
        </tbody>
      </table>
      </div>
    );
  }
  if (eventTab === "claims") {
    return (
      <div className="table-scroll">
      <table>
        <thead>
          <tr>
            <th>Time</th>
            <th>Kind</th>
            <th>Claim Quote</th>
            <th>Ledger</th>
            <th>Tx</th>
          </tr>
        </thead>
        <tbody>
          {events.map((event) => (
            <tr key={event.event_id}>
              <td>{fmtUnixTs(event.created_at)}</td>
              <td>
                <span className="badge">{event.kind}</span>
              </td>
              <td className="muted">
                {fmtMaybeUsd(
                  event.body?.derived?.fee_quote_usd,
                  event.body?.derived?.fee_quote_xlm,
                  xlmUsd,
                )}
              </td>
              <td className="muted">{fmtNum(event.ledger, 0)}</td>
              <td className="muted">{event.tx_hash ? shortAddr(event.tx_hash) : "—"}</td>
            </tr>
          ))}
        </tbody>
      </table>
      </div>
    );
  }
  if (eventTab === "updates") {
    return (
      <div className="table-scroll">
      <table>
        <thead>
          <tr>
            <th>Time</th>
            <th>Kind</th>
            <th>Reserve Quote</th>
            <th>Ledger</th>
            <th>Tx</th>
          </tr>
        </thead>
        <tbody>
          {events.map((event) => (
            <tr key={event.event_id}>
              <td>{fmtUnixTs(event.created_at)}</td>
              <td>
                <span className="badge">{event.kind}</span>
              </td>
              <td className="muted">
                {fmtMaybeUsd(
                  event.body?.derived?.reserves_quote_usd,
                  event.body?.derived?.reserves_quote_xlm,
                  xlmUsd,
                )}
              </td>
              <td className="muted">{fmtNum(event.ledger, 0)}</td>
              <td className="muted">{event.tx_hash ? shortAddr(event.tx_hash) : "—"}</td>
            </tr>
          ))}
        </tbody>
      </table>
      </div>
    );
  }
  return (
    <div className="table-scroll">
    <table>
      <thead>
        <tr>
          <th>Time</th>
          <th>Kind</th>
          <th>Ledger</th>
          <th>Summary</th>
          <th>Tx</th>
        </tr>
      </thead>
      <tbody>
        {events.map((event) => (
          <tr key={event.event_id}>
            <td>{fmtUnixTs(event.created_at)}</td>
            <td>
              <span className="badge">{event.kind}</span>
            </td>
            <td>{fmtNum(event.ledger, 0)}</td>
            <td className="muted">{summarizeEvent(event, xlmUsd)}</td>
            <td className="muted">{event.tx_hash ? shortAddr(event.tx_hash) : "—"}</td>
          </tr>
        ))}
      </tbody>
    </table>
    </div>
  );
}

function summarizeEvent(event: PoolEventRow, xlmUsd?: number | null) {
  const derived = event.body?.derived ?? {};
  if (event.kind === "trade") {
    const vol = resolveQuoteUsd(derived.volume_quote_usd, derived.volume_quote_xlm, xlmUsd);
    return [
      valueText(derived.token_in),
      valueText(derived.amount_in),
      "->",
      valueText(derived.token_out),
      valueText(derived.amount_out),
      vol != null ? `· vol ${fmtUsd(vol)}` : null,
    ]
      .filter(Boolean)
      .join(" ");
  }
  if (event.kind === "deposit_liquidity" || event.kind === "withdraw_liquidity") {
    const totalUsd = resolveQuoteUsd(derived.total_quote_usd, derived.total_quote_xlm, xlmUsd);
    const total = totalUsd != null ? `≈ ${fmtUsd(totalUsd)}` : null;
    const shareAmount = valueText(derived.share_amount);
    return [event.kind === "deposit_liquidity" ? "shares +" : "shares -", shareAmount, total]
      .filter(Boolean)
      .join(" ");
  }
  if (event.kind === "claim_fees" || event.kind === "claim_protocol_fee") {
    const totalUsd = resolveQuoteUsd(derived.fee_quote_usd, derived.fee_quote_xlm, xlmUsd);
    const total = totalUsd != null ? `≈ ${fmtUsd(totalUsd)}` : null;
    return [event.kind.replaceAll("_", " "), total].filter(Boolean).join(" ");
  }
  if (event.kind === "update_reserves" || event.kind === "reserves_sync") {
    const totalUsd = resolveQuoteUsd(
      derived.reserves_quote_usd,
      derived.reserves_quote_xlm,
      xlmUsd,
    );
    const total = totalUsd != null ? `reserves ≈ ${fmtUsd(totalUsd)}` : null;
    return total ?? event.kind.replaceAll("_", " ");
  }
  return event.kind.replaceAll("_", " ");
}

function valueText(value: unknown) {
  if (value == null) return null;
  if (typeof value === "string") return value;
  if (typeof value === "number") return String(value);
  return null;
}

function numValue(value: unknown) {
  return typeof value === "number" ? value : Number(value ?? 0);
}

/** Prefer API *_quote_usd; else xlm × detail.quote.xlm_usd; never label as XLM. */
function resolveQuoteUsd(
  usd: unknown,
  xlm: unknown,
  xlmUsd?: number | null,
): number | null {
  if (usd != null && usd !== "") {
    const n = numValue(usd);
    if (Number.isFinite(n)) return n;
  }
  if (xlm != null && xlm !== "" && xlmUsd != null && xlmUsd > 0) {
    const n = numValue(xlm);
    if (Number.isFinite(n)) return n * xlmUsd;
  }
  return null;
}

function fmtMaybeUsd(usd: unknown, xlm: unknown, xlmUsd?: number | null) {
  const value = resolveQuoteUsd(usd, xlm, xlmUsd);
  if (value == null) return "—";
  return fmtUsd(value);
}

function shortAsset(value: unknown) {
  const text = valueText(value);
  return text ? shortAddr(text) : "—";
}

function fmtCadence(value: number | null | undefined) {
  if (value == null || Number.isNaN(value) || value <= 0) return "—";
  const mins = Math.round(value / 60);
  if (mins < 60) return `${mins}m`;
  const hours = (value / 3600).toFixed(1);
  return `${hours}h`;
}

function eventMixSegments(summary: NonNullable<PoolDetailResponse["activity_summary"]>) {
  const items = [
    { label: "Swaps", count: summary.swap_count_24h, color: "#3dcf9a" },
    { label: "Deposits", count: summary.deposit_count_24h, color: "#4ea8de" },
    { label: "Withdraws", count: summary.withdraw_count_24h, color: "#f4a261" },
    { label: "Claims", count: summary.claim_count_24h, color: "#e9c46a" },
    { label: "Updates", count: summary.update_count_24h, color: "#7f8fa6" },
  ];
  const total = items.reduce((sum, item) => sum + item.count, 0);
  if (total <= 0) {
    return [{ label: "No activity", count: 0, ratio: 1, color: "#1e2b27" }];
  }
  return items
    .filter((item) => item.count > 0)
    .map((item) => ({ ...item, ratio: item.count / total }));
}

export default function PoolDetailPage() {
  return (
    <Suspense fallback={<div className="empty">Loading…</div>}>
      <PoolDetailInner />
    </Suspense>
  );
}
