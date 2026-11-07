use super::{ProviderAccess, Service, State, store::Receipt};
use crate::contract::{ApiError, ApprovalView, Phase, Result, TaskRequest, validate_key};
use fishnet_core::{
    TaskState,
    conversation::{Context, Message, Role},
};

impl Service {
    pub(super) async fn prepare_conversation(
        &self,
        state: &State,
        request: &TaskRequest,
        id: &str,
    ) -> Result<(Option<Context>, bool)> {
        let TaskRequest::Chat {
            message,
            parent_task_id,
            ..
        } = request
        else {
            return Ok((None, false));
        };
        if matches!(self.inner.config.provider, ProviderAccess::Fixture)
            && request.model_account().is_some_and(|(model, _)| {
                model
                    .as_deref()
                    .is_some_and(|model| model != "conversation-fixture-v1")
            })
        {
            return Err(ApiError::invalid(
                "fixture provider requires conversation-fixture-v1",
            ));
        }
        if message.trim().is_empty() || message.len() > 4096 {
            return Err(ApiError::invalid("message requires 1..=4096 bytes"));
        }
        let mut context = match parent_task_id {
            Some(parent_id) => self.inherit_conversation(state, request, parent_id).await?,
            None => Context {
                schema_version: 1,
                root_task_id: id.into(),
                messages: Vec::new(),
            },
        };
        context.messages.push(Message {
            role: Role::User,
            text: message.clone(),
        });
        if !context.valid() {
            return Err(ApiError::invalid(
                "conversation limit reached (16 turns / 32 KiB); use /new. No context was discarded",
            ));
        }
        Ok((Some(context), parent_task_id.is_some()))
    }
    async fn inherit_conversation(
        &self,
        state: &State,
        request: &TaskRequest,
        parent_id: &str,
    ) -> Result<Context> {
        validate_key(parent_id)?;
        let parent = &state
            .tasks
            .get(parent_id)
            .ok_or_else(super::not_found)?
            .receipt;
        if parent.provider != self.inner.config.provider
            || parent.submission.task.model_account() != request.model_account()
        {
            return Err(ApiError::new(
                "forbidden",
                "provider, account, or model changed; start a new conversation for consent",
            ));
        }
        if !parent.phase.terminal() || parent.decision != Some(true) {
            return Err(ApiError::new(
                "conflict",
                "previous turn is not completed and approved",
            ));
        }
        let mut context = parent
            .conversation
            .clone()
            .ok_or_else(|| ApiError::invalid("parent is not a conversation"))?;
        let record = self
            .record(parent_id.to_owned())
            .await?
            .ok_or_else(ApiError::storage)?;
        if record.incomplete
            || !matches!(
                record.state,
                TaskState::UnverifiedCompletion | TaskState::VerifiedSuccess
            )
        {
            return Err(ApiError::new(
                "conflict",
                "previous turn has no completed response; inspect it before retrying",
            ));
        }
        let text = record
            .outcome
            .and_then(|outcome| outcome.final_text)
            .ok_or_else(ApiError::storage)?;
        if text.truncated {
            return Err(ApiError::invalid(
                "previous response is truncated; start a new conversation",
            ));
        }
        context.messages.push(Message {
            role: Role::Assistant,
            text: text.text,
        });
        Ok(context)
    }
}

pub(super) fn approval(receipt: &Receipt) -> Option<ApprovalView> {
    if receipt.phase != Phase::AwaitingApproval {
        return None;
    }
    let (model, account) = receipt.submission.task.model_account()?;
    let task = receipt.prepared.as_ref();
    let context = receipt.conversation.as_ref();
    let objective = task.map(|task| task.objective.clone()).or_else(|| {
        context
            .and_then(|context| context.messages.last())
            .map(|message| message.text.clone())
    })?;
    let (provider, model) = match &receipt.provider {
        ProviderAccess::Chatgpt { .. } => ("ChatGPT account".into(), model.clone()),
        ProviderAccess::ApiKey {
            endpoint,
            default_model,
            ..
        } => (
            format!("API key: {endpoint}"),
            model.clone().or_else(|| default_model.clone()),
        ),
        ProviderAccess::Fixture => (
            "Offline scripted fixture".into(),
            Some("conversation-fixture-v1".into()),
        ),
    };
    let fingerprint = fishnet_core::provider::fingerprint(
        &serde_json::to_vec(&(task, context, &receipt.provider, account, &model))
            .unwrap_or_default(),
    );
    Some(ApprovalView {
        conversation: context.is_some(),
        revision: 1,
        objective,
        provider,
        model,
        account: if matches!(receipt.provider, ProviderAccess::ApiKey { .. }) {
            "host API-key profile".into()
        } else {
            account.into()
        },
        root: task.map_or_else(std::path::PathBuf::new, |task| task.workspace.root.clone()),
        allowed_writes: task.map_or_else(Vec::new, |task| task.workspace.allowed_writes.clone()),
        required_changes: task
            .map_or_else(Vec::new, |task| task.workspace.required_changes.clone()),
        protected_paths: task.map_or_else(Vec::new, |task| task.workspace.protected_paths.clone()),
        commands: task.map_or_else(Vec::new, |task| task.workspace.commands.clone()),
        checks: task.map_or_else(Vec::new, |task| task.workspace.checks.clone()),
        limits: task.map_or_else(fishnet_core::conversation::limits, |task| {
            task.limits.clone()
        }),
        fingerprint,
    })
}

#[cfg(test)]
mod tests;
