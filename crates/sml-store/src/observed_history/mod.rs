//! Bounded observed-history references for DKV A3, not a source truth engine.
//!
//! Input claims and compressed numeric memory have independent authority.
//! Counts and element budgets are not allocator or process RAM measurements.

mod journal;
mod query;

pub use journal::{HistoryError, HistoryLimits, ObservationEvent, ObservationLog, SubjectKey};
pub use query::{HistoryQuery, HistorySummary, ObservedState};
