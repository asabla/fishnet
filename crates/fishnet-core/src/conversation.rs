//! Bounded, explicit conversation context. It grants no workspace or tool access.
use serde::{Deserialize, Serialize};

pub const POLICY: &str = "conversation-no-tools-v1";
pub const REVISION: &str = "conversation-text-v1";
pub const MAX_BYTES: usize = 32_768;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    User,
    Assistant,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Message {
    pub role: Role,
    pub text: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Context {
    pub schema_version: u32,
    pub root_task_id: String,
    pub messages: Vec<Message>,
}

impl Context {
    #[must_use]
    pub fn valid(&self) -> bool {
        self.schema_version == 1
            && !self.root_task_id.is_empty()
            && self.root_task_id.len() <= 64
            && self
                .root_task_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
            && !self.messages.is_empty()
            && self.messages.len() <= 31
            && self.messages.len() % 2 == 1
            && self.messages.iter().enumerate().all(|(index, message)| {
                message.role
                    == if index % 2 == 0 {
                        Role::User
                    } else {
                        Role::Assistant
                    }
                    && !message.text.trim().is_empty()
                    && message.text.len() <= 8192
            })
            && self
                .messages
                .iter()
                .map(|message| message.text.len())
                .sum::<usize>()
                <= MAX_BYTES
    }

    #[must_use]
    pub fn into_contract(self) -> crate::protocol::TaskContract {
        crate::protocol::TaskContract {
            objective: self
                .messages
                .last()
                .map_or_else(String::new, |message| message.text.clone()),
            expected_key: "conversation".into(),
            expected_value: None,
            policy_version: POLICY.into(),
            fixture_revision: REVISION.into(),
            workspace: None,
            conversation: Some(self),
        }
    }
}

#[must_use]
pub const fn limits() -> crate::Limits {
    crate::Limits {
        deadline_ms: 120_000,
        max_provider_calls: 1,
        max_output_bytes: 8192,
    }
}

/// Deterministic offline conversation provider. Its output is not a verified task outcome.
#[derive(Debug)]
pub struct Fixture;
impl crate::provider::ModelProvider for Fixture {
    fn version(&self) -> &'static str {
        "conversation-fixture-v1"
    }

    async fn respond(
        &mut self,
        context: crate::provider::ProviderContext<'_>,
        _previous: Option<&crate::protocol::ToolResult>,
        cancellation: &crate::Cancellation,
        deadline: std::time::Instant,
    ) -> Result<crate::provider::ProviderTurn, crate::protocol::OperationError> {
        cancellation.check(deadline)?;
        let messages = context
            .contract
            .conversation
            .as_ref()
            .map_or(&[][..], |context| context.messages.as_slice());
        let current = messages.last().map_or("", |message| message.text.as_str());
        let previous = messages
            .iter()
            .rev()
            .skip(1)
            .find(|message| message.role == Role::User)
            .map_or("none", |message| message.text.as_str());
        Ok(crate::provider::ProviderTurn {
            reply: crate::protocol::ModelReply::Final {
                text: format!(
                    "Offline fixture response: {current}\nPrevious user message: {previous}"
                ),
            },
            observation: None,
        })
    }
}
