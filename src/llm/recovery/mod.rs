//! Opt-in recovery of one immutable completion request, never an agent or tool loop.
mod adapter;
mod capabilities;
mod engine;
mod frames;
mod policy;
mod types;
pub use adapter::{RecoveryGateway, RecoveryProvider};
pub use capabilities::{CapabilityEvidence, CapabilityEvidenceSet};
pub use engine::RecoveryStreamEvent;
pub use policy::*;
pub use types::*;
