function resolveApiBase() {
  if (process.env.NEXT_PUBLIC_API_BASE) {
    return process.env.NEXT_PUBLIC_API_BASE;
  }
  if (typeof window !== "undefined") {
    const host = window.location.hostname.toLowerCase();
    if (host === "lumenlp.xyz" || host === "www.lumenlp.xyz" || host.endsWith(".pages.dev")) {
      return "https://api.lumenlp.xyz";
    }
  }
  return "http://127.0.0.1:3301";
}

const API_BASE = resolveApiBase();


export type Position = {
  pool_address: string;
  venue?: string;
  pool_type: string;
  tokens: string[];
  token_labels?: string[];
  fee_bps: number;
  amounts: number[];
  value_quote: number | null;
  il_est: number | null;
  pnl: number | null;
  fees_unclaimed_quote: number | null;
  status: string;
  shares: number | null;
  cl_ranges?: Array<{
    tick_lower: number;
    tick_upper: number;
    liquidity: number;
    in_range: boolean;
  }> | null;
  note?: string | null;
};

export type Summary = {
  address: string;
  net_worth: number;
  fees_unclaimed: number | null;
  il_est_avg: number | null;
  position_count: number;
  indexed_pool_count?: number;
  last_snapshot_at?: string | null;
  note?: string | null;
};

export type ActivityWindow = {
  since_ts: number;
  event_count: number;
  deposit_count: number;
  withdraw_count: number;
  claim_count: number;
  deposit_quote_xlm: number;
  withdraw_quote_xlm: number;
  claim_quote_xlm: number;
  deposit_quote_usd?: number | null;
  withdraw_quote_usd?: number | null;
  claim_quote_usd?: number | null;
  unclaimed_fee_delta_quote_xlm?: number | null;
  unclaimed_fee_delta_quote_usd?: number | null;
  accrued_fee_quote_xlm?: number | null;
  accrued_fee_quote_usd?: number | null;
  distinct_pools: number;
  last_activity_at?: number | null;
  net_liquidity_quote_xlm: number;
  avg_deposit_quote_xlm?: number | null;
  avg_deposit_quote_usd?: number | null;
};

export type LpProfile = {
  address: string;
  venue_id: string;
  portfolio: {
    net_worth_xlm: number;
    net_worth_usd?: number | null;
    fees_unclaimed_xlm: number | null;
    fees_unclaimed_usd?: number | null;
    il_est_avg: number | null;
    position_count: number;
    cl_in_range: number;
    cl_out_of_range: number;
  };
  first_activity_at?: number | null;
  last_activity_at?: number | null;
  windows?: {
    "7d": ActivityWindow;
    "30d": ActivityWindow;
  };
  activity_30d: ActivityWindow;
  lifetime?: {
    deposit_count: number;
    withdraw_count: number;
    claim_count: number;
    deposit_quote_xlm: number;
    withdraw_quote_xlm: number;
    claim_quote_xlm: number;
    deposit_quote_usd?: number | null;
    withdraw_quote_usd?: number | null;
    claim_quote_usd?: number | null;
    distinct_pools: number;
    net_liquidity_quote_xlm: number;
  };
  proxies?: {
    fee_capital_ratio_7d?: number | null;
    fee_capital_ratio_30d?: number | null;
    fee_capital_ratio_lifetime?: number | null;
    claim_intensity_30d?: number | null;
    avg_monthly_claimed_xlm?: number | null;
    avg_monthly_claimed_usd?: number | null;
    months_active_indexed?: number | null;
    labels?: Record<string, string>;
  };
  positions: Position[];
  recent_events: Array<{
    event_id: string;
    kind: string;
    pool_address: string;
    token_labels?: string[];
    pool_type?: string | null;
    fee_bps?: number | null;
    venue?: string | null;
    created_at: number;
    tx_hash?: string | null;
    quote_xlm?: number | null;
  }>;
  indexed_pool_count?: number;
  note?: string | null;
  honesty?: string;
  xlm_usd?: number | null;
};

