"use client";

import { Suspense, useEffect, useMemo, useState } from "react";
import Link from "next/link";
import { usePathname, useRouter, useSearchParams } from "next/navigation";
import { TokenPairMark, tokenVisualsFromMeta } from "@/components/TokenIdentity";
import {
  fetchPools,
  fmtNum,
  fmtPct,
  fmtTs,
  fmtUnixTs,
  fmtUsd,
  pickUsd,
  shortAddr,
  type PoolRow,
  type QuoteInfo,
} from "@/lib/api";
type PoolWindow = "5m" | "1h" | "6h" | "24h";
type SortKey =
  | "score"
  | "fee_tvl"
  | "fee"
  | "tx_count"
  | "event_count"
  | "activity_24h"
  | "net_liq_24h"
  | "claim_quote_24h"
  | "cadence_24h"
  | "liquidity";

type ViewMode = "table" | "card";

function poolTokenPairLabel(pool: PoolRow) {
  const labels =
    pool.token_meta?.map((token) => normalizedTokenLabel(token.symbol?.trim())).filter(Boolean) ??
    [];
  if (labels.length >= 2) {
    return `${labels[0]} / ${labels[1]}`;
  }
  const fallback = (pool.tokens ?? []).slice(0, 2).map(shortAddr);
  return fallback.length ? fallback.join(" / ") : shortAddr(pool.address);
}

function poolTokenSubtitle(pool: PoolRow) {
  const labels =
    pool.token_meta
      ?.map(
        (token) =>
          token.name?.trim() ||
          normalizedTokenLabel(token.symbol?.trim()) ||
          shortAddr(token.address),
      )
      .filter(Boolean) ?? [];
  if (labels.length >= 2) {
    return labels.join(" / ");
  }
  return (pool.tokens ?? []).slice(0, 2).map(shortAddr).join(" / ");
}

function poolTokenVisuals(pool: PoolRow) {
  return tokenVisualsFromMeta(pool.token_meta, pool.tokens);
}

function normalizedTokenLabel(label: string | null | undefined) {
  if (!label) return "";
  return label.toLowerCase() === "native" ? "XLM" : label;
}

function poolTypeLabel(type: string | null | undefined) {
  if (!type) return "Unknown";
  if (type === "constant_product") return "AMM";
  if (type === "concentrated") return "CLMM";
  if (type === "stable") return "Stable";
  return type;
}

function fmtActivitySummary(pool: PoolRow, xlmUsd?: number | null) {
  const s = pool.activity_summary;
  if (!s) return "—";
  const netLiq = pickUsd(s.net_liquidity_delta_usd_24h, s.net_liquidity_delta_quote_24h, xlmUsd);
  const netLiqLabel =
    netLiq.value != null ? fmtUsd(netLiq.value, 1) : fmtNum(s.net_liquidity_delta_quote_24h, 1);
  return `Swaps ${fmtNum(s.swap_count_24h, 0)} · Events ${fmtNum(
    s.event_count_24h,
    0,
  )} · Net liq ${netLiqLabel} · Claims ${fmtNum(
    s.claim_count_24h,
    0,
  )}`;
}

function cadenceSortValue(value: number | null | undefined) {
  if (value == null || Number.isNaN(value) || value <= 0) return 0;
  return 1 / value;
}

function cadenceLabel(value: number | null | undefined) {
  if (value == null || Number.isNaN(value) || value <= 0) return "—";
  const mins = value / 60;
  if (mins <= 15) return "hot";
  if (mins <= 60) return "warm";
  return "cold";
}

function poolScore(pool: PoolRow, windowKey: "5m" | "1h" | "6h" | "24h") {
  if (pool.score != null && Number.isFinite(pool.score)) {
    return pool.score;
  }
  const window = pool.window_metrics?.[windowKey];
  const feeTvl = window?.fee_tvl ?? 0;
  const volume = window?.volume ?? 0;
  const liquidity = Math.max(pool.tvl ?? 0, 1);
  const volumeEfficiency = volume / liquidity;
  const netLiq = pool.activity_summary?.net_liquidity_delta_quote_24h ?? 0;
  const netLiqRatio = netLiq / liquidity;
  const cadence = cadenceSortValue(pool.activity_summary?.avg_update_interval_secs_24h);
  return feeTvl * 10_000 + volumeEfficiency * 200 + netLiqRatio * 100 + cadence * 3_600;
}

