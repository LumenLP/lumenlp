//! Multi-DEX LP adaptor surface.
//!
//! Strategies and copy-runtime should depend on [`DexAdaptor`] + shared types,
//! not on Aquarius-specific modules. Aquarius is the reference implementation;
//! other venues may expose read/indexed analytics before Copy LP execution is
//! enabled.

use {
    crate::types::{PoolType, SharePoolState, UserPosition},
    anyhow::{anyhow, Result},
    serde::{Deserialize, Serialize},
};

/// Stable venue identifier used in APIs, configs, and copy sessions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VenueId {
    Aquarius,
    SushiV3,
    Phoenix,
    SoroswapAmm,
    Comet,
    Classic,
}

impl VenueId {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Aquarius => "aquarius",
            Self::SushiV3 => "sushi_v3",
            Self::Phoenix => "phoenix",
            Self::SoroswapAmm => "soroswap_amm",
            Self::Comet => "comet",
            Self::Classic => "classic",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "aquarius" => Some(Self::Aquarius),
            "sushi_v3" | "sushi" => Some(Self::SushiV3),
            "phoenix" => Some(Self::Phoenix),
            "soroswap_amm" | "soroswap" => Some(Self::SoroswapAmm),
            "comet" => Some(Self::Comet),
            "classic" | "sdex" => Some(Self::Classic),
            _ => None,
        }
    }
}

/// What a venue adaptor currently supports (support matrix row).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct VenueCapabilities {
    pub list_pools: bool,
    pub positions: bool,
    pub liquidity_events: bool,
    pub quotes: bool,
    /// Build unsigned / draft LP actions (deposit, withdraw, CL adjust, …).
    pub draft_ops: bool,
    pub deposit: bool,
    pub withdraw: bool,
    pub claim: bool,
    pub copy_scale: bool,
}

impl VenueCapabilities {
    pub const fn empty() -> Self {
        Self {
            list_pools: false,
            positions: false,
            liquidity_events: false,
            quotes: false,
            draft_ops: false,
            deposit: false,
            withdraw: false,
            claim: false,
            copy_scale: false,
        }
    }

    pub const fn aquarius_production() -> Self {
        Self {
            list_pools: true,
            positions: true,
            liquidity_events: true,
            quotes: true,
            draft_ops: true,
            deposit: true,
            withdraw: true,
            claim: true,
            copy_scale: true,
        }
    }

    pub const fn indexed_analytics(liquidity_events: bool) -> Self {
        Self {
            list_pools: true,
            positions: false,
            liquidity_events,
            quotes: true,
            draft_ops: false,
            deposit: false,
            withdraw: false,
            claim: false,
            copy_scale: false,
        }
    }

    pub const fn supports(self, kind: DraftOpKind) -> bool {
        match kind {
            DraftOpKind::Deposit => self.deposit,
            DraftOpKind::Withdraw => self.withdraw,
            DraftOpKind::Claim => self.claim,
            DraftOpKind::OpenRange | DraftOpKind::CloseRange | DraftOpKind::AdjustRange => self.draft_ops,
        }
    }
}

/// Normalized LP action kind emitted by strategies / copy runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DraftOpKind {
    Deposit,
    Withdraw,
    Claim,
    OpenRange,
    CloseRange,
    AdjustRange,
}

/// Venue-agnostic draft for a single LP action (amounts still venue-encoded
/// JSON).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DraftOp {
    pub venue_id: VenueId,
    pub pool_address: String,
    pub kind: DraftOpKind,
    pub position_key: String,
    /// Opaque venue-specific payload (token amounts, ticks, …).
    pub amounts: serde_json::Value,
    pub quote_xlm: Option<f64>,
}

/// Venue-neutral pool metadata passed from a venue reader to strategy code.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PoolDescriptor {
    pub venue_id: VenueId,
    pub address: String,
    pub pool_type: PoolType,
    pub tokens: Vec<String>,
    pub fee_bps: u32,
}

/// Venue-neutral LP position view. Amounts remain in token base units.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PositionDescriptor {
    pub venue_id: VenueId,
    pub pool_address: String,
    pub position_key: String,
    pub tokens: Vec<String>,
    pub amounts: Vec<f64>,
    pub quote_xlm: Option<f64>,
    pub status: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LiquidityEventKind {
    Deposit,
    Withdraw,
    Claim,
    RangeOpen,
    RangeClose,
    RangeAdjust,
}

