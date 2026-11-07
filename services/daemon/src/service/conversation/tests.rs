use super::*;
use crate::{
    Config,
    contract::{Approval, Submission, TaskView},
};
use std::{error::Error, fs, path::PathBuf, time::Duration};
type TestResult = std::result::Result<(), Box<dyn Error>>;
struct Store(PathBuf);
impl Store {
    fn new() -> std::result::Result<Self, Box<dyn Error>> {
        let root = std::env::temp_dir()
            .canonicalize()?
            .join(format!("fishnet-chat-{}", crate::new_id()?));
        fs::create_dir(&root)?;
        Ok(Self(root))
    }
    fn config(&self) -> Config {
        Config {
            root: self.0.clone(),
            directory: self.0.join("service"),
            runs: self.0.join("runs"),
            allow_code: true,
            provider: ProviderAccess::Fixture,
        }
    }
}
impl Drop for Store {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn message(id: &str, parent: Option<&str>, text: &str) -> Submission {
    Submission {
        request_id: id.into(),
        task: TaskRequest::Chat {
            message: text.into(),
            parent_task_id: parent.map(String::from),
            model: None,
            account: "default".into(),
        },
    }
}
async fn completed(service: &Service, id: &str) -> std::result::Result<TaskView, Box<dyn Error>> {
    for _ in 0..100 {
        let view = service.snapshot(id).await?;
        if view.phase.terminal() {
            return Ok(view);
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    Err("conversation did not finish".into())
}
async fn approve(service: &Service, id: &str, yes: bool) -> TestResult {
    service
        .approve(
            id,
            Approval {
                expected_revision: 1,
                approve: yes,
            },
        )
        .await?;
    Ok(())
}
#[tokio::test]
async fn consent_history_and_restart_do_not_replay_or_grant_tools() -> TestResult {
    let store = Store::new()?;
    let service = Service::open(store.config()).await?;
    let request = message("first", None, "Remember the name Ada.");
    let first = service.submit(request.clone()).await?;
    assert_eq!(first.phase, Phase::AwaitingApproval);
    let grant = first.approval.as_ref().ok_or("missing consent")?;
    assert!(
        grant.conversation
            && grant.commands.is_empty()
            && grant.allowed_writes.is_empty()
            && grant.root.as_os_str().is_empty()
    );
    assert!(first.run.is_none());
    approve(&service, &first.task_id, true).await?;
    let first = completed(&service, &first.task_id).await?;
    let run = first.run.as_ref().ok_or("no run")?;
    assert_eq!(run.state, TaskState::UnverifiedCompletion);
    assert_eq!(run.statistics.logical_tool_calls, 0);
    assert_eq!(run.statistics.process_attempts, 0);
    assert_eq!(service.submit(request).await?.task_id, first.task_id);
    let second = service
        .submit(message(
            "second",
            Some(&first.task_id),
            "What name did I give?",
        ))
        .await?;
    assert!(second.approval.is_none());
    let second = completed(&service, &second.task_id).await?;
    assert!(
        second
            .run
            .as_ref()
            .and_then(|run| run.outcome.as_ref())
            .and_then(|outcome| outcome.final_text.as_ref())
            .is_some_and(|text| text
                .text
                .contains("Previous user message: Remember the name Ada."))
    );
    let record_path = fishnet_core::storage::record_path(&store.config().runs, &second.task_id)?;
    let before = fs::read(&record_path)?;
    service.shutdown().await;
    drop(service);
    let restarted = Service::open(store.config()).await?;
    assert_eq!(
        restarted
            .snapshot(&second.task_id)
            .await?
            .run
            .ok_or("no saved run")?
            .state,
        TaskState::UnverifiedCompletion
    );
    assert_eq!(fs::read(&record_path)?, before);
    let third = restarted
        .submit(message("third", Some(&second.task_id), "Continue."))
        .await?;
    assert!(third.approval.is_none());
    completed(&restarted, &third.task_id).await?;
    let record = fishnet_core::inspect(&store.config().runs, &third.task_id)?;
    let context = record
        .events
        .iter()
        .find_map(|event| match &event.data {
            fishnet_core::protocol::EventData::RunCreated { contract, .. } => {
                contract.conversation.as_ref()
            }
            _ => None,
        })
        .ok_or("no context")?;
    assert_eq!(context.root_task_id, first.task_id);
    assert_eq!(context.messages.len(), 5);
    restarted.shutdown().await;
    Ok(())
}
#[tokio::test]
async fn denied_cancelled_changed_or_nonconversation_scopes_cannot_inherit_consent() -> TestResult {
    let store = Store::new()?;
    let service = Service::open(store.config()).await?;
    let pending = service.submit(message("pending", None, "Hello")).await?;
    assert!(
        service
            .submit(message("early", Some(&pending.task_id), "Again"))
            .await
            .is_err()
    );
    approve(&service, &pending.task_id, false).await?;
    assert!(
        service
            .submit(message("denied", Some(&pending.task_id), "Again"))
            .await
            .is_err()
    );
    let cancelled = service.submit(message("cancel", None, "Hello")).await?;
    service.cancel(&cancelled.task_id).await?;
    assert!(
        service
            .submit(message("after-cancel", Some(&cancelled.task_id), "Again"))
            .await
            .is_err()
    );
    let first = service.submit(message("approved", None, "Hello")).await?;
    approve(&service, &first.task_id, true).await?;
    completed(&service, &first.task_id).await?;
    let mut changed = message("changed", Some(&first.task_id), "Again");
    if let TaskRequest::Chat { account, .. } = &mut changed.task {
        *account = "other".into();
    }
    assert!(service.submit(changed).await.is_err());
    let mut changed_model = message("changed-model", Some(&first.task_id), "Again");
    if let TaskRequest::Chat { model, .. } = &mut changed_model.task {
        *model = Some("conversation-fixture-v1".into());
    }
    assert!(service.submit(changed_model).await.is_err());
    let fixture = service
        .submit(Submission {
            request_id: "tool-fixture".into(),
            task: TaskRequest::Fixture {
                scenario: "success".into(),
                delay_ms: 0,
                limits: fishnet_core::Limits::default(),
            },
        })
        .await?;
    completed(&service, &fixture.task_id).await?;
    assert!(
        service
            .submit(message("non-chat", Some(&fixture.task_id), "Again"))
            .await
            .is_err()
    );
    assert!(
        service
            .submit(message("oversized", None, &"x".repeat(4097)))
            .await
            .is_err()
    );
    service.shutdown().await;
    drop(service);
    let mut config = store.config();
    config.provider = ProviderAccess::ApiKey {
        endpoint: "https://example.invalid/v1/responses".into(),
        key_env: "FISHNET_MISSING_TEST_KEY".into(),
        default_model: Some("fixture".into()),
    };
    let restarted = Service::open(config).await?;
    assert!(
        restarted
            .submit(message("changed-provider", Some(&first.task_id), "Again"))
            .await
            .is_err()
    );
    restarted.shutdown().await;
    Ok(())
}
