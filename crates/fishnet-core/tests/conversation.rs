use fishnet_core::{
    Cancellation, RunOptions, TaskState,
    conversation::{self, Context, Message, Role},
    fixture::{ScriptStep, ScriptedProvider},
    protocol::{EventData, ModelReply, OperationResult, ToolCall, ToolResult},
};
use std::{error::Error, fs, time::Duration};

type TestResult = Result<(), Box<dyn Error>>;
fn context() -> Context {
    Context {
        schema_version: 1,
        root_task_id: "root".into(),
        messages: vec![Message {
            role: Role::User,
            text: "hello".into(),
        }],
    }
}
#[test]
fn context_requires_alternating_bounded_messages_and_a_valid_identity() {
    let mut value = context();
    assert!(value.valid());
    value.messages.push(Message {
        role: Role::User,
        text: "invalid order".into(),
    });
    assert!(!value.valid());
    value = context();
    value.messages = (0..33)
        .map(|i| Message {
            role: if i % 2 == 0 {
                Role::User
            } else {
                Role::Assistant
            },
            text: "hello".into(),
        })
        .collect();
    assert!(!value.valid());
    value.messages.truncate(5);
    for message in &mut value.messages {
        message.text = "x".repeat(8192);
    }
    assert!(!value.valid());
    value = context();
    value.root_task_id = "../root".into();
    assert!(!value.valid());
}
#[test]
fn untrusted_tool_calls_cannot_escape_conversation_policy() -> TestResult {
    let store = std::env::temp_dir().join(format!("fishnet-chat-policy-{}", std::process::id()));
    let provider = ScriptedProvider::new(
        vec![ScriptStep {
            expected_previous_result: None,
            reply: Ok(ModelReply::ToolCall(ToolCall {
                name: "fixture.lookup".into(),
                arguments: serde_json::json!({"key":"greeting"}),
            })),
        }],
        Duration::ZERO,
    )?;
    let options = RunOptions {
        store: store.clone(),
        contract: context().into_contract(),
        limits: fishnet_core::Limits {
            max_provider_calls: 2,
            ..conversation::limits()
        },
        cancellation: Cancellation::default(),
    };
    let run = fishnet_core::run(options, provider, |_| ())?;
    let record = fishnet_core::inspect(&store, &run.run_id)?;
    assert_ne!(record.state, TaskState::VerifiedSuccess);
    assert!(record.events.iter().any(|event| matches!(
        &event.data,
        EventData::OperationFinished {
            result: OperationResult::Tool {
                result: ToolResult::Rejected { .. }
            }
        }
    )));
    assert!(!record.events.iter().any(|event| matches!(
        &event.data,
        EventData::OperationFinished {
            result: OperationResult::Tool {
                result: ToolResult::Found { .. }
            }
        }
    )));
    fs::remove_dir_all(store)?;
    Ok(())
}
#[test]
fn fixture_responses_remain_unverified_and_cancelled_runs_stay_inspectable() -> TestResult {
    let store = std::env::temp_dir().join(format!("fishnet-chat-outcome-{}", std::process::id()));
    for cancel in [false, true] {
        let cancellation = Cancellation::default();
        if cancel {
            cancellation.cancel();
        }
        let run = fishnet_core::run(
            RunOptions {
                store: store.clone(),
                contract: context().into_contract(),
                limits: conversation::limits(),
                cancellation,
            },
            conversation::Fixture,
            |_| (),
        )?;
        let record = fishnet_core::inspect(&store, &run.run_id)?;
        assert_eq!(
            record.state,
            if cancel {
                TaskState::Cancelled
            } else {
                TaskState::UnverifiedCompletion
            }
        );
        assert_eq!(record.statistics.logical_tool_calls, 0);
        assert_eq!(record.statistics.provider_attempts, u32::from(!cancel));
    }
    fs::remove_dir_all(store)?;
    Ok(())
}
