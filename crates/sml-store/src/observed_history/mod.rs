//! Bounded observed-history references for DKV A3, not a source truth engine.
//!
//! Input claims and compressed numeric memory have independent authority.
//! Counts and element budgets are not allocator or process RAM measurements.

mod cache;
mod journal;
mod query;
mod screen;
mod verified;
mod wire;

pub use cache::{FrozenDeltaKvRead, FrozenReadError};
pub use journal::{HistoryError, HistoryLimits, ObservationEvent, ObservationLog, SubjectKey};
pub use query::{HistoryQuery, HistorySummary, ObservedState};
pub use screen::{screen_candidate, ScreenedCandidate, ScreeningReason};
pub use verified::{
    verify_history, HistoryVerificationError, HistoryVerificationLimits, VerifiedHistory,
};
pub use wire::{encode_event, parse_event, WireError, MAX_EVENT_BYTES};