/// Normalized observable LP event. Venue-specific payload stays opaque.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LiquidityEvent {
    pub venue_id: VenueId,
    pub event_id: String,
    pub tx_hash: Option<String>,
    pub ledger: u32,
    pub pool_address: String,
    pub actor: Option<String>,
    pub kind: LiquidityEventKind,
    pub payload: serde_json::Value,
    pub quote_xlm: Option<f64>,
}

/// Input to the common draft builder. Adapters encode `payload` for their
/// venue.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DraftRequest {
    pub pool_address: String,
    pub kind: DraftOpKind,
    pub position_key: String,
    pub payload: serde_json::Value,
    pub quote_xlm: Option<f64>,
}

/// Status row for the public multi-DEX support matrix.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VenueSupportRow {
    pub venue_id: VenueId,
    pub name: &'static str,
    pub status: VenueStatus,
    pub capabilities: VenueCapabilities,
    pub notes: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VenueStatus {
    Production,
    Scaffold,
    Deferred,
}

/// Multi-DEX LP adaptor contract.
///
/// Heavy RPC work stays in venue modules (`dex::aquarius`, `pool-indexer`, …).
/// The trait is the **stable identity + capability surface** strategies bind
/// to. Production hydrate / index paths for Aquarius live under
/// `dex::aquarius::{pool,router}` and `pool-indexer`; they must be reachable
/// via [`VenueId::Aquarius`] without leaking Aquarius types into strategy
/// configs.
pub trait DexAdaptor: Send + Sync {
    fn venue_id(&self) -> VenueId;
    fn name(&self) -> &'static str;
    fn status(&self) -> VenueStatus;
    fn capabilities(&self) -> VenueCapabilities;
    fn notes(&self) -> &'static str {
        ""
    }

    /// Convert a venue-native pool state into the strategy-facing shape.
    fn normalize_pool(&self, state: &SharePoolState) -> PoolDescriptor {
        PoolDescriptor {
            venue_id: self.venue_id(),
            address: state.address.clone(),
            pool_type: state.pool_type,
            tokens: state.tokens.clone(),
            fee_bps: state.fee_bps,
        }
    }

    /// Convert an existing venue position into the strategy-facing shape.
    fn normalize_position(&self, position: &UserPosition) -> PositionDescriptor {
        PositionDescriptor {
            venue_id: self.venue_id(),
            pool_address: position.pool_address.clone(),
            position_key: position
                .shares
                .map(|shares| format!("shares:{shares}"))
                .unwrap_or_else(|| "position:unknown".into()),
            tokens: position.tokens.clone(),
            amounts: position.amounts.clone(),
            quote_xlm: position.value_quote,
            status: position.status.clone(),
        }
    }

    /// Validate and normalize an event emitted by a venue-specific parser.
    fn normalize_event(&self, event: LiquidityEvent) -> Result<LiquidityEvent> {
        if event.venue_id != self.venue_id() {
            return Err(anyhow!(
                "event venue mismatch: expected {}, got {}",
                self.venue_id().as_str(),
                event.venue_id.as_str()
            ));
        }
        if event.event_id.is_empty() || event.pool_address.is_empty() {
            return Err(anyhow!("liquidity event requires event_id and pool_address"));
        }
        Ok(event)
    }

    /// Build a normalized unsigned operation or fail closed when unsupported.
    fn build_draft_op(&self, request: DraftRequest) -> Result<DraftOp> {
        if request.pool_address.is_empty() || request.position_key.is_empty() {
            return Err(anyhow!("draft operation requires pool_address and position_key"));
        }
        if !self.capabilities().supports(request.kind) {
            return Err(anyhow!(
                "{} does not support {}",
                self.name(),
                serde_json::to_string(&request.kind).unwrap_or_else(|_| "operation".into())
            ));
        }
        Ok(DraftOp {
            venue_id: self.venue_id(),
            pool_address: request.pool_address,
            kind: request.kind,
            position_key: request.position_key,
            amounts: request.payload,
            quote_xlm: request.quote_xlm,
        })
    }

    fn support_row(&self) -> VenueSupportRow {
        VenueSupportRow {
            venue_id: self.venue_id(),
            name: self.name(),
            status: self.status(),
            capabilities: self.capabilities(),
            notes: self.notes(),
        }
    }
}

