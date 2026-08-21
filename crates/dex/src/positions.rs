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
    metrics::{cp_position_amounts, value_xlm},
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
    let mut out = Vec::new();
    for pool in pools {
        let state = match venue {
            "phoenix" => phoenix::hydrate_pool(rpc, pool).await,
            "soroswap" => soroswap::hydrate_pool(rpc, pool).await,
            "comet" => crate::comet::hydrate_pool(rpc, pool).await,
            _ => continue,
        };
        let Ok(state) = state else { continue };
        let Some(share_token) = state.share_token.as_deref() else {
            continue;
        };
        if state.total_shares == 0 {
            continue;
        }
        let Ok(shares) = share_balance(rpc, share_token, user).await else {
            continue;
        };
        if shares == 0 {
            continue;
        }
        let (a, b) = if state.reserves.len() >= 2 {
            cp_position_amounts(shares, state.total_shares, state.reserves[0], state.reserves[1])
        } else {
            (0.0, 0.0)
        };
        let amounts = vec![a, b];
        let value_quote = book
            .required(&state.tokens)
            .as_ref()
            .and_then(|p| value_xlm(&amounts, p));
        out.push(UserPosition {
            pool_address: state.address,
            venue: venue.to_owned(),
            pool_type: state.pool_type,
            tokens: state.tokens,
            fee_bps: state.fee_bps,
            amounts,
            value_quote,
            il_est: None,
            pnl: None,
            fees_unclaimed_quote: None,
            status: "ok".into(),
            shares: Some(shares),
            cl_ranges: None,
            note: Some("LP share balance verified against the venue pool; il/pnl=n/a without cost basis".into()),
        });
    }
    out
}
