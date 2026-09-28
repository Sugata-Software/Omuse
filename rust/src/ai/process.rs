use crate::ai::types::{AiError, ProviderId};
use serde_json::Value;
use std::{
    ffi::OsStr,
    io::{Read, Write},
    path::Path,
    process::{Child, ChildStdin, Command, ExitStatus, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError, SyncSender},
    },
    thread,
    time::{Duration, Instant},
};

const ENV_ALLOWLIST: &[&str] = &[
    "HOME",
    "USER",
    "LOGNAME",
    "LANG",
    "LC_ALL",
    "PATH",
    "TMPDIR",
    "XDG_CACHE_HOME",
    "XDG_CONFIG_HOME",
    "XDG_DATA_HOME",
    "XDG_RUNTIME_DIR",
    "DBUS_SESSION_BUS_ADDRESS",
    "SSL_CERT_DIR",
    "SSL_CERT_FILE",
];

pub(crate) fn sanitized_command(executable: &Path) -> Command {
    let mut command = Command::new(executable);
    command.env_clear();
    for name in ENV_ALLOWLIST {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    command.env("NO_COLOR", "1");
    command
}

#[derive(Debug)]
pub(crate) struct Capture {
    pub status: ExitStatus,
    pub stdout: Vec<u8>,
    pub truncated: bool,
}

pub(crate) fn capture_bounded<I, S>(
    executable: &Path,
    args: I,
    cwd: &Path,
    timeout: Duration,
    max_bytes: usize,
) -> Result<Capture, AiError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    capture_bounded_with_cancel(executable, args, cwd, timeout, max_bytes, None)
}

/// Like [`capture_bounded`], but stops an account-status child promptly when
/// its owning job is cancelled. A sign-in preflight happens before a provider
/// request, yet it must not leave an invisible subprocess running after the
/// user closes the inspector or starts a local-only session.
pub(crate) fn capture_bounded_cancellable<I, S>(
    executable: &Path,
    args: I,
    cwd: &Path,
    timeout: Duration,
    max_bytes: usize,
    cancelled: &AtomicBool,
) -> Result<Capture, AiError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    capture_bounded_with_cancel(executable, args, cwd, timeout, max_bytes, Some(cancelled))
}

fn capture_bounded_with_cancel<I, S>(
    executable: &Path,
    args: I,
    cwd: &Path,
    timeout: Duration,
    max_bytes: usize,
    cancelled: Option<&AtomicBool>,
) -> Result<Capture, AiError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut command = sanitized_command(executable);
    command
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn()?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| AiError::Protocol("Provider stdout was unavailable".into()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| AiError::Protocol("Provider stderr was unavailable".into()))?;
    let stdout_reader = thread::spawn(move || drain_bounded(stdout, max_bytes));
    let stderr_reader = thread::spawn(move || drain_bounded(stderr, max_bytes));
    let deadline = Instant::now() + timeout;
    let status = loop {
        if cancelled.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(AiError::Protocol("cancelled".into()));
        }
        let exited = match child.try_wait() {
            Ok(exited) => exited,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(AiError::Io(error));
            }
        };
        if let Some(status) = exited {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(AiError::TimedOut);
        }
        thread::sleep(Duration::from_millis(10));
    };
    let (stdout, stdout_truncated) = stdout_reader
        .join()
        .map_err(|_| AiError::Protocol("Provider stdout reader failed".into()))??;
    let (_stderr, stderr_truncated) = stderr_reader
        .join()
        .map_err(|_| AiError::Protocol("Provider stderr reader failed".into()))??;
    Ok(Capture {
        status,
        stdout,
        truncated: stdout_truncated || stderr_truncated,
    })
}

fn drain_bounded(mut reader: impl Read, max_bytes: usize) -> Result<(Vec<u8>, bool), AiError> {
    let mut kept = Vec::with_capacity(max_bytes.min(16 * 1024));
    let mut chunk = [0_u8; 8192];
    let mut truncated = false;
    loop {
        let read = reader.read(&mut chunk)?;
        if read == 0 {
            break;
        }
        let room = max_bytes.saturating_sub(kept.len());
        let take = room.min(read);
        kept.extend_from_slice(&chunk[..take]);
        truncated |= take != read;
    }
    Ok((kept, truncated))
}

#[derive(Debug)]
enum LineRead {
    Line(Vec<u8>),
    TooLong,
    Io,
    Eof,
}

/// A newline-framed JSON child with bounded input frames and a drained stderr.
pub(crate) struct JsonLineChild {
    child: Child,
    stdin: ChildStdin,
    lines: Receiver<LineRead>,
    cleanup_dir: Option<std::path::PathBuf>,
}

