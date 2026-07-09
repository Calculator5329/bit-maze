//! bit-maze library crate.
//!
//! The engine's core is the `.bm` level format (see [`format`]). The binary
//! (`src/main.rs`) is a thin CLI over these functions so that the same code is
//! exercised by both the game and the integration tests.

pub mod check;
pub mod dump;
pub mod format;
pub mod framebuffer;
pub mod newlevel;
pub mod play;
pub mod vm;
pub mod window;
pub mod world;

pub use format::{BmError, Level, Script};
pub use vm::{Halt, Vm, VmHost};
pub use world::{Move, StepOutcome, StepResult, TriggerRun, World};
