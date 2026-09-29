//! Provider-neutral, subscription-backed AI jobs.
//!
//! The adapters in this module only drive official local provider runtimes.
//! They deliberately do not accept API keys and never fall back to separately
//! billed API access.

mod auth;
mod claude;
mod codex;
mod discovery;
mod grok;
mod jobs;
mod process;
mod qualification;
mod results;
mod types;

pub use auth::{AuthEvent, AuthFailure, AuthHandle, AuthOutcome, AuthRequest, begin_auth};
pub use discovery::{DiscoveryConfig, discover_providers};
pub use jobs::{JobHandle, spawn_job};
pub use qualification::QualificationReceipts;
pub use types::{
    AiError, AllowanceWindow, BillingMode, Capability, CapabilityStatus, ConnectionState,
    EvidenceLevel, JobEvent, JobFailure, JobLimits, JobOperation, JobOutcome, JobRequest,
    JobResult, ProviderId, ProviderStatus, ReferenceAsset, ResultAsset, ValidatedClient,
};
