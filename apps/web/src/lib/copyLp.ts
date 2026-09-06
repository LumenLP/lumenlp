import { ApiError, getJson, patchJson, postJson } from "./api";
import { walletAuthHeaders } from "./walletAuth";

export function formatCopyError(error: unknown, fallback: string): string {
  const code = error instanceof ApiError ? error.code : undefined;
  switch (code) {
    case "policy_session_missing":
      return "Bind an on-chain Soroban policy session before validating this operation.";
    case "claim_token_missing":
      return "This fee claim has no verified reward token, so it cannot be prepared safely.";
    case "quote_missing":
      return "This operation has no verified XLM quote and cannot be prepared safely.";
    case "venue_not_enabled":
      return "Automatic Copy LP is currently enabled for Aquarius only; this venue remains analytics-only.";
    case "policy_expired":
      return "The Copy Policy has expired. Start a new session with a fresh expiry.";
    case "policy_binding_conflict":
      return "This session is already bound on-chain. Start a new session to change policy limits or settings.";
    case "auth_required":
    case "auth_invalid":
    case "auth_challenge_invalid":
    case "auth_signature_invalid":
    case "auth_address_mismatch":
      return "Wallet authentication failed. Reconnect the follower wallet and try again.";
    case "per_operation_limit":
      return "This operation exceeds the configured per-operation limit.";
    case "daily_limit":
      return "This operation would exceed the configured daily limit.";
    case "session_inactive":
      return "This Copy session is not active. Resume it before preparing an operation.";
    case "unsupported_operation":
      return "This LP action is not supported by the current Copy Policy adapter.";
    default:
      return error instanceof Error && error.message ? error.message : fallback;
  }
}

export type CopySession = {
  id: string;
  follower_address: string;
  leader_address: string;
  coefficient: number;
  coefficient_ppm?: number | null;
  contract_session_id?: number | null;
  status: "active" | "paused" | "stopped";
  include_claims: boolean;
  policy?: {
    allowed_pools?: string[];
    max_per_op_quote_xlm?: number;
    max_daily_quote_xlm?: number;
    expires_at?: number | null;
  };
  cursor_ts: number;
  watermark_ts?: number;
  watermark_event_id?: string;
  created_at?: number;
  updated_at?: number;
};

export type CopyOp = {
  id: string;
  session_id: string;
  source_event_id: string;
  pool_address: string;
  venue?: string | null;
  kind: string;
  position_key: string;
  leader_amounts: unknown;
  scaled_amounts: unknown;
  leader_quote_xlm?: number | null;
  scaled_quote_xlm?: number | null;
  status: "pending" | "drafted" | "skipped" | "signed" | "failed" | "insufficient" | "rejected";
  note?: string | null;
  created_at: number;
  updated_at?: number;
};

export type CopyOpStatus = Exclude<CopyOp["status"], "pending">;

export type PreparedCopyOp = {
  ready: boolean;
  validated: boolean;
  network: string;
  contract_id: string | null;
  method: "execute_aquarius_standard_op";
  session_id: number;
  source_event_id: string;
  source_event_id_hex?: string;
  pool: string;
  kind: string;
  claim_token?: string | null;
  policy?: {
    coefficient?: number;
    coefficient_ppm?: number | null;
    allowed_pools?: string[];
    max_per_op_quote_xlm?: number;
    max_daily_quote_xlm?: number;
    expires_at?: number | null;
  };
  quote_stroops: number;
  scaled_amounts: unknown;
  amount_values?: string[];
  note: string;
};

export type CopyPositionEntry = {
  copyOpId: string;
  poolAddress: string;
  updatedAt: number;
};

export type CopyPositionMap = Record<string, CopyPositionEntry>;

const STORAGE_PREFIX = "lumenlp.copyPositionMap.";

function isCopyPositionEntry(value: unknown): value is CopyPositionEntry {
  if (!value || typeof value !== "object") return false;
  const row = value as Record<string, unknown>;
  return (
    typeof row.copyOpId === "string" &&
    typeof row.poolAddress === "string" &&
    typeof row.updatedAt === "number"
  );
}

