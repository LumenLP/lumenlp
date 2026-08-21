//! Concentrated liquidity amount math (Uniswap v3-style, f64 for analytics).

/// `sqrt(price)` where price = token1/token0 = 1.0001^tick.
pub fn tick_to_sqrt_price(tick: i32) -> f64 {
    1.0001f64.powf(f64::from(tick) / 2.0)
}

/// Token amounts (base units) for a CL position at `tick_current`.
///
/// Matches Uniswap v3 geometry with real sqrt prices (not Q96 encoding):
/// - below range: all token0
/// - above range: all token1
/// - in range: mix of both
pub fn cl_position_amounts(liquidity: u128, tick_lower: i32, tick_upper: i32, tick_current: i32) -> (f64, f64) {
    if liquidity == 0 || tick_lower >= tick_upper {
        return (0.0, 0.0);
    }
    let sa = tick_to_sqrt_price(tick_lower);
    let sb = tick_to_sqrt_price(tick_upper);
    let sc = tick_to_sqrt_price(tick_current);
    let l = liquidity as f64;

    if tick_current < tick_lower {
        // amount0 = L * (1/sa - 1/sb)
        let amount0 = l * (sb - sa) / (sa * sb);
        (amount0, 0.0)
    } else if tick_current >= tick_upper {
        let amount1 = l * (sb - sa);
        (0.0, amount1)
    } else {
        let amount0 = l * (1.0 / sc - 1.0 / sb);
        let amount1 = l * (sc - sa);
        (amount0, amount1)
    }
}

/// Sum amounts across several CL ranges (same pool / current tick).
pub fn cl_ranges_amounts(ranges: &[(u128, i32, i32)], tick_current: i32) -> (f64, f64) {
    let mut a0 = 0.0;
    let mut a1 = 0.0;
    for &(liq, lo, hi) in ranges {
        let (x, y) = cl_position_amounts(liq, lo, hi, tick_current);
        a0 += x;
        a1 += y;
    }
    (a0, a1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn below_range_is_only_token0() {
        let (a0, a1) = cl_position_amounts(1_000_000_000_000, -1000, -500, -2000);
        assert!(a0 > 0.0);
        assert_eq!(a1, 0.0);
    }

    #[test]
    fn above_range_is_only_token1() {
        let (a0, a1) = cl_position_amounts(1_000_000_000_000, -1000, -500, 0);
        assert_eq!(a0, 0.0);
        assert!(a1 > 0.0);
    }

    #[test]
    fn in_range_has_both() {
        let (a0, a1) = cl_position_amounts(1_000_000_000_000, -1000, 1000, 0);
        assert!(a0 > 0.0 && a1 > 0.0);
    }

    #[test]
    fn zero_liquidity() {
        assert_eq!(cl_position_amounts(0, -10, 10, 0), (0.0, 0.0));
    }
}
