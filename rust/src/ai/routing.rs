use super::{Capability, ConnectionState, EvidenceLevel, ProviderId, ProviderStatus};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt,
};

pub const ROUTING_PREFERENCES_VERSION: u8 = 2;
pub const MAX_PREFERENCES_BYTES: usize = 8 * 1024;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskKind {
    Design,
    Photo,
    Caption,
    Generate,
    Replace,
    Remove,
    Background,
    Expand,
}

impl TaskKind {
    const fn capability(self) -> Capability {
        match self {
            Self::Design | Self::Photo | Self::Caption => Capability::AssistantStreaming,
            Self::Generate => Capability::ImageGeneration,
            Self::Replace | Self::Remove | Self::Background | Self::Expand => {
                Capability::ImageEditing
            }
        }
    }

    const fn uses_image_provider(self) -> bool {
        matches!(
            self,
            Self::Generate | Self::Replace | Self::Remove | Self::Background | Self::Expand
        )
    }

    const fn always_needs_visual_input(self) -> bool {
        matches!(self, Self::Photo | Self::Caption)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "mode",
    content = "provider",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum ProviderChoice {
    #[default]
    Auto,
    Pinned(ProviderId),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RoutingPreferences {
    pub version: u8,
    pub assistant: ProviderId,
    pub image_provider: ProviderId,
    #[serde(default)]
    pub tasks: BTreeMap<TaskKind, ProviderChoice>,
    #[serde(default)]
    pub excluded_from_auto: BTreeSet<ProviderId>,
}

impl Default for RoutingPreferences {
    fn default() -> Self {
        Self {
            version: ROUTING_PREFERENCES_VERSION,
            assistant: ProviderId::CodexSubscription,
            image_provider: ProviderId::CodexSubscription,
            tasks: BTreeMap::new(),
            excluded_from_auto: BTreeSet::new(),
        }
    }
}

impl RoutingPreferences {
    pub fn choice(&self, task: TaskKind) -> ProviderChoice {
        self.tasks.get(&task).copied().unwrap_or_default()
    }

    pub fn set_choice(&mut self, task: TaskKind, choice: ProviderChoice) {
        if choice == ProviderChoice::Auto {
            self.tasks.remove(&task);
        } else {
            self.tasks.insert(task, choice);
        }
    }

    /// Loads the current format or migrates the small legacy connection file.
    ///
    /// Legacy provider fields are migrated independently so one stale value
    /// does not discard the other valid selection. The legacy direct-API flag
    /// is deliberately ignored; this model has no API-billing route to enable.
    pub fn from_json_bounded(bytes: &[u8]) -> Result<Self, RoutingPreferencesError> {
        if bytes.len() > MAX_PREFERENCES_BYTES {
            return Err(RoutingPreferencesError::TooLarge {
                max_bytes: MAX_PREFERENCES_BYTES,
            });
        }

        let value: Value = serde_json::from_slice(bytes)
            .map_err(|error| RoutingPreferencesError::Malformed(error.to_string()))?;
        let object = value.as_object().ok_or_else(|| {
            RoutingPreferencesError::Malformed("preferences must be a JSON object".into())
        })?;

        let version = object
            .get("version")
            .map(|version| {
                version.as_u64().ok_or_else(|| {
                    RoutingPreferencesError::Malformed(
                        "routing preference version must be a non-negative integer".into(),
                    )
                })
            })
            .transpose()?;

        match version {
            None | Some(1) => Ok(migrate_legacy(&value)),
            Some(2) => {
                let mut value = value;
                match value.get("directApiEnabled") {
                    None | Some(Value::Bool(false)) => {}
                    Some(Value::Bool(true)) => {
                        return Err(RoutingPreferencesError::Malformed(
                            "direct API access cannot be enabled by routing preferences".into(),
                        ));
                    }
                    Some(_) => {
                        return Err(RoutingPreferencesError::Malformed(
                            "directApiEnabled must be false".into(),
                        ));
                    }
                }
                value
                    .as_object_mut()
                    .expect("routing preferences were checked as an object")
                    .remove("directApiEnabled");
                let preferences: Self = serde_json::from_value(value).map_err(|error| {
                    RoutingPreferencesError::Malformed(format!(
                        "version 2 routing preferences are invalid: {error}"
                    ))
                })?;
                if preferences.version != ROUTING_PREFERENCES_VERSION {
                    return Err(RoutingPreferencesError::UnsupportedVersion(
                        preferences.version as u64,
                    ));
                }
                Ok(preferences)
            }
            Some(version) => Err(RoutingPreferencesError::UnsupportedVersion(version)),
        }
    }
}

fn migrate_legacy(value: &Value) -> RoutingPreferences {
    let provider = |field: &str| {
        value
            .get(field)
            .cloned()
            .and_then(|value| serde_json::from_value(value).ok())
            .unwrap_or(ProviderId::CodexSubscription)
    };
    RoutingPreferences {
        assistant: provider("assistant"),
        image_provider: provider("imageProvider"),
        ..RoutingPreferences::default()
    }
}

#[derive(Clone, Debug)]
pub struct ResolvedRoute {
    pub provider: ProviderId,
    pub first_use: bool,
    pub reason: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RouteUnavailable {
    pub task: TaskKind,
    pub provider: Option<ProviderId>,
    pub reason: String,
}

impl fmt::Display for RouteUnavailable {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.reason)
    }
}

impl Error for RouteUnavailable {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RoutingPreferencesError {
    TooLarge { max_bytes: usize },
    Malformed(String),
    UnsupportedVersion(u64),
}

impl fmt::Display for RoutingPreferencesError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLarge { max_bytes } => {
                write!(formatter, "routing preferences exceed {max_bytes} bytes")
            }
            Self::Malformed(detail) => write!(formatter, "invalid routing preferences: {detail}"),
            Self::UnsupportedVersion(version) => {
                write!(
                    formatter,
                    "unsupported routing preference version {version}"
                )
            }
        }
    }
}

