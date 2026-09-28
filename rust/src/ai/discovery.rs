use crate::ai::{
    claude, codex,
    process::{capture_bounded, provider_version_is_valid},
    qualification::QualificationReceipts,
    types::{
        AiError, BillingMode, Capability, CapabilityStatus, ConnectionState, EvidenceLevel,
        ProviderId, ProviderStatus, ValidatedClient,
    },
};
use std::{
    collections::BTreeMap,
    env, fs,
    io::Read,
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Clone, Debug)]
pub struct DiscoveryConfig {
    pub candidate_paths: BTreeMap<ProviderId, PathBuf>,
    pub probe_timeout: Duration,
    pub max_protocol_line_bytes: usize,
    pub work_dir: PathBuf,
    /// Operation-specific end-to-end receipts recorded only after a completed
    /// user-authorized operation. An advertised feature flag alone never adds
    /// evidence here.
    pub qualification_receipts: QualificationReceipts,
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        Self {
            candidate_paths: BTreeMap::new(),
            probe_timeout: Duration::from_secs(8),
            max_protocol_line_bytes: 2 * 1024 * 1024,
            work_dir: env::temp_dir().join(format!("omuse-ai-discovery-{}", uuid::Uuid::new_v4())),
            qualification_receipts: QualificationReceipts::default(),
        }
    }
}

impl DiscoveryConfig {
    pub fn with_candidate(mut self, provider: ProviderId, path: impl Into<PathBuf>) -> Self {
        self.candidate_paths.insert(provider, path.into());
        self
    }

    pub fn with_qualification_receipts(mut self, receipts: QualificationReceipts) -> Self {
        self.qualification_receipts = receipts;
        self
    }
}

pub fn discover_providers(config: &DiscoveryConfig) -> Vec<ProviderStatus> {
    if fs::create_dir_all(&config.work_dir).is_err() {
        return ProviderId::ALL
            .into_iter()
            .map(|provider| {
                ProviderStatus::unavailable(provider, "Connection probe workspace unavailable")
            })
            .collect();
    }
    ProviderId::ALL
        .into_iter()
        .map(|provider| discover_one(provider, config))
        .collect()
}

fn discover_one(provider: ProviderId, config: &DiscoveryConfig) -> ProviderStatus {
    let candidate = config
        .candidate_paths
        .get(&provider)
        .cloned()
        .or_else(|| find_on_path(provider.command_name()));
    let Some(candidate) = candidate else {
        return ProviderStatus::unavailable(provider, "Official runtime not found");
    };
    let client = match validate_identity(provider, &candidate, config) {
        Ok(client) => client,
        Err(_) => {
            let mut status =
                ProviderStatus::unavailable(provider, "Runtime identity could not be verified");
            status.connection = ConnectionState::IdentityUnverified;
            return status;
        }
    };
    match probe_account(&client, config) {
        Ok(probe) if probe.signed_in => {
            ready_status(client, config, probe.image_generation_advertised)
        }
        Ok(_) => ProviderStatus {
            provider,
            display_name: provider.display_name(),
            connection: ConnectionState::SignedOut,
            billing: BillingMode::Unknown,
            version: Some(client.version.clone()),
            capabilities: provider
                .known_capabilities()
                .into_iter()
                .map(|capability| {
                    CapabilityStatus::new(
                        capability,
                        EvidenceLevel::Unknown,
                        "Sign in through the official provider runtime",
                    )
                })
                .collect(),
            detail: "Official runtime found; subscription sign-in is unavailable".into(),
            client: Some(client),
        },
        Err(error) => ProviderStatus {
            provider,
            display_name: provider.display_name(),
            connection: ConnectionState::Degraded,
            billing: BillingMode::Unknown,
            version: Some(client.version.clone()),
            capabilities: provider
                .known_capabilities()
                .into_iter()
                .map(|capability| {
                    CapabilityStatus::new(
                        capability,
                        EvidenceLevel::Unknown,
                        "Account probe did not complete",
                    )
                })
                .collect(),
            detail: probe_error_detail(&error).into(),
            client: Some(client),
        },
    }
}

