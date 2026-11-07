//! Local execution. The core owns policy, state, tools, and diagnostics.
pub mod auth;
pub mod control;
pub mod conversation;
pub mod fixture;
pub mod process;
pub mod protocol;
pub mod provider;
pub mod runtime;
pub mod storage;
pub mod tool;
pub mod workspace;

pub use control::Cancellation;
pub use fixture::{Scenario, ScriptedProvider};
pub use protocol::{Event, Limits, Outcome, TaskState};
pub use runtime::{RunOptions, run};
pub use storage::{RunRecord, inspect};
