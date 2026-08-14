//! Soroswap AMM adaptor (scaffold). Not the swap aggregator.

use crate::adaptor::{DexAdaptor, ScaffoldAdaptor, VenueId};

pub fn scaffold() -> ScaffoldAdaptor {
    ScaffoldAdaptor {
        venue_id: VenueId::SoroswapAmm,
        name: "Soroswap AMM",
        notes: "Scaffold. AMM only (not aggregator).",
    }
}

pub fn adaptor() -> impl DexAdaptor {
    scaffold()
}
