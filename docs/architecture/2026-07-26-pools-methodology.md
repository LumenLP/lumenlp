# LumenLP Pools Methodology

Date: July 26, 2026

## Scope

This document describes the current MVP methodology used in the LumenLP pool list and pool detail views.

The current implementation is designed for:

- live Aquarius pool comparison
- transparent event-derived activity metrics
- demo and grant explanation

It is not yet a final production methodology.

## Data Sources

Current inputs come from two internal layers:

1. Pool snapshots
2. Pool event indexing

Pool snapshots provide:

- estimated TVL
- rollup windows
- fee / volume / fee-TVL metrics

Pool event indexing provides:

- swaps
- deposit liquidity events
- withdraw liquidity events
- fee claim events
- reserve update / sync events

## Window Metrics

The UI currently exposes:

- `5m`
- `1h`
- `6h`
- `24h`

For each window we derive:

- `volume`
- `fee`
- `avg_tvl`
- `fee_tvl`
- `tx_count`

Where rollups exist, the API serves rollup-backed values. Otherwise it falls back to snapshot-derived values.

## Activity Summary

The API returns an `activity_summary` block per pool.

Current fields:

- `event_count_24h`
- `swap_count_24h`
- `volume_quote_24h`
- `fee_quote_24h`
- `deposit_quote_24h`
- `withdraw_quote_24h`
- `net_liquidity_delta_quote_24h`
- `claim_quote_24h`
- `avg_update_interval_secs_24h`
- `latest_update_at_24h`

These metrics are derived from indexed Aquarius contract events and internal quote estimation.

## Current Score

The current MVP ranking uses a composite `score`.

Score components:

- fee-TVL component
- volume efficiency component
- net-liquidity component
- cadence component

Current formula:

```text
score
= fee_tvl_24h * 10_000
+ (volume_24h / max(tvl, 1)) * 200
+ (net_liquidity_delta_quote_24h / max(tvl, 1)) * 100
+ (1 / avg_update_interval_secs_24h) * 3_600
```

The API also returns `score_breakdown` so the ranking is explainable.

## Interpretation

The current score tends to favor pools that have:

- stronger fee generation relative to capital
- meaningful recent activity
- positive recent capital flow
- frequent reserve updates

This is intentionally simple and explainable. It is good enough for MVP ranking, but not yet a final market-quality model.

## Limitations

Current limitations:

- quote estimation is only as good as the internal pricing context
- history is still shallow relative to archive-grade systems
- some pools have sparse recent activity and therefore low-information scores
- token metadata is still address-heavy in parts of the UI
- the score is tuned for discovery, not for guaranteed economic optimality

## Recommended Future Improvements

- move from fixed weights to calibrated ranking logic
- add 7d / 30d context
- add pool-quality filters beyond raw activity
- improve symbol / token identity resolution
- publish a stable public methodology page for users and integrators
