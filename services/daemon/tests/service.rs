#![cfg(unix)]
use std::error::Error;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use fishnet_core::{Limits, TaskState};
use fishnet_service::{
    Config, Service,
    contract::{Approval, Envelope, Phase, Submission, TaskRequest, TaskView},
};
use serde_json::{Value, json};
use tower::ServiceExt;

type TestResult = Result<(), Box<dyn Error>>;
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Root(PathBuf);
impl Root {
    fn new() -> Result<Self, Box<dyn Error>> {
        let path = std::env::temp_dir().canonicalize()?.join(format!(
            "fishnet-service-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path)?;
        Ok(Self(path))
    }
    fn config(&self) -> Config {
        Config {
            root: self.0.clone(),
            directory: self.0.join("service"),
            runs: self.0.join("runs"),
            ..Config::default()
        }
    }
    fn task(&self) -> Result<(), Box<dyn Error>> {
        fs::create_dir(self.0.join("tasks"))?;
        fs::write(self.0.join("work.txt"), "original")?;
        fs::write(self.0.join("check.txt"), "protected")?;
        fs::write(
            self.0.join("tasks/change.json"),
            serde_json::to_vec(
                &json!({"schema_version":1,"objective":"Update work.txt and pass the protected check",
            "workspace":{"root":".","allowed_writes":["work.txt"],"required_changes":["work.txt"],"protected_paths":["check.txt"],
            "commands":[{"id":"verify","program":"/usr/bin/true","arguments":[],"timeout_ms":1000}],"checks":["verify"]}}),
            )?,
        )?;
        Ok(())
    }
}
impl Drop for Root {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn fixture(id: &str, delay_ms: u64) -> Submission {
    Submission {
        request_id: id.into(),
        task: TaskRequest::Fixture {
            scenario: "success".into(),
            delay_ms,
            limits: Limits {
                deadline_ms: 5000,
                ..Limits::default()
            },
        },
    }
}
async fn terminal(service: &Service, id: &str) -> Result<TaskView, Box<dyn Error>> {
    Ok(tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let view = service.snapshot(id).await?;
            if view.phase.terminal() {
                return Ok::<_, fishnet_service::contract::ApiError>(view);
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await??)
}
async fn request(
    app: Router,
    method: &str,
    path: &str,
    token: Option<&str>,
    body: Value,
) -> Result<(StatusCode, Value), Box<dyn Error>> {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json");
    if let Some(token) = token {
        builder = builder.header("authorization", format!("Bearer {token}"));
    }
    let response = app
        .oneshot(builder.body(Body::from(serde_json::to_vec(&body)?))?)
        .await?;
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1_048_576).await?;
    Ok((status, serde_json::from_slice(&bytes)?))
}

#[tokio::test]
async fn headless_client_reconnects_and_duplicate_submissions_never_repeat_work() -> TestResult {
    let root = Root::new()?;
    let service = Service::open(root.config()).await?;
    let path = service.publish("http://127.0.0.1:1/".into()).await?;
    let token = fishnet_service::read_connection(&path)?.token;
    let app = fishnet_service::http::router(service.clone());
    let body = serde_json::to_value(Envelope::new(fixture("once", 30)))?;
    let (first, second) = tokio::join!(
        request(app.clone(), "POST", "/v1/tasks", Some(&token), body.clone()),
        request(app.clone(), "POST", "/v1/tasks", Some(&token), body)
    );
    let (status, first) = first?;
    assert_eq!(status, StatusCode::OK);
    let second = second?.1;
    assert_eq!(
        first.pointer("/data/task_id"),
        second.pointer("/data/task_id")
    );
    let id = first
        .pointer("/data/task_id")
        .and_then(Value::as_str)
        .ok_or("missing task ID")?;
    // Disconnect the initial client. Execution stays owned by the service.
    drop(app);
    let done = terminal(&service, id).await?;
    assert_eq!(
        done.run.ok_or("missing run")?.state,
        TaskState::VerifiedSuccess
    );
    let mut after = 0;
    let mut sequences = Vec::new();
    loop {
        let (_, body) = request(
            fishnet_service::http::router(service.clone()),
            "GET",
            &format!("/v1/tasks/{id}/events?after={after}"),
            Some(&token),
            Value::Null,
        )
        .await?;
        let page: Envelope<fishnet_service::contract::EventPage> = serde_json::from_value(body)?;
        assert!(page.data.events.len() <= 8);
        sequences.extend(page.data.events.iter().map(|e| e.sequence));
        after = page.data.cursor;
        if after == page.data.snapshot.cursor {
            break;
        }
    }
    assert_eq!(sequences, (1..=after).collect::<Vec<_>>());
    assert_eq!(service.list().await?.len(), 1);
    let conflict = service
        .submit(fixture("once", 31))
        .await
        .err()
        .ok_or("conflict missing")?;
    assert_eq!(conflict.code, "conflict");
    service.shutdown().await;
    drop(service);
    let restarted = Service::open(root.config()).await?;
    let same = restarted.submit(fixture("once", 30)).await?;
    assert_eq!(same.task_id, id);
    assert_eq!(
        same.run.ok_or("missing run")?.statistics.logical_tool_calls,
        1
    );
    restarted.shutdown().await;
    Ok(())
}

#[tokio::test]
async fn all_reads_and_commands_require_authentication_and_a_supported_version() -> TestResult {
    let root = Root::new()?;
    let service = Service::open(root.config()).await?;
    let path = service.publish("http://127.0.0.1:1/".into()).await?;
    let token = fishnet_service::read_connection(&path)?.token;
    let app = fishnet_service::http::router(service.clone());
    for (method, path) in [
        ("GET", "/v1/tasks"),
        ("GET", "/v1/tasks/unknown/events"),
        ("POST", "/v1/tasks"),
        ("POST", "/v1/tasks/unknown/cancel"),
    ] {
        assert_eq!(
            request(app.clone(), method, path, None, Value::Null)
                .await?
                .0,
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            request(app.clone(), method, path, Some("incorrect"), Value::Null)
                .await?
                .0,
            StatusCode::UNAUTHORIZED
        );
    }
    let (status, _) = request(
        app.clone(),
        "POST",
        "/v1/tasks",
        Some(&token),
        json!({"schema_version":999,"data":fixture("bad-version",0)}),
    )
    .await?;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(service.list().await?.is_empty());
    let request = Request::builder()
        .uri("/v1/tasks")
        .header("authorization", format!("Bearer {token}"))
        .header("origin", "https://untrusted.invalid")
        .body(Body::empty())?;
    assert_eq!(app.oneshot(request).await?.status(), StatusCode::FORBIDDEN);
    service.shutdown().await;
    Ok(())
}

#[tokio::test]
async fn approval_is_durable_exclusive_and_cannot_change_the_prepared_grants() -> TestResult {
    let root = Root::new()?;
    root.task()?;
    let mut config = root.config();
    config.allow_code = true;
    config.provider = fishnet_service::ProviderAccess::ApiKey {
        endpoint: "https://api.openai.com/v1/responses".into(),
        key_env: format!("FISHNET_MISSING_TEST_KEY_{}", std::process::id()),
        default_model: Some("fixture".into()),
    };
    let service = Service::open(config.clone()).await?;
    let pending = service
        .submit(Submission {
            request_id: "approval".into(),
            task: TaskRequest::Code {
                task_path: "tasks/change.json".into(),
                model: None,
                account: "default".into(),
            },
        })
        .await?;
    assert_eq!(pending.phase, Phase::AwaitingApproval);
    let grant = pending.approval.ok_or("no grant")?;
    assert_eq!(grant.allowed_writes, vec!["work.txt"]);
    fs::write(root.0.join("tasks/change.json"), "changed after submission")?;
    let decision = Approval {
        expected_revision: 1,
        approve: false,
    };
    let (left, right) = tokio::join!(
        service.approve(&pending.task_id, decision.clone()),
        service.approve(&pending.task_id, decision)
    );
    assert_eq!(usize::from(left.is_ok()) + usize::from(right.is_ok()), 1);
    assert_eq!(
        service.snapshot(&pending.task_id).await?.phase,
        Phase::Rejected
    );
    assert_eq!(fs::read_to_string(root.0.join("work.txt"))?, "original");
    service.shutdown().await;
    drop(service);
    let restarted = Service::open(config).await?;
    assert_eq!(
        restarted.snapshot(&pending.task_id).await?.phase,
        Phase::Rejected
    );
    assert!(
        restarted
            .approve(
                &pending.task_id,
                Approval {
                    expected_revision: 1,
                    approve: true
                }
            )
            .await
            .is_err()
    );
    restarted.shutdown().await;
    Ok(())
}

#[tokio::test]
async fn cancellation_and_active_capacity_are_service_owned() -> TestResult {
    let root = Root::new()?;
    let service = Service::open(root.config()).await?;
    let mut ids = Vec::new();
    for n in 0..4 {
        ids.push(
            service
                .submit(fixture(&format!("active-{n}"), 1000))
                .await?
                .task_id,
        );
    }
    assert_eq!(
        service
            .submit(fixture("over-capacity", 1000))
            .await
            .err()
            .ok_or("no capacity error")?
            .code,
        "capacity"
    );
    for id in &ids {
        service.cancel(id).await?;
    }
    for id in &ids {
        assert_eq!(
            terminal(&service, id).await?.run.ok_or("no run")?.state,
            TaskState::Cancelled
        );
    }
    service.shutdown().await;
    Ok(())
}

#[tokio::test]
async fn restart_recovers_unknown_work_without_replaying_it() -> TestResult {
    let root = Root::new()?;
    let service = Service::open(root.config()).await?;
    let view = service.submit(fixture("restart", 0)).await?;
    let done = terminal(&service, &view.task_id).await?;
    service.shutdown().await;
    drop(service);
    let receipt_path = root.0.join("service/requests/restart.json");
    let mut receipt: Value = serde_json::from_slice(&fs::read(&receipt_path)?)?;
    receipt
        .as_object_mut()
        .ok_or("no receipt")?
        .insert("phase".into(), json!("executing"));
    fs::write(&receipt_path, serde_json::to_vec(&receipt)?)?;
    let journal = root.0.join("runs").join(format!("{}.jsonl", view.task_id));
    let text = fs::read_to_string(&journal)?;
    let prefix = format!("{}\n", text.lines().take(3).collect::<Vec<_>>().join("\n"));
    fs::write(&journal, &prefix)?;
    let partial = receipt_path.with_extension("pending");
    fs::write(&partial, b"{\"schema_version\":")?;
    fs::set_permissions(&partial, fs::Permissions::from_mode(0o600))?;
    let restarted = Service::open(root.config()).await?;
    assert!(!partial.exists());
    assert_eq!(fs::read_dir(root.0.join("service/incidents"))?.count(), 1);
    let recovered = restarted.snapshot(&view.task_id).await?;
    assert_eq!(recovered.phase, Phase::Interrupted);
    let run = recovered.run.ok_or("no run")?;
    assert!(run.incomplete && run.outcome.is_none());
    assert_eq!(run.pending_operation_ids.len(), 1);
    assert!(done.run.ok_or("no original run")?.outcome.is_some());
    let duplicate = restarted.submit(fixture("restart", 0)).await?;
    assert_eq!(duplicate.task_id, view.task_id);
    assert_eq!(fs::read_to_string(journal)?, prefix);
    restarted.shutdown().await;
    Ok(())
}

#[tokio::test]
async fn service_credentials_reject_loose_permissions_and_duplicate_owners() -> TestResult {
    let root = Root::new()?;
    let service = Service::open(root.config()).await?;
    assert!(Service::open(root.config()).await.is_err());
    let path = service.publish("http://127.0.0.1:1/".into()).await?;
    assert!(!format!("{service:?}").contains(&fishnet_service::read_connection(&path)?.token));
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644))?;
    assert!(fishnet_service::read_connection(&path).is_err());
    service.shutdown().await;
    Ok(())
}

#[tokio::test]
async fn slow_event_consumers_are_bounded_and_leave_room_for_cancel() -> TestResult {
    let root = Root::new()?;
    let service = Service::open(root.config()).await?;
    let mut submission = fixture("slow-consumers", 60_000);
    if let TaskRequest::Fixture { limits, .. } = &mut submission.task {
        limits.deadline_ms = 300_000;
    }
    let id = service.submit(submission).await?.task_id;
    tokio::time::timeout(Duration::from_secs(5), async {
        while service.snapshot(&id).await.map_or(0, |v| v.cursor) < 3 {
            tokio::task::yield_now().await;
        }
    })
    .await?;
    let after = service.snapshot(&id).await?.cursor;
    let connection = service.publish("http://127.0.0.1:1/".into()).await?;
    let token = fishnet_service::read_connection(&connection)?.token;
    let app = fishnet_service::http::router(service.clone());
    let mut consumers = Vec::new();
    for _ in 0..8 {
        let app = app.clone();
        let token = token.clone();
        let path = format!("/v1/tasks/{id}/events?after={after}&wait_ms=15000");
        consumers.push(tokio::spawn(async move {
            let request = Request::builder()
                .uri(path)
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())?;
            Ok::<_, Box<dyn Error + Send + Sync>>(app.oneshot(request).await?.status())
        }));
    }
    tokio::time::sleep(Duration::from_millis(30)).await;
    assert!(consumers.iter().all(|c| !c.is_finished()));
    let status = request(
        app.clone(),
        "GET",
        &format!("/v1/tasks/{id}/events?after={after}&wait_ms=15000"),
        Some(&token),
        Value::Null,
    )
    .await?
    .0;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(
        request(
            app,
            "POST",
            &format!("/v1/tasks/{id}/cancel"),
            Some(&token),
            serde_json::to_value(Envelope::new(()))?
        )
        .await?
        .0,
        StatusCode::OK
    );
    for consumer in consumers {
        assert_eq!(consumer.await?.map_err(|e| e.to_string())?, StatusCode::OK);
    }
    assert_eq!(
        terminal(&service, &id).await?.run.ok_or("no run")?.state,
        TaskState::Cancelled
    );
    service.shutdown().await;
    Ok(())
}

