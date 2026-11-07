//! M1 local execution. The core owns policy, state, tools, and diagnostics.
pub mod control;
pub mod fixture;
pub mod protocol;
pub mod runtime;
pub mod storage;
pub mod tool;

pub use control::Cancellation;
pub use fixture::{Scenario, ScriptedProvider};
pub use protocol::{Event, Limits, Outcome, TaskState};
pub use runtime::{RunOptions, run};
pub use storage::{RunRecord, inspect};
