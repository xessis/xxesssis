//! Orbit speculative / shadow workspace engine.
//!
//! Simulates in-memory file edits, validates bracket/brace/parenthesis syntax
//! integrity to prevent catastrophic breaks, and computes Myers line diffs
//! before changes are applied to real editor buffers or committed.

pub mod bracket;
pub mod buffer;
pub mod error;
pub mod session;

pub use bracket::{
    validate_brackets, BracketKind, BracketMismatch, BracketSpan, BracketValidationReport, Language,
};
pub use buffer::ShadowBuffer;
pub use error::{Result, ShadowError};
pub use session::{
    EditOperation, FileCommitRecord, SessionState, SpeculativeEditSession, SpeculativeFileEdit,
    TransactionSummary,
};
