use std::collections::VecDeque;
use std::error::Error;
use std::time::{Duration, Instant};

use super::*;
use crate::fixture::{Scenario, ScriptedProvider};
use crate::protocol::TaskContract;

type TestResult = Result<(), Box<dyn Error>>;

fn contract() -> Result<TaskContract, Box<dyn Error>> {
    Ok(ScriptedProvider::scenario(Scenario::Success, Duration::ZERO)?.1)
}

fn context(contract: &TaskContract) -> ProviderContext<'_> {
    ProviderContext {
        contract,
        instructions: "Fixture instructions.",
        tools: &[],
        remaining_provider_calls: 16,
    }
}

fn response(output: &[Value]) -> Value {
    json!({"id":"resp_test","model":"fixture-resolved","status":"completed","output":output,
        "usage":{"input_tokens":40,"output_tokens":12,"input_tokens_details":{"cached_tokens":5}}})
}

fn call(id: &str, arguments: &str) -> Value {
    json!({"type":"function_call","id":format!("item_{id}"),"call_id":id,"name":"read_file","namespace":"fishnet",
        "arguments":arguments,"status":"completed"})
}

fn stream(response: &Value) -> Vec<Vec<u8>> {
    let data = format!(
        "data: {}\n\n",
        json!({"type":"response.completed","response":response})
    );
    data.as_bytes().chunks(7).map(<[u8]>::to_vec).collect()
}

#[tokio::test]
async fn direct_adapter_preserves_native_items_result_ids_usage_and_prompt_sections() -> TestResult
{
    let mut provider = OpenAiProvider::chatgpt("fixture-model".into(), "test-not-a-credential")?;
    let reasoning = json!({"type":"reasoning","id":"reasoning_test","summary":[],"encrypted_content":"opaque-fixture"});
    let message = json!({"type":"message","id":"msg_test","role":"assistant","phase":"final_answer",
        "content":[{"type":"output_text","text":"Done."}]});
    provider.mock = Some(MockTransport {
        streams: VecDeque::from([
            stream(&response(&[
                reasoning.clone(),
                call("call_one", r#"{"path":"file","offset":0,"max_bytes":20}"#),
            ])),
            stream(&response(&[message])),
        ]),
        requests: Vec::new(),
    });
    let task = contract()?;
    let control = Cancellation::default();
    let deadline = Instant::now() + Duration::from_secs(5);
    let first = provider
        .respond(context(&task), None, &control, deadline)
        .await?;
    assert!(matches!(first.reply, ModelReply::ToolCall(_)));
    let result = ToolResult::Output {
        data: json!({"text":"file data"}),
    };
    let mut final_context = context(&task);
    final_context.remaining_provider_calls = 1;
    let second = provider
        .respond(final_context, Some(&result), &control, deadline)
        .await?;
    assert!(matches!(second.reply, ModelReply::Final { .. }));
    let observation = second.observation.ok_or("missing observation")?;
    assert_eq!(observation.cached_input_tokens, Some(5));
    assert_eq!(observation.model, "fixture-resolved");
    assert_eq!(observation.transport, "sse");
    assert_eq!(observation.input_tokens, Some(40));
    assert!(observation.raw_usage.is_some());
    assert_eq!(observation.prompt_fingerprints.len(), 4);
    let requests = &provider.mock.as_ref().ok_or("no mock")?.requests;
    let first_request = requests.first().ok_or("no request")?;
    assert_eq!(first_request.get("store"), Some(&json!(false)));
    assert_eq!(first_request.get("stream"), Some(&json!(true)));
    assert!(first_request.get("max_output_tokens").is_none());
    assert_eq!(
        first_request.pointer("/tools/0/type"),
        Some(&json!("namespace"))
    );
    assert_eq!(
        requests
            .last()
            .and_then(|request| request.get("tool_choice")),
        Some(&json!("none"))
    );
    let input = requests
        .last()
        .and_then(|value| value.get("input"))
        .and_then(Value::as_array)
        .ok_or("no input")?;
    assert!(input.contains(&reasoning));
    assert!(input.iter().any(
        |item| item.get("type") == Some(&json!("function_call_output"))
            && item.get("call_id") == Some(&json!("call_one"))
    ));
    assert!(!serde_json::to_string(requests)?.contains("test-not-a-credential"));
    Ok(())
}

#[tokio::test]
async fn interrupted_or_failed_streams_never_return_an_executable_call() -> TestResult {
    for chunks in [
        vec![b"data: {\"type\":\"response.function_call_arguments.delta\",\"delta\":\"{\"}\n\n".to_vec()],
        vec![b"data: {\"type\":\"response.failed\",\"response\":{\"error\":{\"code\":\"subscription_sharing_usage_limit_exceeded\"}}}\n\n".to_vec()],
        vec![b"data: malformed\n\n".to_vec()],
    ] {
        let mut provider = OpenAiProvider::chatgpt("fixture-model".into(),"fixture-token")?;
        provider.mock = Some(MockTransport { streams:VecDeque::from([chunks]),requests:Vec::new() });
        let task = contract()?;
        assert!(provider.respond(context(&task),None,&Cancellation::default(),Instant::now()+Duration::from_secs(5)).await.is_err());
    }
    Ok(())
}

#[test]
fn malformed_multiple_duplicate_and_ungranted_namespace_calls_are_rejected() -> TestResult {
    let mut provider = OpenAiProvider::chatgpt("fixture-model".into(), "fixture-token")?;
    assert!(
        provider
            .accept(&response(&[call("bad", "not-json")]))
            .is_err()
    );
    assert!(
        provider
            .accept(&response(&[call("a", "{}"), call("b", "{}")]))
            .is_err()
    );
    provider.accept(&response(&[call("used", "{}")]))?;
    assert!(provider.accept(&response(&[call("used", "{}")])).is_err());
    let mut wrong = call("wrong", "{}");
    wrong
        .as_object_mut()
        .ok_or("not an object")?
        .insert("namespace".into(), json!("other"));
    assert!(provider.accept(&response(&[wrong])).is_err());
    assert!(
        provider
            .accept(&json!({"status":"incomplete","output":[]}))
            .is_err()
    );
    Ok(())
}

#[test]
fn api_credentials_cannot_follow_redirects_or_use_insecure_remote_endpoints() {
    for endpoint in [
        "http://remote.invalid/v1/responses",
        "https://user:pass@api.openai.com/v1/responses",
        "https://api.openai.com/v1/responses?key=secret",
    ] {
        assert!(OpenAiProvider::new(endpoint, "fixture".into(), "fixture-token").is_err());
    }
}