fn probe_error_detail(error: &AiError) -> String {
    match error {
        AiError::TimedOut => "Official runtime account and isolation probe timed out".into(),
        AiError::Io(_) | AiError::Disconnected => "Official runtime could not be started".into(),
        // These messages are authored by Omuse and contain only protocol field
        // or feature names. Keeping them makes a failed isolation proof
        // actionable without exposing provider output, paths, or account data.
        AiError::Protocol(message)
            if message.starts_with("Codex runtime isolation")
                || message.starts_with("Codex omitted its effective feature list")
                || message.starts_with("Claude Code account status")
                || message.starts_with("Claude Code omitted its account state") =>
        {
            message.clone()
        }
        AiError::Protocol(message) if message.contains("isolation") => {
            "Official runtime isolation check failed".into()
        }
        AiError::Protocol(_) => "Official runtime protocol check failed".into(),
        AiError::InvalidRequest(_) => "Official runtime account state is not usable".into(),
        AiError::IdentityUnverified(_) => "Official runtime identity could not be verified".into(),
        AiError::UnsupportedCapability { .. } => "Official runtime capability check failed".into(),
    }
}

fn validate_identity(
    provider: ProviderId,
    candidate: &Path,
    config: &DiscoveryConfig,
) -> Result<ValidatedClient, AiError> {
    let executable = candidate.canonicalize()?;
    if !executable.is_file() || is_shell_wrapper(&executable)? {
        return Err(AiError::IdentityUnverified(
            "Provider entry point is an unverified wrapper".into(),
        ));
    }
    let args: &[&str] = match provider {
        ProviderId::CodexSubscription | ProviderId::ClaudeCode => &["--version"],
        ProviderId::GrokBuild => &["version"],
    };
    let capture = capture_bounded(
        &executable,
        args.iter().copied(),
        &config.work_dir,
        config.probe_timeout,
        4096,
    )?;
    if !capture.status.success()
        || capture.truncated
        || !provider_version_is_valid(provider, &capture.stdout)
    {
        return Err(AiError::IdentityUnverified(
            "Provider version fingerprint did not match".into(),
        ));
    }
    let help_args: &[&str] = match provider {
        ProviderId::CodexSubscription => &["app-server", "--help"],
        ProviderId::ClaudeCode => &["--help"],
        ProviderId::GrokBuild => &["agent", "--help"],
    };
    let help = capture_bounded(
        &executable,
        help_args.iter().copied(),
        &config.work_dir,
        config.probe_timeout,
        192 * 1024,
    )?;
    if !help.status.success() || help.truncated || !provider_help_is_valid(provider, &help.stdout) {
        return Err(AiError::IdentityUnverified(
            "Provider interface fingerprint did not match".into(),
        ));
    }
    if provider == ProviderId::ClaudeCode {
        let auth_help = capture_bounded(
            &executable,
            ["auth", "login", "--help"],
            &config.work_dir,
            config.probe_timeout,
            32 * 1024,
        )?;
        if !auth_help.status.success()
            || auth_help.truncated
            || !std::str::from_utf8(&auth_help.stdout)
                .is_ok_and(|output| output.contains("--claudeai") && output.contains("--console"))
        {
            return Err(AiError::IdentityUnverified(
                "Claude Code subscription sign-in interface did not match".into(),
            ));
        }
    }
    let version = String::from_utf8_lossy(&capture.stdout)
        .lines()
        .next()
        .unwrap_or("unknown")
        .chars()
        .filter(|character| !character.is_control())
        .take(96)
        .collect();
    Ok(ValidatedClient {
        provider,
        executable,
        version,
        // Grok ACP is implemented, but it cannot run until its official CLI
        // offers or Omuse supplies a verified profile that excludes inherited
        // hooks, plugins, skills and user configuration while retaining only
        // cached subscription authentication.
        operation_profile_qualified: provider != ProviderId::GrokBuild,
    })
}

fn provider_help_is_valid(provider: ProviderId, output: &[u8]) -> bool {
    let Ok(output) = std::str::from_utf8(output) else {
        return false;
    };
    match provider {
        ProviderId::CodexSubscription => {
            output.contains("app server")
                && output.contains("--strict-config")
                && output.contains("generate-json-schema")
        }
        ProviderId::ClaudeCode => {
            output.contains("Claude Code")
                && output.contains("--output-format")
                && output.contains("--safe-mode")
                && output.contains("--restricted")
                && output.contains("--json-schema")
        }
        ProviderId::GrokBuild => output.contains("stdio") && output.contains("agent"),
    }
}

