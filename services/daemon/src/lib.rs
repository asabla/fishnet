//! Durable task coordination and versioned clients around the owned Rust runtime.
pub mod cli;
pub mod client;
pub mod contract;
pub mod daemon;
pub mod display;
pub mod http;
mod service;

pub use service::local;
pub use service::{Config, ProviderAccess, Service};
pub use service::{Connection, read_connection};

/// Generate an opaque command or task identity without account data.
///
/// # Errors
/// Returns an error if the operating system random source fails.
pub fn new_id() -> contract::Result<String> {
    let mut bytes = [0; 32];
    getrandom::getrandom(&mut bytes).map_err(|_| contract::ApiError::storage())?;
    Ok(fishnet_core::provider::fingerprint(&bytes))
}