export function copyPositionMapKey(follower: string | null | undefined) {
  const key = (follower ?? "anonymous").trim() || "anonymous";
  return `${STORAGE_PREFIX}${key}`;
}

export function readCopyPositionMap(follower: string | null): CopyPositionMap {
  if (typeof window === "undefined") return {};
  const raw = localStorage.getItem(copyPositionMapKey(follower));
  if (!raw) return {};
  try {
    const parsed = JSON.parse(raw) as unknown;
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) return {};
    const out: CopyPositionMap = {};
    for (const [positionKey, entry] of Object.entries(parsed)) {
      if (isCopyPositionEntry(entry)) {
        out[positionKey] = entry;
      }
    }
    return out;
  } catch {
    localStorage.removeItem(copyPositionMapKey(follower));
    return {};
  }
}

export function writeCopyPositionMap(
  follower: string | null,
  map: CopyPositionMap,
): void {
  if (typeof window === "undefined") return;
  localStorage.setItem(copyPositionMapKey(follower), JSON.stringify(map));
}

export function rememberCopyPosition(
  follower: string | null,
  positionKey: string,
  copyOpId: string,
  poolAddress: string,
): void {
  const map = readCopyPositionMap(follower);
  map[positionKey] = { copyOpId, poolAddress, updatedAt: Date.now() };
  writeCopyPositionMap(follower, map);
}

export async function createCopySession(body: {
  follower_address: string;
  leader_address: string;
  coefficient: number;
  include_claims?: boolean;
  allowed_pools?: string[];
  max_per_op_quote_xlm?: number;
  max_daily_quote_xlm?: number;
  expires_at?: number | null;
  contract_session_id?: number;
}): Promise<CopySession> {
  return postJson<CopySession>("/v1/copy/sessions", body, await walletAuthHeaders(body.follower_address));
}

export async function listCopySessions(follower: string): Promise<CopySession[]> {
  const res = await getJson<{ sessions: CopySession[] }>(
    `/v1/copy/sessions?follower=${encodeURIComponent(follower)}`,
  );
  return res.sessions;
}

export async function listCopyOps(
  sessionId: string,
  status?: string,
): Promise<CopyOp[]> {
  const qs = status ? `?status=${encodeURIComponent(status)}` : "";
  const res = await getJson<{ session_id: string; ops: CopyOp[] }>(
    `/v1/copy/sessions/${encodeURIComponent(sessionId)}/ops${qs}`,
  );
  return res.ops;
}

export async function patchCopySession(
  id: string,
  followerAddress: string,
  body: {
    status?: string;
    coefficient?: number;
    include_claims?: boolean;
    contract_session_id?: number;
  },
): Promise<CopySession> {
  return patchJson<CopySession>(
    `/v1/copy/sessions/${encodeURIComponent(id)}`,
    { ...body, follower_address: followerAddress },
    await walletAuthHeaders(followerAddress),
  );
}

export async function setCopyOpStatus(
  id: string,
  followerAddress: string,
  status: CopyOpStatus,
): Promise<void> {
  await postJson<{ id: string; status: string }>(
    `/v1/copy/ops/${encodeURIComponent(id)}/status`,
    { follower_address: followerAddress, status },
    await walletAuthHeaders(followerAddress),
  );
}

export async function getCopyOp(id: string): Promise<CopyOp> {
  return getJson<CopyOp>(`/v1/copy/ops/${encodeURIComponent(id)}`);
}

export async function prepareCopyOp(id: string, followerAddress: string): Promise<PreparedCopyOp> {
  return postJson<PreparedCopyOp>(
    `/v1/copy/ops/${encodeURIComponent(id)}/prepare`,
    { follower_address: followerAddress },
    await walletAuthHeaders(followerAddress),
  );
}

export function copyOpToDraftSnapshot(op: CopyOp): {
  kind: string;
  leaderAmounts: unknown;
  scaledAmounts: unknown;
  leaderQuoteXlm?: number | null;
  scaledQuoteXlm?: number | null;
} {
  return {
    kind: op.kind,
    leaderAmounts: op.leader_amounts,
    scaledAmounts: op.scaled_amounts,
    leaderQuoteXlm: op.leader_quote_xlm,
    scaledQuoteXlm: op.scaled_quote_xlm,
  };
}
