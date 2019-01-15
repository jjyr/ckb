use ckb_core::{BlockNumber, Cycle};
use ckb_script::ScriptError;
use ckb_shared::error::SharedError;
use numext_fixed_hash::H256;
use numext_fixed_uint::U256;

/// Block verification error
#[derive(Debug, PartialEq, Clone, Eq)]
pub enum Error {
    /// PoW proof is corrupt or does not meet the difficulty target.
    Pow(PowError),
    /// The field timestamp in block header is invalid.
    Timestamp(TimestampError),
    /// The field number in block header is invalid.
    Number(NumberError),
    /// The field difficulty in block header is invalid.
    Difficulty(DifficultyError),
    /// Committed transactions verification error. It contains error for the first transaction that
    /// fails the verification. The errors are stored as a tuple, where the first item is the
    /// transaction index in the block and the second item is the transaction verification error.
    Transactions((usize, TransactionError)),
    /// This is a wrapper of error encountered when invoking chain API.
    Chain(SharedError),
    /// The committed transactions list is empty.
    CommitTransactionsEmpty,
    /// There are duplicate proposed transactions.
    ProposalTransactionDuplicate,
    /// There are duplicate committed transactions.
    CommitTransactionDuplicate,
    /// The merkle tree hash of proposed transactions does not match the one in header.
    ProposalTransactionsRoot,
    /// The merkle tree hash of committed transactions does not match the one in header.
    CommitTransactionsRoot,
    /// The parent of the block is unknown.
    UnknownParent(H256),
    /// Uncles does not meet the consensus requirements.
    Uncles(UnclesError),
    /// Cellbase transaction is invalid.
    Cellbase(CellbaseError),
    /// This error is returned when the committed transactions does not meet the 2-phases
    /// propose-then-commit consensus rule.
    Commit(CommitError),
    /// Cycles consumed by all scripts in all commit transactions of the block not match txs_cycles
    /// in the block header
    ScriptCycles(ScriptCyclesError),
}

#[derive(Debug, PartialEq, Clone, Eq)]
pub enum CommitError {
    /// Ancestor not found, should not happen, we check header first and check ancestor.
    AncestorNotFound,
    /// Break propose-then-commit consensus rule.
    Invalid,
}

#[derive(Debug, PartialEq, Clone, Eq)]
pub enum CellbaseError {
    InvalidInput,
    InvalidReward,
    InvalidQuantity,
    InvalidPosition,
}

#[derive(Debug, PartialEq, Clone, Eq)]
pub enum UnclesError {
    OverCount {
        max: usize,
        actual: usize,
    },
    MissMatchCount {
        expected: u32,
        actual: u32,
    },
    InvalidDepth {
        max: BlockNumber,
        min: BlockNumber,
        actual: BlockNumber,
    },
    InvalidHash {
        expected: H256,
        actual: H256,
    },
    InvalidDifficulty,
    InvalidDifficultyEpoch,
    InvalidProof,
    ProposalTransactionsRoot,
    ProposalTransactionDuplicate,
    Duplicate(H256),
    InvalidInclude(H256),
    InvalidCellbase,
}

#[derive(Debug, PartialEq, Clone, Eq)]
pub enum PowError {
    Boundary { expected: U256, actual: U256 },
    InvalidProof,
}

#[derive(Debug, PartialEq, Clone, Copy, Eq)]
pub enum TimestampError {
    BlockTimeTooOld { min: u64, found: u64 },
    BlockTimeTooNew { max: u64, found: u64 },
}

#[derive(Debug, PartialEq, Clone, Copy, Eq)]
pub struct NumberError {
    pub expected: u64,
    pub actual: u64,
}

#[derive(Debug, PartialEq, Clone, Eq)]
pub enum DifficultyError {
    MixMismatch { expected: U256, actual: U256 },
    AncestorNotFound,
}

#[derive(Debug, PartialEq, Clone, Copy, Eq)]
pub enum TransactionError {
    NullInput,
    /// Occur output's bytes_len exceed capacity
    CapacityOverflow,
    DuplicateInputs,
    Empty,
    /// Sum of all outputs capacity exceed sum of all inputs in the transaction
    OutputsSumOverflow,
    InvalidScript,
    ScriptFailure(ScriptError),
    InvalidSignature,
    DoubleSpent,
    UnknownInput,
}

#[derive(Debug, PartialEq, Clone, Eq)]
pub enum ScriptCyclesError {
    /// Cycles consumed by all scripts in all commit transactions of the block exceed
    /// the maximum allowed cycles in consensus rules
    ExceededMaximum,
    /// Actually consumed cycles mismatch with txs_cycles field in the block header
    Mismatch { expected: Cycle, actual: Cycle },
}

impl From<SharedError> for Error {
    fn from(e: SharedError) -> Self {
        Error::Chain(e)
    }
}

impl From<ScriptCyclesError> for Error {
    fn from(e: ScriptCyclesError) -> Self {
        Error::ScriptCycles(e)
    }
}
