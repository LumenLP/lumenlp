"use client";

import { Suspense, useCallback, useEffect, useState } from "react";
import Link from "next/link";
import { useRouter, useSearchParams } from "next/navigation";
import {
  fetchLpLeaders,
  fetchLpProfile,
  fmtNum,
  fmtUsd,
  shortAddr,
  type ActivityWindow,
  type LeaderBoardRow,
  type LpProfile,
  type Position,
} from "@/lib/api";

function isGAddress(a: string) {
  return a.startsWith("G") && a.length >= 56;
}

function tokenLabel(p: Position) {
  if (p.tokens.length >= 2) {
    return `${shortAddr(p.tokens[0])} / ${shortAddr(p.tokens[1])}`;
  }
  return p.tokens.map(shortAddr).join(" · ") || "—";
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

function quoteLabel(usd: number | null | undefined, xlm: number | null | undefined, digits = 1) {
  if (fmtUsd(usd) !== "—") return fmtUsd(usd);
  if (xlm == null || !Number.isFinite(xlm)) return "—";
  return `${fmtNum(xlm, digits)} XLM`;
}

function formatTs(ts?: number | null) {
  if (ts == null) return "—";
  return new Date(ts * 1000).toLocaleDateString();
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

function feeCapitalPct(claimXlm: number, depositXlm: number): number | null {
  if (!(depositXlm > 0) || !Number.isFinite(claimXlm)) return null;
  return (claimXlm / depositXlm) * 100;
}

function windowOrFallback(profile: LpProfile, key: "7d" | "30d"): ActivityWindow {
  return profile.windows?.[key] ?? profile.activity_30d;
}

function poolMix(events: LpProfile["recent_events"]) {
  const map = new Map<string, { count: number; quote: number }>();
  for (const e of events) {
    const cur = map.get(e.pool_address) ?? { count: 0, quote: 0 };
    cur.count += 1;
    if (e.quote_xlm != null && Number.isFinite(e.quote_xlm)) cur.quote += e.quote_xlm;
    map.set(e.pool_address, cur);
  }
  return [...map.entries()]
    .map(([pool, v]) => ({ pool, ...v }))
    .sort((a, b) => b.count - a.count || b.quote - a.quote)
    .slice(0, 6);
}

function LeadersInner() {
  const BOARD_PAGE_SIZE = 24;
  const router = useRouter();
  const searchParams = useSearchParams();
  const initial = searchParams.get("address") ?? "";
  const [input, setInput] = useState(initial);
  const [profile, setProfile] = useState<LpProfile | null>(null);
  const [board, setBoard] = useState<LeaderBoardRow[]>([]);
  const [boardHonesty, setBoardHonesty] = useState<string | null>(null);
  const [windowDays, setWindowDays] = useState(1);
  const [boardSort, setBoardSort] = useState<"fees" | "fee_cap" | "activity">("fees");
  const [boardPage, setBoardPage] = useState(1);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [boardLoading, setBoardLoading] = useState(true);

  const load = useCallback(
    async (address: string) => {
      const trimmed = address.trim();
      if (!isGAddress(trimmed)) {
        setError("Paste a valid G… Stellar address");
        return;
      }
      setLoading(true);
      setError(null);
      try {
        const data = await fetchLpProfile(trimmed);
        setProfile(data);
        setInput(trimmed);
        router.replace(`/leaders?address=${encodeURIComponent(trimmed)}`);
      } catch (e) {
        setProfile(null);
        setError(e instanceof Error ? e.message : "Failed to load LP profile");
      } finally {
        setLoading(false);
      }
    },
    [router],
  );

  useEffect(() => {
    if (initial && isGAddress(initial.trim())) {
      void load(initial);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps -- only on first mount from query
  }, []);

  useEffect(() => {
    let cancelled = false;
    setBoardLoading(true);
    void (async () => {
      try {
        const data = await fetchLpLeaders(500, windowDays, boardSort === "activity" ? "activity" : "fees");
        if (!cancelled) {
          setBoard(data.leaders);
          setBoardHonesty(data.honesty ?? null);
        }
      } catch (e) {
        if (!cancelled) {
          setBoard([]);
          setError(e instanceof Error ? e.message : "Failed to load leaders board");
        }
      } finally {
        if (!cancelled) setBoardLoading(false);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [windowDays, boardSort]);

  useEffect(() => {
    setBoardPage(1);
  }, [windowDays, boardSort]);

  useEffect(() => {
    if (profile) {
      document.getElementById("leader-profile")?.scrollIntoView({ behavior: "smooth", block: "start" });
    }
  }, [profile?.address]);

  const w7 = profile ? windowOrFallback(profile, "7d") : null;
  const w30 = profile ? windowOrFallback(profile, "30d") : null;
  const proxies = profile?.proxies;
  const sortedBoard = [...board].sort((a, b) => {
    if (boardSort === "activity") return 0;
    if (boardSort === "fee_cap") {
      const score = (row: LeaderBoardRow) => {
        if (row.fee_capital_ratio != null && Number.isFinite(row.fee_capital_ratio)) {
          return row.fee_capital_ratio;
        }
        const fee = row.accrued_fee_quote_xlm ?? row.claim_quote_xlm;
        if (row.deposit_quote_xlm > 0) return fee / row.deposit_quote_xlm;
        return -1;
      };
      return score(b) - score(a);
    }
    return (b.accrued_fee_quote_xlm ?? b.claim_quote_xlm) - (a.accrued_fee_quote_xlm ?? a.claim_quote_xlm);
  });
  const boardPageCount = Math.max(1, Math.ceil(sortedBoard.length / BOARD_PAGE_SIZE));
  const visibleBoard = sortedBoard.slice((boardPage - 1) * BOARD_PAGE_SIZE, boardPage * BOARD_PAGE_SIZE);

  function ratioPct(ratio?: number | null) {
    if (ratio == null || !Number.isFinite(ratio)) return null;
    return ratio * 100;
  }


  return (
    <div className="leaders-layout">
      <div className="panel">
        <div className="panel-head">Leaders</div>
        <p className="muted leaders-blurb">
          Scout Stellar LPs by accrued fees, open a profile, then copy. Accrued fees combine
          indexed claims with the latest verified unclaimed position fees — not full PnL.
        </p>
        <div className="leaders-search">
          <input
            className="filter-input leaders-search-input"
            placeholder="Paste leader G… address"
            value={input}
            onChange={(e) => setInput(e.target.value)}
            spellCheck={false}
            onKeyDown={(e) => {
              if (e.key === "Enter") void load(input);
            }}
          />
          <button
            type="button"
            className="primary"
            disabled={loading}
            onClick={() => void load(input)}
          >
            {loading ? "Loading…" : "Analyze"}
          </button>
        </div>
        {error ? <div className="error leaders-error">{error}</div> : null}
      </div>

      <div className="panel">
        <div className="panel-head leaders-board-head">
          <div className="leaders-board-title">
            <span>Top LPs</span>
            <span className="muted">last {windowDays}d</span>
          </div>
          <div className="leaders-filters">
            <div className="leaders-seg" role="group" aria-label="Time window">
              {[1, 7, 30].map((d) => (
                <button
                  key={d}
                  type="button"
                  className={windowDays === d ? "is-on" : undefined}
                  onClick={() => setWindowDays(d)}
                >
                  {d}d
                </button>
              ))}
            </div>
            <div className="leaders-seg" role="group" aria-label="Sort by">
              <button
                type="button"
                className={boardSort === "fees" ? "is-on" : undefined}
                onClick={() => setBoardSort("fees")}
              >
                Accrued fees
              </button>
              <button
                type="button"
                className={boardSort === "fee_cap" ? "is-on" : undefined}
                onClick={() => setBoardSort("fee_cap")}
              >
                Fee / cap
              </button>
              <button
                type="button"
                className={boardSort === "activity" ? "is-on" : undefined}
                onClick={() => setBoardSort("activity")}
              >
                Activity
              </button>
            </div>
          </div>
        </div>
        {boardLoading ? (
          <div className="empty">Loading board…</div>
        ) : sortedBoard.length === 0 ? (
          <div className="empty">No LP activity or verified fee snapshots in this window yet.</div>
        ) : (
          <div className="leaders-card-grid">
            {visibleBoard.map((row, i) => {
              const feeCap =
                ratioPct(row.fee_capital_ratio) ??
                feeCapitalPct(row.claim_quote_xlm, row.deposit_quote_xlm);
              const active = profile?.address === row.address;
              return (
                <div
                  key={row.address}
                  role="button"
                  tabIndex={0}
                  className={`leaders-board-card${active ? " is-active" : ""}`}
                  onClick={() => router.push(`/leaders/view?address=${encodeURIComponent(row.address)}`)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter" || e.key === " ") {
                      e.preventDefault();
                      router.push(`/leaders/view?address=${encodeURIComponent(row.address)}`);
                    }
                  }}
                >
                  <div className="leaders-board-card-top">
                    <div className="leaders-board-id">
                      <span className="leaders-rank">#{(boardPage - 1) * BOARD_PAGE_SIZE + i + 1}</span>
                      <span className="leaders-board-addr" title={row.address}>
                        {shortAddr(row.address)}
                      </span>
                    </div>
                    <span className="leaders-ago">{formatRelative(row.last_activity_at)}</span>
                  </div>
                  <div className="leaders-board-fee leaders-pos">
                    {row.accrued_fee_quote_xlm != null
                      ? quoteLabel(row.accrued_fee_quote_usd, row.accrued_fee_quote_xlm)
                      : quoteLabel(row.claim_quote_usd, row.claim_quote_xlm)}
                  </div>
                  <div className="leaders-board-sub muted">
                    {row.accrued_fee_quote_xlm != null ? "Accrued fees" : "Claimed fees"}
                    {row.accrued_fee_quote_xlm != null && row.unclaimed_fee_quote_xlm != null
                      ? ` · ${quoteLabel(row.unclaimed_fee_quote_usd, row.unclaimed_fee_quote_xlm)} unclaimed`
                      : row.accrued_fee_quote_xlm == null
                        ? " · unclaimed not verified"
                        : null}
                    {` · ${row.claim_count} claim${row.claim_count === 1 ? "" : "s"}`}
                  </div>
                  <div className="leaders-board-meta">
                    <div>
                      <span className="leaders-meta-k">Pools</span>
                      <span className="leaders-meta-v">{row.distinct_pools}</span>
                    </div>
                    <div>
                      <span className="leaders-meta-k">Deposits</span>
                      <span className="leaders-meta-v">
                        {quoteLabel(row.deposit_quote_usd, row.deposit_quote_xlm)}
                      </span>
                    </div>
                    <div>
                    <span className="leaders-meta-k">Accrued / cap</span>
                      <span className="leaders-meta-v">
                        {feeCap != null ? `${feeCap < 0.01 && feeCap > 0 ? "<0.01" : feeCap.toFixed(2)}%` : "—"}
                      </span>
                    </div>
                  </div>
                  <div className="leaders-row-actions">
                    <span className="leaders-card-hint muted">Open profile</span>
                    <Link
                      className="leaders-copy-link"
                      href={`/copy?leader=${encodeURIComponent(row.address)}`}
                      onClick={(e) => e.stopPropagation()}
                    >
                      Copy
                    </Link>
                  </div>
                </div>
              );
            })}
          </div>
        )}
        {!boardLoading && boardPageCount > 1 ? (
          <div className="leaders-filters" style={{ justifyContent: "flex-end", marginTop: 14 }}>
            <div className="leaders-seg" role="group" aria-label="Leader pages">
              <button type="button" disabled={boardPage === 1} onClick={() => setBoardPage((page) => Math.max(1, page - 1))}>
                Previous
              </button>
              <span className="muted" style={{ alignSelf: "center", padding: "0 8px" }}>
                Page {boardPage} / {boardPageCount}
              </span>
              <button type="button" disabled={boardPage === boardPageCount} onClick={() => setBoardPage((page) => Math.min(boardPageCount, page + 1))}>
                Next
              </button>
            </div>
          </div>
        ) : null}
        {boardHonesty ? (
          <p className="sign-disabled-note leaders-foot-note">{boardHonesty}</p>
        ) : null}
      </div>

      {profile && w7 && w30 ? (
        <>
          <div className="panel leaders-profile-card" id="leader-profile">
            <div className="leaders-profile-top">
              <div>
                <div className="leaders-profile-addr" title={profile.address}>
                  {shortAddr(profile.address)}
                </div>
                <div className="muted">
                  First {formatTs(profile.first_activity_at)} · Last{" "}
                  {formatRelative(profile.last_activity_at)}
                  {proxies?.months_active_indexed != null
                    ? ` · ~${proxies.months_active_indexed.toFixed(1)} mo indexed`
                    : ""}
                </div>
              </div>
              <Link
                className="primary"
                href={`/copy?leader=${encodeURIComponent(profile.address)}`}
              >
                Copy this leader
              </Link>
            </div>

            <div className="leaders-hero-row">
              <div className="leaders-hero-metric">
                <span className="filter-label">Claimed fees · 30d</span>
                <strong className="leaders-pos">
                  {quoteLabel(w30.claim_quote_usd, w30.claim_quote_xlm, 2)}
                </strong>
                <span className="muted">
                  {w30.claim_count} claims · {w30.distinct_pools} pools
                </span>
              </div>
              <div className="leaders-hero-metric">
                <span className="filter-label">Avg monthly claimed</span>
                <strong className="leaders-pos">
                  {proxies?.avg_monthly_claimed_xlm != null
                    ? quoteLabel(
                        proxies.avg_monthly_claimed_usd,
                        proxies.avg_monthly_claimed_xlm,
                        2,
                      )
                    : "—"}
                </strong>
                <span className="muted">lifetime claims ÷ active months (proxy)</span>
              </div>
              <div className="leaders-hero-metric">
                <span className="filter-label">Fee / capital · 30d</span>
                <strong className="leaders-pos">
                  {(() => {
                    const pct =
                      ratioPct(proxies?.fee_capital_ratio_30d) ??
                      feeCapitalPct(w30.claim_quote_xlm, w30.deposit_quote_xlm);
                    return pct != null ? `${pct.toFixed(2)}%` : "—";
                  })()}
                </strong>
                <span className="muted">claimed ÷ deposits (not ROI)</span>
              </div>
            </div>

            <div className="leaders-stats leaders-stats-dense">
              <div className="leaders-stat">
                <span className="filter-label">Unclaimed fees</span>
                <strong className="leaders-pos">
                  {quoteLabel(
                    profile.portfolio.fees_unclaimed_usd,
                    profile.portfolio.fees_unclaimed_xlm,
                    2,
                  )}
                </strong>
              </div>
              <div className="leaders-stat">
                <span className="filter-label">Open positions</span>
                <strong>{profile.portfolio.position_count}</strong>
              </div>
              <div className="leaders-stat">
                <span className="filter-label">Net worth</span>
                <strong>
                  {quoteLabel(profile.portfolio.net_worth_usd, profile.portfolio.net_worth_xlm, 2)}
                </strong>
              </div>
              <div className="leaders-stat">
                <span className="filter-label">Claim intensity · 30d</span>
                <strong>
                  {proxies?.claim_intensity_30d != null
                    ? `${proxies.claim_intensity_30d.toFixed(2)}×`
                    : "—"}
                </strong>
              </div>
              <div className="leaders-stat">
                <span className="filter-label">Avg deposit · 30d</span>
                <strong>
                  {w30.avg_deposit_quote_xlm != null
                    ? quoteLabel(w30.avg_deposit_quote_usd, w30.avg_deposit_quote_xlm, 2)
                    : "—"}
                </strong>
              </div>
              <div className="leaders-stat">
                <span className="filter-label">Lifetime claimed</span>
                <strong className="leaders-pos">
                  {profile.lifetime
                    ? quoteLabel(profile.lifetime.claim_quote_usd, profile.lifetime.claim_quote_xlm, 2)
                    : "—"}
                </strong>
              </div>
            </div>

            <div className="leaders-window-table-wrap">
              <table className="leaders-window-table">
                <thead>
                  <tr>
                    <th>Window</th>
                    <th>Events</th>
                    <th>Claimed fees</th>
                    <th>Fee / capital</th>
                    <th>Net liquidity</th>
                    <th>Pools</th>
                  </tr>
                </thead>
                <tbody>
                  {(
                    [
                      ["7D", w7],
                      ["30D", w30],
                    ] as const
                  ).map(([label, w]) => {
                    const pct = feeCapitalPct(w.claim_quote_xlm, w.deposit_quote_xlm);
                    return (
                      <tr key={label}>
                        <td>{label}</td>
                        <td>
                          {w.event_count}
                          <span className="muted">
                            {" "}
                            ({w.deposit_count}d / {w.withdraw_count}w / {w.claim_count}c)
                          </span>
                        </td>
                        <td className="leaders-pos">
                          {quoteLabel(w.claim_quote_usd, w.claim_quote_xlm)}
                        </td>
                        <td className="leaders-pos">
                          {pct != null ? `${pct.toFixed(2)}%` : "—"}
                        </td>
                        <td>{fmtNum(w.net_liquidity_quote_xlm, 1)} XLM</td>
                        <td>{w.distinct_pools}</td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </div>

            {profile.note ? <p className="muted">{profile.note}</p> : null}
            <p className="sign-disabled-note">{profile.honesty}</p>
          </div>

          {(() => {
            const mix = poolMix(profile.recent_events);
            if (mix.length === 0) return null;
            return (
              <div className="panel">
                <div className="panel-head">Pool mix · recent events</div>
                <div className="leaders-pool-mix">
                  {mix.map((row) => (
                    <div key={row.pool} className="leaders-pool-mix-row">
                      <Link href={`/pools/view?address=${encodeURIComponent(row.pool)}`}>
                        {shortAddr(row.pool)}
                      </Link>
                      <span className="muted">{row.count} events</span>
                      <span>{row.quote > 0 ? `${fmtNum(row.quote, 1)} XLM` : "—"}</span>
                    </div>
                  ))}
                </div>
              </div>
            );
          })()}

          <div className="panel">
            <div className="panel-head">Open positions</div>
            {profile.positions.length === 0 ? (
              <div className="empty">No open positions in scanned pools.</div>
            ) : (
              <div className="leaders-positions">
                {profile.positions.map((p) => (
                  <div key={`${p.pool_address}-${p.status}`} className="leaders-position">
                    <div className="leaders-position-head">
                      <Link href={`/pools/view?address=${encodeURIComponent(p.pool_address)}`}>
                        {tokenLabel(p)}
                      </Link>
                      <span className="badge">{venueLabel(p.venue, p.pool_type)}</span>
                    </div>
                    <div className="muted">
                      Value {fmtNum(p.value_quote, 2)} XLM
                      {p.fees_unclaimed_quote != null
                        ? ` · fees ${fmtNum(p.fees_unclaimed_quote, 3)}`
                        : ""}
                      {p.il_est != null ? ` · IL ~(${(p.il_est * 100).toFixed(2)}%)` : ""}
                    </div>
                  </div>
                ))}
              </div>
            )}
          </div>

          <div className="panel">
            <div className="panel-head">Recent LP events</div>
            {profile.recent_events.length === 0 ? (
              <div className="empty">No deposit/withdraw/claim events with actor in the last 30d.</div>
            ) : (
              <div className="leaders-events">
                {profile.recent_events.map((e) => (
                  <div key={e.event_id} className="leaders-event">
                    <span className="badge">{e.kind.replace("_liquidity", "").replace("_", " ")}</span>
                    <Link href={`/pools/view?address=${encodeURIComponent(e.pool_address)}`}>
                      {shortAddr(e.pool_address)}
                    </Link>
                    <span className="muted">
                      {e.quote_xlm != null ? `${fmtNum(e.quote_xlm, 2)} XLM · ` : ""}
                      {new Date(e.created_at * 1000).toLocaleString()}
                    </span>
                  </div>
                ))}
              </div>
            )}
          </div>
        </>
      ) : null}
    </div>
  );
}

export default function LeadersPage() {
  return (
    <Suspense
      fallback={
        <div className="panel">
          <div className="empty">Loading leaders…</div>
        </div>
      }
    >
      <LeadersInner />
    </Suspense>
  );
}