function PoolsPageInner() {
  const windows = ["5m", "1h", "6h", "24h"] as const;
  const sortOptions = ["score", "fee_tvl", "fee", "tx_count", "event_count", "activity_24h", "net_liq_24h", "claim_quote_24h", "cadence_24h", "liquidity"] as const;
  const viewModes = ["table", "card"] as const;
  const activityFilters = ["all", "active", "swaps", "fresh"] as const;
  const tvlBuckets = ["all", "micro", "small", "mid", "large"] as const;
  const feeTvlBuckets = ["all", "high", "mid", "low", "zero"] as const;
  const router = useRouter();
  const pathname = usePathname();
  const searchParams = useSearchParams();
  const [pools, setPools] = useState<PoolRow[]>([]);
  const [quote, setQuote] = useState<QuoteInfo | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const [indexedPoolCount, setIndexedPoolCount] = useState<number | null>(null);
  const [lastSnapshotAt, setLastSnapshotAt] = useState<string | null>(null);
  const [indexerStatus, setIndexerStatus] = useState<{
    cursor_ledger?: number | null;
    event_count: number;
    swap_count: number;
    rollup_count: number;
    distinct_event_pools: number;
    distinct_rollup_pools: number;
    last_event_at?: number | null;
    last_rollup_at?: number | null;
  } | null>(null);
  const [note, setNote] = useState<string | null>(null);
  const [windowKey, setWindowKey] = useState<(typeof windows)[number]>("24h");
  const [sortKey, setSortKey] = useState<(typeof sortOptions)[number]>("score");
  const [viewMode, setViewMode] = useState<ViewMode>("card");
  const [poolTypeFilter, setPoolTypeFilter] = useState<string>("all");
  const [activityFilter, setActivityFilter] = useState<(typeof activityFilters)[number]>("all");
  const [feeTierFilter, setFeeTierFilter] = useState<string>("all");
  const [tvlBucketFilter, setTvlBucketFilter] = useState<(typeof tvlBuckets)[number]>("all");
  const [feeTvlBucketFilter, setFeeTvlBucketFilter] = useState<(typeof feeTvlBuckets)[number]>("all");
  const [q, setQ] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [urlReady, setUrlReady] = useState(false);

  const poolTypes = useMemo(
    () => ["all", ...Array.from(new Set(pools.map((pool) => pool.pool_type).filter(Boolean))).sort()],
    [pools],
  );
  const feeTiers = useMemo(
    () => ["all", ...Array.from(new Set(pools.map((pool) => String(pool.fee_bps)).filter(Boolean))).sort((a, b) => Number(a) - Number(b))],
    [pools],
  );

  useEffect(() => {
    const nextWindow = searchParams.get("window");
    if (nextWindow && windows.includes(nextWindow as (typeof windows)[number])) {
      setWindowKey(nextWindow as (typeof windows)[number]);
    }
    const nextSort = searchParams.get("sort");
    if (nextSort && sortOptions.includes(nextSort as (typeof sortOptions)[number])) {
      setSortKey(nextSort as (typeof sortOptions)[number]);
    }
    const nextView = searchParams.get("view");
    if (nextView && viewModes.includes(nextView as ViewMode)) {
      setViewMode(nextView as ViewMode);
    }
    const nextType = searchParams.get("type");
    if (nextType) setPoolTypeFilter(nextType);
    const nextActivity = searchParams.get("activity");
    if (nextActivity && activityFilters.includes(nextActivity as (typeof activityFilters)[number])) {
      setActivityFilter(nextActivity as (typeof activityFilters)[number]);
    }
    const nextFeeTier = searchParams.get("feeTier");
    if (nextFeeTier) setFeeTierFilter(nextFeeTier);
    const nextTvl = searchParams.get("tvl");
    if (nextTvl && tvlBuckets.includes(nextTvl as (typeof tvlBuckets)[number])) {
      setTvlBucketFilter(nextTvl as (typeof tvlBuckets)[number]);
    }
    const nextFeeTvl = searchParams.get("feeTvl");
    if (nextFeeTvl && feeTvlBuckets.includes(nextFeeTvl as (typeof feeTvlBuckets)[number])) {
      setFeeTvlBucketFilter(nextFeeTvl as (typeof feeTvlBuckets)[number]);
    }
    const nextQuery = searchParams.get("q");
    if (nextQuery != null) setQ(nextQuery);
    setUrlReady(true);
  }, [searchParams]);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    fetchPools()
      .then((r) => {
        if (cancelled) return;
        setPools(r.pools ?? []);
        setQuote(r.quote ?? null);
        setIndexedPoolCount(r.indexed_pool_count ?? r.pools?.length ?? 0);
        setLastSnapshotAt(r.last_snapshot_at ?? null);
        setIndexerStatus(r.indexer_status ?? null);
        setNote(r.note ?? null);
        if (r.pools?.[0]) setSelected(r.pools[0].address);
      })
      .catch((e: Error) => {
        if (!cancelled) setError(e.message);
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  function buildCurrentQuery() {
    const params = new URLSearchParams();
    if (windowKey !== "24h") params.set("window", windowKey);
    if (sortKey !== "score") params.set("sort", sortKey);
    if (viewMode !== "card") params.set("view", viewMode);
    if (poolTypeFilter !== "all") params.set("type", poolTypeFilter);
    if (activityFilter !== "all") params.set("activity", activityFilter);
    if (feeTierFilter !== "all") params.set("feeTier", feeTierFilter);
    if (tvlBucketFilter !== "all") params.set("tvl", tvlBucketFilter);
    if (feeTvlBucketFilter !== "all") params.set("feeTvl", feeTvlBucketFilter);
    if (q.trim()) params.set("q", q.trim());
    return params.toString();
  }

  function resetFilters() {
    setWindowKey("24h");
    setSortKey("score");
    setViewMode("card");
    setPoolTypeFilter("all");
    setActivityFilter("all");
    setFeeTierFilter("all");
    setTvlBucketFilter("all");
    setFeeTvlBucketFilter("all");
    setQ("");
  }

  useEffect(() => {
    if (!urlReady) return;
    const next = buildCurrentQuery();
    router.replace(next ? `${pathname}?${next}` : pathname, { scroll: false });
  }, [
    activityFilter,
    feeTierFilter,
    feeTvlBucketFilter,
    pathname,
    poolTypeFilter,
    q,
    router,
    sortKey,
    tvlBucketFilter,
    urlReady,
    viewMode,
    windowKey,
  ]);

  const filtered = useMemo(() => {
    const needle = q.trim().toLowerCase();
    const base = !needle
      ? pools
      : pools.filter(
      (p) =>
        p.address.toLowerCase().includes(needle) ||
        p.pool_type.toLowerCase().includes(needle) ||
        poolTokenPairLabel(p).toLowerCase().includes(needle) ||
        poolTokenSubtitle(p).toLowerCase().includes(needle) ||
        (p.token_meta ?? []).some(
          (token) =>
            normalizedTokenLabel(token.symbol?.trim()).toLowerCase().includes(needle) ||
            token.name?.trim().toLowerCase().includes(needle),
        ) ||
        (p.tokens ?? []).some((t) => t.toLowerCase().includes(needle)),
    );
    const filteredByType =
      poolTypeFilter === "all"
        ? base
        : base.filter((pool) => pool.pool_type === poolTypeFilter);
    const nowUnix = Date.now() / 1000;
    const filteredByActivity = filteredByType.filter((pool) => {
      if (activityFilter === "all") return true;
      if (activityFilter === "active") {
        return (pool.activity_summary?.event_count_24h ?? 0) > 0;
      }
      if (activityFilter === "swaps") {
        return (pool.activity_summary?.swap_count_24h ?? 0) > 0;
      }
      return pool.activity?.first_event_at != null && nowUnix - pool.activity.first_event_at <= 7 * 24 * 3600;
    });
    const filteredByFeeTier =
      feeTierFilter === "all"
        ? filteredByActivity
        : filteredByActivity.filter((pool) => String(pool.fee_bps) === feeTierFilter);
    const filteredByTvl = filteredByFeeTier.filter((pool) => {
      if (tvlBucketFilter === "all") return true;
      if (tvlBucketFilter === "micro") return (pool.tvl ?? 0) < 100_000;
      if (tvlBucketFilter === "small") return (pool.tvl ?? 0) >= 100_000 && (pool.tvl ?? 0) < 1_000_000;
      if (tvlBucketFilter === "mid") return (pool.tvl ?? 0) >= 1_000_000 && (pool.tvl ?? 0) < 10_000_000;
      return (pool.tvl ?? 0) >= 10_000_000;
    });
    const filteredByFeeTvl = filteredByTvl.filter((pool) => {
      const value = pool.window_metrics?.[windowKey]?.fee_tvl ?? 0;
      if (feeTvlBucketFilter === "all") return true;
      if (feeTvlBucketFilter === "high") return value >= 0.05;
      if (feeTvlBucketFilter === "mid") return value >= 0.01 && value < 0.05;
      if (feeTvlBucketFilter === "low") return value > 0 && value < 0.01;
      return value <= 0;
    });
    return [...filteredByFeeTvl].sort((a, b) => {
      const aWindow = a.window_metrics?.[windowKey];
      const bWindow = b.window_metrics?.[windowKey];
      const aValue =
        sortKey === "score"
          ? poolScore(a, windowKey)
          : sortKey === "liquidity"
          ? a.tvl
          : sortKey === "activity_24h"
            ? (a.activity_summary?.event_count_24h ?? 0)
          : sortKey === "net_liq_24h"
            ? (a.activity_summary?.net_liquidity_delta_quote_24h ?? 0)
          : sortKey === "claim_quote_24h"
            ? (a.activity_summary?.claim_quote_24h ?? 0)
          : sortKey === "cadence_24h"
            ? cadenceSortValue(a.activity_summary?.avg_update_interval_secs_24h)
          : sortKey === "event_count"
            ? (a.activity?.event_count ?? 0)
          : sortKey === "tx_count"
            ? (aWindow?.tx_count ?? 0)
          : sortKey === "fee"
            ? (aWindow?.fee ?? 0)
            : (aWindow?.fee_tvl ?? 0);
      const bValue =
        sortKey === "score"
          ? poolScore(b, windowKey)
          : sortKey === "liquidity"
          ? b.tvl
          : sortKey === "activity_24h"
            ? (b.activity_summary?.event_count_24h ?? 0)
          : sortKey === "net_liq_24h"
            ? (b.activity_summary?.net_liquidity_delta_quote_24h ?? 0)
          : sortKey === "claim_quote_24h"
            ? (b.activity_summary?.claim_quote_24h ?? 0)
          : sortKey === "cadence_24h"
            ? cadenceSortValue(b.activity_summary?.avg_update_interval_secs_24h)
          : sortKey === "event_count"
            ? (b.activity?.event_count ?? 0)
          : sortKey === "tx_count"
            ? (bWindow?.tx_count ?? 0)
          : sortKey === "fee"
            ? (bWindow?.fee ?? 0)
            : (bWindow?.fee_tvl ?? 0);
      return bValue - aValue;
    });
  }, [activityFilter, feeTierFilter, feeTvlBucketFilter, poolTypeFilter, pools, q, sortKey, tvlBucketFilter, windowKey]);

  const maxSamples = Math.max(
    0,
    ...filtered.map((pool) => pool.window_metrics?.[windowKey]?.samples ?? 0),
  );
  const lowSampleWarning =
    windowKey === "5m"
      ? "Current snapshot cadence is hourly, so 5m metrics are sparse until the sampler runs more frequently."
      : maxSamples <= 1
        ? `The ${windowKey} window has thin sampling right now, so fee estimates may be noisy.`
        : null;

  const metricColumnClass = (keys: readonly SortKey[]) =>
    keys.includes(sortKey) ? "metric-col-active" : undefined;
  const activeFilterChips = [
    q.trim() ? { key: "q", label: `Search: ${q.trim()}`, clear: () => setQ("") } : null,
    windowKey !== "24h"
      ? { key: "window", label: `Window: ${windowKey}`, clear: () => setWindowKey("24h") }
      : null,
    sortKey !== "score"
      ? {
          key: "sort",
          label: `Sort: ${sortKey.replaceAll("_", " ")}`,
          clear: () => setSortKey("score"),
        }
      : null,
    poolTypeFilter !== "all"
      ? { key: "type", label: `Type: ${poolTypeLabel(poolTypeFilter)}`, clear: () => setPoolTypeFilter("all") }
      : null,
    activityFilter !== "all"
      ? {
          key: "activity",
          label: `Activity: ${activityFilter}`,
          clear: () => setActivityFilter("all"),
        }
      : null,
    feeTierFilter !== "all"
      ? { key: "feeTier", label: `Fee: ${feeTierFilter} bps`, clear: () => setFeeTierFilter("all") }
      : null,
    tvlBucketFilter !== "all"
      ? { key: "tvl", label: `TVL: ${tvlBucketFilter}`, clear: () => setTvlBucketFilter("all") }
      : null,
    feeTvlBucketFilter !== "all"
      ? {
          key: "feeTvl",
          label: `Fee/TVL: ${feeTvlBucketFilter}`,
          clear: () => setFeeTvlBucketFilter("all"),
        }
      : null,
  ].filter(Boolean) as { key: string; label: string; clear: () => void }[];

  return (
    <>
      <div className="panel pools-workbench-panel">
        <div className="panel-head pools-head">
          <span>Pools</span>
          <span className="pools-head-meta">
            {fmtNum(filtered.length, 0)} / {fmtNum(pools.length, 0)} shown · indexed{" "}
            {fmtNum(indexedPoolCount ?? pools.length, 0)} · latest {fmtTs(lastSnapshotAt)}
          </span>
        </div>
        <div className="toolbar filters-panel">
          {activeFilterChips.length > 0 ? (
            <div className="active-filter-bar">
              <div className="active-filter-list">
                {activeFilterChips.map((chip) => (
                  <button
                    key={chip.key}
                    type="button"
                    className="active-filter-chip"
                    onClick={chip.clear}
                  >
                    {chip.label} ×
                  </button>
                ))}
              </div>
              <button type="button" onClick={resetFilters}>
                Reset all
              </button>
            </div>
          ) : null}
          <div className="filter-grid">
            <label className="filter-field filter-field-search">
              <span className="filter-label">Search</span>
              <input
                className="filter-input"
                placeholder="Pool / token / address"
                value={q}
                onChange={(e) => setQ(e.target.value)}
              />
            </label>
            <div className="filter-field">
              <span className="filter-label">Type</span>
              <div className="segmented filter-control">
                {poolTypes.map((type) => (
                  <button
                    key={type}
                    type="button"
                    className={type === poolTypeFilter ? "primary" : undefined}
                    onClick={() => setPoolTypeFilter(type)}
                  >
                    {type === "all" ? "All" : poolTypeLabel(type)}
                  </button>
                ))}
              </div>
            </div>
            <div className="filter-field">
              <span className="filter-label">Fee tier</span>
              <div className="segmented filter-control">
                {feeTiers.map((tier) => (
                  <button
                    key={tier}
                    type="button"
                    className={tier === feeTierFilter ? "primary" : undefined}
                    onClick={() => setFeeTierFilter(tier)}
                  >
                    {tier === "all" ? "All" : `${tier} bps`}
                  </button>
                ))}
              </div>
            </div>
            <div className="filter-field">
              <span className="filter-label">Activity</span>
              <div className="segmented filter-control">
                {activityFilters.map((filter) => (
                  <button
                    key={filter}
                    type="button"
                    className={filter === activityFilter ? "primary" : undefined}
                    onClick={() => setActivityFilter(filter)}
                  >
                    {filter === "all"
                      ? "All"
                      : filter === "active"
                        ? "Active 24h"
                        : filter === "swaps"
                          ? "Swaps 24h"
                          : "Fresh 7d"}
                  </button>
                ))}
              </div>
            </div>
            <div className="filter-field">
              <span className="filter-label">Liquidity</span>
              <div className="segmented filter-control">
                {tvlBuckets.map((bucket) => (
                  <button
                    key={bucket}
                    type="button"
                    className={bucket === tvlBucketFilter ? "primary" : undefined}
                    onClick={() => setTvlBucketFilter(bucket)}
                  >
                    {bucket === "all"
                      ? "All"
                      : bucket === "micro"
                        ? "< 100k"
                        : bucket === "small"
                          ? "100k-1m"
                          : bucket === "mid"
                            ? "1m-10m"
                            : "10m+"}
                  </button>
                ))}
              </div>
            </div>
            <div className="filter-field">
              <span className="filter-label">Fee/TVL</span>
              <div className="segmented filter-control">
                {feeTvlBuckets.map((bucket) => (
                  <button
                    key={bucket}
                    type="button"
                    className={bucket === feeTvlBucketFilter ? "primary" : undefined}
                    onClick={() => setFeeTvlBucketFilter(bucket)}
                  >
                    {bucket === "all"
                      ? "All"
                      : bucket === "high"
                        ? ">= 5%"
                        : bucket === "mid"
                          ? "1%-5%"
                          : bucket === "low"
                            ? "0-1%"
                            : "0%"}
                  </button>
                ))}
              </div>
            </div>
          </div>
          <div className="filter-footer">
            <div className="filter-field filter-field-inline">
              <span className="filter-label">Window</span>
              <div className="segmented filter-control">
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
            </div>
            <div className="filter-field filter-field-wide">
              <span className="filter-label">Sort</span>
              <div className="segmented filter-control">
                {sortOptions.map((option) => (
                  <button
                    key={option}
                    type="button"
                    className={option === sortKey ? "primary" : undefined}
                    onClick={() => setSortKey(option)}
                  >
                    {option === "fee_tvl"
                      ? "Fee/TVL"
                      : option === "score"
                        ? "Score"
                      : option === "liquidity"
                        ? "Liquidity"
                        : option === "activity_24h"
                          ? "24h Act"
                        : option === "net_liq_24h"
                          ? "24h Liq Δ"
                        : option === "claim_quote_24h"
                          ? "24h Claims"
                        : option === "cadence_24h"
                          ? "24h Cadence"
                        : option === "event_count"
                          ? "Events"
                        : option === "tx_count"
                          ? "Txs"
                          : "Fee"}
                  </button>
                ))}
              </div>
            </div>
            <div className="filter-field filter-field-inline">
              <span className="filter-label">View</span>
              <div className="segmented filter-control">
              {viewModes.map((mode) => (
                <button
                  key={mode}
                  type="button"
                  className={mode === viewMode ? "primary" : undefined}
                  onClick={() => setViewMode(mode)}
                >
                  {mode === "card" ? "Card" : "Table"}
                </button>
              ))}
              </div>
            </div>
          </div>
        </div>
      </div>

      {error ? <div className="panel error">{error}</div> : null}
      {(lowSampleWarning || note) ? (
        <div className="status-strip">
          {lowSampleWarning ? <span>{lowSampleWarning}</span> : null}
          {note ? <span>{note}</span> : null}
        </div>
      ) : null}

      <div className="panel">
        <div className="panel-head">Ranked pools</div>
        {loading ? (
          <div className="empty pools-loading">
            <div className="pools-loading-spinner" aria-hidden />
            <div>Loading pools…</div>
            <div className="muted" style={{ fontSize: "0.8rem", marginTop: 6 }}>
              Fetching ranked metrics from the API
            </div>
          </div>
        ) : viewMode === "table" ? (
          <div className="table-scroll">
            <table className="terminal-table">
              <thead>
                <tr>
                  <th className="sticky-col">Pool</th>
                  <th className={metricColumnClass(["score"])}>Signal</th>
                  <th className={metricColumnClass(["liquidity"])}>TVL</th>
                  <th className={metricColumnClass(["fee_tvl"])}>Fee/TVL</th>
                  <th className={metricColumnClass(["fee"])}>Fee</th>
                  <th className={metricColumnClass(["tx_count"])}>Txs</th>
                  <th className={metricColumnClass(["activity_24h", "net_liq_24h", "claim_quote_24h"])}>24h Activity</th>
                  <th className={metricColumnClass(["cadence_24h"])}>Cadence</th>
                  <th className={metricColumnClass(["event_count"])}>Events</th>
                  <th>Swaps</th>
                  <th>Type</th>
                  <th>Last Event</th>
                  <th>Updated</th>
                  <th>Snapshot</th>
                </tr>
              </thead>
              <tbody>
                {filtered.map((p) => (
                  <tr
                    key={p.address}
                    className="terminal-row"
                    style={{
                      background: selected === p.address ? "rgba(132,204,22,0.06)" : undefined,
                      cursor: "pointer",
                    }}
                    onClick={() => {
                      setSelected(p.address);
                      router.push(`/pools/view?address=${encodeURIComponent(p.address)}`);
                    }}
                  >
                    <td className="sticky-col">
	                      <div className="pool-table-identity">
	                        <div className="pair-heading">
	                          <TokenPairMark tokens={poolTokenVisuals(p)} size="sm" />
	                          <Link
	                            href={`/pools/view?address=${encodeURIComponent(p.address)}`}
	                            style={{ fontWeight: 600 }}
                          >
                            <span className="token-pair">
                              {poolTokenPairLabel(p)
                                .split(" / ")
                                .map((label) => (
                                  <span key={`${p.address}-${label}`} className="token-chip">
                                    {label}
                                  </span>
                                ))}
                            </span>
	                          </Link>
	                        </div>
	                        <div className="muted pool-table-address">
	                          {shortAddr(p.address)}
	                        </div>
	                      </div>
                    </td>
                    <td className={metricColumnClass(["score"])}>
                      <div className="signal-cell">
                        <div className="signal-score">
                          {fmtNum(p.score ?? poolScore(p, windowKey), 2)}
                        </div>
                        <div className="signal-subtext">
                          {fmtPct(p.window_metrics?.[windowKey]?.fee_tvl ?? 0)} fee/TVL
                        </div>
                      </div>
                    </td>
                    <td className={metricColumnClass(["liquidity"])}>
                      {fmtUsd(pickUsd(p.tvl_usd, p.tvl, quote?.xlm_usd ?? p.quote?.xlm_usd).value)}
                    </td>
                    <td className={`metric-positive ${metricColumnClass(["fee_tvl"]) ?? ""}`.trim()}>{fmtPct(p.window_metrics?.[windowKey]?.fee_tvl ?? 0)}</td>
                    <td className={metricColumnClass(["fee"])}>
                      {fmtUsd(
                        pickUsd(
                          p.window_metrics?.[windowKey]?.fee_usd,
                          p.window_metrics?.[windowKey]?.fee,
                          quote?.xlm_usd ?? p.quote?.xlm_usd,
                        ).value,
                      )}
                    </td>
                    <td className={metricColumnClass(["tx_count"])}>{fmtNum(p.window_metrics?.[windowKey]?.tx_count ?? 0, 0)}</td>
                    <td className={`activity-cell muted ${metricColumnClass(["activity_24h", "net_liq_24h", "claim_quote_24h"]) ?? ""}`.trim()}>
                      {fmtActivitySummary(p, quote?.xlm_usd ?? p.quote?.xlm_usd)}
                    </td>
                    <td className={metricColumnClass(["cadence_24h"])}>
                      <span className={`badge cadence-${cadenceLabel(p.activity_summary?.avg_update_interval_secs_24h)}`}>
                        {cadenceLabel(p.activity_summary?.avg_update_interval_secs_24h)}
                      </span>
                      <div className="muted" style={{ fontSize: "0.7rem" }}>
                        {p.activity_summary?.avg_update_interval_secs_24h
                          ? `${Math.round(p.activity_summary.avg_update_interval_secs_24h / 60)}m`
                          : "—"}
                      </div>
                    </td>
                    <td className={metricColumnClass(["event_count"])}>{fmtNum(p.activity?.event_count ?? 0, 0)}</td>
                    <td>{fmtNum(p.activity?.swap_count ?? 0, 0)}</td>
                    <td className="muted">
                      <span className="badge">{poolTypeLabel(p.pool_type)}</span>
                    </td>
	                    <td className="muted">{fmtUnixTs(p.activity?.last_event_at)}</td>
                    <td className="muted">
                      {p.window_metrics?.[windowKey]?.as_of_ts
                        ? fmtTs(new Date((p.window_metrics[windowKey].as_of_ts ?? 0) * 1000).toISOString())
                        : "—"}
                    </td>
                    <td className="muted">{fmtTs(p.last_snapshot_at)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        ) : (
          <div className="pool-card-grid">
            {filtered.map((pool) => (
              <Link
                key={pool.address}
                href={`/pools/view?address=${encodeURIComponent(pool.address)}`}
                className={`pool-metric-card${selected === pool.address ? " is-selected" : ""}`}
                onClick={() => setSelected(pool.address)}
              >
                <div className="pool-metric-card-top">
                  <div className="pool-card-icons">
                    <TokenPairMark tokens={poolTokenVisuals(pool)} size="sm" />
                    <div className="pool-card-mini-icons">
                      <span className="badge">{pool.fee_bps} bps</span>
                      <span className="badge">{poolTypeLabel(pool.pool_type)}</span>
                    </div>
                  </div>
                </div>
                <div className="pool-card-title-row">
                  <span className="pool-card-pair">{poolTokenPairLabel(pool)}</span>
                  <div className="pool-card-highlight">
                    <div className="watchlist-label">Fee / TVL</div>
                    <div className="pool-card-highlight-value">
                      {fmtPct(pool.window_metrics?.[windowKey]?.fee_tvl ?? 0)}
                    </div>
                  </div>
                </div>
                <div className="pool-card-money-row">
                  <div>
                    <div className="watchlist-label">Liquidity</div>
                    <div className="pool-card-money">
                      {fmtUsd(
                        pickUsd(pool.tvl_usd, pool.tvl, quote?.xlm_usd ?? pool.quote?.xlm_usd).value,
                      )}
                    </div>
                  </div>
                  <div>
                    <div className="watchlist-label">Fee ({windowKey})</div>
                    <div className="pool-card-money">
                      {fmtUsd(
                        pickUsd(
                          pool.window_metrics?.[windowKey]?.fee_usd,
                          pool.window_metrics?.[windowKey]?.fee,
                          quote?.xlm_usd ?? pool.quote?.xlm_usd,
                        ).value,
                      )}
                    </div>
                  </div>
                </div>
                <div className="pool-card-meta-grid">
                  <div>
                    <div className="watchlist-label">Score</div>
                    <div className="watchlist-value">{fmtNum(pool.score ?? poolScore(pool, windowKey), 2)}</div>
                  </div>
                  <div>
                    <div className="watchlist-label">Txs</div>
                    <div className="watchlist-value">{fmtNum(pool.window_metrics?.[windowKey]?.tx_count ?? 0, 0)}</div>
                  </div>
                  <div>
                    <div className="watchlist-label">Events</div>
                    <div className="watchlist-value">{fmtNum(pool.activity_summary?.event_count_24h ?? 0, 0)}</div>
                  </div>
                </div>
                <div className="pool-card-section">
                  <div className="pool-card-section-title">Fee / TVL Ratio</div>
                  <div className="pool-card-ratio-grid">
                    {(["24h", "6h", "1h", "5m"] as const).map((window) => (
                      <div key={`${pool.address}-${window}`}>
                        <div className="watchlist-label">{window}</div>
                        <div className="watchlist-value metric-positive">
                          {fmtPct(pool.window_metrics?.[window]?.fee_tvl ?? 0)}
                        </div>
                      </div>
                    ))}
                  </div>
                </div>
                <div className="pool-card-section">
                  <div className="pool-card-section-title">Flow</div>
                  <div className="pool-card-flow muted">{fmtActivitySummary(pool, quote?.xlm_usd ?? pool.quote?.xlm_usd)}</div>
                </div>
              </Link>
            ))}
          </div>
        )}
        {!loading && filtered.length === 0 ? (
          <div className="empty">No pools yet — start api-server after snapshotter.</div>
        ) : null}
      </div>
    </>
  );
}

export default function PoolsPage() {
  return (
    <Suspense fallback={<div className="empty">Loading pools…</div>}>
      <PoolsPageInner />
    </Suspense>
  );
}
