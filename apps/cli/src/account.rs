use std::io::{self, Write};
use std::path::PathBuf;

use fishnet_core::{Cancellation, auth};

#[expect(
    clippy::redundant_pub_crate,
    reason = "Private binary modules use crate-visible entry points to satisfy unreachable_pub."
)]
pub(crate) fn execute(
    mut args: impl Iterator<Item = String>,
    list_models: bool,
) -> Result<u8, Box<dyn std::error::Error>> {
    let arguments = args.by_ref().collect::<Vec<_>>();
    if arguments.is_empty() && !list_models
        || arguments
            .iter()
            .any(|arg| matches!(arg.as_str(), "--help" | "-h"))
    {
        println!(
            "Account commands:\n  fishnet auth login                 Sign in with ChatGPT\n  fishnet auth status                Show local account status without a request\n  fishnet auth logout                Sign out and request session revocation\n  fishnet models                     List available models (makes an account request)\nOptions: --account LABEL, --auth-dir DIR, --json\nCredentials stay in .fishnet/auth. Do not paste tokens into chat."
        );
        return Ok(0);
    }
    let json = arguments.iter().any(|arg| arg == "--json");
    let mut args = arguments.into_iter().filter(|arg| arg != "--json");
    let operation = if list_models {
        "models".into()
    } else {
        args.next()
            .ok_or("auth requires login, status, or logout")?
    };
    let mut directory = PathBuf::from(".fishnet/auth");
    let mut label = String::from("default");
    while let Some(option) = args.next() {
        let value = args.next().ok_or("missing account option value")?;
        match option.as_str() {
            "--auth-dir" => directory = value.into(),
            "--account" => label = value,
            _ => return Err(format!("unknown account option: {option}").into()),
        }
    }
    let cancellation = Cancellation::default();
    let signal = cancellation.clone();
    ctrlc::set_handler(move || signal.cancel())?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let mut output = io::stdout();
    let result = match operation.as_str() {
        "login" => serde_json::to_value(runtime.block_on(auth::login(
            &directory,
            &label,
            &cancellation,
            |url| {
                let result = if json {
                    serde_json::to_writer(&mut output, &serde_json::json!({"type":"authorization_required","url":url,"account":label})).map_err(io::Error::other).and_then(|()| writeln!(output))
                } else {
                    writeln!(output, "Open this sign-in page in your browser:\n\n{url}\n\nWaiting for account authorization. Ctrl-C cancels.")
                };
                if result.and_then(|()| output.flush()).is_err()
                {
                    cancellation.cancel();
                }
            },
        ))?)?,
        "status" => serde_json::to_value(auth::status(&directory, &label)?)?,
        "models" => serde_json::to_value(runtime.block_on(auth::models(
            &directory,
            &label,
            &cancellation,
        ))?)?,
        "logout" => {
            serde_json::json!({"signed_out":true,"remote_revocation_confirmed":runtime.block_on(auth::logout(&directory,&label,&cancellation))?})
        }
        _ => return Err("auth requires login, status, or logout".into()),
    };
    if json {
        serde_json::to_writer(&mut output, &result)?;
        writeln!(output)?;
    } else {
        show(&mut output, &operation, &label, &result)?;
    }
    Ok(0)
}

fn show(
    output: &mut impl Write,
    operation: &str,
    label: &str,
    result: &serde_json::Value,
) -> io::Result<()> {
    if operation == "models" {
        for model in result.as_array().into_iter().flatten() {
            writeln!(
                output,
                "{}  {}",
                model
                    .get("id")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default(),
                model
                    .get("display_name")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
            )?;
        }
    } else if operation == "logout" {
        writeln!(
            output,
            "Account {label}: signed out. Remote revocation confirmed: {}",
            result
                .get("remote_revocation_confirmed")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false)
        )?;
    } else if result.get("connected").and_then(serde_json::Value::as_bool) == Some(true) {
        writeln!(
            output,
            "Account {label}: connected. ChatGPT plan permission: {}",
            if result
                .get("plan_authorized")
                .and_then(serde_json::Value::as_bool)
                == Some(true)
            {
                "authorized"
            } else {
                "not authorized"
            }
        )?;
    } else {
        writeln!(
            output,
            "Account {label}: signed out. Run fishnet auth login."
        )?;
    }
    Ok(())
}