impl Error for RoutingPreferencesError {}

/// Resolves exactly one route. It does not submit work, retry another provider,
/// or infer capacity from an optional allowance snapshot.
pub fn resolve_route(
    task: TaskKind,
    needs_reference_images: bool,
    preferences: &RoutingPreferences,
    statuses: &[ProviderStatus],
) -> Result<ResolvedRoute, RouteUnavailable> {
    let capability = task.capability();
    let needs_visual_input = task.always_needs_visual_input() || needs_reference_images;

    if let ProviderChoice::Pinned(provider) = preferences.choice(task) {
        return route_for_provider(
            task,
            provider,
            capability,
            needs_visual_input,
            statuses,
            "Using the provider pinned for this task.",
        );
    }

    let preferred = if task.uses_image_provider() {
        preferences.image_provider
    } else {
        preferences.assistant
    };
    if !preferences.excluded_from_auto.contains(&preferred) {
        if let Ok(route) = route_for_provider(
            task,
            preferred,
            capability,
            needs_visual_input,
            statuses,
            "Using your preferred provider for this task.",
        ) {
            return Ok(route);
        }
    }

    for evidence in [EvidenceLevel::Verified, EvidenceLevel::Unknown] {
        for provider in ProviderId::ALL {
            if provider == preferred || preferences.excluded_from_auto.contains(&provider) {
                continue;
            }
            let Ok(route) = route_for_provider(
                task,
                provider,
                capability,
                needs_visual_input,
                statuses,
                "Your preferred provider is unavailable; using another ready provider.",
            ) else {
                continue;
            };
            if route.first_use == (evidence == EvidenceLevel::Unknown) {
                return Ok(route);
            }
        }
    }

    Err(RouteUnavailable {
        task,
        provider: None,
        reason: if needs_visual_input {
            "No ready provider supports the visual input required by this task.".into()
        } else {
            format!(
                "No ready provider supports {} for this task.",
                capability_label(capability)
            )
        },
    })
}

