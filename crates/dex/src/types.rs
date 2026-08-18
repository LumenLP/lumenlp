use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PoolType {
    ConstantProduct,
    Stable,
    Concentrated,
    Weighted,
    Unknown,
}

impl PoolType {
    pub fn parse(s: &str) -> Self {
        match s {
            "constant_product" | "volatile" => Self::ConstantProduct,
            "stable" => Self::Stable,
            "concentrated" => Self::Concentrated,
            "weighted" => Self::Weighted,
            _ => Self::Unknown,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::ConstantProduct => "constant_product",
            Self::Stable => "stable",
            Self::Concentrated => "concentrated",
            Self::Weighted => "weighted",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SharePoolState {
    pub address: String,
    pub pool_type: PoolType,
    pub tokens: Vec<String>,
    pub reserves: Vec<u128>,
    pub fee_bps: u32,
    pub total_shares: u128,
    pub share_token: Option<String>,
    pub amp: Option<u128>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClPositionRange {
    pub tick_lower: i32,
    pub tick_upper: i32,
    pub liquidity: u128,
    pub tokens_owed_0: u128,
    pub tokens_owed_1: u128,
    pub in_range: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserPosition {
    pub pool_address: String,
    pub pool_type: PoolType,
    pub tokens: Vec<String>,
    pub fee_bps: u32,
    /// Underlying token amounts (base units as f64).
    pub amounts: Vec<f64>,
    pub value_quote: Option<f64>,
    pub il_est: Option<f64>,
    pub pnl: Option<f64>,
    pub fees_unclaimed_quote: Option<f64>,
    pub status: String,
    pub shares: Option<u128>,
    pub cl_ranges: Option<Vec<ClPositionRange>>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolSnapshotRow {
    pub pool_address: String,
    pub ts: String,
    pub tvl: f64,
    pub volume_24h: f64,
    pub est_apr: f64,
    pub reserves_json: String,
}
