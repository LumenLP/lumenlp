"use client";

import { Suspense, useEffect, useMemo, useState } from "react";
import Link from "next/link";
import { useSearchParams } from "next/navigation";
import {
  fetchLpProfile,
  fmtNum,
  fmtUsd,
  shortAddr,
  type ActivityWindow,
  type LpProfile,
  type Position,
} from "@/lib/api";

function tokenLabel(position: Position) {
  if (position.token_labels?.length) {
    return displayTokenLabels(position.token_labels).join(" / ");
  }
  if (position.tokens.length >= 2) {
    return `${shortAddr(position.tokens[0])} / ${shortAddr(position.tokens[1])}`;
  }
  return position.tokens.map(shortAddr).join(" · ") || "—";
}

function displayTokenLabels(labels: string[]) {
  return labels.map((label) => label.toLowerCase() === "native" ? "XLM" : label);
}

function venueLabel(venue?: string, poolType?: string) {
  switch (venue) {
    case "aquarius": return "Aquarius";
    case "sushi":
    case "sushi_v3": return "Sushi V3";
    case "soroswap":
    case "soroswap_amm": return "Soroswap";
    case "phoenix": return "Phoenix";
    case "comet": return "Comet";
    default: return venue === "unknown" ? "Unknown DEX" : "—";
  }
}

function poolTypeLabel(poolType?: string | null) {
  switch (poolType) {
    case "constant_product": return "AMM";
    case "concentrated": return "CLMM";
    case "stable": return "Stable";
    case "weighted": return "Weighted";
    default: return !poolType || poolType === "unknown" ? "Unknown" : poolType;
  }
}

function positionStatusLabel(status?: string) {
  return status === "fee_unavailable" ? "Fee unavailable" : null;
}

function quoteLabel(usd: number | null | undefined, xlm: number | null | undefined, digits = 1) {
  if (fmtUsd(usd) !== "—") return fmtUsd(usd);
  if (xlm == null || !Number.isFinite(xlm)) return "—";
  return `${fmtNum(xlm, digits)} XLM`;
}

function formatTs(ts?: number | null) {
  return ts == null ? "—" : new Date(ts * 1000).toLocaleDateString();
}

function formatRelative(ts?: number | null) {
  if (ts == null) return "—";
  const delta = Date.now() / 1000 - ts;
  if (delta < 60) return "just now";
  if (delta < 3600) return `${Math.floor(delta / 60)}m ago`;
  if (delta < 86400) return `${Math.floor(delta / 3600)}h ago`;
  if (delta < 86400 * 30) return `${Math.floor(delta / 86400)}d ago`;
  return formatTs(ts);
}

function feeCapitalPct(claimXlm: number, depositXlm: number) {
  if (!(depositXlm > 0) || !Number.isFinite(claimXlm)) return null;
  return (claimXlm / depositXlm) * 100;
}

function windowOrFallback(profile: LpProfile, key: "7d" | "30d"): ActivityWindow {
  return profile.windows?.[key] ?? profile.activity_30d;
}

function poolMix(events: LpProfile["recent_events"]) {
  const map = new Map<string, { count: number; quote: number; tokenLabels: string[]; venue?: string | null; poolType?: string | null; feeBps?: number | null }>();
  for (const event of events) {
    const current = map.get(event.pool_address) ?? {
      count: 0,
      quote: 0,
      tokenLabels: event.token_labels ?? [],
      venue: event.venue,
      poolType: event.pool_type,
      feeBps: event.fee_bps,
    };
    current.count += 1;
    if (event.quote_xlm != null && Number.isFinite(event.quote_xlm)) current.quote += event.quote_xlm;
    if (current.tokenLabels.length === 0 && event.token_labels?.length) current.tokenLabels = event.token_labels;
    if (!current.venue && event.venue) current.venue = event.venue;
    if (!current.poolType && event.pool_type) current.poolType = event.pool_type;
    if (current.feeBps == null && event.fee_bps != null) current.feeBps = event.fee_bps;
    map.set(event.pool_address, current);
  }
  return [...map.entries()]
    .map(([pool, value]) => ({ pool, ...value }))
    .sort((a, b) => b.count - a.count || b.quote - a.quote)
    .slice(0, 6);
}

