//! Sushi V3 adaptor (scaffold).

use crate::adaptor::{DexAdaptor, ScaffoldAdaptor, VenueId};

pub fn scaffold() -> ScaffoldAdaptor {
    ScaffoldAdaptor {
        venue_id: VenueId::SushiV3,
        name: "Sushi V3",
        notes: "Scaffold. CL events/draft planned.",
    }
}

pub fn adaptor() -> impl DexAdaptor {
    scaffold()
}
