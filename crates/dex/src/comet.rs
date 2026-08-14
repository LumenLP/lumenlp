//! Comet weighted AMM adaptor (scaffold).

use crate::adaptor::{DexAdaptor, ScaffoldAdaptor, VenueId};

pub fn scaffold() -> ScaffoldAdaptor {
    ScaffoldAdaptor {
        venue_id: VenueId::Comet,
        name: "Comet",
        notes: "Scaffold. Weighted pools; mainnet gate if liquidity thin.",
    }
}

pub fn adaptor() -> impl DexAdaptor {
    scaffold()
}
