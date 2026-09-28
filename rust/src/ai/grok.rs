use crate::ai::{
    process::{JsonLineChild, sanitize_provider_error},
    types::{AiError, JobEvent, JobFailure, JobOutcome, JobRequest, JobResult, ProviderId},
};
use serde_json::{Value, json};
use std::{
    ffi::OsString,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::SyncSender,
    },
    time::{Duration, Instant},
};

#[allow(dead_code)] // Enabled once the isolated Grok ACP profile is qualified.
pub(crate) fn subscription_signed_in(
    executable: &Path,
    cwd: &Path,
    timeout: Duration,
    max_line_bytes: usize,
) -> Result<bool, AiError> {
    let cancel = AtomicBool::new(false);
    let deadline = Instant::now() + timeout;
    let mut process = start(executable, cwd, max_line_bytes)?;
    let init = initialize(&mut process, deadline, &cancel)?;
    if !has_cached_token(&init) {
        return Ok(false);
    }
    process.send(&json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "authenticate",
        "params": { "methodId": "cached_token", "_meta": { "headless": true } }
    }))?;
    process.wait_for_id(2, deadline, &cancel)?;
    Ok(true)
}

pub(crate) fn run(
    request: JobRequest,
    cancelled: Arc<AtomicBool>,
    events: SyncSender<JobEvent>,
) -> JobOutcome {
    match run_inner(&request, &cancelled, &events) {
        Ok(result) => {
            let _ = events.try_send(JobEvent::Finished);
            JobOutcome::Completed(result)
        }
        Err(RunError::Cancelled) => JobOutcome::Cancelled,
        Err(RunError::BeforeSubmit(error)) => JobOutcome::Failed(failure(error)),
        Err(RunError::AfterSubmit(error)) => JobOutcome::OutcomeUnknown(failure(error)),
        Err(RunError::Provider(error)) => JobOutcome::Failed(error),
    }
}

enum RunError {
    Cancelled,
    BeforeSubmit(AiError),
    AfterSubmit(AiError),
    Provider(JobFailure),
}

