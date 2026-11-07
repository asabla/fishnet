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
                if serde_json::to_writer(
                    &mut output,
                    &serde_json::json!({"type":"authorization_required","url":url,"account":label}),
                )
                .map_err(io::Error::other)
                .and_then(|()| writeln!(output))
                .and_then(|()| output.flush())
                .is_err()
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
    serde_json::to_writer(&mut output, &result)?;
    writeln!(output)?;
    Ok(0)
}