export type LeaderBoardRow = {
  address: string;
  event_count: number;
  deposit_count: number;
  withdraw_count: number;
  claim_count: number;
  deposit_quote_xlm: number;
  withdraw_quote_xlm: number;
  claim_quote_xlm: number;
  deposit_quote_usd?: number | null;
  withdraw_quote_usd?: number | null;
  claim_quote_usd?: number | null;
  unclaimed_fee_quote_xlm?: number | null;
  unclaimed_fee_quote_usd?: number | null;
  unclaimed_fee_delta_quote_xlm?: number | null;
  unclaimed_fee_delta_quote_usd?: number | null;
  accrued_fee_quote_xlm?: number | null;
  accrued_fee_quote_usd?: number | null;
  fee_status?: "verified" | "unavailable" | "not_verified" | string;
  fee_snapshot_at?: number | null;
  fee_snapshot_position_count?: number;
  position_value_quote_xlm?: number | null;
  net_liquidity_quote_xlm: number;
  fee_capital_ratio?: number | null;
  distinct_pools: number;
  last_activity_at?: number | null;
};

export type LeadersBoardResponse = {
  window_days: number;
  since_ts: number;
  xlm_usd?: number | null;
  leaders: LeaderBoardRow[];
  sort?: string;
  fee_data?: {
    latest_snapshot_at?: number | null;
    verified_actor_count?: number;
    actor_count?: number;
    refresh_cadence_seconds?: number;
  };
  honesty?: string;
};

export type VenueSupportRow = {
  venue_id: string;
  name: string;
  copy_execution_enabled: boolean;
};

export type QuoteInfo = {
  currency: string;
  as_of?: string;
  source?: string;
  xlm_usd?: number | null;
  coverage?: "full" | "partial" | "none" | string;
};

export type PoolRow = {
  address: string;
  venue?: string;
  pool_type: string;
  tokens: string[];
  token_meta?: Array<{
    address: string;
    symbol: string;
    name?: string | null;
    issuer?: string | null;
    domain?: string | null;
    icon?: string | null;
  }> | null;
  fee_bps: number;
  score?: number;
  score_breakdown?: {
    fee_tvl_component: number;
    volume_component: number;
    net_liq_component: number;
    cadence_component: number;
    inputs?: {
      fee_tvl_24h?: number | null;
      volume_24h?: number | null;
      tvl?: number | null;
      net_liquidity_delta_quote_24h?: number | null;
      avg_update_interval_secs_24h?: number | null;
    } | null;
  } | null;
  tvl: number;
  tvl_usd?: number | null;
  tvl_status?: "ok" | "missing_price" | "empty_reserves" | string | null;
  volume_24h: number;
  est_apr: number;
  last_snapshot_at: string;
  quote?: QuoteInfo;
  activity?: {
    first_event_at?: number | null;
    last_event_at?: number | null;
    event_count: number;
    swap_count: number;
  } | null;
  activity_summary?: {
    event_count_24h: number;
    swap_count_24h: number;
    volume_quote_24h: number;
    fee_quote_24h: number;
    deposit_quote_24h: number;
    withdraw_quote_24h: number;
    net_liquidity_delta_quote_24h: number;
    claim_quote_24h: number;
    avg_update_interval_secs_24h?: number | null;
    latest_update_at_24h?: number | null;
    deposit_count_24h: number;
    withdraw_count_24h: number;
    claim_count_24h: number;
    update_count_24h: number;
    volume_usd_24h?: number;
    fee_usd_24h?: number;
    deposit_usd_24h?: number;
    withdraw_usd_24h?: number;
    net_liquidity_delta_usd_24h?: number;
    claim_usd_24h?: number;
  } | null;
  window_metrics?: Record<
    string,
    {
      samples: number;
      volume: number;
      fee: number;
      avg_tvl: number;
      fee_tvl: number;
      tx_count?: number;
      as_of_ts?: number;
      volume_usd?: number;
      fee_usd?: number;
    }
  >;
};

export type PositionsResponse = {
  address: string;
  positions: Position[];
  indexed_pool_count?: number;
  last_snapshot_at?: string | null;
  note?: string | null;
};

export type PoolsResponse = {
  pools: PoolRow[];
  pagination?: {
    page: number;
    limit: number;
    total: number;
    pages: number;
  };
  quote?: QuoteInfo;
  indexed_pool_count?: number;
  last_snapshot_at?: string | null;
  indexer_status?: {
    cursor_ledger?: number | null;
    event_count: number;
    swap_count: number;
    rollup_count: number;
    distinct_event_pools: number;
    distinct_rollup_pools: number;
    last_event_at?: number | null;
    last_rollup_at?: number | null;
  } | null;
  note?: string | null;
};

