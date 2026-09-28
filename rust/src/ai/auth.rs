use crate::ai::{
    claude, codex,
    process::sanitized_command,
    types::{AiError, ProviderId, ValidatedClient},
};
use serde_json::{Value, json};
use std::{
    process::Stdio,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError, TryRecvError},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

#[derive(Clone, Debug)]
pub struct AuthRequest {
    pub client: ValidatedClient,
    /// A private, empty directory used only as the login process cwd.
    pub work_dir: std::path::PathBuf,
    pub timeout: Duration,
    pub event_capacity: usize,
}

impl AuthRequest {
    pub fn new(client: ValidatedClient, work_dir: impl Into<std::path::PathBuf>) -> Self {
        Self {
            client,
            work_dir: work_dir.into(),
            timeout: Duration::from_secs(300),
            event_capacity: 16,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AuthEvent {
    Started,
    /// Open this official HTTPS URL only as the direct result of the user's
    /// Connect/Sign in click.
    OpenUrl(String),
    /// The unmodified provider CLI owns the browser flow and token storage.
    AwaitingProviderBrowser,
    Completed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthFailure {
    pub code: &'static str,
    pub message: String,
}

impl AuthFailure {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AuthOutcome {
    Connected,
    Cancelled,
    Failed(AuthFailure),
}

pub struct AuthHandle {
    provider: ProviderId,
    cancelled: Arc<AtomicBool>,
    events: Receiver<AuthEvent>,
    outcome: Receiver<AuthOutcome>,
    worker: Option<JoinHandle<()>>,
}

impl AuthHandle {
    pub fn provider(&self) -> ProviderId {
        self.provider
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

    pub fn try_event(&self) -> Result<Option<AuthEvent>, AiError> {
        match self.events.try_recv() {
            Ok(event) => Ok(Some(event)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => Err(AiError::Disconnected),
        }
    }

    pub fn try_outcome(&mut self) -> Result<Option<AuthOutcome>, AiError> {
        match self.outcome.try_recv() {
            Ok(outcome) => {
                if let Some(worker) = self.worker.take() {
                    let _ = worker.join();
                }
                Ok(Some(outcome))
            }
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => Err(AiError::Disconnected),
        }
    }

    pub fn wait(&mut self, timeout: Duration) -> Result<Option<AuthOutcome>, AiError> {
        match self.outcome.recv_timeout(timeout) {
            Ok(outcome) => {
                if let Some(worker) = self.worker.take() {
                    let _ = worker.join();
                }
                Ok(Some(outcome))
            }
            Err(RecvTimeoutError::Timeout) => Ok(None),
            Err(RecvTimeoutError::Disconnected) => Err(AiError::Disconnected),
        }
    }
}

impl Drop for AuthHandle {
    fn drop(&mut self) {
        self.cancel();
    }
}

pub fn begin_auth(request: AuthRequest) -> Result<AuthHandle, AiError> {
    if !request.work_dir.is_dir() {
        return Err(AiError::InvalidRequest(
            "Create the private sign-in workspace before connecting".into(),
        ));
    }
    validate_private_work_dir(&request.work_dir)?;
    if request.client.provider == ProviderId::GrokBuild {
        return Err(AiError::UnsupportedCapability {
            provider: ProviderId::GrokBuild,
            capability: crate::ai::Capability::AssistantStreaming,
        });
    }
    let provider = request.client.provider;
    let cancelled = Arc::new(AtomicBool::new(false));
    let worker_cancelled = cancelled.clone();
    let (event_sender, events) = mpsc::sync_channel(request.event_capacity.max(1));
    let (outcome_sender, outcome) = mpsc::sync_channel(1);
    let worker = thread::Builder::new()
        .name(format!("omuse-auth-{provider:?}"))
        .spawn(move || {
            let result = match provider {
                ProviderId::CodexSubscription => {
                    codex_login(&request, &worker_cancelled, &event_sender)
                }
                ProviderId::ClaudeCode => claude_login(&request, &worker_cancelled, &event_sender),
                ProviderId::GrokBuild => unreachable!(),
            };
            let _ = outcome_sender.send(result);
        })?;
    Ok(AuthHandle {
        provider,
        cancelled,
        events,
        outcome,
        worker: Some(worker),
    })
}

fn codex_login(
    request: &AuthRequest,
    cancelled: &AtomicBool,
    events: &mpsc::SyncSender<AuthEvent>,
) -> AuthOutcome {
    let _ = events.try_send(AuthEvent::Started);
    let deadline = Instant::now() + request.timeout;
    let mut process = match codex::auth_process(
        &request.client.executable,
        &request.work_dir,
        2 * 1024 * 1024,
    ) {
        Ok(process) => process,
        Err(error) => return auth_error(error),
    };
    if let Err(error) = codex::initialize(&mut process, deadline, cancelled) {
        return cancelled_or_error(cancelled, error);
    }
    if let Err(error) = process.send(&json!({
        "method": "account/login/start",
        "id": 200,
        "params": {
            "type": "chatgpt",
            "appBrand": "codex",
            "useHostedLoginSuccessPage": true,
            "codexStreamlinedLogin": true
        }
    })) {
        return auth_error(error);
    }
    let started = match process.wait_for_id(200, deadline, cancelled) {
        Ok(started) => started,
        Err(error) => return cancelled_or_error(cancelled, error),
    };
    if started.get("type").and_then(Value::as_str) != Some("chatgpt") {
        return failed("protocol_error", "Codex did not start ChatGPT sign-in");
    }
    let Some(login_id) = started.get("loginId").and_then(Value::as_str) else {
        return failed("protocol_error", "Codex omitted the sign-in identity");
    };
    let login_id = login_id.to_owned();
    let Some(auth_url) = started.get("authUrl").and_then(Value::as_str) else {
        return failed("protocol_error", "Codex omitted the official sign-in URL");
    };
    if !valid_auth_url(auth_url) {
        return failed("unsafe_auth_url", "Codex returned an invalid sign-in URL");
    }
    let _ = events.try_send(AuthEvent::OpenUrl(auth_url.to_owned()));

    loop {
        if cancelled.load(Ordering::Relaxed) {
            let _ = process.send(&json!({
                "method": "account/login/cancel",
                "id": 201,
                "params": { "loginId": login_id }
            }));
            process.terminate();
            return AuthOutcome::Cancelled;
        }
        let message = match process.recv_json(deadline, cancelled) {
            Ok(message) => message,
            Err(_) if cancelled.load(Ordering::Relaxed) => {
                let _ = process.send(&json!({
                    "method": "account/login/cancel",
                    "id": 201,
                    "params": { "loginId": login_id }
                }));
                process.terminate();
                return AuthOutcome::Cancelled;
            }
            Err(error) => return auth_error(error),
        };
        if message.get("method").and_then(Value::as_str) != Some("account/login/completed") {
            continue;
        }
        let completed_id = message.pointer("/params/loginId").and_then(Value::as_str);
        if completed_id.is_some() && completed_id != Some(login_id.as_str()) {
            continue;
        }
        if message.pointer("/params/success").and_then(Value::as_bool) != Some(true) {
            return failed("login_failed", "ChatGPT sign-in was not completed");
        }
        if let Err(error) = process.send(&json!({
            "method": "account/read",
            "id": 202,
            "params": { "refreshToken": false }
        })) {
            return auth_error(error);
        }
        let account = match process.wait_for_id(202, deadline, cancelled) {
            Ok(account) => account,
            Err(error) => return cancelled_or_error(cancelled, error),
        };
        if account.pointer("/account/type").and_then(Value::as_str) != Some("chatgpt") {
            return failed(
                "wrong_billing_identity",
                "Codex sign-in did not produce a ChatGPT subscription account",
            );
        }
        let _ = events.try_send(AuthEvent::Completed);
        return AuthOutcome::Connected;
    }
}

fn claude_login(
    request: &AuthRequest,
    cancelled: &AtomicBool,
    events: &mpsc::SyncSender<AuthEvent>,
) -> AuthOutcome {
    let _ = events.try_send(AuthEvent::Started);
    let mut command = sanitized_command(&request.client.executable);
    command
        .args(["auth", "login", "--claudeai"])
        .current_dir(&request.work_dir)
        .stdin(Stdio::null())
        // Login output can contain account details or one-time codes. The
        // official CLI owns the browser flow; Omuse deliberately discards it.
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    for name in ["DISPLAY", "WAYLAND_DISPLAY", "XDG_CURRENT_DESKTOP"] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => return auth_error(AiError::Io(error)),
    };
    let _ = events.try_send(AuthEvent::AwaitingProviderBrowser);
    let deadline = Instant::now() + request.timeout;
    loop {
        if cancelled.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            return AuthOutcome::Cancelled;
        }
        match child.try_wait() {
            Ok(Some(status)) if status.success() => break,
            Ok(Some(_)) => {
                return failed(
                    "login_failed",
                    "Claude Code did not complete its subscription sign-in flow",
                );
            }
            Ok(None) => {}
            Err(error) => return auth_error(AiError::Io(error)),
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return failed("timeout", "Claude Code sign-in timed out");
        }
        thread::sleep(Duration::from_millis(50));
    }
    match claude::subscription_signed_in_cancellable(
        &request.client.executable,
        &request.work_dir,
        Duration::from_secs(8),
        cancelled,
    ) {
        Ok(true) => {
            let _ = events.try_send(AuthEvent::Completed);
            AuthOutcome::Connected
        }
        Ok(false) => failed(
            "wrong_billing_identity",
            "Claude Code sign-in did not produce a subscription login",
        ),
        Err(_) if cancelled.load(Ordering::Relaxed) => AuthOutcome::Cancelled,
        Err(error) => auth_error(error),
    }
}

fn valid_auth_url(url: &str) -> bool {
    if url.len() > 8192 || !url.starts_with("https://") || url.chars().any(char::is_control) {
        return false;
    }
    let authority = url["https://".len()..]
        .split('/')
        .next()
        .unwrap_or_default();
    if authority.contains('@') {
        return false;
    }
    let host = authority
        .split(':')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    host == "openai.com"
        || host.ends_with(".openai.com")
        || host == "chatgpt.com"
        || host.ends_with(".chatgpt.com")
}

fn cancelled_or_error(cancelled: &AtomicBool, error: AiError) -> AuthOutcome {
    if cancelled.load(Ordering::Relaxed) {
        AuthOutcome::Cancelled
    } else {
        auth_error(error)
    }
}

fn auth_error(error: AiError) -> AuthOutcome {
    let code = match &error {
        AiError::TimedOut => "timeout",
        AiError::Io(_) | AiError::Disconnected => "runtime_unavailable",
        AiError::Protocol(_) => "protocol_error",
        AiError::InvalidRequest(_) => "invalid_request",
        AiError::IdentityUnverified(_) => "identity_unverified",
        AiError::UnsupportedCapability { .. } => "unsupported_provider",
    };
    // Provider output is never included here. AiError messages are either
    // Omuse-authored or sanitized at the protocol boundary.
    AuthOutcome::Failed(AuthFailure::new(code, error.to_string()))
}

fn failed(code: &'static str, message: impl Into<String>) -> AuthOutcome {
    AuthOutcome::Failed(AuthFailure::new(code, message))
}

#[cfg(unix)]
fn validate_private_work_dir(path: &std::path::Path) -> Result<(), AiError> {
    use std::os::unix::fs::PermissionsExt;
    if std::fs::metadata(path)?.permissions().mode() & 0o077 != 0 {
        return Err(AiError::InvalidRequest(
            "The sign-in workspace must be private to the current user".into(),
        ));
    }
    Ok(())
}

#[cfg(not(unix))]
fn validate_private_work_dir(_path: &std::path::Path) -> Result<(), AiError> {
    Err(AiError::InvalidRequest(
        "Provider sign-in is not qualified on this platform".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_urls_require_https_and_reject_userinfo() {
        assert!(valid_auth_url(
            "https://auth.openai.com/oauth/authorize?x=1"
        ));
        assert!(!valid_auth_url("http://auth.openai.com/oauth"));
        assert!(!valid_auth_url("https://user@example.com/oauth"));
        assert!(!valid_auth_url("https://openai.com.example.org/oauth"));
        assert!(!valid_auth_url("https://auth.openai.com/\nsecret"));
    }
}
