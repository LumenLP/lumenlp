"use client";

import { Suspense, useEffect, useState } from "react";
import { useRouter, useSearchParams } from "next/navigation";
import { fmtNum, shortAddr } from "@/lib/api";
import {
  copyOpToDraftSnapshot,
  createCopySession,
  listCopyOps,
  listCopySessions,
  patchCopySession,
  prepareCopyOp,
  rememberCopyPosition,
  setCopyOpStatus,
  type CopyOp,
  type CopySession,
  type PreparedCopyOp,
} from "@/lib/copyLp";
import { useIdentity } from "@/lib/identity";
import { newStrategyId, upsertStrategy } from "@/lib/strategies";
import { copyPolicyConfig } from "@/lib/copyPolicy";

const COEFF_PRESETS = [0.1, 1, 2] as const;
const POLL_MS = 20_000;

function isGAddress(a: string) {
  return a.startsWith("G") && a.length >= 56;
}

function venueLabel(venue: string | null | undefined) {
  if (venue === "aquarius") return "Aquarius";
  if (venue === "soroswap" || venue === "soroswap_amm") return "Soroswap";
  if (venue === "phoenix") return "Phoenix";
  if (venue === "sushi" || venue === "sushi_v3") return "Sushi V3";
  if (venue === "comet") return "Comet";
  return !venue || venue === "unknown" ? "Unknown DEX" : venue;
}

function copyExecutionEnabled(venue: string | null | undefined) {
  return venue === "aquarius";
}

function formatOpQuote(op: CopyOp): string {
  if (op.leader_quote_xlm != null && op.scaled_quote_xlm != null) {
    return `${fmtNum(op.leader_quote_xlm, 2)} → ${fmtNum(op.scaled_quote_xlm, 2)} XLM`;
  }
  if (op.leader_amounts != null && op.scaled_amounts != null) {
    return `${JSON.stringify(op.leader_amounts)} → ${JSON.stringify(op.scaled_amounts)}`;
  }
  return "—";
}

function pickSession(sessions: CopySession[]): CopySession | null {
  return (
    sessions.find((s) => s.status === "active" || s.status === "paused") ??
    sessions[0] ??
    null
  );
}