function LeaderViewInner() {
  const searchParams = useSearchParams();
  const address = searchParams.get("address")?.trim() ?? "";
  const [profile, setProfile] = useState<LpProfile | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!address.startsWith("G") || address.length < 56) {
      setError("Invalid leader address");
      return;
    }
    let cancelled = false;
    const refreshProfile = () => {
      void fetchLpProfile(address)
        .then((data) => {
          if (!cancelled) {
            setProfile(data);
            setError(null);
          }
        })
        .catch((reason) => {
          if (!cancelled) setError(reason instanceof Error ? reason.message : "Failed to load LP profile");
        });
    };
    setError(null);
    refreshProfile();
    const timer = window.setInterval(refreshProfile, 60_000);
    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
  }, [address]);

  const mix = useMemo(() => (profile ? poolMix(profile.recent_events) : []), [profile]);
  if (error && !profile) {
    return (
      <div className="panel">
        <Link href="/leaders" className="muted">← Back to leaders</Link>
        <div className="error">{error}</div>
      </div>
    );
  }
  if (!profile) {
    return (
      <div className="panel">
        <Link href="/leaders" className="muted">← Back to leaders</Link>
        <div className="empty">Loading leader profile…</div>
      </div>
    );
  }

  const w7 = windowOrFallback(profile, "7d");
  const w30 = windowOrFallback(profile, "30d");
  const feeCapital = feeCapitalPct(w30.accrued_fee_quote_xlm ?? w30.claim_quote_xlm, w30.deposit_quote_xlm);
  const proxies = profile.proxies;

  return (
    <div className="leaders-layout">
      <div className="leaders-profile-nav">
        <Link href="/leaders" className="muted">← Back to leaders</Link>
        <span className="muted">Leader profile</span>
      </div>
      <div className="panel leaders-profile-card">
        <div className="leaders-profile-top">
          <div>
            <div className="leaders-profile-addr" title={profile.address}>{shortAddr(profile.address)}</div>
            <div className="muted">
              First {formatTs(profile.first_activity_at)} · Last {formatRelative(profile.last_activity_at)}
              {proxies?.months_active_indexed != null ? ` · ~${proxies.months_active_indexed.toFixed(1)} mo indexed` : ""}
            </div>
          </div>
          <Link className="primary" href={`/copy?leader=${encodeURIComponent(profile.address)}`}>Copy this leader</Link>
        </div>

        <div className="leaders-hero-row">
          <div className="leaders-hero-metric">
            <span className="filter-label">{w30.accrued_fee_quote_xlm != null ? "Accrued fees" : "Claimed fees"} · 30d</span>
            <strong className="leaders-pos">{quoteLabel(w30.accrued_fee_quote_usd ?? w30.claim_quote_usd, w30.accrued_fee_quote_xlm ?? w30.claim_quote_xlm, 2)}</strong>
            <span className="muted">{w30.claim_count} claims · {w30.distinct_pools} pools{w30.accrued_fee_quote_xlm == null ? " · baseline pending" : ""}</span>
          </div>
          <div className="leaders-hero-metric">
            <span className="filter-label">Avg monthly claimed</span>
            <strong className="leaders-pos">
              {proxies?.avg_monthly_claimed_xlm != null
                ? quoteLabel(proxies.avg_monthly_claimed_usd, proxies.avg_monthly_claimed_xlm, 2)
                : "—"}
            </strong>
            <span className="muted">lifetime claims ÷ active months (proxy)</span>
          </div>
          <div className="leaders-hero-metric">
            <span className="filter-label">Accrued fee / capital · 30d</span>
            <strong className="leaders-pos">{feeCapital != null ? `${feeCapital.toFixed(2)}%` : "—"}</strong>
            <span className="muted">accrued or claimed ÷ deposits (not ROI)</span>
          </div>
        </div>

        <div className="leaders-stats leaders-stats-dense">
          <div className="leaders-stat"><span className="filter-label">Current unclaimed fees</span><strong className="leaders-pos">{quoteLabel(profile.portfolio.fees_unclaimed_usd, profile.portfolio.fees_unclaimed_xlm, 2)}</strong></div>
          <div className="leaders-stat"><span className="filter-label">Open positions</span><strong>{profile.portfolio.position_count}</strong></div>
          <div className="leaders-stat"><span className="filter-label">Net worth</span><strong>{quoteLabel(profile.portfolio.net_worth_usd, profile.portfolio.net_worth_xlm, 2)}</strong></div>
          <div className="leaders-stat"><span className="filter-label">Claim intensity · 30d</span><strong>{proxies?.claim_intensity_30d != null ? `${proxies.claim_intensity_30d.toFixed(2)}×` : "—"}</strong></div>
          <div className="leaders-stat"><span className="filter-label">Avg deposit · 30d</span><strong>{w30.avg_deposit_quote_xlm != null ? quoteLabel(w30.avg_deposit_quote_usd, w30.avg_deposit_quote_xlm, 2) : "—"}</strong></div>
          <div className="leaders-stat"><span className="filter-label">Lifetime claimed</span><strong className="leaders-pos">{profile.lifetime ? quoteLabel(profile.lifetime.claim_quote_usd, profile.lifetime.claim_quote_xlm, 2) : "—"}</strong></div>
        </div>

        <div className="leaders-window-table-wrap">
          <table className="leaders-window-table">
            <thead><tr><th>Window</th><th>Events</th><th>Accrued / claimed fees</th><th>Fee / capital</th><th>Net liquidity</th><th>Pools</th></tr></thead>
            <tbody>{([['7D', w7], ['30D', w30]] as const).map(([label, window]) => {
              const fee = window.accrued_fee_quote_xlm ?? window.claim_quote_xlm;
              const feeUsd = window.accrued_fee_quote_usd ?? window.claim_quote_usd;
              const pct = feeCapitalPct(fee, window.deposit_quote_xlm);
              return <tr key={label}><td>{label}</td><td>{window.event_count} <span className="muted">({window.deposit_count}d / {window.withdraw_count}w / {window.claim_count}c)</span></td><td className="leaders-pos">{quoteLabel(feeUsd, fee)}{window.accrued_fee_quote_xlm == null ? <span className="muted"> · claimed</span> : null}</td><td className="leaders-pos">{pct != null ? `${pct.toFixed(2)}%` : "—"}</td><td>{fmtNum(window.net_liquidity_quote_xlm, 1)} XLM</td><td>{window.distinct_pools}</td></tr>;
            })}</tbody>
          </table>
        </div>
        {profile.note ? <p className="muted">{profile.note}</p> : null}
        <p className="sign-disabled-note">{profile.honesty}</p>
      </div>

      {mix.length > 0 ? <div className="panel"><div className="panel-head">Pool mix · recent events</div><div className="leaders-pool-mix">{mix.map((row) => <div key={row.pool} className="leaders-pool-mix-row"><div><Link href={`/pools/view?address=${encodeURIComponent(row.pool)}`}>{row.tokenLabels.length ? displayTokenLabels(row.tokenLabels).join(" / ") : shortAddr(row.pool)}</Link><div className="muted">{venueLabel(row.venue ?? undefined, row.poolType ?? undefined)} · {poolTypeLabel(row.poolType)}{row.feeBps != null ? ` · ${row.feeBps} bps` : ""} · {shortAddr(row.pool)}</div></div><span className="muted">{row.count} events</span><span>{row.quote > 0 ? `${fmtNum(row.quote, 1)} XLM` : "—"}</span></div>)}</div></div> : null}

      <div className="panel"><div className="panel-head">Open positions</div>{profile.positions.length === 0 ? <div className="empty">No open positions in scanned pools.</div> : <div className="leaders-positions">{profile.positions.map((position) => <div key={`${position.pool_address}-${position.status}`} className="leaders-position"><div className="leaders-position-head"><div><Link href={`/pools/view?address=${encodeURIComponent(position.pool_address)}`}>{tokenLabel(position)}</Link><div className="muted">{shortAddr(position.pool_address)} · {poolTypeLabel(position.pool_type)} · {position.fee_bps} bps</div></div><div className="leaders-position-badges"><span className="badge">{venueLabel(position.venue, position.pool_type)}</span>{positionStatusLabel(position.status) ? <span className="badge">{positionStatusLabel(position.status)}</span> : null}</div></div><div className="muted">Value {fmtNum(position.value_quote, 2)} XLM{position.fees_unclaimed_quote != null ? ` · fees ${fmtNum(position.fees_unclaimed_quote, 3)}` : " · unclaimed fee unavailable"}{position.il_est != null ? ` · IL ~(${(position.il_est * 100).toFixed(2)}%)` : ""}</div></div>)}</div>}</div>

      <div className="panel"><div className="panel-head">Recent LP events</div>{profile.recent_events.length === 0 ? <div className="empty">No deposit/withdraw/claim events with actor in the last 30d.</div> : <div className="leaders-events">{profile.recent_events.map((event) => <div key={event.event_id} className="leaders-event"><span className="badge">{event.kind.replace("_liquidity", "").replace("_", " ")}</span><div><Link href={`/pools/view?address=${encodeURIComponent(event.pool_address)}`}>{event.token_labels?.length ? displayTokenLabels(event.token_labels).join(" / ") : shortAddr(event.pool_address)}</Link><div className="muted">{venueLabel(event.venue ?? undefined, event.pool_type ?? undefined)} · {poolTypeLabel(event.pool_type)}{event.fee_bps != null ? ` · ${event.fee_bps} bps` : ""} · {shortAddr(event.pool_address)}</div></div><span className="muted">{event.quote_xlm != null ? `${fmtNum(event.quote_xlm, 2)} XLM · ` : ""}{new Date(event.created_at * 1000).toLocaleString()}</span></div>)}</div>}</div>
    </div>
  );
}

export default function LeaderViewPage() {
  return <Suspense fallback={<div className="panel"><div className="empty">Loading leader profile…</div></div>}><LeaderViewInner /></Suspense>;
}
