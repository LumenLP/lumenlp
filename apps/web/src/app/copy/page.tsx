"use client";

import { Suspense, useEffect, useState } from "react";
import { useRouter, useSearchParams } from "next/navigation";
import { fmtNum, shortAddr } from "@/lib/api";
import {
  copyOpToDraftSnapshot,
  createCopySession,
  formatCopyError,
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

function isCAddress(a: string) {
  return a.startsWith("C") && a.length >= 56;
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
  const [maxPerOp, setMaxPerOp] = useState("100");
  const [maxDaily, setMaxDaily] = useState("500");
  const [expiryDays, setExpiryDays] = useState("30");
  const [allowedPoolsText, setAllowedPoolsText] = useState("");
  const [contractSessionId, setContractSessionId] = useState("");
  const [session, setSession] = useState<CopySession | null>(null);
  const [ops, setOps] = useState<CopyOp[]>([]);
  const [prepared, setPrepared] = useState<Record<string, PreparedCopyOp>>({});
  const [error, setError] = useState<string | null>(null);
  const [starting, setStarting] = useState(false);
  const [actionBusy, setActionBusy] = useState<string | null>(null);
  const [bindingPolicy, setBindingPolicy] = useState(false);
  const [nowMs, setNowMs] = useState(() => Date.now());

  const effectiveCoeff = customCoeff.trim() ? Number(customCoeff) : coefficient;
  const sessionLive = session?.status === "active" || session?.status === "paused";
  const policyExpired = Boolean(
    session?.policy?.expires_at != null && session.policy.expires_at * 1000 <= nowMs,
  );
  const policyReady =
    session?.contract_session_id != null && policy.executionEnabled && !policyExpired;

  useEffect(() => {
    const timer = setInterval(() => setNowMs(Date.now()), 30_000);
    return () => clearInterval(timer);
  }, []);

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
          setError(formatCopyError(e, "Failed to load copy sessions"));
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
    const maxPerOpXlm = Number(maxPerOp);
    const maxDailyXlm = Number(maxDaily);
    if (!Number.isFinite(maxPerOpXlm) || maxPerOpXlm <= 0 || !Number.isFinite(maxDailyXlm) || maxDailyXlm <= 0) {
      setError("Per-operation and daily limits must be positive numbers");
      return;
    }
    if (maxDailyXlm < maxPerOpXlm) {
      setError("Daily limit must be at least the per-operation limit");
      return;
    }
    const expiryDaysValue = Number(expiryDays);
    if (!Number.isInteger(expiryDaysValue) || expiryDaysValue < 1 || expiryDaysValue > 365) {
      setError("Policy expiry must be a whole number between 1 and 365 days");
      return;
    }
    const allowedPools = allowedPoolsText
      .split(/[\s,]+/)
      .map((pool) => pool.trim())
      .filter(Boolean);
    if (allowedPools.some((pool) => !isCAddress(pool))) {
      setError("Allowed pools must be complete Stellar contract addresses starting with C");
      return;
    }
    const contractSessionIdValue = contractSessionId.trim() ? Number(contractSessionId) : null;
    if (
      contractSessionIdValue !== null &&
      (!Number.isInteger(contractSessionIdValue) || contractSessionIdValue < 0)
    ) {
      setError("On-chain policy session ID must be a non-negative whole number");
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
        max_per_op_quote_xlm: maxPerOpXlm,
        max_daily_quote_xlm: maxDailyXlm,
        expires_at: Math.floor(Date.now() / 1000) + expiryDaysValue * 24 * 60 * 60,
        allowed_pools: allowedPools,
        contract_session_id: contractSessionIdValue ?? undefined,
      });
      setSession(created);
    } catch (e) {
      setError(formatCopyError(e, "Failed to start copy session"));
    } finally {
      setStarting(false);
    }
  }

  async function onPatchStatus(status: "active" | "paused" | "stopped") {
    if (!session || !address) return;
    setActionBusy(status);
    setError(null);
    try {
      const updated = await patchCopySession(session.id, address, { status });
      setSession(updated);
    } catch (e) {
      setError(formatCopyError(e, "Session update failed"));
    } finally {
      setActionBusy(null);
    }
  }

  async function onBindPolicySession() {
    if (!session || !address) return;
    const value = Number(contractSessionId.trim());
    if (!Number.isInteger(value) || value < 0) {
      setError("On-chain policy session ID must be a non-negative whole number");
      return;
    }
    setBindingPolicy(true);
    setError(null);
    try {
      const updated = await patchCopySession(session.id, address, { contract_session_id: value });
      setSession(updated);
    } catch (e) {
      setError(formatCopyError(e, "Failed to bind policy session"));
    } finally {
      setBindingPolicy(false);
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
      await setCopyOpStatus(op.id, address, "drafted");
      setOps((prev) =>
        prev.map((row) => (row.id === op.id ? { ...row, status: "drafted" } : row)),
      );
      router.push(
        `/strategies?pool=${encodeURIComponent(op.pool_address)}&copyOp=${encodeURIComponent(op.id)}`,
      );
    } catch (e) {
      setError(formatCopyError(e, "Failed to generate draft"));
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
      setError(formatCopyError(e, "Failed to prepare policy call"));
    } finally {
      setActionBusy(null);
    }
  }

  async function onSkip(op: CopyOp) {
    if (!address) return;
    setActionBusy(`skip-${op.id}`);
    setError(null);
    try {
      await setCopyOpStatus(op.id, address, "skipped");
      setOps((prev) =>
        prev.map((row) => (row.id === op.id ? { ...row, status: "skipped" } : row)),
      );
    } catch (e) {
      setError(formatCopyError(e, "Failed to skip op"));
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

      {connected && (!sessionLive || policyExpired) ? (
        <div className="panel">
          <div className="panel-head">
            {policyExpired ? "Re-arm copy session" : "Start copy session"}
          </div>
          <div className="strategy-config">
            {policyExpired ? (
              <p className="sign-disabled-note">
                The previous policy session is retained for review. Start a new session to follow
                this Leader with a fresh expiry and limits.
              </p>
            ) : null}
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

            <div className="copy-op-head">
              <span className="muted">Safety limits</span>
              <span className="muted">Applied to policy-approved operations</span>
            </div>
            <div className="strategy-config" style={{ padding: 0 }}>
              <label className="filter-field">
                <span className="filter-label">Max per operation (XLM)</span>
                <input
                  className="filter-input"
                  type="number"
                  min="0.000001"
                  step="any"
                  value={maxPerOp}
                  onChange={(e) => setMaxPerOp(e.target.value)}
                  placeholder="100"
                />
              </label>
              <label className="filter-field">
                <span className="filter-label">Max per day (XLM)</span>
                <input
                  className="filter-input"
                  type="number"
                  min="0.000001"
                  step="any"
                  value={maxDaily}
                  onChange={(e) => setMaxDaily(e.target.value)}
                  placeholder="500"
                />
              </label>
              <label className="filter-field">
                <span className="filter-label">Policy expiry (days)</span>
                <input
                  className="filter-input"
                  type="number"
                  min="1"
                  max="365"
                  step="1"
                  value={expiryDays}
                  onChange={(e) => setExpiryDays(e.target.value)}
                  placeholder="30"
                />
              </label>
            </div>
            <label className="filter-field">
              <span className="filter-label">Allowed pools (optional)</span>
              <textarea
                className="filter-input"
                rows={2}
                value={allowedPoolsText}
                onChange={(e) => setAllowedPoolsText(e.target.value)}
                placeholder="Paste complete C… pool addresses, separated by spaces or lines"
                spellCheck={false}
              />
              <span className="muted">
                Leave blank to allow all validated Aquarius pools touched by this Leader. A pool
                allowlist is safer for unattended execution.
              </span>
            </label>
            <label className="filter-field">
              <span className="filter-label">On-chain policy session ID (optional)</span>
              <input
                className="filter-input"
                type="number"
                min="0"
                step="1"
                value={contractSessionId}
                onChange={(e) => setContractSessionId(e.target.value)}
                placeholder="Set after registering the Soroban policy session"
              />
              <span className="muted">
                Bind an existing on-chain session. LumenLP does not create or sign the registration
                transaction from this field.
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
              <span className="muted">Safety limits</span>
              <span>
                {session.policy?.max_per_op_quote_xlm || "∞"} XLM / op · {session.policy?.max_daily_quote_xlm || "∞"} XLM / day
              </span>
            </div>
            <div className="copy-op-head">
              <span className="muted">Policy expiry</span>
              <span>
                {session.policy?.expires_at
                  ? `${new Date(session.policy.expires_at * 1000).toLocaleDateString()}${policyExpired ? " (expired)" : ""}`
                  : "Not configured"}
              </span>
            </div>
            <div className="copy-op-head">
              <span className="muted">Pool scope</span>
              <span>
                {session.policy?.allowed_pools?.length
                  ? `${session.policy.allowed_pools.length} allowlisted pool${session.policy.allowed_pools.length === 1 ? "" : "s"}`
                  : "All validated Aquarius pools"}
              </span>
            </div>
            <div className="copy-op-head">
              <span className="muted">On-chain policy session</span>
              <span>{session.contract_session_id ?? "Not bound"}</span>
            </div>
            {session.contract_session_id == null ? (
              <p className="sign-disabled-note">
                Bind an existing Soroban policy session before validating or preparing automatic
                Copy LP operations. The queue can still be reviewed while policy is unbound.
              </p>
            ) : null}
            {session.contract_session_id == null ? (
              <div className="copy-op-actions">
                <input
                  className="filter-input"
                  type="number"
                  min="0"
                  step="1"
                  value={contractSessionId}
                  onChange={(e) => setContractSessionId(e.target.value)}
                  placeholder="Existing on-chain session ID"
                />
                <button
                  type="button"
                  onClick={() => void onBindPolicySession()}
                  disabled={bindingPolicy}
                >
                  {bindingPolicy ? "Binding…" : "Bind policy"}
                </button>
              </div>
            ) : null}
            {policyExpired ? (
              <p className="sign-disabled-note">
                This Copy Policy has expired. Start a new session before preparing automatic
                operations.
              </p>
            ) : null}
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
                    disabled={actionBusy !== null || policyExpired}
                  >
                    {policyExpired ? "Expired" : "Resume"}
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
                const canPrepare = policyReady && session.status === "active";
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
                          disabled={actionBusy !== null || !copyExecutionEnabled(op.venue) || !canPrepare}
                        >
                          {actionBusy === `prepare-${op.id}`
                            ? "Validating…"
                            : policyExpired
                              ? "Policy expired"
                              : session.status !== "active"
                                ? "Resume first"
                                : policyReady
                              ? "Validate policy intent"
                              : "Bind policy first"}
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
