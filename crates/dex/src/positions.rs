//! Venue-aware LP position readers.
//!
//! Pool addresses come from indexed liquidity events. They must be routed to
//! the adapter that owns the pool ABI; probing every address with Aquarius
//! methods creates false `unknown` positions for other DEXes.

use {
    crate::{
        aquarius::{pool::share_balance, positions::positions_for_address},
        phoenix, soroswap,
        types::{SharePoolState, UserPosition},
        SorobanRpc,
    },
    metrics::value_xlm,
    std::sync::Arc,
    tokio::task::JoinSet,
};

pub async fn positions_for_venue(
    rpc: &SorobanRpc,
    user: &str,
    venue: &str,
    pools: &[String],
    pricing: &[SharePoolState],
) -> Vec<UserPosition> {
    match venue {
        "aquarius" => positions_for_address(rpc, user, pools, pricing).await,
        "phoenix" => share_positions(rpc, user, pools, pricing, "phoenix").await,
        "soroswap" | "soroswap_amm" => share_positions(rpc, user, pools, pricing, "soroswap").await,
        "comet" => share_positions(rpc, user, pools, pricing, "comet").await,
        // Sushi V3 has no enumerable owner position list. Its event-derived
        // range reader is called separately by the API after this dispatch.
        "sushi" | "sushi_v3" => Vec::new(),
        _ => Vec::new(),
    }
}

async fn share_positions(
    rpc: &SorobanRpc,
    user: &str,
    pools: &[String],
    pricing: &[SharePoolState],
    venue: &str,
) -> Vec<UserPosition> {
    let book = crate::aquarius::pricing::price_book_from_pools(pricing);
    let book = Arc::new(book);
    let mut out = Vec::new();
    // These pools are already narrowed by indexed actor activity; use a wider
    // batch to keep profile cold reads responsive across multiple venues.
    for batch in pools.chunks(16) {
        let mut tasks = JoinSet::new();
        for pool in batch {
            let rpc = rpc.clone();
            let user = user.to_owned();
            let pool = pool.clone();
            let venue = venue.to_owned();
            let book = Arc::clone(&book);
            tasks.spawn(async move { load_share_position(rpc, user, pool, venue, book).await });
        }
        while let Some(result) = tasks.join_next().await {
            if let Ok(Some(position)) = result {
                out.push(position);
            }
        }
    }
    out
}

async fn load_share_position(
    rpc: SorobanRpc,
    user: String,
    pool: String,
    venue: String,
    book: Arc<metrics::PriceBook>,
) -> Option<UserPosition> {
    let state = match venue.as_str() {
        "phoenix" => phoenix::hydrate_pool(&rpc, &pool).await,
        "soroswap" => soroswap::hydrate_pool(&rpc, &pool).await,
        "comet" => crate::comet::hydrate_pool(&rpc, &pool).await,
        _ => return None,
    };
    let Ok(state) = state else { return None };
    let Some(share_token) = state.share_token.as_deref() else {
        return None;
    };
    if state.total_shares == 0 {
        return None;
    }
    let Ok(shares) = share_balance(&rpc, share_token, &user).await else {
        return None;
    };
    if shares == 0 {
        return None;
    }
    let amounts = proportional_share_amounts(shares, state.total_shares, &state.reserves);
    let value_quote = book
        .required(&state.tokens)
        .as_ref()
        .and_then(|p| value_xlm(&amounts, p));
    Some(UserPosition {
        pool_address: state.address,
        venue,
        pool_type: state.pool_type,
        tokens: state.tokens,
        fee_bps: state.fee_bps,
        amounts,
        value_quote,
        il_est: None,
        pnl: None,
        fees_unclaimed_quote: None,
        // The share balance and pool value are verified, but these venues do
        // not expose a fee accumulator through the current read boundary.
        // Keep that limitation explicit instead of making null fees look like
        // a successfully measured zero.
        status: "fee_unavailable".into(),
        shares: Some(shares),
        cl_ranges: None,
        note: Some("LP share balance verified against the venue pool; il/pnl=n/a without cost basis".into()),
    })
}

fn proportional_share_amounts(shares: u128, total_shares: u128, reserves: &[u128]) -> Vec<f64> {
    if total_shares == 0 {
        return vec![0.0; reserves.len()];
    }
    reserves
        .iter()
        .map(|reserve| (*reserve as f64) * (shares as f64) / (total_shares as f64))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::proportional_share_amounts;

    #[test]
    fn weighted_positions_preserve_all_token_legs() {
        assert_eq!(
            proportional_share_amounts(250, 1_000, &[100, 200, 300]),
            vec![25.0, 50.0, 75.0]
        );
    }

    #[test]
    fn zero_supply_does_not_create_position_amounts() {
        assert_eq!(proportional_share_amounts(10, 0, &[100, 200]), vec![0.0, 0.0]);
    }
}
