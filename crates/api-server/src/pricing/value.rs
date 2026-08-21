use std::collections::HashMap;

/// Prices keyed by contract address (C…) → USD per human token unit.
pub type UsdPriceMap = HashMap<String, f64>;

pub fn amount_to_usd(human_amount: f64, usd_price: f64) -> Option<f64> {
    if !human_amount.is_finite() || !usd_price.is_finite() || usd_price <= 0.0 {
        return None;
    }
    Some(human_amount * usd_price)
}

/// Sum reserve_i * price_i. Returns None if any listed token lacks a price.
/// Handlers currently bridge TVL via XLM until human reserves + decimals are
/// available.
#[allow(dead_code)]
pub fn tvl_usd(tokens: &[String], human_reserves: &[f64], prices: &UsdPriceMap) -> Option<f64> {
    if tokens.len() != human_reserves.len() || tokens.is_empty() {
        return None;
    }
    let mut sum = 0.0;
    for (token, amt) in tokens.iter().zip(human_reserves.iter()) {
        let price = prices.get(token).copied().filter(|p| p.is_finite() && *p > 0.0)?;
        if !amt.is_finite() {
            return None;
        }
        sum += amt * price;
    }
    Some(sum)
}

pub fn xlm_quote_to_usd(xlm_amount: f64, xlm_usd: f64) -> Option<f64> {
    amount_to_usd(xlm_amount, xlm_usd)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuoteCoverage {
    Full,
    Partial,
    None,
}

pub fn coverage_for(tokens: &[String], prices: &UsdPriceMap) -> QuoteCoverage {
    if tokens.is_empty() {
        return QuoteCoverage::None;
    }
    let priced = tokens
        .iter()
        .filter(|t| prices.get(t.as_str()).is_some_and(|p| p.is_finite() && *p > 0.0))
        .count();
    if priced == 0 {
        QuoteCoverage::None
    } else if priced == tokens.len() {
        QuoteCoverage::Full
    } else {
        QuoteCoverage::Partial
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn amount_to_usd_basic() {
        assert!((amount_to_usd(100.0, 0.5).unwrap() - 50.0).abs() < 1e-9);
    }

    #[test]
    fn tvl_requires_all_legs() {
        let mut prices = UsdPriceMap::new();
        prices.insert("A".into(), 1.0);
        assert!(tvl_usd(&["A".into(), "B".into()], &[10.0, 20.0], &prices).is_none());
        prices.insert("B".into(), 2.0);
        assert!((tvl_usd(&["A".into(), "B".into()], &[10.0, 20.0], &prices).unwrap() - 50.0).abs() < 1e-9);
    }

    #[test]
    fn bridge_xlm_quote() {
        assert!((xlm_quote_to_usd(10.0, 0.2).unwrap() - 2.0).abs() < 1e-9);
    }

    #[test]
    fn coverage_levels() {
        let mut prices = UsdPriceMap::new();
        prices.insert("A".into(), 1.0);
        assert_eq!(coverage_for(&["A".into(), "B".into()], &prices), QuoteCoverage::Partial);
        prices.insert("B".into(), 1.0);
        assert_eq!(coverage_for(&["A".into(), "B".into()], &prices), QuoteCoverage::Full);
        assert_eq!(coverage_for(&["C".into()], &UsdPriceMap::new()), QuoteCoverage::None);
    }
}
