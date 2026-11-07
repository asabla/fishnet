use std::io::{self, Write};
use std::path::PathBuf;

use fishnet_core::provider::openai::OpenAiProvider;
use fishnet_core::{Cancellation, RunOptions, TaskState, auth, run, workspace};

#[derive(Debug)]
struct Options {
    task: PathBuf,
    root: Option<PathBuf>,
    store: PathBuf,
    auth_directory: PathBuf,
    account: String,
    provider: String,
    model: Option<String>,
    endpoint: String,
    key_env: String,
}

#[expect(
    clippy::redundant_pub_crate,
    reason = "Private binary modules use crate-visible entry points to satisfy unreachable_pub."
)]
pub(crate) fn execute(
    args: impl Iterator<Item = String>,
) -> Result<u8, Box<dyn std::error::Error>> {
    let options = parse(args)?;
    let task = workspace::load_task(&options.task, options.root.as_deref())?;
    let cancellation = Cancellation::default();
    let signal = cancellation.clone();
    ctrlc::set_handler(move || signal.cancel())?;
    let provider = create_provider(&options, &cancellation)?;
    let limits = task.limits.clone();
    let run_options = RunOptions {
        store: options.store,
        limits,
        contract: task.into_contract(),
        cancellation,
    };
    let mut output = io::stdout();
    let mut output_error = None;
    let result = run(run_options, provider, |event| {
        if output_error.is_none() {
            output_error = serde_json::to_writer(&mut output, event)
                .map_err(io::Error::other)
                .and_then(|()| writeln!(output))
                .and_then(|()| output.flush())
                .err();
        }
    })?;
    if let Some(error) = output_error {
        return Err(error.into());
    }
    Ok(match result.outcome.state {
        TaskState::VerifiedSuccess => 0,
        TaskState::UnverifiedCompletion => 3,
        TaskState::Cancelled => 130,
        _ => 2,
    })
}

fn create_provider(
    options: &Options,
    cancellation: &Cancellation,
) -> Result<OpenAiProvider, Box<dyn std::error::Error>> {
    match options.provider.as_str() {
        "chatgpt" => {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?;
            let models = runtime.block_on(auth::models(
                &options.auth_directory,
                &options.account,
                cancellation,
            ))?;
            let model = if let Some(selected) = &options.model {
                models
                    .iter()
                    .find(|model| &model.id == selected)
                    .ok_or("selected model is not listed for this ChatGPT account")?
            } else {
                models
                    .first()
                    .ok_or("ChatGPT account has no visible models")?
            };
            let token = runtime.block_on(auth::bearer(
                &options.auth_directory,
                &options.account,
                cancellation,
            ))?;
            Ok(OpenAiProvider::chatgpt(model.id.clone(), &token)?)
        }
        "openai" | "responses" => {
            let key = std::env::var(&options.key_env)
                .map_err(|_| format!("credential variable {} is not set", options.key_env))?;
            let model = options
                .model
                .clone()
                .ok_or("API providers require --model")?;
            Ok(OpenAiProvider::new(&options.endpoint, model, &key)?)
        }
        _ => Err("provider must be chatgpt, openai, or responses".into()),
    }
}

fn parse(mut args: impl Iterator<Item = String>) -> Result<Options, String> {
    let mut options = Options {
        task: PathBuf::new(),
        root: None,
        store: ".fishnet/runs".into(),
        auth_directory: ".fishnet/auth".into(),
        account: "default".into(),
        provider: "chatgpt".into(),
        model: None,
        endpoint: "https://api.openai.com/v1/responses".into(),
        key_env: "OPENAI_API_KEY".into(),
    };
    while let Some(option) = args.next() {
        let value = args
            .next()
            .ok_or_else(|| format!("missing value for {option}"))?;
        match option.as_str() {
            "--task" => options.task = value.into(),
            "--root" => options.root = Some(value.into()),
            "--store" => options.store = value.into(),
            "--auth-dir" => options.auth_directory = value.into(),
            "--account" => options.account = value,
            "--provider" => options.provider = value,
            "--model" => options.model = Some(value),
            "--endpoint" => options.endpoint = value,
            "--key-env" => options.key_env = value,
            _ => return Err(format!("unknown coding option: {option}")),
        }
    }
    if options.task.as_os_str().is_empty() {
        return Err("code requires --task PATH".into());
    }
    if options.provider == "chatgpt" && options.endpoint != "https://api.openai.com/v1/responses" {
        return Err(
            "ChatGPT credentials can be sent only to the public OpenAI Responses endpoint".into(),
        );
    }
    Ok(options)
}
