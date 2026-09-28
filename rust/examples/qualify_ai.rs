//! Bounded live qualification for Omuse's subscription-runtime adapters.
//!
//! `--discover` performs identity/account/capability probes only. The
//! `--assistant`, `--image`, and `--edit` modes make exactly one explicitly
//! requested model submission and never fall back to API-key billing.

use omuse::ai::{
    Capability, ConnectionState, DiscoveryConfig, JobEvent, JobOperation, JobOutcome, JobRequest,
    ProviderId, ProviderStatus, ReferenceAsset, discover_providers, spawn_job,
};
use serde_json::json;
use std::{
    env, fs,
    path::PathBuf,
    process::ExitCode,
    thread,
    time::{Duration, Instant},
};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("qualification=failed detail={message}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = env::args().skip(1).collect();
    let mode = match args.as_slice() {
        [flag] if flag == "--discover" => Mode::Discover,
        [flag, provider] if flag == "--assistant" => {
            Mode::Assistant(parse_provider(provider, false)?)
        }
        [flag, provider] if flag == "--image" => Mode::Image(parse_provider(provider, true)?),
        [flag, provider] if flag == "--edit" => Mode::Edit(parse_provider(provider, true)?),
        _ => {
            return Err(
                "usage: qualify_ai --discover | --assistant codex|claude|grok | --image codex | --edit codex".into(),
            );
        }
    };

    let work_dir = qualification_dir()?;
    let config = DiscoveryConfig {
        work_dir: work_dir.clone(),
        probe_timeout: Duration::from_secs(12),
        ..DiscoveryConfig::default()
    };
    let statuses = discover_providers(&config);
    print_discovery(&statuses);
    match mode {
        Mode::Discover => return Ok(()),
        Mode::Assistant(provider) => qualify_assistant(provider, statuses, work_dir),
        Mode::Image(provider) => qualify_image(provider, statuses, work_dir),
        Mode::Edit(provider) => qualify_edit(provider, statuses, work_dir),
    }
}

enum Mode {
    Discover,
    Assistant(ProviderId),
    Image(ProviderId),
    Edit(ProviderId),
}

fn parse_provider(value: &str, image: bool) -> Result<ProviderId, String> {
    let provider = match value {
        "codex" => ProviderId::CodexSubscription,
        "claude" if !image => ProviderId::ClaudeCode,
        "grok" if !image => ProviderId::GrokBuild,
        _ if image => return Err("only Codex is qualified for this image operation".into()),
        _ => return Err("provider must be codex, claude, or grok".into()),
    };
    Ok(provider)
}

fn qualification_dir() -> Result<PathBuf, String> {
    let path = env::temp_dir().join(format!("omuse-ai-qualify-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&path).map_err(|_| "could not create the private qualification workspace")?;
    set_private_permissions(&path)?;
    Ok(path)
}

#[cfg(unix)]
fn set_private_permissions(path: &std::path::Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(|_| "could not protect the qualification workspace".to_owned())
}

#[cfg(not(unix))]
fn set_private_permissions(_path: &std::path::Path) -> Result<(), String> {
    Err("qualification workspace isolation is not available on this platform".into())
}

fn print_discovery(statuses: &[ProviderStatus]) {
    for status in statuses {
        println!(
            "provider={} connection={:?} billing={:?} version={} detail={}",
            provider_slug(status.provider),
            status.connection,
            status.billing,
            status.version.as_deref().unwrap_or("unknown"),
            serde_json::to_string(&status.detail).unwrap_or_else(|_| "\"unavailable\"".into())
        );
        for capability in &status.capabilities {
            println!(
                "capability={} operation={:?} evidence={:?}",
                provider_slug(status.provider),
                capability.capability,
                capability.evidence
            );
        }
    }
}

fn qualify_assistant(
    provider: ProviderId,
    statuses: Vec<ProviderStatus>,
    work_dir: PathBuf,
) -> Result<(), String> {
    let client = ready_client(provider, Capability::AssistantStreaming, statuses)?;
    let schema = json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "summary": { "type": "string", "maxLength": 160 },
            "operations": {
                "type": "array",
                "maxItems": 1,
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "kind": { "type": "string", "enum": ["addCaption"] },
                        "text": { "type": "string", "maxLength": 80 }
                    },
                    "required": ["kind", "text"]
                }
            }
        },
        "required": ["summary", "operations"]
    });
    let mut request = JobRequest::new(
        client,
        JobOperation::Assistant,
        "Return one harmless creative plan for a square welcome card. Add one short caption. Do not inspect files or call tools.",
        work_dir,
    )
    .with_output_schema(schema);
    request.limits.max_runtime = Duration::from_secs(90);
    let outcome = run_job(request)?;
    match outcome {
        JobOutcome::Completed(result) if result.structured_output.is_some() => {
            println!(
                "qualification=passed provider={} operation=assistant structured=true text_bytes={}",
                provider_slug(provider),
                result.text.len()
            );
            Ok(())
        }
        JobOutcome::Completed(_) => Err("assistant returned no validated structured output".into()),
        other => Err(outcome_code(&other).into()),
    }
}

