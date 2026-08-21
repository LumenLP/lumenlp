//! XLM price book helpers for Aquarius pools.

use {
    crate::{types::SharePoolState, PoolType, NATIVE_SAC},
    metrics::{build_xlm_price_book, PriceBook},
};

/// Build price book from hydrated pool states.
/// Skip concentrated pools — their reserve ratios are not spot prices.
pub fn price_book_from_pools(pools: &[SharePoolState]) -> PriceBook {
    let edges: Vec<(Vec<String>, Vec<u128>)> = pools
        .iter()
        .filter(|p| p.pool_type != PoolType::Concentrated)
        .map(|p| (p.tokens.clone(), p.reserves.clone()))
        .collect();
    build_xlm_price_book(NATIVE_SAC, &edges)
}

/// Prices for a token list; missing → None entries.
pub fn prices_xlm(book: &PriceBook, tokens: &[String]) -> Vec<Option<f64>> {
    book.prices_for(tokens)
}

/// Prefer full price vector; if incomplete, still return partial with 0 for
/// unknown only when `allow_zero_missing` (TVL undershoot vs inventing 1e-7).
pub fn prices_or_none(book: &PriceBook, tokens: &[String]) -> Option<Vec<f64>> {
    book.required(tokens)
}
