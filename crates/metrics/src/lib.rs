//! Pure LP math — no I/O. Used by snapshotter and api-server.

pub mod clmm;
pub mod pricing;

pub use clmm::{cl_position_amounts, cl_ranges_amounts, tick_to_sqrt_price};
pub use pricing::{build_xlm_price_book, value_xlm, PriceBook};

/// Annualized fee APR from 24h volume and TVL.
/// `fee_bps` e.g. 30 = 0.3%.
pub fn fee_apr_24h(fee_bps: u32, volume_24h: f64, tvl: f64) -> f64 {
    if tvl <= 0.0 || volume_24h <= 0.0 || fee_bps == 0 {
        return 0.0;
    }
    let fee_rate = f64::from(fee_bps) / 10_000.0;
    fee_rate * volume_24h / tvl * 365.0
}

/// User's token amounts from CP/stable share ownership.
pub fn cp_position_amounts(
    user_shares: u128,
    total_shares: u128,
    reserve_a: u128,
    reserve_b: u128,
) -> (f64, f64) {
    if total_shares == 0 || user_shares == 0 {
        return (0.0, 0.0);
    }
    let s = user_shares as f64 / total_shares as f64;
    (s * reserve_a as f64, s * reserve_b as f64)
}

/// Mark-to-market value in quote units.
pub fn position_value(amount_a: f64, amount_b: f64, price_a: f64, price_b: f64) -> f64 {
    amount_a * price_a + amount_b * price_b
}

/// Classic CP IL vs HODL: `v_lp / v_hodl - 1` (negative ⇒ underperformance).
///
/// Given entry amounts `(a0, b0)` and current prices `(price_a, price_b)` in the
/// same quote currency, a constant-product LP that never rebalanced holds value
/// `2 * sqrt(a0 * b0 * price_a * price_b)`.
pub fn cp_il_vs_hodl(amount_a0: f64, amount_b0: f64, price_a: f64, price_b: f64) -> f64 {
    if amount_a0 <= 0.0 || amount_b0 <= 0.0 || price_a <= 0.0 || price_b <= 0.0 {
        return 0.0;
    }
    let hodl = amount_a0 * price_a + amount_b0 * price_b;
    if hodl <= 0.0 {
        return 0.0;
    }
    let lp = 2.0 * (amount_a0 * amount_b0 * price_a * price_b).sqrt();
    lp / hodl - 1.0
}

/// TVL in quote units from reserve amounts and per-token prices.
pub fn tvl_from_reserves(reserves: &[f64], prices: &[f64]) -> f64 {
    reserves.iter().zip(prices.iter()).map(|(r, p)| r * p).sum()
}

/// Unclaimed fees value in quote units.
pub fn fees_value(fee_amounts: &[f64], prices: &[f64]) -> f64 {
    tvl_from_reserves(fee_amounts, prices)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fee_apr_annualizes_24h_volume() {
        let apr = fee_apr_24h(30, 1_000.0, 10_000.0);
        assert!((apr - 0.1095).abs() < 1e-9);
    }

    #[test]
    fn fee_apr_zero_when_tvl_zero() {
        assert_eq!(fee_apr_24h(30, 1_000.0, 0.0), 0.0);
    }

    #[test]
    fn cp_amounts_scale_with_share() {
        let (a, b) = cp_position_amounts(25, 100, 1_000, 2_000);
        assert!((a - 250.0).abs() < 1e-9);
        assert!((b - 500.0).abs() < 1e-9);
    }

    #[test]
    fn cp_il_zero_when_prices_match_entry_ratio() {
        let il = cp_il_vs_hodl(100.0, 200.0, 2.0, 1.0);
        assert!(il.abs() < 1e-9, "il={il}");
    }

    #[test]
    fn cp_il_negative_when_price_moves() {
        let il = cp_il_vs_hodl(100.0, 200.0, 4.0, 1.0);
        assert!(il < 0.0, "il={il}");
    }
}