/// Reference production adaptor — Aquarius (CP + CL).
#[derive(Debug, Default, Clone, Copy)]
pub struct AquariusAdaptor;

impl DexAdaptor for AquariusAdaptor {
    fn venue_id(&self) -> VenueId {
        VenueId::Aquarius
    }

    fn name(&self) -> &'static str {
        "Aquarius"
    }

    fn status(&self) -> VenueStatus {
        VenueStatus::Production
    }

    fn capabilities(&self) -> VenueCapabilities {
        VenueCapabilities::aquarius_production()
    }

    fn notes(&self) -> &'static str {
        "Reference impl. Pools via router; events via pool-indexer; copy-scale live."
    }
}

/// Placeholder for Copy LP execution until a venue's production adaptor ships.
#[derive(Debug, Clone, Copy)]
pub struct ScaffoldAdaptor {
    pub venue_id: VenueId,
    pub name: &'static str,
    pub notes: &'static str,
}

/// Read/indexed venue surface whose Copy LP operations remain disabled.
#[derive(Debug, Clone, Copy)]
pub struct IndexedAnalyticsAdaptor {
    pub venue_id: VenueId,
    pub name: &'static str,
    pub capabilities: VenueCapabilities,
    pub notes: &'static str,
}

impl DexAdaptor for IndexedAnalyticsAdaptor {
    fn venue_id(&self) -> VenueId {
        self.venue_id
    }

    fn name(&self) -> &'static str {
        self.name
    }

    fn status(&self) -> VenueStatus {
        VenueStatus::Scaffold
    }

    fn capabilities(&self) -> VenueCapabilities {
        self.capabilities
    }

    fn notes(&self) -> &'static str {
        self.notes
    }
}

impl DexAdaptor for ScaffoldAdaptor {
    fn venue_id(&self) -> VenueId {
        self.venue_id
    }

    fn name(&self) -> &'static str {
        self.name
    }

    fn status(&self) -> VenueStatus {
        VenueStatus::Scaffold
    }

    fn capabilities(&self) -> VenueCapabilities {
        VenueCapabilities::empty()
    }

    fn notes(&self) -> &'static str {
        self.notes
    }
}

/// Built-in registry used by docs / API support matrix.
pub fn default_venue_registry() -> Vec<Box<dyn DexAdaptor>> {
    vec![
        Box::new(AquariusAdaptor),
        Box::new(IndexedAnalyticsAdaptor {
            venue_id: VenueId::SushiV3,
            name: "Sushi V3",
            capabilities: VenueCapabilities::indexed_analytics(true),
            notes: "Read-only indexed CLMM analytics and LP lifecycle events; Copy LP execution is fail-closed.",
        }),
        Box::new(IndexedAnalyticsAdaptor {
            venue_id: VenueId::Phoenix,
            name: "Phoenix",
            capabilities: VenueCapabilities::indexed_analytics(true),
            notes: "Read-only indexed AMM analytics and liquidity events; Copy LP execution is fail-closed.",
        }),
        Box::new(IndexedAnalyticsAdaptor {
            venue_id: VenueId::SoroswapAmm,
            name: "Soroswap AMM",
            capabilities: VenueCapabilities::indexed_analytics(true),
            notes: "Read-only indexed AMM analytics and liquidity events; Copy LP execution is fail-closed.",
        }),
        Box::new(IndexedAnalyticsAdaptor {
            venue_id: VenueId::Comet,
            name: "Comet",
            capabilities: VenueCapabilities::indexed_analytics(true),
            notes: "Read-only indexed weighted-pool analytics; Copy LP execution is fail-closed.",
        }),
    ]
}