export type PoolDetailResponse = {
  address: string;
  venue?: string | null;
  token_meta?: Array<{
    address: string;
    symbol: string;
    name?: string | null;
    issuer?: string | null;
    domain?: string | null;
    icon?: string | null;
  }> | null;
  score?: number | null;
  score_breakdown?: {
    fee_tvl_component: number;
    volume_component: number;
    net_liq_component: number;
    cadence_component: number;
    inputs?: {
      fee_tvl_24h?: number | null;
      volume_24h?: number | null;
      tvl?: number | null;
      net_liquidity_delta_quote_24h?: number | null;
      avg_update_interval_secs_24h?: number | null;
    } | null;
  } | null;
  pool_type?: string | null;
  tokens?: string[] | null;
  fee_bps?: number | null;
  tvl?: number | null;
  tvl_usd?: number | null;
  tvl_status?: "ok" | "missing_price" | "empty_reserves" | string | null;
  tvl_source?: string | null;
  latest?: HistoryPoint | null;
  quote?: QuoteInfo;
  activity?: {
    first_event_at?: number | null;
    last_event_at?: number | null;
    event_count: number;
    swap_count: number;
  } | null;
  activity_summary?: {
    event_count_24h: number;
    swap_count_24h: number;
    volume_quote_24h: number;
    fee_quote_24h: number;
    deposit_quote_24h: number;
    withdraw_quote_24h: number;
    net_liquidity_delta_quote_24h: number;
    claim_quote_24h: number;
    avg_update_interval_secs_24h?: number | null;
    latest_update_at_24h?: number | null;
    deposit_count_24h: number;
    withdraw_count_24h: number;
    claim_count_24h: number;
    update_count_24h: number;
    volume_usd_24h?: number;
    fee_usd_24h?: number;
    deposit_usd_24h?: number;
    withdraw_usd_24h?: number;
    net_liquidity_delta_usd_24h?: number;
    claim_usd_24h?: number;
  } | null;
  window_metrics?: Record<
    string,
    {
      samples: number;
      volume: number;
      fee: number;
      avg_tvl: number;
      fee_tvl: number;
      tx_count?: number;
      as_of_ts?: number;
      volume_usd?: number;
      fee_usd?: number;
    }
  >;
  last_snapshot_at?: string | null;
  indexed_pool_count?: number | null;
  note?: string | null;
};

export type PoolEventRow = {
  event_id: string;
  tx_hash?: string | null;
  ledger: number;
  created_at: number;
  pool_address: string;
  kind: string;
  body?: {
    contract_id?: string;
    topic?: unknown[];
    data?: unknown[];
    derived?: Record<string, unknown> | null;
  } | null;
};

export type HistoryPoint = {
  pool_address: string;
  ts: string;
  tvl: number;
  volume_24h: number;
  est_apr: number;
};

async function requestJson<T>(path: string, init?: RequestInit): Promise<T> {
  // Public analytics responses advertise a short max-age from the API. Keep
  // GETs cacheable in the browser, while writes remain explicitly uncached.
  const method = (init?.method ?? "GET").toUpperCase();
  const res = await fetch(`${API_BASE}${path}`, {
    cache: method === "GET" ? "default" : "no-store",
    ...init,
  });
  if (!res.ok) {
    const body = await res.text();
    throw new Error(`${res.status}: ${body}`);
  }
  return res.json() as Promise<T>;
}

export function getJson<T>(path: string): Promise<T> {
  return requestJson<T>(path);
}

export function postJson<T>(path: string, body?: unknown): Promise<T> {
  return requestJson<T>(path, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: body != null ? JSON.stringify(body) : undefined,
  });
}

