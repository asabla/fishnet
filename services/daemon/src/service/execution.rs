use std::time::Duration;

use fishnet_core::provider::openai::OpenAiProvider;
use fishnet_core::{Cancellation, Event, RunOptions, Scenario, ScriptedProvider};

use super::store::Receipt;
use crate::contract::{ApiError, Result, TaskRequest};
use crate::service::{Config, ProviderAccess};

pub(super) async fn execute(
    config: &Config,
    receipt: &Receipt,
    cancellation: Cancellation,
    observe: impl FnMut(&Event) + Send,
) -> Result<()> {
    match &receipt.submission.task {
        TaskRequest::Chat { model, account, .. } => {
            execute_chat(
                config,
                receipt,
                model.clone(),
                account,
                cancellation,
                observe,
            )
            .await?;
        }
        TaskRequest::Fixture {
            scenario,
            delay_ms,
            limits,
        } => {
            let scenario = Scenario::parse(scenario)
                .ok_or_else(|| ApiError::invalid("invalid fixture scenario"))?;
            let (provider, contract) =
                ScriptedProvider::scenario(scenario, Duration::from_millis(*delay_ms))
                    .map_err(|_| ApiError::invalid("invalid fixture"))?;
            let options = RunOptions {
                store: config.runs.clone(),
                limits: limits.clone(),
                contract,
                cancellation,
            };
            fishnet_core::runtime::run_identified(
                options,
                provider,
                observe,
                receipt.task_id.clone(),
            )
            .await
            .map_err(|_| ApiError::storage())?;
        }
        TaskRequest::Code { model, account, .. } | TaskRequest::Rust { model, account, .. } => {
            let task = receipt.prepared.clone().ok_or_else(ApiError::storage)?;
            let provider =
                provider(&receipt.provider, model.clone(), account, &cancellation).await?;
            let options = RunOptions {
                store: config.runs.clone(),
                limits: task.limits.clone(),
                contract: task.into_contract(),
                cancellation,
            };
            fishnet_core::runtime::run_identified(
                options,
                provider,
                observe,
                receipt.task_id.clone(),
            )
            .await
            .map_err(|_| ApiError::storage())?;
        }
    }
    Ok(())
}

async fn execute_chat(
    config: &Config,
    receipt: &Receipt,
    model: Option<String>,
    account: &str,
    cancellation: Cancellation,
    observe: impl FnMut(&Event) + Send,
) -> Result<()> {
    let contract = receipt
        .conversation
        .clone()
        .ok_or_else(ApiError::storage)?
        .into_contract();
    let options = RunOptions {
        store: config.runs.clone(),
        limits: fishnet_core::conversation::limits(),
        contract,
        cancellation,
    };
    if receipt.provider == ProviderAccess::Fixture {
        fishnet_core::runtime::run_identified(
            options,
            fishnet_core::conversation::Fixture,
            observe,
            receipt.task_id.clone(),
        )
        .await
        .map_err(|_| ApiError::storage())?;
    } else {
        let provider = provider(
            &receipt.provider,
            model.clone(),
            account,
            &options.cancellation,
        )
        .await?;
        fishnet_core::runtime::run_identified(options, provider, observe, receipt.task_id.clone())
            .await
            .map_err(|_| ApiError::storage())?;
    }
    Ok(())
}

async fn provider(
    access: &ProviderAccess,
    model: Option<String>,
    account: &str,
    cancel: &Cancellation,
) -> Result<OpenAiProvider> {
    let result = match access {
        ProviderAccess::Fixture => {
            return Err(ApiError::invalid(
                "fixture provider does not run coding tasks",
            ));
        }
        ProviderAccess::Chatgpt { auth_directory } => {
            let models = fishnet_core::auth::models(auth_directory, account, cancel)
                .await
                .map_err(|e| ApiError::new("provider_error", &e.message))?;
            let model = if let Some(model) = model {
                models
                    .into_iter()
                    .find(|m| m.id == model)
                    .ok_or_else(|| ApiError::invalid("model is not available to this account"))?
                    .id
            } else {
                models
                    .into_iter()
                    .next()
                    .ok_or_else(|| ApiError::invalid("account has no models"))?
                    .id
            };
            let token = fishnet_core::auth::bearer(auth_directory, account, cancel)
                .await
                .map_err(|e| ApiError::new("provider_error", &e.message))?;
            OpenAiProvider::chatgpt(model, &token)
        }
        ProviderAccess::ApiKey {
            endpoint,
            key_env,
            default_model,
        } => {
            let key = std::env::var(key_env)
                .map_err(|_| ApiError::invalid("configured service API key is unavailable"))?;
            let model = model
                .or_else(|| default_model.clone())
                .ok_or_else(|| ApiError::invalid("API provider requires a model"))?;
            OpenAiProvider::new(endpoint, model, &key)
        }
    };
    result.map_err(|e| ApiError::new("provider_error", &e.message))
}