impl JsonLineChild {
    pub(crate) fn spawn<I, S>(
        executable: &Path,
        args: I,
        cwd: &Path,
        max_line_bytes: usize,
    ) -> Result<Self, AiError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        Self::spawn_with_env(
            executable,
            args,
            cwd,
            max_line_bytes,
            std::iter::empty::<(&str, &str)>(),
        )
    }

    pub(crate) fn spawn_with_env<I, S, E, K, V>(
        executable: &Path,
        args: I,
        cwd: &Path,
        max_line_bytes: usize,
        env_overrides: E,
    ) -> Result<Self, AiError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
        E: IntoIterator<Item = (K, V)>,
        K: AsRef<OsStr>,
        V: AsRef<OsStr>,
    {
        let mut command = sanitized_command(executable);
        command
            .args(args)
            .current_dir(cwd)
            .envs(env_overrides)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command.spawn()?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| AiError::Protocol("Provider stdin was unavailable".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| AiError::Protocol("Provider stdout was unavailable".into()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| AiError::Protocol("Provider stderr was unavailable".into()))?;
        let (sender, lines) = mpsc::sync_channel(32);
        thread::spawn(move || read_lines(stdout, max_line_bytes, sender));
        // Provider stderr is diagnostic-only and may contain account details or
        // local paths. Drain it to prevent deadlock, but never retain or expose it.
        thread::spawn(move || {
            let _ = drain_bounded(stderr, 0);
        });
        Ok(Self {
            child,
            stdin,
            lines,
            cleanup_dir: None,
        })
    }

    pub(crate) fn remove_dir_on_drop(mut self, path: std::path::PathBuf) -> Self {
        self.cleanup_dir = Some(path);
        self
    }

    pub(crate) fn send(&mut self, value: &Value) -> Result<(), AiError> {
        serde_json::to_writer(&mut self.stdin, value)?;
        self.stdin.write_all(b"\n")?;
        self.stdin.flush()?;
        Ok(())
    }

    pub(crate) fn recv_json(
        &mut self,
        deadline: Instant,
        cancelled: &AtomicBool,
    ) -> Result<Value, AiError> {
        loop {
            if cancelled.load(Ordering::Relaxed) {
                return Err(AiError::Protocol("cancelled".into()));
            }
            let now = Instant::now();
            if now >= deadline {
                return Err(AiError::TimedOut);
            }
            let wait = deadline
                .saturating_duration_since(now)
                .min(Duration::from_millis(50));
            match self.lines.recv_timeout(wait) {
                Ok(LineRead::Line(line)) => return Ok(serde_json::from_slice(&line)?),
                Ok(LineRead::TooLong) => {
                    return Err(AiError::Protocol(
                        "Provider emitted an oversized protocol frame".into(),
                    ));
                }
                Ok(LineRead::Io) => {
                    return Err(AiError::Protocol(
                        "Provider output could not be read".into(),
                    ));
                }
                Ok(LineRead::Eof) => return Err(AiError::Disconnected),
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => return Err(AiError::Disconnected),
            }
        }
    }

    pub(crate) fn wait_for_id(
        &mut self,
        id: i64,
        deadline: Instant,
        cancelled: &AtomicBool,
    ) -> Result<Value, AiError> {
        loop {
            let message = self.recv_json(deadline, cancelled)?;
            if message.get("id").and_then(Value::as_i64) != Some(id) {
                continue;
            }
            if let Some(error) = message.get("error") {
                let text = error
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("Provider rejected the request");
                return Err(AiError::Protocol(sanitize_provider_error(text)));
            }
            return message
                .get("result")
                .cloned()
                .ok_or_else(|| AiError::Protocol("Provider response omitted its result".into()));
        }
    }

    pub(crate) fn terminate(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for JsonLineChild {
    fn drop(&mut self) {
        self.terminate();
        if let Some(path) = self.cleanup_dir.take() {
            let is_runtime_dir = path
                .file_name()
                .and_then(OsStr::to_str)
                .is_some_and(|name| name.starts_with(".codex-runtime-"));
            if is_runtime_dir {
                let _ = std::fs::remove_dir_all(path);
            }
        }
    }
}

fn read_lines(mut reader: impl Read, max: usize, sender: SyncSender<LineRead>) {
    let mut chunk = [0_u8; 8192];
    let mut line = Vec::with_capacity(max.min(64 * 1024));
    let mut overflow = false;
    loop {
        let read = match reader.read(&mut chunk) {
            Ok(read) => read,
            Err(_) => {
                let _ = sender.send(LineRead::Io);
                return;
            }
        };
        if read == 0 {
            if overflow {
                let _ = sender.send(LineRead::TooLong);
            } else if !line.is_empty() {
                let _ = sender.send(LineRead::Line(line));
            }
            let _ = sender.send(LineRead::Eof);
            return;
        }
        for byte in &chunk[..read] {
            if *byte == b'\n' {
                let message = if overflow {
                    LineRead::TooLong
                } else {
                    if line.last() == Some(&b'\r') {
                        line.pop();
                    }
                    LineRead::Line(std::mem::take(&mut line))
                };
                if sender.send(message).is_err() {
                    return;
                }
                overflow = false;
                continue;
            }
            if overflow {
                continue;
            }
            if line.len() >= max {
                overflow = true;
                line.clear();
            } else {
                line.push(*byte);
            }
        }
    }
}

pub(crate) fn sanitize_provider_error(message: &str) -> String {
    // Provider errors can themselves be pretty-printed JSON. Extract only the
    // human-readable message, never the request body or account envelope.
    let parsed = serde_json::from_str::<Value>(message).ok();
    let message = parsed
        .as_ref()
        .and_then(|value| {
            value
                .pointer("/error/message")
                .or_else(|| value.get("message"))
        })
        .and_then(Value::as_str)
        .unwrap_or(message);
    let mut sanitized = message
        .lines()
        .next()
        .unwrap_or("Provider error")
        .to_owned();
    if sanitized.chars().count() > 240 {
        sanitized = sanitized.chars().take(240).collect();
        sanitized.push('…');
    }
    for marker in ["sk-", "xai-", "Bearer "] {
        if let Some(index) = sanitized.find(marker) {
            sanitized.truncate(index);
            sanitized.push_str("[redacted]");
        }
    }
    sanitized
        .split_whitespace()
        .map(|word| {
            let content = word.trim_start_matches(['\'', '"', '(', '[']);
            if content.starts_with('/')
                || content.starts_with("file://")
                || content.starts_with("http://")
                || content.starts_with("https://")
                || content.contains('@')
            {
                "[redacted]"
            } else {
                word
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub(crate) fn provider_version_is_valid(provider: ProviderId, output: &[u8]) -> bool {
    let Ok(output) = std::str::from_utf8(output) else {
        return false;
    };
    let output = output.trim();
    match provider {
        ProviderId::CodexSubscription => output.starts_with("codex-cli "),
        ProviderId::ClaudeCode => output.contains("(Claude Code)") && starts_with_digit(output),
        ProviderId::GrokBuild => {
            let lower = output.to_ascii_lowercase();
            lower.contains("grok") && lower.chars().any(|character| character.is_ascii_digit())
        }
    }
}

fn starts_with_digit(text: &str) -> bool {
    text.chars()
        .next()
        .is_some_and(|value| value.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

    #[test]
    fn line_reader_rejects_oversized_frames_without_growing_unbounded() {
        let (sender, receiver) = mpsc::sync_channel(4);
        read_lines(&b"12345\n{}\n"[..], 4, sender);
        assert!(matches!(receiver.recv().unwrap(), LineRead::TooLong));
        assert!(matches!(receiver.recv().unwrap(), LineRead::Line(line) if line == b"{}"));
        assert!(matches!(receiver.recv().unwrap(), LineRead::Eof));
    }

    #[test]
    fn provider_errors_are_bounded_and_redacted() {
        let error = sanitize_provider_error("failed with Bearer secret-token\n/home/user/private");
        assert_eq!(error, "failed with [redacted]");
        assert_eq!(
            sanitize_provider_error("Unable to load '/home/carlo/private.png' for me@example.test"),
            "Unable to load [redacted] for [redacted]"
        );
        let unicode = sanitize_provider_error(&"ç🚀".repeat(200));
        assert_eq!(unicode.chars().count(), 241);
        assert!(unicode.ends_with('…'));
        assert_eq!(
            sanitize_provider_error(
                "{\n  \"error\": {\"message\": \"Invalid response schema\", \"secret\": \"hidden\"}\n}"
            ),
            "Invalid response schema"
        );
    }

    #[test]
    fn cancelled_receive_does_not_block() {
        let cancel = AtomicBool::new(true);
        assert!(cancel.load(Ordering::Relaxed));
    }

    #[cfg(unix)]
    #[test]
    fn cancellable_capture_stops_a_preflight_child_promptly() {
        use std::os::unix::fs::PermissionsExt;

        let root = tempfile::tempdir().unwrap();
        let executable = root.path().join("slow-status");
        std::fs::write(&executable, "#!/bin/sh\nwhile :; do :; done\n").unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
        let cancelled = AtomicBool::new(true);
        let started = Instant::now();
        let error = capture_bounded_cancellable(
            &executable,
            std::iter::empty::<&std::ffi::OsStr>(),
            root.path(),
            Duration::from_secs(10),
            1024,
            &cancelled,
        )
        .unwrap_err();

        assert!(matches!(error, AiError::Protocol(ref message) if message == "cancelled"));
        assert!(started.elapsed() < Duration::from_secs(1));
    }
}