fn run_inner(
    request: &JobRequest,
    cancelled: &AtomicBool,
    events: &SyncSender<JobEvent>,
) -> Result<JobResult, RunError> {
    request.validate().map_err(RunError::BeforeSubmit)?;
    if request.output_schema.is_some() {
        return Err(RunError::BeforeSubmit(AiError::InvalidRequest(
            "Strict structured output has not been qualified for Grok ACP".into(),
        )));
    }
    let work_dir = request
        .work_dir
        .canonicalize()
        .map_err(AiError::from)
        .map_err(RunError::BeforeSubmit)?;
    let deadline = Instant::now() + request.limits.max_runtime;
    let mut process = start(
        &request.client.executable,
        &work_dir,
        request.limits.max_protocol_line_bytes,
    )
    .map_err(RunError::BeforeSubmit)?;
    let _ = events.try_send(JobEvent::Started);
    let init = initialize(&mut process, deadline, cancelled).map_err(RunError::BeforeSubmit)?;
    if !has_cached_token(&init) {
        return Err(RunError::BeforeSubmit(AiError::InvalidRequest(
            "Grok Build does not expose a cached subscription login".into(),
        )));
    }
    process
        .send(&json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "authenticate",
            "params": { "methodId": "cached_token", "_meta": { "headless": true } }
        }))
        .map_err(RunError::BeforeSubmit)?;
    process
        .wait_for_id(2, deadline, cancelled)
        .map_err(RunError::BeforeSubmit)?;
    process
        .send(&json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "session/new",
            "params": { "cwd": work_dir, "mcpServers": [] }
        }))
        .map_err(RunError::BeforeSubmit)?;
    let session = process
        .wait_for_id(3, deadline, cancelled)
        .map_err(RunError::BeforeSubmit)?;
    let session_id = session
        .get("sessionId")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            RunError::BeforeSubmit(AiError::Protocol("Grok ACP omitted sessionId".into()))
        })?
        .to_owned();
    process
        .send(&json!({
            "jsonrpc": "2.0",
            "id": 4,
            "method": "session/prompt",
            "params": {
                "sessionId": session_id,
                "prompt": [{ "type": "text", "text": request.prompt }]
            }
        }))
        .map_err(|error| {
            // session/prompt is the billable boundary. A broken write cannot
            // prove the ACP runtime did not receive it, so never make retry
            // look automatically safe.
            if cancelled.load(Ordering::Relaxed) {
                RunError::Cancelled
            } else {
                RunError::AfterSubmit(error)
            }
        })?;
    let _ = events.try_send(JobEvent::Submitted);
    let mut text = String::new();
    let mut prompt_completed = false;
    loop {
        if cancelled.load(Ordering::Relaxed) {
            process.terminate();
            return Err(RunError::Cancelled);
        }
        let receive_deadline = if prompt_completed {
            (Instant::now() + Duration::from_millis(350)).min(deadline)
        } else {
            deadline
        };
        let message = match process.recv_json(receive_deadline, cancelled) {
            Ok(message) => message,
            Err(AiError::TimedOut) if prompt_completed => {
                return Ok(JobResult {
                    provider: ProviderId::GrokBuild,
                    text,
                    structured_output: None,
                    assets: Vec::new(),
                });
            }
            Err(error) if cancelled.load(Ordering::Relaxed) => {
                let _ = error;
                return Err(RunError::Cancelled);
            }
            Err(error) => return Err(RunError::AfterSubmit(error)),
        };
        if message.get("id").and_then(Value::as_i64) == Some(4) {
            if let Some(error) = message.get("error") {
                let detail = error
                    .get("message")
                    .and_then(Value::as_str)
                    .map(sanitize_provider_error)
                    .unwrap_or_else(|| "Grok ACP request failed".into());
                return Err(RunError::Provider(JobFailure::new(
                    "provider_failed",
                    detail,
                    false,
                )));
            }
            prompt_completed = true;
            continue;
        }
        if message.get("method").and_then(Value::as_str) != Some("session/update") {
            continue;
        }
        let update = message.pointer("/params/update").unwrap_or(&Value::Null);
        let update_type = update
            .get("sessionUpdate")
            .and_then(Value::as_str)
            .unwrap_or("");
        if update_type.contains("tool") || update_type.contains("terminal") {
            process.terminate();
            return Err(RunError::AfterSubmit(AiError::Protocol(
                "Grok ACP attempted a disallowed tool operation".into(),
            )));
        }
        if update_type == "agent_message_chunk" {
            if let Some(delta) = update.pointer("/content/text").and_then(Value::as_str) {
                request
                    .limits
                    .append_result_text(&mut text, delta)
                    .map_err(RunError::AfterSubmit)?;
                let _ = events.try_send(JobEvent::TextDelta(delta.to_owned()));
            }
        }
    }
}

fn start(executable: &Path, cwd: &Path, max_line_bytes: usize) -> Result<JsonLineChild, AiError> {
    let args: Vec<OsString> = vec![
        "--no-auto-update".into(),
        "--cwd".into(),
        cwd.as_os_str().to_owned(),
        "--tools".into(),
        "".into(),
        "--no-subagents".into(),
        "--no-memory".into(),
        "--disable-web-search".into(),
        "--max-turns".into(),
        "1".into(),
        "agent".into(),
        "stdio".into(),
    ];
    JsonLineChild::spawn(executable, args, cwd, max_line_bytes)
}

fn initialize(
    process: &mut JsonLineChild,
    deadline: Instant,
    cancelled: &AtomicBool,
) -> Result<Value, AiError> {
    process.send(&json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": 1,
            "clientCapabilities": {
                "fs": { "readTextFile": false, "writeTextFile": false },
                "terminal": false
            }
        }
    }))?;
    process.wait_for_id(1, deadline, cancelled)
}

fn has_cached_token(init: &Value) -> bool {
    init.get("authMethods")
        .and_then(Value::as_array)
        .is_some_and(|methods| {
            methods
                .iter()
                .any(|method| method.get("id").and_then(Value::as_str) == Some("cached_token"))
        })
}

fn failure(error: AiError) -> JobFailure {
    let code = match &error {
        AiError::TimedOut => "timeout",
        AiError::UnsupportedCapability { .. } => "unsupported_capability",
        AiError::InvalidRequest(_) => "invalid_request",
        AiError::IdentityUnverified(_) => "identity_unverified",
        AiError::Io(_) | AiError::Disconnected => "runtime_unavailable",
        AiError::Protocol(_) => "protocol_error",
    };
    JobFailure::new(code, error.to_string(), false)
}
