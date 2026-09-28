use serde::{Deserialize, Serialize};
use std::{
    fmt,
    path::{Path, PathBuf},
    sync::mpsc::{RecvTimeoutError, TryRecvError},
    time::Duration,
};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProviderId {
    CodexSubscription,
    ClaudeCode,
    GrokBuild,
}

impl ProviderId {
    pub const ALL: [Self; 3] = [Self::CodexSubscription, Self::ClaudeCode, Self::GrokBuild];

    pub const fn display_name(self) -> &'static str {
        match self {
            Self::CodexSubscription => "ChatGPT via Codex",
            Self::ClaudeCode => "Claude Code",
            Self::GrokBuild => "Grok Build",
        }
    }

    pub(crate) const fn command_name(self) -> &'static str {
        match self {
            Self::CodexSubscription => "codex",
            Self::ClaudeCode => "claude",
            Self::GrokBuild => "grok",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BillingMode {
    SubscriptionAllowance,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ConnectionState {
    Unavailable,
    IdentityUnverified,
    SignedOut,
    Ready,
    Degraded,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Capability {
    AssistantStreaming,
    ImageGeneration,
    ImageEditing,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EvidenceLevel {
    Verified,
    Unavailable,
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityStatus {
    pub capability: Capability,
    pub evidence: EvidenceLevel,
    /// A short, credential-free explanation suitable for a connection panel.
    pub detail: String,
}

impl CapabilityStatus {
    pub(crate) fn new(
        capability: Capability,
        evidence: EvidenceLevel,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            capability,
            evidence,
            detail: detail.into(),
        }
    }
}

/// An executable that passed a provider-specific identity fingerprint.
///
/// Construction is intentionally private. A command merely named `codex`,
/// `claude`, or `grok` is not sufficient evidence that it is an official
/// runtime.
#[derive(Clone, Debug)]
pub struct ValidatedClient {
    pub(crate) provider: ProviderId,
    pub(crate) executable: PathBuf,
    pub(crate) version: String,
    pub(crate) operation_profile_qualified: bool,
}

impl ValidatedClient {
    pub fn provider(&self) -> ProviderId {
        self.provider
    }

    pub fn version(&self) -> &str {
        &self.version
    }

    // The binary's deterministic UI tests compile this library as a dependency,
    // so cfg(test) alone cannot expose their offline client fixture. Production
    // builds do not enable ui-test and retain private validated construction.
    #[cfg(any(test, feature = "ui-test"))]
    #[doc(hidden)]
    pub fn fixture(provider: ProviderId, executable: PathBuf) -> Self {
        Self {
            provider,
            executable,
            version: "fixture".into(),
            operation_profile_qualified: true,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ProviderStatus {
    pub provider: ProviderId,
    pub display_name: &'static str,
    pub connection: ConnectionState,
    pub billing: BillingMode,
    pub version: Option<String>,
    pub capabilities: Vec<CapabilityStatus>,
    /// Sanitized status text. It never contains command output, credentials,
    /// account email addresses, or absolute user paths.
    pub detail: String,
    pub client: Option<ValidatedClient>,
}

impl ProviderStatus {
    pub fn capability(&self, capability: Capability) -> Option<&CapabilityStatus> {
        self.capabilities
            .iter()
            .find(|status| status.capability == capability)
    }

    pub fn can(&self, capability: Capability) -> bool {
        self.connection == ConnectionState::Ready
            && self
                .capability(capability)
                .is_some_and(|status| status.evidence == EvidenceLevel::Verified)
    }

    /// True when the official runtime advertises enough support to run the
    /// first explicit qualification job. This does not mean the operation is
    /// release-qualified; use `can` for that stricter check.
    pub fn may_attempt(&self, capability: Capability) -> bool {
        self.connection == ConnectionState::Ready
            && self
                .capability(capability)
                .is_some_and(|status| status.evidence != EvidenceLevel::Unavailable)
    }

    pub(crate) fn unavailable(provider: ProviderId, detail: impl Into<String>) -> Self {
        Self {
            provider,
            display_name: provider.display_name(),
            connection: ConnectionState::Unavailable,
            billing: BillingMode::Unknown,
            version: None,
            capabilities: provider
                .known_capabilities()
                .into_iter()
                .map(|capability| {
                    CapabilityStatus::new(capability, EvidenceLevel::Unknown, "Not checked")
                })
                .collect(),
            detail: detail.into(),
            client: None,
        }
    }
}

impl ProviderId {
    pub(crate) fn known_capabilities(self) -> Vec<Capability> {
        match self {
            Self::CodexSubscription => vec![
                Capability::AssistantStreaming,
                Capability::ImageGeneration,
                Capability::ImageEditing,
            ],
            Self::ClaudeCode | Self::GrokBuild => vec![Capability::AssistantStreaming],
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JobOperation {
    Assistant,
    GenerateImage,
    EditImage,
}

impl JobOperation {
    pub const fn required_capability(self) -> Capability {
        match self {
            Self::Assistant => Capability::AssistantStreaming,
            Self::GenerateImage => Capability::ImageGeneration,
            Self::EditImage => Capability::ImageEditing,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ReferenceAsset {
    pub path: PathBuf,
}

#[derive(Clone, Debug)]
pub struct JobLimits {
    pub max_runtime: Duration,
    pub max_protocol_line_bytes: usize,
    pub max_prompt_bytes: usize,
    /// Bound the accumulated provider prose and any structured assistant
    /// result retained for local review. Streaming frames are separately
    /// bounded, but a long sequence of valid frames must not grow memory
    /// without limit.
    pub max_result_text_bytes: usize,
    pub max_reference_count: usize,
    pub max_asset_bytes: u64,
    pub max_image_pixels: u64,
    pub event_capacity: usize,
}

impl Default for JobLimits {
    fn default() -> Self {
        Self {
            max_runtime: Duration::from_secs(180),
            max_protocol_line_bytes: 48 * 1024 * 1024,
            max_prompt_bytes: 32 * 1024,
            max_result_text_bytes: 512 * 1024,
            max_reference_count: 8,
            max_asset_bytes: 64 * 1024 * 1024,
            max_image_pixels: 64 * 1024 * 1024,
            event_capacity: 128,
        }
    }
}

impl JobLimits {
    const HARD_MAX_RUNTIME: Duration = Duration::from_secs(300);
    const HARD_MAX_PROTOCOL_LINE_BYTES: usize = 48 * 1024 * 1024;
    const HARD_MAX_PROMPT_BYTES: usize = 32 * 1024;
    const HARD_MAX_RESULT_TEXT_BYTES: usize = 512 * 1024;
    const HARD_MAX_REFERENCE_COUNT: usize = 8;
    const HARD_MAX_ASSET_BYTES: u64 = 64 * 1024 * 1024;
    const HARD_MAX_IMAGE_PIXELS: u64 = 64 * 1024 * 1024;
    const HARD_MAX_EVENT_CAPACITY: usize = 128;

    fn validate_bounds(&self) -> Result<(), AiError> {
        if self.max_runtime.is_zero()
            || self.max_runtime > Self::HARD_MAX_RUNTIME
            || self.max_protocol_line_bytes == 0
            || self.max_protocol_line_bytes > Self::HARD_MAX_PROTOCOL_LINE_BYTES
            || self.max_prompt_bytes == 0
            || self.max_prompt_bytes > Self::HARD_MAX_PROMPT_BYTES
            || self.max_result_text_bytes == 0
            || self.max_result_text_bytes > Self::HARD_MAX_RESULT_TEXT_BYTES
            || self.max_reference_count > Self::HARD_MAX_REFERENCE_COUNT
            || self.max_asset_bytes == 0
            || self.max_asset_bytes > Self::HARD_MAX_ASSET_BYTES
            || self.max_image_pixels == 0
            || self.max_image_pixels > Self::HARD_MAX_IMAGE_PIXELS
            || self.event_capacity > Self::HARD_MAX_EVENT_CAPACITY
        {
            return Err(AiError::InvalidRequest(
                "AI job limits exceed supported safety bounds".into(),
            ));
        }
        Ok(())
    }

    pub(crate) fn append_result_text(
        &self,
        target: &mut String,
        delta: &str,
    ) -> Result<(), AiError> {
        if target
            .len()
            .checked_add(delta.len())
            .is_none_or(|size| size > self.max_result_text_bytes)
        {
            return Err(AiError::Protocol(
                "Provider response exceeds the configured result limit".into(),
            ));
        }
        target.push_str(delta);
        Ok(())
    }

    pub(crate) fn validate_structured_output(
        &self,
        value: &serde_json::Value,
    ) -> Result<(), AiError> {
        let encoded = serde_json::to_vec(value)?;
        if encoded.len() > self.max_result_text_bytes {
            return Err(AiError::Protocol(
                "Provider structured output exceeds the configured result limit".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct JobRequest {
    pub id: uuid::Uuid,
    pub client: ValidatedClient,
    pub operation: JobOperation,
    pub prompt: String,
    /// Optional strict JSON Schema for assistant output. Image operations do
    /// not use this field.
    pub output_schema: Option<serde_json::Value>,
    pub references: Vec<ReferenceAsset>,
    /// A newly-created, job-specific directory owned by Omuse. References
    /// must already be staged below this directory.
    pub work_dir: PathBuf,
    pub limits: JobLimits,
}

impl JobRequest {
    pub fn new(
        client: ValidatedClient,
        operation: JobOperation,
        prompt: impl Into<String>,
        work_dir: impl Into<PathBuf>,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4(),
            client,
            operation,
            prompt: prompt.into(),
            output_schema: None,
            references: Vec::new(),
            work_dir: work_dir.into(),
            limits: JobLimits::default(),
        }
    }

    pub fn with_references(mut self, references: Vec<ReferenceAsset>) -> Self {
        self.references = references;
        self
    }

    pub fn with_output_schema(mut self, schema: serde_json::Value) -> Self {
        self.output_schema = Some(schema);
        self
    }

    pub fn validate(&self) -> Result<(), AiError> {
        self.limits.validate_bounds()?;
        if self.prompt.trim().is_empty() {
            return Err(AiError::InvalidRequest("The prompt is empty".into()));
        }
        if self.prompt.len() > self.limits.max_prompt_bytes {
            return Err(AiError::InvalidRequest("The prompt is too large".into()));
        }
        if self.references.len() > self.limits.max_reference_count {
            return Err(AiError::InvalidRequest("Too many reference images".into()));
        }
        if !self.references.is_empty() && self.client.provider != ProviderId::CodexSubscription {
            return Err(AiError::InvalidRequest(
                "Reference images are not qualified for this provider runtime".into(),
            ));
        }
        if self.output_schema.is_some() && self.operation != JobOperation::Assistant {
            return Err(AiError::InvalidRequest(
                "Structured output is only available for assistant jobs".into(),
            ));
        }
        if let Some(schema) = &self.output_schema {
            let encoded = serde_json::to_vec(schema)?;
            if encoded.len() > 64 * 1024 {
                return Err(AiError::InvalidRequest(
                    "The structured-output schema is too large".into(),
                ));
            }
        }
        if self.operation == JobOperation::EditImage && self.references.is_empty() {
            return Err(AiError::InvalidRequest(
                "Image editing requires at least one reference image".into(),
            ));
        }
        if self.client.provider == ProviderId::ClaudeCode
            && self.operation != JobOperation::Assistant
        {
            return Err(AiError::UnsupportedCapability {
                provider: self.client.provider,
                capability: self.operation.required_capability(),
            });
        }
        if self.client.provider == ProviderId::GrokBuild
            && self.operation != JobOperation::Assistant
        {
            return Err(AiError::UnsupportedCapability {
                provider: self.client.provider,
                capability: self.operation.required_capability(),
            });
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum JobEvent {
    Started,
    Submitted,
    TextDelta(String),
    ImageReady(ResultAsset),
    UsageLimit { resets_at_unix_seconds: Option<i64> },
    Finished,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultAsset {
    pub path: PathBuf,
    pub media_type: String,
    pub width: u32,
    pub height: u32,
    pub byte_len: u64,
    pub provider_item_id: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JobResult {
    pub provider: ProviderId,
    pub text: String,
    pub structured_output: Option<serde_json::Value>,
    pub assets: Vec<ResultAsset>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JobFailure {
    pub code: &'static str,
    pub message: String,
    pub retryable: bool,
}

impl JobFailure {
    pub(crate) fn new(code: &'static str, message: impl Into<String>, retryable: bool) -> Self {
        Self {
            code,
            message: message.into(),
            retryable,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum JobOutcome {
    Completed(JobResult),
    Cancelled,
    Failed(JobFailure),
    /// The provider may have accepted a potentially billable submission, but
    /// Omuse could not determine the terminal outcome. The UI must not retry it
    /// automatically.
    OutcomeUnknown(JobFailure),
}

#[derive(Debug)]
pub enum AiError {
    InvalidRequest(String),
    IdentityUnverified(String),
    UnsupportedCapability {
        provider: ProviderId,
        capability: Capability,
    },
    Io(std::io::Error),
    Protocol(String),
    Disconnected,
    TimedOut,
}

impl fmt::Display for AiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRequest(message)
            | Self::IdentityUnverified(message)
            | Self::Protocol(message) => f.write_str(message),
            Self::UnsupportedCapability {
                provider,
                capability,
            } => write!(
                f,
                "{} does not provide {capability:?} through its qualified subscription runtime",
                provider.display_name()
            ),
            Self::Io(error) => write!(f, "AI runtime I/O failed: {error}"),
            Self::Disconnected => f.write_str("AI runtime disconnected"),
            Self::TimedOut => f.write_str("AI runtime timed out"),
        }
    }
}

impl std::error::Error for AiError {}

impl From<std::io::Error> for AiError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<serde_json::Error> for AiError {
    fn from(value: serde_json::Error) -> Self {
        Self::Protocol(format!("Invalid provider message: {value}"))
    }
}

impl From<RecvTimeoutError> for AiError {
    fn from(value: RecvTimeoutError) -> Self {
        match value {
            RecvTimeoutError::Timeout => Self::TimedOut,
            RecvTimeoutError::Disconnected => Self::Disconnected,
        }
    }
}

impl From<TryRecvError> for AiError {
    fn from(value: TryRecvError) -> Self {
        match value {
            TryRecvError::Empty => Self::TimedOut,
            TryRecvError::Disconnected => Self::Disconnected,
        }
    }
}

pub(crate) fn is_within(path: &Path, root: &Path) -> bool {
    path.starts_with(root)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_references_are_rejected_before_a_provider_process_is_started() {
        let root = tempfile::tempdir().unwrap();
        let reference = root.path().join("reference.png");
        let request = JobRequest::new(
            ValidatedClient::fixture(ProviderId::ClaudeCode, root.path().join("claude")),
            JobOperation::Assistant,
            "Make a caption",
            root.path(),
        )
        .with_references(vec![ReferenceAsset { path: reference }]);

        let error = request.validate().unwrap_err();
        assert!(error.to_string().contains("Reference images"));
    }

    #[test]
    fn response_limits_reject_growth_without_truncating_a_plan() {
        let limits = JobLimits {
            max_result_text_bytes: 4,
            ..JobLimits::default()
        };
        let mut text = "abc".to_owned();
        let error = limits.append_result_text(&mut text, "de").unwrap_err();
        assert!(error.to_string().contains("result limit"));
        assert_eq!(text, "abc");
    }

    #[test]
    fn callers_cannot_relax_hard_job_bounds() {
        let client = ValidatedClient::fixture(ProviderId::CodexSubscription, "fixture".into());
        let request = JobRequest {
            limits: JobLimits {
                max_asset_bytes: JobLimits::HARD_MAX_ASSET_BYTES + 1,
                ..JobLimits::default()
            },
            ..JobRequest::new(
                client,
                JobOperation::Assistant,
                "Plan a poster",
                "workspace",
            )
        };
        assert!(request.validate().is_err());
    }
}
