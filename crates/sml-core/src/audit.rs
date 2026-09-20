//! Structural counter gates for SML candidate executions.

use core::fmt;

use crate::ExecutionCounters;

/// Rejects execution evidence that reports prohibited attention-like work.
pub fn validate_non_attention_candidate(
    counters: &ExecutionCounters,
) -> Result<(), CandidateAuditError> {
    if counters.pairwise_comparisons != 0 {
        return Err(CandidateAuditError::PairwiseComparisons(
            counters.pairwise_comparisons,
        ));
    }
    if counters.attention_score_ops != 0 {
        return Err(CandidateAuditError::AttentionScoreOperations(
            counters.attention_score_ops,
        ));
    }
    Ok(())
}

/// Fail-closed reasons for the SML candidate audit gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CandidateAuditError {
    PairwiseComparisons(u64),
    AttentionScoreOperations(u64),
}

impl fmt::Display for CandidateAuditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for CandidateAuditError {}
