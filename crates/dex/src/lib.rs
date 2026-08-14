//! Multi-DEX LP clients for Stellar (Soroban RPC-first).
//!
//! - Shared: [`rpc`], [`types`], [`adaptor`], snapshot [`db`]
//! - Venues: [`aquarius`] (production), [`sushi`], [`phoenix`], [`soroswap`], [`comet`] (scaffolds)

pub mod adaptor;
pub mod aquarius;
pub mod comet;
pub mod db;
pub mod phoenix;
pub mod rpc;
pub mod soroswap;
pub mod sushi;
pub mod types;

pub use adaptor::{
    support_matrix, AquariusAdaptor, DexAdaptor, DraftOp, DraftOpKind, ScaffoldAdaptor, VenueId,
    VenueCapabilities, VenueStatus, VenueSupportRow,
};
pub use aquarius::AQUARIUS_ROUTER;
pub use rpc::SorobanRpc;
pub use types::{PoolType, SharePoolState};

/// Mainnet native XLM SAC (Stellar Asset Contract).
pub const NATIVE_SAC: &str = "CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA";

pub const MAINNET_PASSPHRASE: &str = "Public Global Stellar Network ; September 2015";