fn route_for_provider(
    task: TaskKind,
    provider: ProviderId,
    capability: Capability,
    needs_visual_input: bool,
    statuses: &[ProviderStatus],
    reason: &str,
) -> Result<ResolvedRoute, RouteUnavailable> {
    let unavailable = |reason: &str| RouteUnavailable {
        task,
        provider: Some(provider),
        reason: reason.into(),
    };
    if needs_visual_input && provider != ProviderId::CodexSubscription {
        return Err(unavailable(
            "The selected provider does not support visual input for this task.",
        ));
    }

    let status = statuses
        .iter()
        .find(|status| status.provider == provider)
        .ok_or_else(|| unavailable("The selected provider was not discovered."))?;
    match status.connection {
        ConnectionState::Ready => {}
        ConnectionState::SignedOut => {
            return Err(unavailable("The selected provider needs sign-in."));
        }
        ConnectionState::IdentityUnverified => {
            return Err(unavailable("The selected provider runtime is unverified."));
        }
        ConnectionState::Unavailable => {
            return Err(unavailable("The selected provider is unavailable."));
        }
        ConnectionState::Degraded => {
            return Err(unavailable("The selected provider connection is degraded."));
        }
    }

    let client = status
        .client
        .as_ref()
        .ok_or_else(|| unavailable("The selected provider has no validated client."))?;
    if client.provider() != provider {
        return Err(unavailable(
            "The selected provider client does not match its verified runtime.",
        ));
    }

    let evidence = status
        .capability(capability)
        .map(|capability| capability.evidence)
        .ok_or_else(|| unavailable("The selected provider does not advertise this capability."))?;
    match evidence {
        EvidenceLevel::Unavailable => Err(unavailable(
            "The selected provider does not support this capability.",
        )),
        EvidenceLevel::Verified | EvidenceLevel::Unknown => Ok(ResolvedRoute {
            provider,
            first_use: evidence == EvidenceLevel::Unknown,
            reason: reason.into(),
        }),
    }
}