struct AccountProbe {
    signed_in: bool,
    image_generation_advertised: bool,
}

fn probe_account(
    client: &ValidatedClient,
    config: &DiscoveryConfig,
) -> Result<AccountProbe, AiError> {
    match client.provider {
        ProviderId::CodexSubscription => {
            let probe = codex::probe(
                &client.executable,
                &config.work_dir,
                config.probe_timeout,
                config.max_protocol_line_bytes,
            )?;
            Ok(AccountProbe {
                signed_in: probe.signed_in_with_chatgpt,
                image_generation_advertised: probe.image_generation_advertised,
            })
        }
        ProviderId::ClaudeCode => Ok(AccountProbe {
            signed_in: claude::subscription_signed_in(
                &client.executable,
                &config.work_dir,
                config.probe_timeout,
            )?,
            image_generation_advertised: false,
        }),
        ProviderId::GrokBuild => Err(AiError::Protocol(
            "Grok ACP configuration isolation is not yet qualified".into(),
        )),
    }
}

fn ready_status(
    client: ValidatedClient,
    config: &DiscoveryConfig,
    advertised_image_generation: bool,
) -> ProviderStatus {
    let provider = client.provider;
    let mut capabilities = Vec::new();
    for capability in provider.known_capabilities() {
        let qualified = config.qualification_receipts.contains(&client, capability);
        let (evidence, detail) = if qualified {
            (
                EvidenceLevel::Verified,
                "Tested by a completed Omuse request on this runtime",
            )
        } else if matches!(
            capability,
            Capability::ImageGeneration | Capability::ImageEditing
        ) && !advertised_image_generation
        {
            (
                EvidenceLevel::Unavailable,
                "The runtime does not advertise image generation",
            )
        } else {
            (
                EvidenceLevel::Unknown,
                "Signed in, but this operation is not yet tested",
            )
        };
        capabilities.push(CapabilityStatus::new(capability, evidence, detail));
    }
    ProviderStatus {
        provider,
        display_name: provider.display_name(),
        connection: ConnectionState::Ready,
        billing: BillingMode::SubscriptionAllowance,
        version: Some(client.version.clone()),
        capabilities,
        detail: "Official runtime and subscription login verified".into(),
        client: Some(client),
    }
}

fn find_on_path(name: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;
    env::split_paths(&path)
        .map(|directory| directory.join(name))
        .find(|candidate| candidate.is_file())
}

fn is_shell_wrapper(path: &Path) -> Result<bool, AiError> {
    let mut file = fs::File::open(path)?;
    let mut prefix = [0_u8; 160];
    let read = file.read(&mut prefix)?;
    let prefix = String::from_utf8_lossy(&prefix[..read]).to_ascii_lowercase();
    let Some(first_line) = prefix.lines().next() else {
        return Ok(false);
    };
    Ok(first_line.starts_with("#!")
        && ["/sh", "/bash", "/zsh", "env sh", "env bash", "env zsh"]
            .iter()
            .any(|marker| first_line.contains(marker)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, os::unix::fs::PermissionsExt};

    #[test]
    fn discovery_rejects_command_name_only_shell_wrappers() {
        let root = tempfile::tempdir().unwrap();
        let wrapper = root.path().join("codex");
        fs::write(&wrapper, "#!/bin/sh\nprintf 'codex-cli 999.0.0\\n'\n").unwrap();
        fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o700)).unwrap();
        let config = DiscoveryConfig {
            work_dir: root.path().join("probe"),
            ..DiscoveryConfig::default()
        };
        fs::create_dir_all(&config.work_dir).unwrap();
        assert!(validate_identity(ProviderId::CodexSubscription, &wrapper, &config).is_err());
    }

    #[test]
    fn discovery_defaults_to_no_capability_receipts() {
        let client = ValidatedClient::fixture(ProviderId::CodexSubscription, "/bin/true".into());
        let config = DiscoveryConfig::default();
        assert!(
            !config
                .qualification_receipts
                .contains(&client, Capability::ImageGeneration)
        );
    }
}
