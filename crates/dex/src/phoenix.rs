//! Phoenix Protocol adaptor (scaffold).

use crate::adaptor::{DexAdaptor, ScaffoldAdaptor, VenueId};

pub fn scaffold() -> ScaffoldAdaptor {
    ScaffoldAdaptor {
        venue_id: VenueId::Phoenix,
        name: "Phoenix",
        notes: "Scaffold. CP AMM read+draft planned.",
    }
}

pub fn adaptor() -> impl DexAdaptor {
    scaffold()
}
