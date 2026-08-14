# Leaders Profile (Copy scouting) — Design Spec

**Date:** 2026-08-06  
**Status:** Approved for implementation (UI-first)  
**Product:** LumenLP (`lumenlp.xyz`)  
**Reference UX:** [LP Agent portfolio / Smart LP](https://app.lpagent.io/smart-lp) profile cards  

## Goal

Help users **pick who to Copy** by ranking Aquarius LP actors and showing an honest profile card (fees, activity, open exposure) — without claiming full realized PnL.

## Decisions (locked)

| Topic | Choice |
|-------|--------|
| Approach | **UI-first** with real indexed metrics; no fake Win Rate / Profit |
| Route | Keep `/leaders` + `?address=`; board + inline detail |
| Primary earnings proxy | **Claimed Fees** (`claim_fees` / `claim_protocol_fee` quote XLM → USD) |
| Naming | Never label proxies as `Profit` / `Win Rate` / `EV` in v1 |
| Position scan | Only pools the actor touched in ~90d (avoid full-catalogue RPC timeout) |
| Windows | **7d + 30d** for activity; ALL deferred until indexer depth is trusted |
| CTA | `Copy this leader` → `/copy?leader=` |

## Non-goals (v1)

- Win Rate, EV, Avg Monthly Profit, full cost-basis PnL
- Scanning all ~335 Aquarius pools per profile request
- Separate `/portfolio` route (can alias later)
- Non-Aquarius venues

## Page structure

1. **Search** — paste G… address → load profile  
2. **Leader board** — ranked cards/rows by claimed fees (7d/30d toggle)  
3. **Profile summary card** (selected address) — dense metrics like LP Agent  
4. **Detail sections** — 7d/30d activity, open positions, recent events  

## Summary card field map

| LP Agent | LumenLP v1 | Source |
|----------|------------|--------|
| Fee Earned | Claimed Fees | `windows.30d.claim_quote_*` |
| Total Pools | Pools Touched | `distinct_pools` |
| Avg Age | First / Last Activity | indexer min/max `created_at` for actor LP events |
| Avg Invested | Avg Deposit Size (optional) | `deposit_quote / deposit_count` when count > 0 |
| 7D / 30D Profit | 7D / 30D Claimed Fees + Net Liquidity | multi-window activity |
| (exposure) | Open Positions / Unclaimed Fees / Net Worth | existing `portfolio` |
| Win Rate / EV / Monthly Profit | **omitted** | — |

## API

### `GET /v1/lp/profile?address=`

Enhance response:

```json
{
  "address": "G…",
  "portfolio": { "...existing..." },
  "first_activity_at": 123,
  "last_activity_at": 456,
  "windows": {
    "7d": { "claim_quote_xlm": 0, "deposit_quote_xlm": 0, "withdraw_quote_xlm": 0, "net_liquidity_quote_xlm": 0, "claim_count": 0, "deposit_count": 0, "withdraw_count": 0, "distinct_pools": 0, "event_count": 0 },
    "30d": { "...same shape..." }
  },
  "activity_30d": { "...keep for back-compat = windows.30d..." },
  "positions": [],
  "recent_events": [],
  "honesty": "Claimed fees are an earnings proxy, not full PnL vs entry."
}
```

### `GET /v1/lp/leaders?limit=&window_days=`

Unchanged sort (`claim_quote_xlm` desc). Board UI may show denser cards using existing row fields.

## Implementation notes

- Reuse `IndexDb::actor_liquidity_activity` for 7d and 30d (or one pass with dual buckets).
- Add `actor_first_last_activity(actor)` for first/last timestamps (all indexed history or since earliest event).
- Frontend: restyle profile block to match existing LumenLP tokens; keep honesty note visible.
- Deploy API + site after verify.

## Later (v2) — partially shipped

Shipped as honest **proxies** (not Win Rate / Profit):
- `lifetime` aggregates via SQL SUM
- `proxies.fee_capital_ratio_*`, `claim_intensity_30d`, `avg_monthly_claimed_*`
- Board client sort: Fees vs Fee/cap

Still deferred:
- True cost-basis PnL / Win Rate pairing
- Dedicated `/leaders/[address]` route if share links matter
- `ALL` window as first-class when history coverage is documented