const fn capability_label(capability: Capability) -> &'static str {
    match capability {
        Capability::AssistantStreaming => "assistant requests",
        Capability::ImageGeneration => "image generation",
        Capability::ImageEditing => "image editing",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::{BillingMode, CapabilityStatus, ValidatedClient};
    use std::path::PathBuf;

    fn status(
        provider: ProviderId,
        connection: ConnectionState,
        capability: Capability,
        evidence: EvidenceLevel,
        client: bool,
    ) -> ProviderStatus {
        ProviderStatus {
            provider,
            display_name: provider.display_name(),
            connection,
            billing: BillingMode::SubscriptionAllowance,
            version: Some("fixture".into()),
            capabilities: vec![CapabilityStatus::new(capability, evidence, "fixture")],
            allowance: None,
            detail: "fixture".into(),
            client: client.then(|| {
                ValidatedClient::fixture(provider, PathBuf::from(provider.display_name()))
            }),
        }
    }

    #[test]
    fn preferences_use_stable_choice_format_and_sparse_auto_overrides() {
        let mut preferences = RoutingPreferences::default();
        preferences.set_choice(
            TaskKind::Design,
            ProviderChoice::Pinned(ProviderId::ClaudeCode),
        );
        let value = serde_json::to_value(&preferences).unwrap();
        assert_eq!(value["version"], 2);
        assert_eq!(value["tasks"]["design"]["mode"], "pinned");
        assert_eq!(value["tasks"]["design"]["provider"], "claudeCode");

        preferences.set_choice(TaskKind::Design, ProviderChoice::Auto);
        assert_eq!(preferences.choice(TaskKind::Design), ProviderChoice::Auto);
        assert!(!preferences.tasks.contains_key(&TaskKind::Design));
    }

    #[test]
    fn migrates_legacy_fields_independently_and_never_carries_api_state() {
        let preferences = RoutingPreferences::from_json_bounded(
            br#"{"assistant":"claudeCode","imageProvider":"stale","directApiEnabled":true}"#,
        )
        .unwrap();
        assert_eq!(preferences.version, ROUTING_PREFERENCES_VERSION);
        assert_eq!(preferences.assistant, ProviderId::ClaudeCode);
        assert_eq!(preferences.image_provider, ProviderId::CodexSubscription);
        assert!(preferences.tasks.is_empty());
        assert!(
            !serde_json::to_string(&preferences)
                .unwrap()
                .contains("directApi")
        );
    }

    #[test]
    fn malformed_v2_pin_fails_closed() {
        let error = RoutingPreferences::from_json_bounded(
            br#"{"version":2,"assistant":"codexSubscription","imageProvider":"codexSubscription","tasks":{"design":{"mode":"pinned","provider":"unknown"}},"excludedFromAuto":[]}"#,
        )
        .unwrap_err();
        assert!(error.to_string().contains("version 2"));
    }

    #[test]
    fn v2_accepts_only_a_disabled_legacy_api_marker() {
        let disabled = RoutingPreferences::from_json_bounded(
            br#"{"version":2,"assistant":"claudeCode","imageProvider":"codexSubscription","tasks":{},"excludedFromAuto":[],"directApiEnabled":false}"#,
        )
        .unwrap();
        assert_eq!(disabled.assistant, ProviderId::ClaudeCode);

        let enabled = RoutingPreferences::from_json_bounded(
            br#"{"version":2,"assistant":"claudeCode","imageProvider":"codexSubscription","tasks":{},"excludedFromAuto":[],"directApiEnabled":true}"#,
        )
        .unwrap_err();
        assert!(enabled.to_string().contains("cannot be enabled"));
    }

    #[test]
    fn auto_skips_signed_out_and_clientless_statuses() {
        let preferences = RoutingPreferences {
            assistant: ProviderId::ClaudeCode,
            ..RoutingPreferences::default()
        };
        let statuses = vec![
            status(
                ProviderId::ClaudeCode,
                ConnectionState::SignedOut,
                Capability::AssistantStreaming,
                EvidenceLevel::Verified,
                true,
            ),
            status(
                ProviderId::GrokBuild,
                ConnectionState::Ready,
                Capability::AssistantStreaming,
                EvidenceLevel::Verified,
                false,
            ),
            status(
                ProviderId::CodexSubscription,
                ConnectionState::Ready,
                Capability::AssistantStreaming,
                EvidenceLevel::Verified,
                true,
            ),
        ];
        assert_eq!(
            resolve_route(TaskKind::Design, false, &preferences, &statuses)
                .unwrap()
                .provider,
            ProviderId::CodexSubscription
        );
    }

    #[test]
    fn photo_always_routes_visual_input_to_codex() {
        let preferences = RoutingPreferences {
            assistant: ProviderId::ClaudeCode,
            ..RoutingPreferences::default()
        };
        let statuses = vec![
            status(
                ProviderId::ClaudeCode,
                ConnectionState::Ready,
                Capability::AssistantStreaming,
                EvidenceLevel::Verified,
                true,
            ),
            status(
                ProviderId::CodexSubscription,
                ConnectionState::Ready,
                Capability::AssistantStreaming,
                EvidenceLevel::Verified,
                true,
            ),
        ];
        assert_eq!(
            resolve_route(TaskKind::Photo, false, &preferences, &statuses)
                .unwrap()
                .provider,
            ProviderId::CodexSubscription
        );
    }

    #[test]
    fn image_task_is_unavailable_without_an_image_capable_runtime() {
        let statuses = vec![status(
            ProviderId::ClaudeCode,
            ConnectionState::Ready,
            Capability::AssistantStreaming,
            EvidenceLevel::Verified,
            true,
        )];
        let error = resolve_route(
            TaskKind::Generate,
            false,
            &RoutingPreferences::default(),
            &statuses,
        )
        .unwrap_err();
        assert_eq!(error.provider, None);
        assert!(error.reason.contains("image generation"));
    }

    #[test]
    fn pinned_provider_never_falls_back() {
        let mut preferences = RoutingPreferences::default();
        preferences.set_choice(
            TaskKind::Generate,
            ProviderChoice::Pinned(ProviderId::ClaudeCode),
        );
        let statuses = vec![
            status(
                ProviderId::ClaudeCode,
                ConnectionState::Ready,
                Capability::AssistantStreaming,
                EvidenceLevel::Verified,
                true,
            ),
            status(
                ProviderId::CodexSubscription,
                ConnectionState::Ready,
                Capability::ImageGeneration,
                EvidenceLevel::Verified,
                true,
            ),
        ];
        let error = resolve_route(TaskKind::Generate, false, &preferences, &statuses).unwrap_err();
        assert_eq!(error.provider, Some(ProviderId::ClaudeCode));
    }

    #[test]
    fn auto_exclusion_skips_the_preferred_provider() {
        let mut preferences = RoutingPreferences::default();
        preferences
            .excluded_from_auto
            .insert(ProviderId::CodexSubscription);
        let statuses = vec![
            status(
                ProviderId::CodexSubscription,
                ConnectionState::Ready,
                Capability::AssistantStreaming,
                EvidenceLevel::Verified,
                true,
            ),
            status(
                ProviderId::ClaudeCode,
                ConnectionState::Ready,
                Capability::AssistantStreaming,
                EvidenceLevel::Verified,
                true,
            ),
        ];
        assert_eq!(
            resolve_route(TaskKind::Design, false, &preferences, &statuses)
                .unwrap()
                .provider,
            ProviderId::ClaudeCode
        );
    }

    #[test]
    fn explicit_unknown_capability_is_a_first_use_route() {
        let mut preferences = RoutingPreferences::default();
        preferences.set_choice(
            TaskKind::Design,
            ProviderChoice::Pinned(ProviderId::ClaudeCode),
        );
        let statuses = vec![status(
            ProviderId::ClaudeCode,
            ConnectionState::Ready,
            Capability::AssistantStreaming,
            EvidenceLevel::Unknown,
            true,
        )];
        let route = resolve_route(TaskKind::Design, false, &preferences, &statuses).unwrap();
        assert_eq!(route.provider, ProviderId::ClaudeCode);
        assert!(route.first_use);
    }

    #[test]
    fn preferred_unknown_route_wins_before_verified_fallback() {
        let preferences = RoutingPreferences {
            assistant: ProviderId::ClaudeCode,
            ..RoutingPreferences::default()
        };
        let statuses = vec![
            status(
                ProviderId::ClaudeCode,
                ConnectionState::Ready,
                Capability::AssistantStreaming,
                EvidenceLevel::Unknown,
                true,
            ),
            status(
                ProviderId::CodexSubscription,
                ConnectionState::Ready,
                Capability::AssistantStreaming,
                EvidenceLevel::Verified,
                true,
            ),
        ];
        let route = resolve_route(TaskKind::Design, false, &preferences, &statuses).unwrap();
        assert_eq!(route.provider, ProviderId::ClaudeCode);
        assert!(route.first_use);
    }

    #[test]
    fn mismatched_validated_client_is_rejected() {
        let mut claude = status(
            ProviderId::ClaudeCode,
            ConnectionState::Ready,
            Capability::AssistantStreaming,
            EvidenceLevel::Verified,
            true,
        );
        claude.client = Some(ValidatedClient::fixture(
            ProviderId::CodexSubscription,
            PathBuf::from("fixture"),
        ));
        let mut preferences = RoutingPreferences::default();
        preferences.set_choice(
            TaskKind::Design,
            ProviderChoice::Pinned(ProviderId::ClaudeCode),
        );
        let error = resolve_route(TaskKind::Design, false, &preferences, &[claude]).unwrap_err();
        assert!(error.reason.contains("does not match"));
    }

    #[test]
    fn oversized_preferences_are_rejected_before_parsing() {
        let bytes = vec![b' '; MAX_PREFERENCES_BYTES + 1];
        assert_eq!(
            RoutingPreferences::from_json_bounded(&bytes),
            Err(RoutingPreferencesError::TooLarge {
                max_bytes: MAX_PREFERENCES_BYTES
            })
        );
    }
}