function CopyInner() {
  const router = useRouter();
  const searchParams = useSearchParams();
  const leaderFromQuery = searchParams.get("leader") ?? "";
  const { address, connected } = useIdentity();
  const policy = copyPolicyConfig();
  const [leaderAddress, setLeaderAddress] = useState(leaderFromQuery);
  const [coefficient, setCoefficient] = useState<number>(1);
  const [customCoeff, setCustomCoeff] = useState("");
  const [includeClaims, setIncludeClaims] = useState(true);
  const [session, setSession] = useState<CopySession | null>(null);
  const [ops, setOps] = useState<CopyOp[]>([]);
  const [prepared, setPrepared] = useState<Record<string, PreparedCopyOp>>({});
  const [error, setError] = useState<string | null>(null);
  const [starting, setStarting] = useState(false);
  const [actionBusy, setActionBusy] = useState<string | null>(null);

  const effectiveCoeff = customCoeff.trim() ? Number(customCoeff) : coefficient;
  const sessionLive = session?.status === "active" || session?.status === "paused";

  useEffect(() => {
    if (leaderFromQuery) setLeaderAddress(leaderFromQuery);
  }, [leaderFromQuery]);

  useEffect(() => {
    if (!connected || !address) {
      setSession(null);
      return;
    }
    let cancelled = false;
    void (async () => {
      try {
        const sessions = await listCopySessions(address);
        if (!cancelled) setSession(pickSession(sessions));
      } catch (e) {
        if (!cancelled) {
          setError(e instanceof Error ? e.message : "Failed to load copy sessions");
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [address, connected]);

  useEffect(() => {
    if (!session || !sessionLive) {
      setOps([]);
      return;
    }
    let cancelled = false;
    const fetchOps = async () => {
      try {
        const list = await listCopyOps(session.id);
        if (!cancelled) setOps(list);
      } catch {
        /* poll errors are non-fatal */
      }
    };
    void fetchOps();
    const timer = setInterval(() => void fetchOps(), POLL_MS);
    return () => {
      cancelled = true;
      clearInterval(timer);
    };
  }, [session, sessionLive]);

  async function onStart() {
    if (!address || !isGAddress(leaderAddress.trim())) return;
    if (!Number.isFinite(effectiveCoeff) || effectiveCoeff <= 0) {
      setError("Coefficient must be a positive number");
      return;
    }
    setStarting(true);
    setError(null);
    try {
      const created = await createCopySession({
        follower_address: address,
        leader_address: leaderAddress.trim(),
        coefficient: effectiveCoeff,
        include_claims: includeClaims,
      });
      setSession(created);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to start copy session");
    } finally {
      setStarting(false);
    }
  }

  async function onPatchStatus(status: "active" | "paused" | "stopped") {
    if (!session) return;
    setActionBusy(status);
    setError(null);
    try {
      const updated = await patchCopySession(session.id, { status });
      setSession(updated);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Session update failed");
    } finally {
      setActionBusy(null);
    }
  }

  async function onGenerateDraft(op: CopyOp) {
    if (!address) return;
    if (!copyExecutionEnabled(op.venue)) {
      setError(`${venueLabel(op.venue)} is analytics-only; draft execution is disabled.`);
      return;
    }
    setActionBusy(`draft-${op.id}`);
    setError(null);
    try {
      upsertStrategy(address, {
        id: newStrategyId(),
        kind: "stay_in_range",
        poolAddress: op.pool_address,
        copyOpId: op.id,
        positionKey: op.position_key,
        copyDraft: copyOpToDraftSnapshot(op),
        status: "suggested",
        params: { widthBps: 800 },
        updatedAt: Date.now(),
      });
      rememberCopyPosition(address, op.position_key, op.id, op.pool_address);
      await setCopyOpStatus(op.id, "drafted");
      setOps((prev) =>
        prev.map((row) => (row.id === op.id ? { ...row, status: "drafted" } : row)),
      );
      router.push(
        `/strategies?pool=${encodeURIComponent(op.pool_address)}&copyOp=${encodeURIComponent(op.id)}`,
      );
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to generate draft");
    } finally {
      setActionBusy(null);
    }
  }

  async function onPreparePolicy(op: CopyOp) {
    if (!address) return;
    setActionBusy(`prepare-${op.id}`);
    setError(null);
    try {
      const result = await prepareCopyOp(op.id, address);
      setPrepared((prev) => ({ ...prev, [op.id]: result }));
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to prepare policy call");
    } finally {
      setActionBusy(null);
    }
  }

  async function onSkip(op: CopyOp) {
    setActionBusy(`skip-${op.id}`);
    setError(null);
    try {
      await setCopyOpStatus(op.id, "skipped");
      setOps((prev) =>
        prev.map((row) => (row.id === op.id ? { ...row, status: "skipped" } : row)),
      );
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to skip op");
    } finally {
      setActionBusy(null);
    }
  }

  return (
    <div className="copy-layout">
      <div className="copy-banner">
        Arm a Soroban Copy Policy once, then LumenLP can automatically submit approved Aquarius
        LP actions through the relayer — non-custodial and bounded by your limits.
      </div>
      <div className="copy-banner">
        Copy LP execution is currently enabled for Aquarius pools only. Activity from other
        Stellar DEXes remains available for analysis, but is kept out of the execution queue
        until its adapter is validated.
      </div>

      <div className="panel">
        <div className="panel-head">On-chain Copy Policy</div>
        <div className="strategy-config">
          <div className="copy-op-head">
            <span className="muted">Network</span>
            <span>{policy.network === "testnet" ? "Stellar Testnet" : "Stellar Mainnet"}</span>
          </div>
          <div className="copy-op-head">
            <span className="muted">Policy contract</span>
            <a href={policy.explorerUrl} target="_blank" rel="noreferrer">
              {shortAddr(policy.contractId)}
            </a>
          </div>
          <div className="copy-op-head">
            <span className="muted">Execution</span>
            <span className="badge">
              {policy.executionEnabled ? "Testnet, gated" : "Read-only"}
            </span>
          </div>
          <p className="sign-disabled-note">
            The policy contract is the final safety boundary. You authorize the policy from your
            wallet; LumenLP never receives your private keys or takes custody of your funds. The
            relayer can submit only policy-approved, allowlisted operations within the configured
            limits.
          </p>
        </div>
      </div>

      {!connected ? (
        <div className="panel">
          <div className="panel-head">Follower identity</div>
          <div className="empty">
            Connect a wallet or paste your G… address in the header to start copying.
          </div>
        </div>
      ) : null}

      {error ? <div className="error">{error}</div> : null}

      {connected && !sessionLive ? (
        <div className="panel">
          <div className="panel-head">Start copy session</div>
          <div className="strategy-config">
            <label className="filter-field">
              <span className="filter-label">Leader address</span>
              <input
                className="filter-input"
                value={leaderAddress}
                onChange={(e) => setLeaderAddress(e.target.value)}
                placeholder="G… address to follow"
                spellCheck={false}
              />
            </label>

            <div className="filter-field">
              <span className="filter-label">Coefficient</span>
              <div className="copy-coeff-presets">
                {COEFF_PRESETS.map((preset) => (
                  <button
                    key={preset}
                    type="button"
                    className={!customCoeff && coefficient === preset ? "active" : ""}
                    onClick={() => {
                      setCoefficient(preset);
                      setCustomCoeff("");
                    }}
                  >
                    {preset}×
                  </button>
                ))}
              </div>
              <label className="filter-field" style={{ marginTop: 8 }}>
                <span className="filter-label">Custom</span>
                <input
                  className="filter-input"
                  type="number"
                  min={0}
                  step="any"
                  value={customCoeff}
                  onChange={(e) => setCustomCoeff(e.target.value)}
                  placeholder="e.g. 0.5"
                />
              </label>
              {Number.isFinite(effectiveCoeff) && effectiveCoeff > 1 ? (
                <p className="muted" style={{ marginTop: 8 }}>
                  Scaled capital and inventory risk increase with coefficients above 1×.
                </p>
              ) : null}
            </div>

            <label className="filter-field">
              <span className="filter-label">Fee claims</span>
              <span>
                <input
                  type="checkbox"
                  checked={includeClaims}
                  onChange={(e) => setIncludeClaims(e.target.checked)}
                />{" "}
                Copy verified fee claim events
              </span>
              <span className="muted">
                Claims without a single verified reward token are rejected before preparation.
              </span>
            </label>

            <div className="landing-actions" style={{ justifyContent: "flex-start" }}>
              <button
                type="button"
                className="primary"
                onClick={() => void onStart()}
                disabled={
                  starting ||
                  !isGAddress(leaderAddress.trim()) ||
                  !Number.isFinite(effectiveCoeff) ||
                  effectiveCoeff <= 0
                }
              >
                {starting ? "Starting…" : "Start"}
              </button>
            </div>
          </div>
        </div>
      ) : null}

      {connected && session ? (
        <div className="panel">
          <div className="panel-head">Active session</div>
          <div className="strategy-config">
            <div className="copy-op-head">
              <span className="muted">Leader</span>
              <span title={session.leader_address}>{shortAddr(session.leader_address)}</span>
            </div>
            <div className="copy-op-head">
              <span className="muted">Coefficient</span>
              <span>{session.coefficient}×</span>
            </div>
            <div className="copy-op-head">
              <span className="muted">Fee claims</span>
              <span>{session.include_claims ? "Included" : "Excluded"}</span>
            </div>
            <div className="copy-op-head">
              <span className="muted">Status</span>
              <span className="badge">{session.status}</span>
            </div>
            {sessionLive ? (
              <div className="copy-op-actions">
                {session.status === "active" ? (
                  <button
                    type="button"
                    onClick={() => void onPatchStatus("paused")}
                    disabled={actionBusy !== null}
                  >
                    Pause
                  </button>
                ) : (
                  <button
                    type="button"
                    className="primary"
                    onClick={() => void onPatchStatus("active")}
                    disabled={actionBusy !== null}
                  >
                    Resume
                  </button>
                )}
                <button
                  type="button"
                  onClick={() => void onPatchStatus("stopped")}
                  disabled={actionBusy !== null}
                >
                  Stop
                </button>
              </div>
            ) : (
              <p className="sign-disabled-note">Session stopped. Start a new one above.</p>
            )}
          </div>
        </div>
      ) : null}

      {connected && sessionLive ? (
        <div className="panel">
          <div className="panel-head">Queue ({ops.length})</div>
          {ops.length === 0 ? (
            <div className="empty">No copy ops yet — waiting for leader LP activity.</div>
          ) : (
            <div className="copy-queue">
              {ops.map((op) => {
                const done = ["drafted", "skipped", "rejected", "failed", "insufficient"].includes(op.status);
                const preparedOp = prepared[op.id];
                return (
                  <div key={op.id} className="copy-op">
                    <div className="copy-op-head">
                      <strong>{op.kind}</strong>
                      <span className="badge">{venueLabel(op.venue)}</span>
                      <span className="badge">{op.status}</span>
                    </div>
                    <div className="muted" title={op.pool_address}>
                      Pool {shortAddr(op.pool_address)}
                    </div>
                    <div>{formatOpQuote(op)}</div>
                    {op.note ? <div className="sign-disabled-note">{op.note}</div> : null}
                    {preparedOp ? (
                      <div className="sign-disabled-note">
                        Policy intent validated: {preparedOp.method} · session {preparedOp.session_id}
                        {" "}· {preparedOp.quote_stroops.toLocaleString()} stroops
                        {preparedOp.claim_token ? (
                          <> · reward token {shortAddr(preparedOp.claim_token)}</>
                        ) : null}
                        {preparedOp.policy ? (
                          <>
                            {" "}· policy {preparedOp.policy.coefficient ?? "—"}× · per-op {preparedOp.policy.max_per_op_quote_xlm || "∞"} XLM · daily {preparedOp.policy.max_daily_quote_xlm || "∞"} XLM
                          </>
                        ) : null}
                      </div>
                    ) : null}
                    {!done ? (
                      <div className="copy-op-actions">
                        <button
                          type="button"
                          onClick={() => void onPreparePolicy(op)}
                          disabled={actionBusy !== null || !copyExecutionEnabled(op.venue)}
                        >
                          {actionBusy === `prepare-${op.id}` ? "Validating…" : "Validate policy intent"}
                        </button>
                        <button
                          type="button"
                          className="primary"
                          onClick={() => void onGenerateDraft(op)}
                          disabled={actionBusy !== null || !copyExecutionEnabled(op.venue)}
                        >
                          {copyExecutionEnabled(op.venue) ? "Generate draft" : "Analytics only"}
                        </button>
                        <button
                          type="button"
                          onClick={() => void onSkip(op)}
                          disabled={actionBusy !== null}
                        >
                          Skip
                        </button>
                      </div>
                    ) : null}
                  </div>
                );
              })}
            </div>
          )}
        </div>
      ) : null}
    </div>
  );
}

export default function CopyPage() {
  return (
    <Suspense
      fallback={
        <div className="panel">
          <div className="empty">Loading copy…</div>
        </div>
      }
    >
      <CopyInner />
    </Suspense>
  );
}
