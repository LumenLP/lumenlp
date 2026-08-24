"use client";

import { Suspense, useCallback, useEffect, useRef, useState } from "react";
import Link from "next/link";
import { useRouter, useSearchParams } from "next/navigation";
import {
  fetchLpLeaders,
  fmtNum,
  fmtUsd,
  shortAddr,
  type LeaderBoardRow,
} from "@/lib/api";

function isGAddress(a: string) {
  return a.startsWith("G") && a.length >= 56;
}

function quoteLabel(usd: number | null | undefined, xlm: number | null | undefined, digits = 1) {
  if (fmtUsd(usd) !== "—") return fmtUsd(usd);
  if (xlm == null || !Number.isFinite(xlm)) return "—";
  return `${fmtNum(xlm, digits)} XLM`;
}

function feeCapitalPct(claimXlm: number, depositXlm: number): number | null {
  if (!(depositXlm > 0) || !Number.isFinite(claimXlm)) return null;
  return (claimXlm / depositXlm) * 100;
}

function formatRelative(ts?: number | null) {
  if (ts == null) return "—";
  const delta = Date.now() / 1000 - ts;
  if (delta < 60) return "just now";
  if (delta < 3600) return `${Math.floor(delta / 60)}m ago`;
  if (delta < 86400) return `${Math.floor(delta / 3600)}h ago`;
  if (delta < 86400 * 30) return `${Math.floor(delta / 86400)}d ago`;
  return new Date(ts * 1000).toLocaleDateString();
}

function LeadersInner() {
  const BOARD_PAGE_SIZE = 24;
  const router = useRouter();
  const searchParams = useSearchParams();
  const initial = searchParams.get("address") ?? "";
  const [input, setInput] = useState(initial);
  const [board, setBoard] = useState<LeaderBoardRow[]>([]);
  const [boardHonesty, setBoardHonesty] = useState<string | null>(null);
  const [feeData, setFeeData] = useState<{
    latest_snapshot_at?: number | null;
    verified_actor_count?: number;
    actor_count?: number;
  }>({});
  const [windowDays, setWindowDays] = useState(1);
  const [boardSort, setBoardSort] = useState<"fees" | "fee_cap" | "activity">("fees");
  const [boardPage, setBoardPage] = useState(1);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [boardLoading, setBoardLoading] = useState(true);
  const firstBoardLoad = useRef(true);

  const load = useCallback(
    async (address: string) => {
      const trimmed = address.trim();
      if (!isGAddress(trimmed)) {
        setError("Paste a valid G… Stellar address");
        return;
      }
      setError(null);
      setLoading(true);
      router.push(`/leaders/view?address=${encodeURIComponent(trimmed)}`);
    },
    [router],
  );

  useEffect(() => {
    if (initial && isGAddress(initial.trim())) {
      router.replace(`/leaders/view?address=${encodeURIComponent(initial.trim())}`);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps -- only on first mount from query
  }, []);

  useEffect(() => {
    let cancelled = false;
    const refreshBoard = async () => {
      const initialLoad = firstBoardLoad.current;
      if (initialLoad) setBoardLoading(true);
      try {
        const data = await fetchLpLeaders(500, windowDays, boardSort === "activity" ? "activity" : "fees");
        if (!cancelled) {
          setBoard(data.leaders);
          setBoardHonesty(data.honesty ?? null);
          setFeeData(data.fee_data ?? {});
          setError(null);
        }
      } catch (e) {
        if (!cancelled) {
          // Keep the last good board visible during a transient API/RPC error.
          setError(e instanceof Error ? e.message : "Failed to load leaders board");
        }
      } finally {
        if (!cancelled && initialLoad) setBoardLoading(false);
        firstBoardLoad.current = false;
      }
    };
    void refreshBoard();
    const timer = window.setInterval(() => void refreshBoard(), 60_000);
    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
  }, [windowDays, boardSort]);

  useEffect(() => {
    setBoardPage(1);
  }, [windowDays, boardSort]);

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
            <span className="muted">
              last {windowDays}d
              {feeData.latest_snapshot_at != null
                ? ` · fee data ${formatRelative(feeData.latest_snapshot_at)}`
                : " · fee data pending"}
            </span>
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
                feeCapitalPct(row.accrued_fee_quote_xlm ?? row.claim_quote_xlm, row.deposit_quote_xlm);
              return (
                <div
                  key={row.address}
                  role="button"
                  tabIndex={0}
                  className="leaders-board-card"
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
                    {row.fee_status === "unavailable"
                      ? "Fee unavailable"
                      : row.accrued_fee_quote_xlm != null
                        ? "Accrued fees"
                        : "Claimed fees"}
                    {row.accrued_fee_quote_xlm != null && row.unclaimed_fee_quote_xlm != null
                      ? ` · ${quoteLabel(row.unclaimed_fee_quote_usd, row.unclaimed_fee_quote_xlm)} unclaimed`
                      : row.accrued_fee_quote_xlm == null
                        ? row.fee_status === "verified" && row.unclaimed_fee_quote_xlm != null
                          ? ` · ${windowDays}d baseline pending`
                          : " · unclaimed not verified"
                        : null}
                    {row.accrued_fee_quote_xlm == null && row.position_value_quote_xlm != null
                      ? ` · position ${fmtNum(row.position_value_quote_xlm, 1)} XLM`
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
        {!boardLoading && feeData.actor_count != null ? (
          <p className="sign-disabled-note leaders-foot-note">
            Fee snapshots: {feeData.verified_actor_count ?? 0}/{feeData.actor_count} actors verified · refreshed continuously
          </p>
        ) : null}
      </div>

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
