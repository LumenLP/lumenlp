# LPAgent Pools MVP Status

Date: July 26, 2026

## Current State

The `/pools` and `/pools/view` experience is now in a usable MVP state for demo and grant discussion.

Implemented:

- Pool list with 5m / 1h / 6h / 24h rollup metrics
- Fee, volume, fee/TVL, tx count, age, activity counts
- 24h activity summary from indexed events
- 24h quote-level liquidity flow metrics
- 24h claim totals
- Reserve update cadence metrics
- Default ranked pool ordering via a composite score
- Pool detail page with:
  - score-adjacent metrics
  - event mix
  - typed recent event tables
  - raw recent activity feed
- Indexer status endpoint and UI visibility

## Current Composite Score

The current list default sort is a pragmatic MVP formula, not a final production ranking model.

Inputs:

- 24h fee/TVL
- 24h volume efficiency (`volume / tvl`)
- 24h net liquidity delta ratio (`net_liquidity_delta / tvl`)
- 24h reserve-update cadence

The API now returns:

- `score`
- `score_breakdown`

This makes the ranking explainable in demos and grant conversations.

## What Is Good Enough Now

Good enough for now:

- Demoing that LPAgent can rank Aquarius pools using live indexed data
- Showing richer-than-snapshot pool analytics
- Explaining how event indexing powers fee, volume, activity, liquidity-flow, and cadence views
- Continuing to accumulate recent history without waiting for full archive-grade backfill

## Main Gaps

Still missing for a more mature product:

- Longer historical coverage
- More complete pool coverage beyond the currently surfaced set
- A more rigorously tuned ranking model
- Better token labeling / symbol resolution
- More explicit user-facing methodology notes
- Historical trend views for liquidity flow, claims, and cadence

## Recommended Next Step

Do not keep expanding UI blindly.

Better next step:

1. Freeze this MVP surface.
2. Add a short methodology / metrics doc for external readers.
3. Do one pass of cleanup on copy, naming, and score explanation.
4. Use that version for demo, grant narrative, and user feedback.