fn qualify_image(
    provider: ProviderId,
    statuses: Vec<ProviderStatus>,
    work_dir: PathBuf,
) -> Result<(), String> {
    let client = ready_client(provider, Capability::ImageGeneration, statuses)?;
    let mut request = JobRequest::new(
        client,
        JobOperation::GenerateImage,
        "A minimal 256 by 256 test tile: one matte cobalt-blue circle centered on a warm off-white background. Flat colors, no texture, no shadow, no text, no logo.",
        work_dir.clone(),
    );
    request.limits.max_runtime = Duration::from_secs(240);
    request.limits.max_image_pixels = 4 * 1024 * 1024;
    let outcome = run_job(request)?;
    match outcome {
        JobOutcome::Completed(result) if result.assets.len() == 1 => {
            let asset = &result.assets[0];
            println!(
                "qualification=passed provider={} operation=image assets=1 media={} width={} height={} bytes={}",
                provider_slug(provider),
                asset.media_type,
                asset.width,
                asset.height,
                asset.byte_len
            );
            println!(
                "workspace=/tmp/{} asset={}",
                work_dir
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("omuse-ai-qualify-redacted"),
                asset
                    .path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("result-image")
            );
            Ok(())
        }
        JobOutcome::Completed(result) => Err(format!(
            "image qualification returned {} validated assets; expected exactly one",
            result.assets.len()
        )),
        other => Err(outcome_code(&other).into()),
    }
}

fn qualify_edit(
    provider: ProviderId,
    statuses: Vec<ProviderStatus>,
    work_dir: PathBuf,
) -> Result<(), String> {
    let client = ready_client(provider, Capability::ImageEditing, statuses)?;
    let reference = write_edit_reference(&work_dir)?;
    let mut request = JobRequest::new(
        client,
        JobOperation::EditImage,
        "Change only the small coral square in the supplied test tile to cobalt blue. Preserve the warm off-white background and every other pixel as closely as possible. Do not add text, logos, shadows, or extra objects.",
        work_dir.clone(),
    )
    .with_references(vec![ReferenceAsset { path: reference }]);
    request.limits.max_runtime = Duration::from_secs(240);
    request.limits.max_image_pixels = 4 * 1024 * 1024;
    let outcome = run_job(request)?;
    match outcome {
        JobOutcome::Completed(result) if result.assets.len() == 1 => {
            let asset = &result.assets[0];
            println!(
                "qualification=passed provider={} operation=edit assets=1 media={} width={} height={} bytes={}",
                provider_slug(provider),
                asset.media_type,
                asset.width,
                asset.height,
                asset.byte_len
            );
            println!(
                "workspace=/tmp/{} asset={}",
                work_dir
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("omuse-ai-qualify-redacted"),
                asset
                    .path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("result-image")
            );
            Ok(())
        }
        JobOutcome::Completed(result) => Err(format!(
            "edit qualification returned {} validated assets; expected exactly one",
            result.assets.len()
        )),
        other => Err(outcome_code(&other).into()),
    }
}

fn write_edit_reference(work_dir: &std::path::Path) -> Result<PathBuf, String> {
    use image::{Rgba, RgbaImage};

    let path = work_dir.join("edit-reference.png");
    let image = RgbaImage::from_fn(64, 64, |x, y| {
        if (20..44).contains(&x) && (20..44).contains(&y) {
            Rgba([240, 136, 90, 255])
        } else {
            Rgba([247, 235, 214, 255])
        }
    });
    image
        .save(&path)
        .map_err(|_| "could not create the bounded edit reference".to_owned())?;
    Ok(path)
}

fn ready_client(
    provider: ProviderId,
    capability: Capability,
    statuses: Vec<ProviderStatus>,
) -> Result<omuse::ai::ValidatedClient, String> {
    let status = statuses
        .into_iter()
        .find(|status| status.provider == provider)
        .ok_or("provider was not discovered")?;
    if status.connection != ConnectionState::Ready {
        return Err("official runtime subscription login is not ready".into());
    }
    if !status.may_attempt(capability) {
        return Err("runtime does not advertise this capability".into());
    }
    status
        .client
        .ok_or("validated runtime client is unavailable".into())
}

fn run_job(request: JobRequest) -> Result<JobOutcome, String> {
    let max_runtime = request.limits.max_runtime;
    let mut handle = spawn_job(request).map_err(|_| "could not start bounded worker")?;
    let deadline = Instant::now() + max_runtime + Duration::from_secs(5);
    let mut saw_submission = false;
    loop {
        while let Ok(Some(event)) = handle.try_recv_event() {
            match event {
                JobEvent::Submitted => saw_submission = true,
                JobEvent::UsageLimit { .. } => {
                    println!("event=usage_limit");
                }
                JobEvent::ImageReady(asset) => {
                    println!(
                        "event=image_ready media={} width={} height={} bytes={}",
                        asset.media_type, asset.width, asset.height, asset.byte_len
                    );
                }
                JobEvent::Started | JobEvent::TextDelta(_) | JobEvent::Finished => {}
            }
        }
        if let Some(outcome) = handle
            .try_outcome()
            .map_err(|_| "worker result channel failed")?
        {
            println!("submitted={saw_submission}");
            if let JobOutcome::Failed(failure) | JobOutcome::OutcomeUnknown(failure) = &outcome {
                // Adapter errors already redact provider credentials and
                // external paths; JSON escaping keeps diagnostics one line.
                println!(
                    "failure_detail={}",
                    serde_json::to_string(&failure.message).unwrap_or_default()
                );
            }
            return Ok(outcome);
        }
        if Instant::now() >= deadline {
            handle.cancel();
            return Err("worker exceeded its bounded deadline".into());
        }
        thread::sleep(Duration::from_millis(25));
    }
}

fn outcome_code(outcome: &JobOutcome) -> &'static str {
    match outcome {
        JobOutcome::Completed(_) => "unexpected_completed_result",
        JobOutcome::Cancelled => "cancelled",
        JobOutcome::Failed(failure) => failure.code,
        JobOutcome::OutcomeUnknown(failure) => failure.code,
    }
}

fn provider_slug(provider: ProviderId) -> &'static str {
    match provider {
        ProviderId::CodexSubscription => "codex",
        ProviderId::ClaudeCode => "claude",
        ProviderId::GrokBuild => "grok",
    }
}