#[tokio::test]
async fn approval_handoff_uses_the_frozen_contract_and_is_not_executed_twice() -> TestResult {
    let root = Root::new()?;
    root.task()?;
    let mut config = root.config();
    config.allow_code = true;
    let key_env = format!("FISHNET_UNSET_APPROVAL_KEY_{}", std::process::id());
    assert!(std::env::var(&key_env).is_err());
    config.provider = fishnet_service::ProviderAccess::ApiKey {
        endpoint: "https://api.openai.com/v1/responses".into(),
        key_env,
        default_model: Some("fixture".into()),
    };
    let service = Service::open(config).await?;
    let view = service
        .submit(Submission {
            request_id: "handoff".into(),
            task: TaskRequest::Code {
                task_path: "tasks/change.json".into(),
                model: None,
                account: "default".into(),
            },
        })
        .await?;
    fs::write(root.0.join("tasks/change.json"), "changed host task file")?;
    let approval = Approval {
        expected_revision: 1,
        approve: true,
    };
    let (left, right) = tokio::join!(
        service.approve(&view.task_id, approval.clone()),
        service.approve(&view.task_id, approval)
    );
    assert_eq!(usize::from(left.is_ok()) + usize::from(right.is_ok()), 1);
    let done = terminal(&service, &view.task_id).await?;
    assert_eq!(done.approval_decision, Some(true));
    assert!(done.approval_wait_ms.is_some());
    assert_eq!(done.phase, Phase::Failed);
    assert!(done.run.is_none());
    assert!(
        done.message
            .ok_or("no error")?
            .contains("API key is unavailable")
    );
    assert_eq!(fs::read_to_string(root.0.join("work.txt"))?, "original");
    service.shutdown().await;
    Ok(())
}