export function patchJson<T>(path: string, body: unknown): Promise<T> {
  return requestJson<T>(path, {
    method: "PATCH",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
}

export function fetchSummary(address: string) {
  return getJson<Summary>(`/v1/positions/summary?address=${encodeURIComponent(address)}`);
}

export function fetchPositions(address: string) {
  return getJson<PositionsResponse>(
    `/v1/positions?address=${encodeURIComponent(address)}`,
  );
}

export function fetchLpProfile(address: string) {
  return getJson<LpProfile>(`/v1/lp/profile?address=${encodeURIComponent(address)}`);
}

export function fetchLpLeaders(limit = 25, windowDays = 30, sort: "fees" | "activity" = "fees") {
  return getJson<LeadersBoardResponse>(
    `/v1/lp/leaders?limit=${limit}&window_days=${windowDays}&sort=${sort}`,
  );
}

export function fetchPools() {
  return getJson<PoolsResponse>("/v1/pools?limit=500&page=1").then(async (first) => {
    const pages = first.pagination?.pages ?? 1;
    if (pages <= 1) return first;

    const rest = await Promise.all(
      Array.from({ length: pages - 1 }, (_, index) =>
        getJson<PoolsResponse>(`/v1/pools?limit=500&page=${index + 2}`),
      ),
    );
    return {
      ...first,
      pools: [first, ...rest].flatMap((page) => page.pools ?? []),
      pagination: {
        ...first.pagination!,
        page: 1,
      },
    };
  });
}

export function fetchVenues() {
  return getJson<{ venues: VenueSupportRow[] }>("/v1/venues");
}

export function fetchPoolHistory(address: string) {
  return getJson<{ address: string; points: HistoryPoint[] }>(
    `/v1/pools/${encodeURIComponent(address)}/history?limit=90`,
  );
}

export function fetchPoolDetail(address: string) {
  return getJson<PoolDetailResponse>(`/v1/pools/${encodeURIComponent(address)}`);
}

export function fetchPoolEvents(address: string, limit = 20) {
  return getJson<{ address: string; events: PoolEventRow[] }>(
    `/v1/pools/${encodeURIComponent(address)}/events?limit=${limit}`,
  );
}

export function shortAddr(a: string) {
  if (a.length < 12) return a;
  return `${a.slice(0, 4)}…${a.slice(-4)}`;
}

export function fmtNum(n: number | null | undefined, digits = 4) {
  if (n == null || Number.isNaN(n)) return "—";
  return n.toLocaleString(undefined, { maximumFractionDigits: digits });
}

/** Compact USD formatting used across the LumenLP pool and leader views. */
export function fmtUsd(n: number | null | undefined, digits = 2) {
  if (n == null || Number.isNaN(n)) return "—";
  const abs = Math.abs(n);
  const sign = n < 0 ? "-" : "";
  if (abs >= 1_000_000_000) {
    return `${sign}$${(abs / 1_000_000_000).toFixed(digits)}b`;
  }
  if (abs >= 1_000_000) {
    return `${sign}$${(abs / 1_000_000).toFixed(digits)}m`;
  }
  if (abs >= 1_000) {
    return `${sign}$${(abs / 1_000).toFixed(digits)}k`;
  }
  if (abs > 0 && abs < 0.01) {
    return `${sign}$${abs.toLocaleString("en-US", {
      maximumFractionDigits: Math.max(digits, 4),
      minimumFractionDigits: Math.max(digits, 4),
    })}`;
  }
  return `${sign}$${abs.toLocaleString("en-US", {
    maximumFractionDigits: digits,
    minimumFractionDigits: Math.min(digits, 2),
  })}`;
}

export function pickUsd(
  usd: number | null | undefined,
  xlm: number | null | undefined,
  xlmUsd?: number | null,
) {
  if (usd != null && Number.isFinite(usd)) return { value: usd, kind: "usd" as const };
  if (xlm != null && xlmUsd != null && Number.isFinite(xlm) && xlmUsd > 0) {
    return { value: xlm * xlmUsd, kind: "usd" as const };
  }
  return { value: null, kind: "none" as const };
}

export function fmtPct(n: number | null | undefined) {
  if (n == null || Number.isNaN(n)) return "—";
  return `${(n * 100).toFixed(2)}%`;
}

export function fmtTs(ts: string | null | undefined) {
  if (!ts) return "—";
  const d = new Date(ts);
  if (Number.isNaN(d.getTime())) return ts;
  return d.toLocaleString(undefined, {
    year: "numeric",
    month: "short",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}

export function fmtUnixTs(ts: number | null | undefined) {
  if (ts == null || Number.isNaN(ts)) return "—";
  return fmtTs(new Date(ts * 1000).toISOString());
}

export function fmtAgeFromUnix(ts: number | null | undefined) {
  if (ts == null || Number.isNaN(ts)) return "—";
  const diffSec = Math.max(0, Math.floor(Date.now() / 1000) - ts);
  const days = Math.floor(diffSec / 86400);
  const hours = Math.floor((diffSec % 86400) / 3600);
  const mins = Math.floor((diffSec % 3600) / 60);
  if (days > 0) return `${days}d`;
  if (hours > 0) return `${hours}h`;
  return `${mins}m`;
}
