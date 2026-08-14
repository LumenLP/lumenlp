use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PoolEventKind {
    Trade,
    DepositLiquidity,
    WithdrawLiquidity,
    UpdateReserves,
    ClaimFees,
    ClaimProtocolFee,
    ReservesSync,
    Unknown,
}

impl PoolEventKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Trade => "trade",
            Self::DepositLiquidity => "deposit_liquidity",
            Self::WithdrawLiquidity => "withdraw_liquidity",
            Self::UpdateReserves => "update_reserves",
            Self::ClaimFees => "claim_fees",
            Self::ClaimProtocolFee => "claim_protocol_fee",
            Self::ReservesSync => "reserves_sync",
            Self::Unknown => "unknown",
        }
    }

    pub fn parse(name: &str) -> Self {
        match name {
            "trade" => Self::Trade,
            "deposit_liquidity" => Self::DepositLiquidity,
            "withdraw_liquidity" => Self::WithdrawLiquidity,
            "update_reserves" => Self::UpdateReserves,
            "claim_fees" => Self::ClaimFees,
            "claim_protocol_fee" => Self::ClaimProtocolFee,
            "reserves_sync" => Self::ReservesSync,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolEvent {
    pub event_id: String,
    pub tx_hash: Option<String>,
    pub ledger: u32,
    pub created_at: i64,
    pub pool_address: String,
    pub kind: PoolEventKind,
    pub body_json: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolSwap {
    pub tx_hash: String,
    pub event_id: String,
    pub ledger: u32,
    pub created_at: i64,
    pub pool_address: String,
    pub dex: String,
    pub token_in: Option<String>,
    pub token_out: Option<String>,
    pub amount_in: Option<String>,
    pub amount_out: Option<String>,
    pub fee_bps: Option<u32>,
    pub volume_quote: Option<f64>,
    pub fee_quote: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolSnapshot5m {
    pub pool_address: String,
    pub bucket_ts: i64,
    pub tvl: f64,
    pub reserves_json: String,
    pub fee_bps: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolRollup {
    pub pool_address: String,
    pub window: String,
    pub as_of_ts: i64,
    pub sample_count: usize,
    pub volume_quote: f64,
    pub fee_quote: f64,
    pub avg_tvl: f64,
    pub fee_tvl: f64,
    pub tx_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowSpec {
    pub label: &'static str,
    pub seconds: i64,
}

pub const WINDOWS: [WindowSpec; 4] = [
    WindowSpec {
        label: "5m",
        seconds: 5 * 60,
    },
    WindowSpec {
        label: "1h",
        seconds: 60 * 60,
    },
    WindowSpec {
        label: "6h",
        seconds: 6 * 60 * 60,
    },
    WindowSpec {
        label: "24h",
        seconds: 24 * 60 * 60,
    },
];