#[tokio::test]
async fn guided_rust_tasks_freeze_host_checks_and_require_approval() -> TestResult {
    let root = Root::new()?;
    fs::write(root.0.join("Cargo.toml"), "[workspace]\n")?;
    fs::write(root.0.join("work.rs"), "original")?;
    fs::write(root.0.join("AGENTS.md"), "protected instructions")?;
    let config = Config {
        allow_code: true,
        ..root.config()
    };
    let service = Service::open(config.clone()).await?;
    let draft = |id: &str, files: Vec<String>| Submission {
        request_id: id.into(),
        task: TaskRequest::Rust {
            objective: "Update work.rs and test the result".into(),
            files,
            model: None,
            account: "default".into(),
        },
    };
    for (id, files) in [
        ("escape", vec!["../outside".into()]),
        ("instructions", vec!["AGENTS.md".into()]),
    ] {
        assert!(service.submit(draft(id, files)).await.is_err());
    }
    let pending = service.submit(draft("new", vec!["work.rs".into()])).await?;
    assert_eq!(pending.phase, Phase::AwaitingApproval);
    let grant = pending.approval.ok_or("no grant")?;
    assert_eq!(grant.allowed_writes, vec!["work.rs"]);
    assert_eq!(grant.required_changes, vec!["work.rs"]);
    assert_eq!(grant.checks, vec!["format", "test", "lint", "docs"]);
    assert_eq!(grant.commands.len(), 4);
    assert!(
        grant
            .commands
            .iter()
            .all(|command| command.program.is_absolute())
    );
    assert!(!root.0.join("runs").exists());
    service.shutdown().await;
    drop(service);
    let restarted = Service::open(config).await?;
    let restored = restarted.snapshot(&pending.task_id).await?;
    assert_eq!(
        restored.approval.ok_or("lost grant")?.fingerprint,
        grant.fingerprint
    );
    restarted
        .approve(
            &pending.task_id,
            Approval {
                expected_revision: 1,
                approve: false,
            },
        )
        .await?;
    assert!(!root.0.join("runs").exists());
    restarted.shutdown().await;
    Ok(())
}

