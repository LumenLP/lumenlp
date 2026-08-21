//! Build token→XLM prices from Aquarius pool reserves (RPC-derived).

use std::collections::HashMap;

/// XLM per **base unit** of a token (e.g. native stroop → `1e-7`).
#[derive(Debug, Clone, Default)]
pub struct PriceBook {
    /// token contract id → XLM per base unit
    prices: HashMap<String, f64>,
}

impl PriceBook {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, token: impl Into<String>, xlm_per_base: f64) {
        if xlm_per_base > 0.0 && xlm_per_base.is_finite() {
            self.prices.insert(token.into(), xlm_per_base);
        }
    }

    pub fn get(&self, token: &str) -> Option<f64> {
        self.prices.get(token).copied()
    }

    pub fn prices_for(&self, tokens: &[String]) -> Vec<Option<f64>> {
        tokens.iter().map(|t| self.get(t)).collect()
    }

    /// Resolve prices; missing tokens → `None` (caller decides fallback).
    pub fn required(&self, tokens: &[String]) -> Option<Vec<f64>> {
        let mut out = Vec::with_capacity(tokens.len());
        for t in tokens {
            out.push(self.get(t)?);
        }
        Some(out)
    }
}

/// Seed native SAC at 1 stroop = 1e-7 XLM, then derive others from 2-token
/// pools that include `native_sac`.
///
/// Spot: for reserves (r_native, r_token), 1 token base buys
/// `r_native / r_token` native base → XLM = that × 1e-7.
///
/// When multiple pools quote the same token (e.g. CLMM vs constant-product
/// XLM/USDC), keep the **median** candidate so skewed concentrated-liquidity
/// reserve ratios do not dominate volume valuation.
pub fn build_xlm_price_book(native_sac: &str, pools: &[(Vec<String>, Vec<u128>)]) -> PriceBook {
    let mut book = PriceBook::new();
    book.insert(native_sac, 1e-7);

    let mut candidates: HashMap<String, Vec<f64>> = HashMap::new();
    // Pass 1: direct native pairs
    for (tokens, reserves) in pools {
        if tokens.len() != 2 || reserves.len() != 2 {
            continue;
        }
        let (t0, t1) = (&tokens[0], &tokens[1]);
        let (r0, r1) = (reserves[0], reserves[1]);
        if r0 == 0 || r1 == 0 {
            continue;
        }
        if t0 == native_sac {
            candidates
                .entry(t1.clone())
                .or_default()
                .push((r0 as f64 / r1 as f64) * 1e-7);
        } else if t1 == native_sac {
            candidates
                .entry(t0.clone())
                .or_default()
                .push((r1 as f64 / r0 as f64) * 1e-7);
        }
    }
    for (token, mut prices) in candidates {
        if let Some(px) = median_f64(&mut prices) {
            book.insert(token, px);
        }
    }

    // Pass 2: one hop via a priced token (e.g. USDC priced, then FOO/USDC)
    for (tokens, reserves) in pools {
        if tokens.len() != 2 || reserves.len() != 2 {
            continue;
        }
        let (t0, t1) = (&tokens[0], &tokens[1]);
        let (r0, r1) = (reserves[0], reserves[1]);
        if r0 == 0 || r1 == 0 {
            continue;
        }
        if book.get(t0).is_some() && book.get(t1).is_none() {
            if let Some(p0) = book.get(t0) {
                // 1 t1 base = (r0/r1) t0 base
                book.insert(t1, (r0 as f64 / r1 as f64) * p0);
            }
        } else if book.get(t1).is_some() && book.get(t0).is_none() {
            if let Some(p1) = book.get(t1) {
                book.insert(t0, (r1 as f64 / r0 as f64) * p1);
            }
        }
    }

    book
}

fn median_f64(values: &mut [f64]) -> Option<f64> {
    let finite: Vec<f64> = values.iter().copied().filter(|v| v.is_finite() && *v > 0.0).collect();
    if finite.is_empty() {
        return None;
    }
    let mut finite = finite;
    finite.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mid = finite.len() / 2;
    Some(if finite.len() % 2 == 0 {
        (finite[mid - 1] + finite[mid]) / 2.0
    } else {
        finite[mid]
    })
}

/// Value in XLM; returns None if any price missing.
pub fn value_xlm(amounts: &[f64], prices: &[f64]) -> Option<f64> {
    if amounts.len() != prices.len() || amounts.is_empty() {
        return None;
    }
    Some(amounts.iter().zip(prices.iter()).map(|(a, p)| a * p).sum())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_pair_prices_token() {
        let native = "NATIVE";
        let usdc = "USDC";
        // 100 XLM (1e9 stroops) / 50 USDC (5e8 base @7dec) → 2 stroops per USDC base
        let pools = vec![(
            vec![native.into(), usdc.into()],
            vec![1_000_000_000u128, 500_000_000u128],
        )];
        let book = build_xlm_price_book(native, &pools);
        assert!((book.get(native).unwrap() - 1e-7).abs() < 1e-18);
        let p = book.get(usdc).unwrap();
        // (1e9/5e8)*1e-7 = 2e-7
        assert!((p - 2e-7).abs() < 1e-18);
    }

    #[test]
    fn one_hop_prices_second_token() {
        let native = "NATIVE";
        let usdc = "USDC";
        let foo = "FOO";
        let pools = vec![
            (
                vec![native.into(), usdc.into()],
                vec![1_000_000_000u128, 1_000_000_000u128],
            ),
            (
                vec![usdc.into(), foo.into()],
                vec![1_000_000_000u128, 2_000_000_000u128],
            ),
        ];
        let book = build_xlm_price_book(native, &pools);
        assert!(book.get(foo).is_some());
        // USDC = 1e-7 XLM/base; FOO = (1e9/2e9)*1e-7 = 0.5e-7
        assert!((book.get(foo).unwrap() - 0.5e-7).abs() < 1e-18);
    }
}
