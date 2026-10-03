use crate::ai::{
    claude, codex,
    process::{capture_bounded_cancellable, provider_version_is_valid},
    qualification::QualificationReceipts,
    types::{
        AiError, AllowanceWindow, BillingMode, Capability, CapabilityStatus, ConnectionState,
        EvidenceLevel, ProviderId, ProviderStatus, ValidatedClient,
    },
};
use std::{
    collections::BTreeMap,
    env, fs,
    io::Read,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
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
    /// Shared cancellation for a superseded or closed connection refresh.
    pub cancellation: Arc<AtomicBool>,
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        Self {
            candidate_paths: BTreeMap::new(),
            probe_timeout: Duration::from_secs(8),
            max_protocol_line_bytes: 2 * 1024 * 1024,
            work_dir: env::temp_dir().join(format!("omuse-ai-discovery-{}", uuid::Uuid::new_v4())),
            qualification_receipts: QualificationReceipts::default(),
            cancellation: Arc::new(AtomicBool::new(false)),
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

    pub fn with_cancellation(mut self, cancellation: Arc<AtomicBool>) -> Self {
        self.cancellation = cancellation;
        self
    }
}

pub fn discover_providers(config: &DiscoveryConfig) -> Vec<ProviderStatus> {
    let remove_probe_root = !config.work_dir.exists();
    if fs::create_dir_all(&config.work_dir).is_err() {
        return ProviderId::ALL
            .into_iter()
            .map(|provider| {
                ProviderStatus::unavailable(provider, "Connection probe workspace unavailable")
            })
            .collect();
    }
    let statuses = map_providers_parallel(|provider| {
        let mut provider_config = config.clone();
        provider_config.work_dir = config.work_dir.join(format!(
            "{}-{}",
            provider.command_name(),
            uuid::Uuid::new_v4()
        ));
        if create_private_probe_dir(&provider_config.work_dir).is_err() {
            return ProviderStatus::unavailable(provider, "Connection probe workspace unavailable");
        }
        let deadline = Instant::now() + provider_config.probe_timeout;
        let status = discover_one(provider, &provider_config, deadline);
        let _ = fs::remove_dir_all(&provider_config.work_dir);
        status
    });
    if remove_probe_root {
        let _ = fs::remove_dir(&config.work_dir);
    }
    statuses
}

fn map_providers_parallel(
    operation: impl Fn(ProviderId) -> ProviderStatus + Sync,
) -> Vec<ProviderStatus> {
    thread::scope(|scope| {
        let operation = &operation;
        let handles = ProviderId::ALL.map(|provider| scope.spawn(move || operation(provider)));
        ProviderId::ALL
            .into_iter()
            .zip(handles)
            .map(|(provider, handle)| {
                handle.join().unwrap_or_else(|_| {
                    ProviderStatus::unavailable(provider, "Connection probe worker failed")
                })
            })
            .collect()
    })
}

fn discover_one(
    provider: ProviderId,
    config: &DiscoveryConfig,
    deadline: Instant,
) -> ProviderStatus {
    if config.cancellation.load(Ordering::Relaxed) {
        return ProviderStatus::unavailable(provider, "Connection check cancelled");
    }
    let candidate = config
        .candidate_paths
        .get(&provider)
        .cloned()
        .or_else(|| find_on_path(provider.command_name()));
    let Some(candidate) = candidate else {
        return ProviderStatus::unavailable(provider, "Official runtime not found");
    };
    let client = match validate_identity(provider, &candidate, config, deadline) {
        Ok(client) => client,
        Err(error) => {
            let mut status = ProviderStatus::unavailable(
                provider,
                if config.cancellation.load(Ordering::Relaxed) {
                    "Connection check cancelled"
                } else if matches!(&error, AiError::IdentityUnverified(_)) {
                    "Runtime identity could not be verified"
                } else {
                    "Official runtime identity probe did not complete"
                },
            );
            if config.cancellation.load(Ordering::Relaxed) {
                return status;
            }
            status.connection = if matches!(&error, AiError::IdentityUnverified(_)) {
                ConnectionState::IdentityUnverified
            } else {
                ConnectionState::Degraded
            };
            return status;
        }
    };
    match probe_account(&client, config, deadline) {
        Ok(probe) if probe.signed_in => ready_status(
            client,
            config,
            probe.image_generation_advertised,
            probe.allowance,
        ),
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
            allowance: None,
            detail: "Official runtime found; subscription sign-in is unavailable".into(),
            client: Some(client),
        },
        Err(_) if config.cancellation.load(Ordering::Relaxed) => {
            ProviderStatus::unavailable(provider, "Connection check cancelled")
        }
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
            allowance: None,
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
                || message.starts_with("Claude Code omitted its account state")
                || message.starts_with("Claude Code omitted its authentication route") =>
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
    deadline: Instant,
) -> Result<ValidatedClient, AiError> {
    let executable = resolve_provider_executable(provider, candidate, config, deadline)?;
    if !executable.is_file() || is_shell_wrapper(&executable)? {
        return Err(AiError::IdentityUnverified(
            "Provider entry point is an unverified wrapper".into(),
        ));
    }
    let args: &[&str] = match provider {
        ProviderId::CodexSubscription | ProviderId::ClaudeCode => &["--version"],
        ProviderId::GrokBuild => &["version"],
    };
    let capture = capture_bounded_cancellable(
        &executable,
        args.iter().copied(),
        &config.work_dir,
        remaining(config, deadline)?,
        4096,
        &config.cancellation,
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
    let help = capture_bounded_cancellable(
        &executable,
        help_args.iter().copied(),
        &config.work_dir,
        remaining(config, deadline)?,
        192 * 1024,
        &config.cancellation,
    )?;
    if !help.status.success() || help.truncated || !provider_help_is_valid(provider, &help.stdout) {
        return Err(AiError::IdentityUnverified(
            "Provider interface fingerprint did not match".into(),
        ));
    }
    if provider == ProviderId::ClaudeCode {
        let auth_help = capture_bounded_cancellable(
            &executable,
            ["auth", "login", "--help"],
            &config.work_dir,
            remaining(config, deadline)?,
            32 * 1024,
            &config.cancellation,
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

/// Resolve the symlink shims created by mise without treating a general
/// wrapper as a provider runtime. Mise installs shims as
/// `<data-dir>/mise/shims/<command> -> <mise executable>`; only that exact
/// shape is allowed to run the manager. The runtime returned by `mise which`
/// still has to pass every normal provider identity and isolation probe.
fn resolve_provider_executable(
    provider: ProviderId,
    candidate: &Path,
    config: &DiscoveryConfig,
    deadline: Instant,
) -> Result<PathBuf, AiError> {
    let executable = candidate.canonicalize()?;
    if !executable.is_file() || is_shell_wrapper(&executable)? {
        return Err(AiError::IdentityUnverified(
            "Provider entry point is an unverified wrapper".into(),
        ));
    }
    if !is_recognized_mise_shim(candidate, &executable, provider.command_name())? {
        return Ok(executable);
    }

    let capture = capture_bounded_cancellable(
        &executable,
        ["--quiet", "which", provider.command_name()],
        &config.work_dir,
        remaining(config, deadline)?,
        4096,
        &config.cancellation,
    )?;
    if !capture.status.success() || capture.truncated {
        return Err(AiError::IdentityUnverified(
            "Runtime manager could not resolve the provider executable".into(),
        ));
    }
    resolved_mise_runtime(
        &capture.stdout,
        candidate,
        &executable,
        provider.command_name(),
    )
}

fn is_recognized_mise_shim(
    candidate: &Path,
    canonical_target: &Path,
    command_name: &str,
) -> Result<bool, AiError> {
    let metadata = fs::symlink_metadata(candidate)?;
    let parent = candidate.parent();
    Ok(metadata.file_type().is_symlink()
        && candidate
            .file_name()
            .is_some_and(|name| name == command_name)
        && parent
            .and_then(Path::file_name)
            .is_some_and(|name| name == "shims")
        && parent
            .and_then(Path::parent)
            .and_then(Path::file_name)
            .is_some_and(|name| name == "mise")
        && canonical_target
            .file_name()
            .is_some_and(|name| name == "mise")
        && canonical_target.is_file()
        && !is_shell_wrapper(canonical_target)?)
}

fn resolved_mise_runtime(
    output: &[u8],
    candidate: &Path,
    mise_executable: &Path,
    command_name: &str,
) -> Result<PathBuf, AiError> {
    let output = std::str::from_utf8(output).map_err(|_| mise_resolution_unverified())?;
    let output = output.strip_suffix('\n').unwrap_or(output);
    if output.is_empty() || output.chars().any(char::is_control) {
        return Err(mise_resolution_unverified());
    }
    let path = Path::new(output);
    if !path.is_absolute() || path.file_name().is_none_or(|name| name != command_name) {
        return Err(mise_resolution_unverified());
    }
    let runtime = path
        .canonicalize()
        .map_err(|_| mise_resolution_unverified())?;
    let candidate_target = candidate
        .canonicalize()
        .map_err(|_| mise_resolution_unverified())?;
    if runtime == mise_executable || runtime == candidate_target || !runtime.is_file() {
        return Err(mise_resolution_unverified());
    }
    Ok(runtime)
}

fn mise_resolution_unverified() -> AiError {
    AiError::IdentityUnverified("Runtime manager returned an invalid provider executable".into())
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
    allowance: Option<Vec<AllowanceWindow>>,
}

fn probe_account(
    client: &ValidatedClient,
    config: &DiscoveryConfig,
    deadline: Instant,
) -> Result<AccountProbe, AiError> {
    match client.provider {
        ProviderId::CodexSubscription => {
            let probe = codex::probe(
                &client.executable,
                &config.work_dir,
                remaining(config, deadline)?,
                config.max_protocol_line_bytes,
                &config.cancellation,
            )?;
            Ok(AccountProbe {
                signed_in: probe.signed_in_with_chatgpt,
                image_generation_advertised: probe.image_generation_advertised,
                allowance: probe.allowance,
            })
        }
        ProviderId::ClaudeCode => Ok(AccountProbe {
            signed_in: claude::subscription_signed_in_cancellable(
                &client.executable,
                &config.work_dir,
                remaining(config, deadline)?,
                &config.cancellation,
            )?,
            image_generation_advertised: false,
            allowance: None,
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
    allowance: Option<Vec<AllowanceWindow>>,
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
        allowance,
        detail: "Official runtime and subscription login verified".into(),
        client: Some(client),
    }
}

fn remaining(config: &DiscoveryConfig, deadline: Instant) -> Result<Duration, AiError> {
    if config.cancellation.load(Ordering::Relaxed) {
        return Err(AiError::Protocol("cancelled".into()));
    }
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        Err(AiError::TimedOut)
    } else {
        Ok(remaining)
    }
}

fn create_private_probe_dir(path: &Path) -> Result<(), AiError> {
    fs::create_dir_all(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    #[cfg(windows)]
    crate::private_dir::make_private(path)?;
    Ok(())
}

fn find_on_path(name: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;
    env::split_paths(&path).find_map(|directory| {
        #[cfg(windows)]
        {
            let executable = directory.join(format!("{name}.exe"));
            if executable.is_file() {
                return Some(executable);
            }
            npm_native_executable(&directory, name)
        }
        #[cfg(not(windows))]
        {
            let candidate = directory.join(name);
            candidate.is_file().then_some(candidate)
        }
    })
}

/// npm installs Codex on Windows as a `codex.cmd` launcher for a Node script
/// that starts the platform package's native `codex.exe`. Recognise only that
/// exact layout and use the binary directly, without the cmd.exe and Node
/// layers; it still has to pass every identity and isolation probe.
#[cfg(windows)]
fn npm_native_executable(directory: &Path, name: &str) -> Option<PathBuf> {
    if name != "codex" || !directory.join("codex.cmd").is_file() {
        return None;
    }
    let (platform, triple) = if cfg!(target_arch = "aarch64") {
        ("codex-win32-arm64", "aarch64-pc-windows-msvc")
    } else {
        ("codex-win32-x64", "x86_64-pc-windows-msvc")
    };
    let package = directory.join("node_modules").join("@openai").join("codex");
    [
        package.join("node_modules").join("@openai").join(platform),
        package,
    ]
    .into_iter()
    .map(|root| {
        root.join("vendor")
            .join(triple)
            .join("bin")
            .join("codex.exe")
    })
    .find(|candidate| candidate.is_file())
}

fn is_shell_wrapper(path: &Path) -> Result<bool, AiError> {
    // Windows script launchers are wrappers as well.
    #[cfg(windows)]
    if path.extension().is_some_and(|extension| {
        ["cmd", "bat", "ps1"]
            .iter()
            .any(|wrapper| extension.eq_ignore_ascii_case(wrapper))
    }) {
        return Ok(true);
    }
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

// The fixtures are executable shell scripts, so these tests are Unix-only.
#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::{
        fs,
        os::unix::fs::{PermissionsExt, symlink},
        sync::atomic::{AtomicUsize, Ordering as AtomicOrdering},
    };

    fn write_executable_fixture(path: &Path, contents: &[u8]) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
    }

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
        assert!(
            validate_identity(
                ProviderId::CodexSubscription,
                &wrapper,
                &config,
                Instant::now() + config.probe_timeout,
            )
            .is_err()
        );
    }

    #[test]
    fn mise_shim_recognition_requires_symlink_layout_target_and_non_wrapper() {
        let root = tempfile::tempdir().unwrap();
        let shims = root.path().join("mise/shims");
        fs::create_dir_all(&shims).unwrap();

        let mise = root.path().join("bin/mise");
        write_executable_fixture(&mise, b"\x7fELFfixture");
        let codex_shim = shims.join("codex");
        symlink(&mise, &codex_shim).unwrap();
        assert!(
            is_recognized_mise_shim(&codex_shim, &mise.canonicalize().unwrap(), "codex").unwrap()
        );

        let unrelated_shims = root.path().join("other/shims");
        fs::create_dir_all(&unrelated_shims).unwrap();
        let unrelated_shim = unrelated_shims.join("codex");
        symlink(&mise, &unrelated_shim).unwrap();
        assert!(
            !is_recognized_mise_shim(&unrelated_shim, &mise.canonicalize().unwrap(), "codex")
                .unwrap()
        );

        let unrelated = root.path().join("bin/provider");
        write_executable_fixture(&unrelated, b"\x7fELFfixture");
        let claude_shim = shims.join("claude");
        symlink(&unrelated, &claude_shim).unwrap();
        assert!(
            !is_recognized_mise_shim(&claude_shim, &unrelated.canonicalize().unwrap(), "claude")
                .unwrap()
        );

        let wrapper = root.path().join("wrapper/mise");
        write_executable_fixture(&wrapper, b"#!/bin/sh\nexit 0\n");
        let grok_shim = shims.join("grok");
        symlink(&wrapper, &grok_shim).unwrap();
        assert!(
            !is_recognized_mise_shim(&grok_shim, &wrapper.canonicalize().unwrap(), "grok").unwrap()
        );
    }

    #[test]
    fn mise_resolution_accepts_one_absolute_existing_provider_path() {
        let root = tempfile::tempdir().unwrap();
        let mise = root.path().join("bin/mise");
        let runtime = root.path().join("installs/codex");
        let candidate = root.path().join("mise/shims/codex");
        write_executable_fixture(&mise, b"\x7fELFfixture");
        write_executable_fixture(&runtime, b"\x7fELFfixture");
        fs::create_dir_all(candidate.parent().unwrap()).unwrap();
        symlink(&mise, &candidate).unwrap();

        let output = format!("{}\n", runtime.display());
        assert_eq!(
            resolved_mise_runtime(
                output.as_bytes(),
                &candidate,
                &mise.canonicalize().unwrap(),
                "codex"
            )
            .unwrap(),
            runtime.canonicalize().unwrap()
        );
    }

    #[test]
    fn mise_resolution_rejects_malformed_missing_and_recursive_output() {
        let root = tempfile::tempdir().unwrap();
        let mise = root.path().join("bin/mise");
        let runtime = root.path().join("installs/codex");
        let candidate = root.path().join("mise/shims/codex");
        write_executable_fixture(&mise, b"\x7fELFfixture");
        write_executable_fixture(&runtime, b"\x7fELFfixture");
        fs::create_dir_all(candidate.parent().unwrap()).unwrap();
        symlink(&mise, &candidate).unwrap();
        let mise = mise.canonicalize().unwrap();

        let multiple = format!("{}\n{}\n", runtime.display(), runtime.display());
        let missing = root.path().join("missing/codex");
        let recursive = format!("{}\n", candidate.display());
        for output in [
            Vec::new(),
            b"relative/codex\n".to_vec(),
            multiple.into_bytes(),
            format!("{}\r\n", runtime.display()).into_bytes(),
            vec![0xff, b'\n'],
            format!("{}\n", missing.display()).into_bytes(),
            recursive.into_bytes(),
        ] {
            assert!(
                matches!(
                    resolved_mise_runtime(&output, &candidate, &mise, "codex"),
                    Err(AiError::IdentityUnverified(_))
                ),
                "unexpectedly accepted {output:?}"
            );
        }
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

    #[test]
    fn provider_workers_overlap_and_preserve_provider_order() {
        let active = AtomicUsize::new(0);
        let maximum = AtomicUsize::new(0);
        let statuses = map_providers_parallel(|provider| {
            let now = active.fetch_add(1, AtomicOrdering::SeqCst) + 1;
            maximum.fetch_max(now, AtomicOrdering::SeqCst);
            std::thread::sleep(Duration::from_millis(50));
            active.fetch_sub(1, AtomicOrdering::SeqCst);
            ProviderStatus::unavailable(provider, "fixture")
        });

        assert!(maximum.load(AtomicOrdering::SeqCst) > 1);
        assert_eq!(
            statuses
                .iter()
                .map(|status| status.provider)
                .collect::<Vec<_>>(),
            ProviderId::ALL.to_vec()
        );
    }

    #[test]
    fn cancelled_discovery_does_not_start_runtime_probes() {
        let root = tempfile::tempdir().unwrap();
        let cancellation = Arc::new(AtomicBool::new(true));
        let config = DiscoveryConfig {
            work_dir: root.path().join("probe"),
            cancellation,
            ..DiscoveryConfig::default()
        };

        let statuses = discover_providers(&config);
        assert_eq!(statuses.len(), ProviderId::ALL.len());
        assert!(statuses.iter().all(|status| {
            status.connection == ConnectionState::Unavailable
                && status.detail == "Connection check cancelled"
                && status.client.is_none()
        }));
    }
}

#[cfg(all(test, windows))]
mod windows_tests {
    use super::*;

    #[test]
    fn npm_codex_launcher_resolves_only_its_native_runtime() {
        let root = tempfile::tempdir().unwrap();
        let npm = root.path();
        let native = npm.join(
            "node_modules/@openai/codex/node_modules/@openai/codex-win32-x64/vendor/x86_64-pc-windows-msvc/bin",
        );
        fs::create_dir_all(&native).unwrap();
        fs::write(native.join("codex.exe"), b"MZ").unwrap();
        // Without npm's launcher the package is not an installed command.
        assert_eq!(npm_native_executable(npm, "codex"), None);
        fs::write(npm.join("codex.cmd"), b"@ECHO off").unwrap();
        assert_eq!(
            npm_native_executable(npm, "codex"),
            Some(native.join("codex.exe"))
        );
        assert_eq!(npm_native_executable(npm, "claude"), None);
    }

    #[test]
    fn windows_script_launchers_are_wrappers() {
        let root = tempfile::tempdir().unwrap();
        for name in ["codex.cmd", "claude.BAT", "grok.ps1"] {
            let path = root.path().join(name);
            fs::write(&path, b"@ECHO off").unwrap();
            assert!(is_shell_wrapper(&path).unwrap(), "{name}");
        }
        let binary = root.path().join("claude.exe");
        fs::write(&binary, b"MZ").unwrap();
        assert!(!is_shell_wrapper(&binary).unwrap());
    }
}