#[tokio::test]
async fn shutdown_requires_authentication_and_a_supported_command_version() -> TestResult {
    let root = Root::new()?;
    let service = Service::open(root.config()).await?;
    let path = service.publish("http://127.0.0.1:1/".into()).await?;
    let token = fishnet_service::read_connection(&path)?.token;
    let app = fishnet_service::http::router(service.clone());
    let body = json!({"schema_version":1,"data":null});
    assert_eq!(
        request(app.clone(), "POST", "/v1/shutdown", None, body.clone())
            .await?
            .0,
        StatusCode::UNAUTHORIZED
    );
    assert!(!service.shutdown_signal().is_cancelled());
    assert_eq!(
        request(
            app.clone(),
            "POST",
            "/v1/shutdown",
            Some(&token),
            json!({"schema_version":99,"data":null})
        )
        .await?
        .0,
        StatusCode::BAD_REQUEST
    );
    assert!(!service.shutdown_signal().is_cancelled());
    assert_eq!(
        request(app, "POST", "/v1/shutdown", Some(&token), body)
            .await?
            .0,
        StatusCode::OK
    );
    assert!(service.shutdown_signal().is_cancelled());
    service.shutdown().await;
    Ok(())
}

#[tokio::test]
async fn conversation_capability_requires_authentication_and_advertises_the_contract() -> TestResult
{
    let root = Root::new()?;
    let service = Service::open(root.config()).await?;
    let path = service.publish("http://127.0.0.1:1/".into()).await?;
    let token = fishnet_service::read_connection(&path)?.token;
    let app = fishnet_service::http::router(service.clone());
    assert_eq!(
        request(app.clone(), "GET", "/v1/capabilities", None, Value::Null)
            .await?
            .0,
        StatusCode::UNAUTHORIZED
    );
    let (status, body) = request(app, "GET", "/v1/capabilities", Some(&token), Value::Null).await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.get("data"), Some(&json!(["conversation-v1"])));
    service.shutdown().await;
    Ok(())
}
