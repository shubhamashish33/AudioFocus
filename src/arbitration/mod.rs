mod debounce;
mod decision;
mod engine;
mod loop_guard;
mod ownership;
mod state;
mod suppression;

pub use engine::{
    ArbitrationConfig, ArbitrationEngine, ArbitrationEvent, ArbitrationHandle, ControllerRegistry,
    PauseExecutionResult, PauseRoute,
};
pub use state::{ArbitrationSnapshot, PauseOrigin};
