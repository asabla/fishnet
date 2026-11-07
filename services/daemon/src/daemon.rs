use std::net::SocketAddr;

use fishnet_core::Cancellation;
use tokio::net::TcpListener;

use crate::{
    Config, ProviderAccess, Service,
    contract::{ApiError, Result},
};

const HELP: &str = "fishnetd: local task service
Usage: fishnetd [--root DIR] [--directory DIR] [--runs DIR]
                [--bind 127.0.0.1:PORT] [--allow-code] [--auth-dir DIR]
                [--provider chatgpt|openai|responses] [--endpoint URL]
                [--key-env NAME] [--model NAME]
Default: loopback, automatic port, .fishnet/service, coding disabled.
Connection credentials are saved privately; they are never printed.
Ctrl-C cancels active work and waits for owned cleanup.";

/// Start the service from explicit host arguments. No provider runs at startup.
///
/// # Errors
/// Returns invalid arguments, listener, storage, or shutdown errors.
pub fn launch(
    mut args: impl Iterator<Item = String>,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let mut config = Config::default();
    let mut bind = "127.0.0.1:0".to_string();
    let mut provider = "chatgpt".to_string();
    let mut endpoint = None;
    let mut key_env = "OPENAI_API_KEY".to_string();
    let mut model = None;
    let mut auth_directory = ".fishnet/auth".into();
    while let Some(option) = args.next() {
        if option == "--help" || option == "-h" {
            println!("{HELP}");
            return Ok(());
        }
        if option == "--allow-code" {
            config.allow_code = true;
            continue;
        }
        let value = args
            .next()
            .ok_or_else(|| format!("missing value for {option}"))?;
        match option.as_str() {
            "--root" => config.root = value.into(),
            "--directory" => config.directory = value.into(),
            "--runs" => config.runs = value.into(),
            "--bind" => bind = value,
            "--auth-dir" => auth_directory = value.into(),
            "--provider" => provider = value,
            "--endpoint" => endpoint = Some(value),
            "--key-env" => key_env = value,
            "--model" => model = Some(value),
            _ => return Err(format!("unknown service option: {option}").into()),
        }
    }
    config.provider = match provider.as_str() {
        "chatgpt" if endpoint.is_none() => ProviderAccess::Chatgpt { auth_directory },
        "openai" if endpoint.is_none() => ProviderAccess::ApiKey {
            endpoint: "https://api.openai.com/v1/responses".into(),
            key_env,
            default_model: model,
        },
        "responses" => ProviderAccess::ApiKey {
            endpoint: endpoint.ok_or("responses requires --endpoint")?,
            key_env,
            default_model: model,
        },
        _ => return Err(
            "provider must be chatgpt, openai, or responses; endpoint overrides require responses"
                .into(),
        ),
    };
    let bind: SocketAddr = bind.parse()?;
    if !bind.ip().is_loopback() {
        return Err(
            "M3 listeners must bind to loopback; use a protected tunnel for remote access".into(),
        );
    }
    let cancellation = Cancellation::default();
    let signal = cancellation.clone();
    ctrlc::set_handler(move || signal.cancel())?;
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async move {
            let service = Service::open(config).await?;
            let listener = TcpListener::bind(bind)
                .await
                .map_err(|_| ApiError::new("listener_error", "cannot bind service listener"))?;
            let endpoint = format!(
                "http://{}/",
                listener.local_addr().map_err(|_| ApiError::storage())?
            );
            let path = service.publish(endpoint.clone()).await?;
            println!(
                "{}",
                serde_json::json!({"api_version":1,"endpoint":endpoint,"connection_file":path})
            );
            serve(listener, service, cancellation).await
        })?;
    Ok(())
}

/// Serve until explicit shutdown. A client disconnect never stops a task.
///
/// # Errors
/// Returns listener or HTTP server failures.
pub async fn serve(listener: TcpListener, service: Service, shutdown: Cancellation) -> Result<()> {
    let app = crate::http::router(service.clone());
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            while !shutdown.is_cancelled() {
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
            service.shutdown().await;
        })
        .await
        .map_err(|_| ApiError::new("listener_error", "service listener failed"))
}
