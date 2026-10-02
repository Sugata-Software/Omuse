use crate::ai::{
    claude, codex, grok,
    types::{AiError, JobEvent, JobOutcome, JobRequest, ProviderId},
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError, TryRecvError},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

pub struct JobHandle {
    id: uuid::Uuid,
    provider: ProviderId,
    cancelled: Arc<AtomicBool>,
    events: Receiver<JobEvent>,
    outcome: Receiver<JobOutcome>,
    worker: Option<JoinHandle<()>>,
}

impl JobHandle {
    pub fn id(&self) -> uuid::Uuid {
        self.id
    }

    pub fn provider(&self) -> ProviderId {
        self.provider
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

    pub fn try_recv_event(&self) -> Result<Option<JobEvent>, AiError> {
        match self.events.try_recv() {
            Ok(event) => Ok(Some(event)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => Err(AiError::Disconnected),
        }
    }

    pub fn recv_event(&self, timeout: Duration) -> Result<Option<JobEvent>, AiError> {
        match self.events.recv_timeout(timeout) {
            Ok(event) => Ok(Some(event)),
            Err(RecvTimeoutError::Timeout) => Ok(None),
            Err(RecvTimeoutError::Disconnected) => Err(AiError::Disconnected),
        }
    }

    /// Returns `Ok(None)` while the worker is still active. A timeout here
    /// never causes an automatic retry or changes the provider job.
    pub fn wait(&mut self, timeout: Duration) -> Result<Option<JobOutcome>, AiError> {
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

    pub fn try_take_outcome(&mut self) -> Result<Option<JobOutcome>, AiError> {
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

    pub fn try_outcome(&mut self) -> Result<Option<JobOutcome>, AiError> {
        self.try_take_outcome()
    }
}

impl Drop for JobHandle {
    fn drop(&mut self) {
        self.cancel();
    }
}

pub fn spawn_job(request: JobRequest) -> Result<JobHandle, AiError> {
    request.validate()?;
    if !request.client.operation_profile_qualified {
        return Err(AiError::IdentityUnverified(
            "The provider runtime profile has not passed isolation qualification".into(),
        ));
    }
    if !request.work_dir.is_dir() {
        return Err(AiError::InvalidRequest(
            "Create the dedicated AI job workspace before starting the job".into(),
        ));
    }
    validate_private_work_dir(&request.work_dir)?;
    let id = request.id;
    let provider = request.client.provider;
    let cancelled = Arc::new(AtomicBool::new(false));
    let worker_cancelled = cancelled.clone();
    let (event_sender, events) = mpsc::sync_channel(request.limits.event_capacity.max(1));
    let (outcome_sender, outcome) = mpsc::sync_channel(1);
    let worker = thread::Builder::new()
        .name(format!("omuse-ai-{id}"))
        .spawn(move || {
            let outcome = match provider {
                ProviderId::CodexSubscription => {
                    codex::run(request, worker_cancelled, event_sender)
                }
                ProviderId::ClaudeCode => claude::run(request, worker_cancelled, event_sender),
                ProviderId::GrokBuild => grok::run(request, worker_cancelled, event_sender),
            };
            let _ = outcome_sender.send(outcome);
        })?;
    Ok(JobHandle {
        id,
        provider,
        cancelled,
        events,
        outcome,
        worker: Some(worker),
    })
}

#[cfg(unix)]
fn validate_private_work_dir(path: &std::path::Path) -> Result<(), AiError> {
    use std::os::unix::fs::PermissionsExt;
    let mode = std::fs::metadata(path)?.permissions().mode();
    if mode & 0o077 != 0 {
        return Err(AiError::InvalidRequest(
            "The AI job workspace must be private to the current user".into(),
        ));
    }
    Ok(())
}

#[cfg(not(unix))]
fn validate_private_work_dir(_path: &std::path::Path) -> Result<(), AiError> {
    Err(AiError::InvalidRequest(
        "AI job workspace isolation is not qualified on this platform".into(),
    ))
}

// AI job workspaces are qualified only on Unix (see `validate_private_work_dir`).
#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::ai::types::{JobOperation, ValidatedClient};
    use std::{fs, os::unix::fs::PermissionsExt, time::Instant};

    #[test]
    fn dropping_a_handle_sets_its_cancellation_flag() {
        let flag = Arc::new(AtomicBool::new(false));
        let (_event_sender, events) = mpsc::sync_channel(1);
        let (_outcome_sender, outcome) = mpsc::sync_channel(1);
        let handle = JobHandle {
            id: uuid::Uuid::new_v4(),
            provider: ProviderId::ClaudeCode,
            cancelled: flag.clone(),
            events,
            outcome,
            worker: None,
        };
        drop(handle);
        assert!(flag.load(Ordering::Relaxed));
    }

    #[test]
    fn fake_app_server_job_can_be_cancelled_without_waiting_for_child_exit() {
        let root = tempfile::tempdir().unwrap();
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let server = fake_codex_server(root.path(), FakeCodexScenario::CancellableAssistant);
        let client = ValidatedClient::fixture(ProviderId::CodexSubscription, server);
        let request = JobRequest::new(
            client,
            JobOperation::Assistant,
            "Plan a poster",
            root.path(),
        );
        let mut handle = spawn_job(request).unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut submitted = false;
        while Instant::now() < deadline {
            if matches!(
                handle.recv_event(Duration::from_millis(50)).ok().flatten(),
                Some(JobEvent::Submitted)
            ) {
                submitted = true;
                break;
            }
        }
        assert!(submitted);
        handle.cancel();
        assert_eq!(
            handle.wait(Duration::from_secs(2)).unwrap(),
            Some(JobOutcome::Cancelled)
        );
    }

    #[test]
    fn fake_app_server_forbidden_tool_event_fails_closed() {
        let root = tempfile::tempdir().unwrap();
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let server = fake_codex_server(root.path(), FakeCodexScenario::ForbiddenToolAssistant);
        let client = ValidatedClient::fixture(ProviderId::CodexSubscription, server);
        let request = JobRequest::new(
            client,
            JobOperation::Assistant,
            "Plan a poster",
            root.path(),
        );
        let mut handle = spawn_job(request).unwrap();
        let outcome = handle
            .wait(Duration::from_secs(2))
            .unwrap()
            .expect("fixture job should terminate");
        assert!(matches!(
            outcome,
            JobOutcome::OutcomeUnknown(ref failure) if failure.code == "protocol_error"
        ));
    }

    #[test]
    fn first_validated_codex_image_completes_once_and_survives_runtime_cleanup() {
        let root = tempfile::tempdir().unwrap();
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let server = fake_codex_server(root.path(), FakeCodexScenario::FirstImageThenWait);
        let client = ValidatedClient::fixture(ProviderId::CodexSubscription, server);
        let request = JobRequest::new(
            client,
            JobOperation::GenerateImage,
            "Create a single square swatch",
            root.path(),
        );

        let mut handle = spawn_job(request).unwrap();
        let outcome = handle
            .wait(Duration::from_secs(2))
            .unwrap()
            .expect("fixture image job should finish after its first image");
        let JobOutcome::Completed(result) = outcome else {
            panic!("first validated Codex image must complete the image job");
        };

        assert_eq!(result.assets.len(), 1);
        let asset = &result.assets[0];
        assert_eq!(asset.provider_item_id.as_deref(), Some("first-image"));
        assert_eq!(asset.width, 1);
        assert_eq!(asset.height, 1);
        assert!(asset.path.is_file());
        assert_eq!(asset.path.parent(), Some(root.path()));
        assert_eq!(
            asset.path.extension().and_then(|value| value.to_str()),
            Some("asset")
        );
        assert!(
            !asset.path.to_string_lossy().contains(".codex-runtime-"),
            "the retained result must not point into the private Codex home"
        );
        assert!(
            !fs::read_dir(root.path()).unwrap().flatten().any(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".codex-runtime-")
            }),
            "the isolated Codex home should be removed after the copied asset is retained"
        );
        assert_eq!(
            fs::read_to_string(root.path().join("submitted-turns"))
                .unwrap()
                .lines()
                .count(),
            1,
            "a generated variation must submit exactly one provider turn"
        );
    }

    #[test]
    fn assistant_rejects_an_unrequested_image_generation_item() {
        let root = tempfile::tempdir().unwrap();
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let server = fake_codex_server(root.path(), FakeCodexScenario::UnexpectedAssistantImage);
        let client = ValidatedClient::fixture(ProviderId::CodexSubscription, server);
        let request = JobRequest::new(
            client,
            JobOperation::Assistant,
            "Plan a poster",
            root.path(),
        );

        let mut handle = spawn_job(request).unwrap();
        let outcome = handle
            .wait(Duration::from_secs(2))
            .unwrap()
            .expect("fixture assistant job should reject the unexpected image");

        assert!(matches!(
            outcome,
            JobOutcome::OutcomeUnknown(ref failure) if failure.code == "protocol_error"
        ));
    }

    #[test]
    fn unexpected_workspace_instructions_fail_before_turn_start() {
        let root = tempfile::tempdir().unwrap();
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let server = fake_codex_server(root.path(), FakeCodexScenario::UnexpectedInstructions);
        let client = ValidatedClient::fixture(ProviderId::CodexSubscription, server);
        let request = JobRequest::new(
            client,
            JobOperation::Assistant,
            "Plan a poster",
            root.path(),
        );

        let mut handle = spawn_job(request).unwrap();
        let outcome = handle
            .wait(Duration::from_secs(2))
            .unwrap()
            .expect("fixture job should reject workspace instructions");

        assert!(matches!(
            outcome,
            JobOutcome::Failed(ref failure) if failure.code == "protocol_error"
        ));
        assert!(
            !root.path().join("submitted-turns").exists(),
            "unexpected instructions must be rejected before turn/start"
        );
    }

    #[test]
    fn codex_preserves_result_events_emitted_before_turn_start_response() {
        let root = tempfile::tempdir().unwrap();
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let server = fake_codex_server(root.path(), FakeCodexScenario::EarlyAssistantDelta);
        let client = ValidatedClient::fixture(ProviderId::CodexSubscription, server);
        let request = JobRequest::new(
            client,
            JobOperation::Assistant,
            "Plan a poster",
            root.path(),
        );

        let mut handle = spawn_job(request).unwrap();
        let outcome = handle
            .wait(Duration::from_secs(2))
            .unwrap()
            .expect("fixture assistant job should complete");

        assert!(matches!(
            outcome,
            JobOutcome::Completed(ref result) if result.text == "early late"
        ));
    }

    #[test]
    fn codex_checks_forbidden_events_emitted_before_turn_start_response() {
        let root = tempfile::tempdir().unwrap();
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let server = fake_codex_server(root.path(), FakeCodexScenario::EarlyForbiddenTool);
        let client = ValidatedClient::fixture(ProviderId::CodexSubscription, server);
        let request = JobRequest::new(
            client,
            JobOperation::Assistant,
            "Plan a poster",
            root.path(),
        );

        let mut handle = spawn_job(request).unwrap();
        let outcome = handle
            .wait(Duration::from_secs(2))
            .unwrap()
            .expect("fixture assistant job should terminate");

        assert!(matches!(
            outcome,
            JobOutcome::OutcomeUnknown(ref failure) if failure.code == "protocol_error"
        ));
    }

    #[derive(Clone, Copy)]
    enum FakeCodexScenario {
        CancellableAssistant,
        ForbiddenToolAssistant,
        FirstImageThenWait,
        UnexpectedAssistantImage,
        UnexpectedInstructions,
        EarlyAssistantDelta,
        EarlyForbiddenTool,
    }

    fn fake_codex_server(
        root: &std::path::Path,
        scenario: FakeCodexScenario,
    ) -> std::path::PathBuf {
        let (name, image_generation_enabled, instruction_sources, before_turn, after_turn) =
            match scenario {
                FakeCodexScenario::CancellableAssistant => {
                    ("fake-codex-cancellable", "false", "[]", ":", ":")
                }
                FakeCodexScenario::ForbiddenToolAssistant => (
                    "fake-codex-forbidden",
                    "false",
                    "[]",
                    ":",
                    r#"printf '%s\n' '{"method":"item/completed","params":{"item":{"id":"bad","type":"commandExecution"}}}'"#,
                ),
                FakeCodexScenario::FirstImageThenWait => (
                    "fake-codex-first-image",
                    "true",
                    "[]",
                    ":",
                    r#"
      mkdir -p "$CODEX_HOME/generated_images"
      printf '%s' 'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVQIHWP4z8DwHwAFgAI/ScL9cQAAAABJRU5ErkJggg==' | base64 -d > "$CODEX_HOME/generated_images/first.png"
      printf '%s\n' "{\"method\":\"item/completed\",\"params\":{\"item\":{\"id\":\"first-image\",\"type\":\"imageGeneration\",\"savedPath\":\"$CODEX_HOME/generated_images/first.png\"}}}"
      while :; do sleep 60; done
"#,
                ),
                FakeCodexScenario::UnexpectedAssistantImage => (
                    "fake-codex-assistant-image",
                    "false",
                    "[]",
                    ":",
                    r#"printf '%s\n' '{"method":"item/completed","params":{"item":{"id":"unexpected-image","type":"imageGeneration"}}}'"#,
                ),
                FakeCodexScenario::UnexpectedInstructions => (
                    "fake-codex-unexpected-instructions",
                    "false",
                    r#"[{"path":"AGENTS.md"}]"#,
                    ":",
                    ":",
                ),
                FakeCodexScenario::EarlyAssistantDelta => (
                    "fake-codex-early-assistant-delta",
                    "false",
                    "[]",
                    r#"printf '%s\n' '{"method":"item/agentMessage/delta","params":{"delta":"early "}}'"#,
                    r#"
      printf '%s\n' '{"method":"item/agentMessage/delta","params":{"delta":"late"}}'
      printf '%s\n' '{"method":"turn/completed","params":{"turn":{"id":"turn-fixture","status":"completed"}}}'
"#,
                ),
                FakeCodexScenario::EarlyForbiddenTool => (
                    "fake-codex-early-forbidden",
                    "false",
                    "[]",
                    r#"printf '%s\n' '{"method":"item/completed","params":{"item":{"id":"bad","type":"commandExecution"}}}'"#,
                    ":",
                ),
            };
        let path = root.join(name);
        let script = format!(
            r#"#!/bin/sh
while IFS= read -r line; do
  case "$line" in
    *'"id":100'*) printf '%s\n' '{{"id":100,"result":{{"data":[],"nextCursor":null}}}}' ;;
    *'"id":101'*) printf '%s\n' '{{"id":101,"result":{{"apps":[]}}}}' ;;
    *'"id":102'*) printf '%s\n' '{{"id":102,"result":{{"data":[{{"name":"apps","enabled":false}},{{"name":"plugins","enabled":false}},{{"name":"hooks","enabled":false}},{{"name":"memories","enabled":false}},{{"name":"browser_use","enabled":false}},{{"name":"computer_use","enabled":false}},{{"name":"in_app_browser","enabled":false}},{{"name":"multi_agent","enabled":false}},{{"name":"multi_agent_v2","enabled":false}},{{"name":"remote_plugin","enabled":false}},{{"name":"recommended_plugins","enabled":false}},{{"name":"shell_tool","enabled":false}},{{"name":"skill_search","enabled":false}},{{"name":"web_search_cached","enabled":false}},{{"name":"workspace_dependencies","enabled":false}},{{"name":"image_generation","enabled":{image_generation_enabled}}},{{"name":"view_image","enabled":false}},{{"name":"skip_host_skill_discovery","enabled":true}}],"nextCursor":null}}}}' ;;
    *'"id":1'*) printf '%s\n' '{{"id":1,"result":{{}}}}' ;;
    *'"method":"initialized"'*) : ;;
    *'"id":2'*) printf '%s\n' '{{"id":2,"result":{{"account":{{"type":"chatgpt","email":null,"planType":"plus"}},"requiresOpenaiAuth":true}}}}' ;;
    *'"id":3'*) printf '%s\n' '{{"id":3,"result":{{"imageGeneration":{image_generation_enabled}}}}}' ;;
    *'"id":4'*) printf '%s\n' '{{"id":4,"result":{{"thread":{{"id":"thread-fixture"}},"instructionSources":{instruction_sources}}}}}' ;;
    *'"id":5'*)
      printf '%s\n' submitted >> "$PWD/submitted-turns"
      {before_turn}
      printf '%s\n' '{{"id":5,"result":{{"turn":{{"id":"turn-fixture"}}}}}}'
      {after_turn}
      ;;
    *'"id":6'*) printf '%s\n' '{{"id":6,"result":{{}}}}' ;;
  esac
done
"#
        );
        fs::write(&path, script).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        path
    }
}
