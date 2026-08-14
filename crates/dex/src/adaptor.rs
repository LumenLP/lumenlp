//! Multi-DEX LP adaptor surface.
//!
//! Strategies and copy-runtime should depend on [`DexAdaptor`] + shared types,
//! not on Aquarius-specific modules. Aquarius is the reference implementation;
//! other venues start as scaffolds until production adaptors land.

use serde::{Deserialize, Serialize};

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
            copy_scale: true,
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

/// Venue-agnostic draft for a single LP action (amounts still venue-encoded JSON).
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
/// Heavy RPC work stays in venue modules (`dex::aquarius`, `pool-indexer`, …). The trait is the **stable
/// identity + capability surface** strategies bind to. Production hydrate /
/// index paths for Aquarius live under `dex::aquarius::{pool,router}` and
/// `pool-indexer`; they must be reachable via [`VenueId::Aquarius`] without
/// leaking Aquarius types into strategy configs.
pub trait DexAdaptor: Send + Sync {
    fn venue_id(&self) -> VenueId;
    fn name(&self) -> &'static str;
    fn status(&self) -> VenueStatus;
    fn capabilities(&self) -> VenueCapabilities;
    fn notes(&self) -> &'static str {
        ""
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

/// Placeholder until a venue's production adaptor ships.
#[derive(Debug, Clone, Copy)]
pub struct ScaffoldAdaptor {
    pub venue_id: VenueId,
    pub name: &'static str,
    pub notes: &'static str,
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
        Box::new(ScaffoldAdaptor {
            venue_id: VenueId::SushiV3,
            name: "Sushi V3",
            notes: "Scaffold. CL events/draft planned for Tranche 2.",
        }),
        Box::new(ScaffoldAdaptor {
            venue_id: VenueId::Phoenix,
            name: "Phoenix",
            notes: "Scaffold. CP AMM read+draft planned for Tranche 2.",
        }),
        Box::new(ScaffoldAdaptor {
            venue_id: VenueId::SoroswapAmm,
            name: "Soroswap AMM",
            notes: "Scaffold. AMM only (not aggregator).",
        }),
        Box::new(ScaffoldAdaptor {
            venue_id: VenueId::Comet,
            name: "Comet",
            notes: "Scaffold. Weighted pools; mainnet gate if liquidity thin.",
        }),
        Box::new(ScaffoldAdaptor {
            venue_id: VenueId::Classic,
            name: "Stellar Classic DEX",
            notes: "Deferred / ADR. Different LP model than Soroban AMMs.",
        }),
    ]
}

pub fn support_matrix() -> Vec<VenueSupportRow> {
    default_venue_registry()
        .iter()
        .map(|a| a.support_row())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let prod: Vec<_> = matrix
            .iter()
            .filter(|r| r.status == VenueStatus::Production)
            .collect();
        assert_eq!(prod.len(), 1);
        assert_eq!(prod[0].venue_id, VenueId::Aquarius);
        assert!(prod[0].capabilities.copy_scale);
    }

    #[test]
    fn five_lp_venues_plus_classic_listed() {
        assert_eq!(support_matrix().len(), 6);
    }
}