pub fn support_matrix() -> Vec<VenueSupportRow> {
    default_venue_registry().iter().map(|a| a.support_row()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Deserialize)]
    struct ContractFixture {
        venue_id: VenueId,
        pool_address: String,
        pool_type: PoolType,
        tokens: Vec<String>,
        reserves: Vec<u128>,
        fee_bps: u32,
        total_shares: u128,
        share_token: Option<String>,
        operations: Vec<DraftOpKind>,
    }

    #[test]
    fn venue_id_roundtrip() {
        for id in [
            VenueId::Aquarius,
            VenueId::SushiV3,
            VenueId::Phoenix,
            VenueId::SoroswapAmm,
            VenueId::Comet,
            VenueId::Classic,
        ] {
            assert_eq!(VenueId::parse(id.as_str()), Some(id));
        }
        assert_eq!(VenueId::parse("sushi"), Some(VenueId::SushiV3));
    }

    #[test]
    fn aquarius_is_only_production_in_default_registry() {
        let matrix = support_matrix();
        let prod: Vec<_> = matrix.iter().filter(|r| r.status == VenueStatus::Production).collect();
        assert_eq!(prod.len(), 1);
        assert_eq!(prod[0].venue_id, VenueId::Aquarius);
        assert!(prod[0].capabilities.copy_scale);
    }

    #[test]
    fn five_target_lp_venues_listed() {
        assert_eq!(support_matrix().len(), 5);
        assert!(support_matrix().iter().all(|row| row.venue_id != VenueId::Classic));
    }

    #[test]
    fn capabilities_are_explicit_for_copy_actions() {
        let capabilities = VenueCapabilities::aquarius_production();
        assert!(capabilities.supports(DraftOpKind::Deposit));
        assert!(capabilities.supports(DraftOpKind::Withdraw));
        assert!(capabilities.supports(DraftOpKind::Claim));
        assert!(capabilities.supports(DraftOpKind::AdjustRange));
        assert!(!VenueCapabilities::empty().supports(DraftOpKind::Deposit));
    }

    #[test]
    fn every_registered_venue_normalizes_shared_pool_fixture() {
        let fixture = SharePoolState {
            address: "CPOOL".into(),
            pool_type: PoolType::ConstantProduct,
            tokens: vec!["CTOKEN0".into(), "CTOKEN1".into()],
            reserves: vec![1_000, 2_000],
            fee_bps: 30,
            total_shares: 1_000,
            share_token: Some("CSHARE".into()),
            amp: None,
        };

        for adaptor in default_venue_registry() {
            let normalized = adaptor.normalize_pool(&fixture);
            assert_eq!(normalized.venue_id, adaptor.venue_id());
            assert_eq!(normalized.address, "CPOOL");
            assert_eq!(normalized.tokens.len(), 2);
        }
    }

    #[test]
    fn scaffold_adaptors_fail_closed_for_copy_operations() {
        let request = DraftRequest {
            pool_address: "CPOOL".into(),
            kind: DraftOpKind::Deposit,
            position_key: "shares:1".into(),
            payload: serde_json::json!({"amounts": [10, 20]}),
            quote_xlm: Some(1.0),
        };

        for adaptor in default_venue_registry() {
            let result = adaptor.build_draft_op(request.clone());
            if adaptor.venue_id() == VenueId::Aquarius {
                assert!(result.is_ok());
            } else {
                assert!(result.is_err(), "{} must fail closed", adaptor.name());
            }
        }
    }

    #[test]
    fn contract_fixtures_match_shared_pool_boundary() {
        let fixtures = [
            include_str!("../fixtures/aquarius-pool.json"),
            include_str!("../fixtures/phoenix-pool.json"),
        ];

        for raw in fixtures {
            let fixture: ContractFixture = serde_json::from_str(raw).unwrap();
            assert!(!fixture.pool_address.is_empty());
            assert_eq!(fixture.tokens.len(), fixture.reserves.len());
            assert!(fixture.fee_bps <= 10_000);
            assert!(fixture.total_shares > 0);
            assert!(fixture.pool_type != PoolType::Unknown);
            assert!(fixture
                .operations
                .iter()
                .all(|kind| matches!(kind, DraftOpKind::Deposit | DraftOpKind::Withdraw | DraftOpKind::Claim)));
            if fixture.venue_id == VenueId::Aquarius {
                assert_eq!(fixture.share_token.as_deref(), Some("CAQUARIUSSHARE"));
            }
        }
    }
}
