//! M1 task contracts and deterministic fixture components.
pub mod control;
pub mod fixture;
pub mod protocol;
pub mod tool;

pub use control::Cancellation;
pub use fixture::{Scenario, ScriptedProvider};
pub use protocol::{Event, Limits, Outcome, TaskState};
