use std::error::Error;
use std::fs;
use std::time::Duration;

use fishnet_core::fixture::{ScriptStep, ScriptedProvider};
use fishnet_core::protocol::{ModelReply, ToolCall};
use serde_json::json;

use super::*;

#[tokio::test]
async fn retained_event_gaps_return_a_snapshot_and_a_recovery_cursor()
-> std::result::Result<(), Box<dyn Error>> {
    let root = std::env::temp_dir()
        .canonicalize()?
        .join(format!("fishnet-gap-{}", crate::new_id()?));
    fs::create_dir(&root)?;
    let config = Config {
        root: root.clone(),
        directory: root.join("service"),
        runs: root.join("runs"),
        ..Config::default()
    };
    seed(&config).await?;
    let service = Service::open(config).await?;
    let gap = service.events("gap", 0, 0).await?;
    assert!(gap.gap && gap.events.is_empty());
    assert!(gap.retained_from > 1);
    assert_eq!(
        gap.snapshot
            .run
            .as_ref()
            .ok_or("no snapshot")?
            .statistics
            .logical_tool_calls,
        32
    );
    let recovered = service.events("gap", gap.retained_from - 1, 0).await?;
    assert!(!recovered.gap);
    assert_eq!(
        recovered.events.first().map(|e| e.sequence),
        Some(gap.retained_from)
    );
    assert!(
        service
            .events("gap", gap.snapshot.cursor + 1, 0)
            .await
            .is_err()
    );
    service.shutdown().await;
    drop(service);
    fs::remove_dir_all(root)?;
    Ok(())
}

async fn seed(config: &Config) -> std::result::Result<(), Box<dyn Error>> {
    let call = ToolCall {
        name: "fixture.lookup".into(),
        arguments: json!({"key":"greeting"}),
    };
    let previous = fishnet_core::tool::execute(&call);
    let steps = (0..32)
        .map(|index| ScriptStep {
            expected_previous_result: if index == 0 {
                None
            } else {
                Some(previous.clone())
            },
            reply: Ok(ModelReply::ToolCall(call.clone())),
        })
        .collect();
    let provider = ScriptedProvider::new(steps, Duration::ZERO)?;
    let (_, contract) =
        ScriptedProvider::scenario(fishnet_core::Scenario::Success, Duration::ZERO)?;
    fishnet_core::runtime::run_identified(
        fishnet_core::RunOptions {
            store: config.runs.clone(),
            limits: fishnet_core::Limits {
                max_provider_calls: 32,
                ..fishnet_core::Limits::default()
            },
            contract,
            cancellation: Cancellation::default(),
        },
        provider,
        |_| (),
        "gap".into(),
    )
    .await?;
    let submission = Submission {
        request_id: "gap".into(),
        task: TaskRequest::Fixture {
            scenario: "success".into(),
            delay_ms: 0,
            limits: fishnet_core::Limits::default(),
        },
    };
    let fingerprint = fishnet_core::provider::fingerprint(&serde_json::to_vec(&submission)?);
    let store = Store::open(config.directory.clone())?;
    store.save(&Receipt {
        conversation: None,
        schema_version: 1,
        decision: None,
        approval_wait_ms: Some(0),
        task_id: "gap".into(),
        submission,
        fingerprint,
        phase: Phase::Finished,
        message: None,
        prepared: None,
        provider: config.provider.clone(),
    })?;
    drop(store);
    Ok(())
}

#[tokio::test]
async fn coding_admission_preserves_pending_approval_while_the_workspace_is_owned()
-> std::result::Result<(), Box<dyn Error>> {
    let root = std::env::temp_dir()
        .canonicalize()?
        .join(format!("fishnet-write-owner-{}", crate::new_id()?));
    fs::create_dir_all(root.join("tasks"))?;
    fs::write(
        root.join("tasks/read.json"),
        serde_json::to_vec(&json!({"schema_version":1,"objective":"Read only",
        "workspace":{"root":".","allowed_writes":[],"required_changes":[],"protected_paths":[],"commands":[],"checks":[]}}))?,
    )?;
    let config = Config {
        root: root.clone(),
        directory: root.join("service"),
        runs: root.join("runs"),
        allow_code: true,
        ..Config::default()
    };
    let service = Service::open(config).await?;
    let request = |id: &str| Submission {
        request_id: id.into(),
        task: TaskRequest::Code {
            task_path: "tasks/read.json".into(),
            model: None,
            account: "default".into(),
        },
    };
    let first = service.submit(request("first")).await?;
    let second = service.submit(request("second")).await?;
    // Hold a valid post-approval/pre-journal owner without contacting a provider.
    let reservation = service.slot()?;
    {
        let mut state = service.inner.state.lock().await;
        let entry = state.tasks.get_mut(&first.task_id).ok_or("missing owner")?;
        entry.receipt.phase = Phase::Executing;
        entry.receipt.decision = Some(true);
        let receipt = entry.receipt.clone();
        drop(state);
        service.persist(receipt).await?;
    }
    let error = service
        .approve(
            &second.task_id,
            Approval {
                expected_revision: 1,
                approve: true,
            },
        )
        .await
        .err()
        .ok_or("parallel coding was accepted")?;
    assert_eq!(error.code, "capacity");
    let still_pending = service.snapshot(&second.task_id).await?;
    assert_eq!(still_pending.phase, Phase::AwaitingApproval);
    assert_eq!(still_pending.approval_decision, None);
    assert!(still_pending.approval.is_some());
    assert!(!root.join("runs").exists());
    drop(reservation);
    service.shutdown().await;
    drop(service);
    fs::remove_dir_all(root)?;
    Ok(())
}
