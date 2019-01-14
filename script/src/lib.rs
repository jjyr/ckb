mod cost_model;
mod syscalls;
#[cfg(test)]
mod tests;
mod verify;

use ckb_vm::Error as VMInternalError;

pub use crate::verify::{ChainContext, TransactionScriptsVerifier};

#[derive(Debug, PartialEq, Clone, Copy, Eq)]
pub enum ScriptError {
    NoScript,
    InvalidReferenceIndex,
    ArgumentError,
    ValidationFailure(u8),
    VMError(VMInternalError),
    ExceededMaximumCycles,
}
