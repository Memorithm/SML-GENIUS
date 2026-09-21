//! Recurrent semantic rewriting for Semantic Boolean Geometry.
//!
//! This layer reuses SML's existing `HaltingPolicy` depth contract while
//! preserving SBG-specific evidence: exact V/M/A/R Hamming deltas for every
//! rewrite cycle. It is a semantic-state runner, not a replacement for the
//! generic Boolean-workspace recurrent runner.

use crate::{HaltingPolicy, RecurrentError};

use super::SemanticState;

/// Exact Boolean change between two SBG states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SemanticStateDelta {
    pub value: u64,
    pub meaning_mask: u64,
    pub ambiguity: u64,
    pub routing: u64,
}

impl SemanticStateDelta {
    /// Computes exact per-field Hamming deltas.
    #[must_use]
    pub fn between<const BITS: usize>(
        previous: &SemanticState<BITS>,
        next: &SemanticState<BITS>,
    ) -> Self {
        Self {
            value: previous.value.xor_popcount(&next.value),
            meaning_mask: previous
                .meaning_mask
                .xor_popcount(&next.meaning_mask),
            ambiguity: previous.ambiguity.xor_popcount(&next.ambiguity),
            routing: previous.routing.xor_popcount(&next.routing),
        }
    }

    /// Total changed Boolean coordinates across V, M, A and R.
    #[must_use]
    pub const fn total(self) -> u128 {
        self.value as u128
            + self.meaning_mask as u128
            + self.ambiguity as u128
            + self.routing as u128
    }

    #[must_use]
    pub const fn is_zero(self) -> bool {
        self.value == 0
            && self.meaning_mask == 0
            && self.ambiguity == 0
            && self.routing == 0
    }
}

/// Result produced by one semantic rewrite cycle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticRewriteStep<const BITS: usize> {
    pub state: SemanticState<BITS>,
    /// When true, a stable delta must not halt this cycle.
    ///
    /// This allows an expert/memory integration to declare that unresolved
    /// internal evidence still requires another bounded recurrent cycle.
    pub stability_blocked: bool,
}

/// Native SBG recurrent rewrite contract.
pub trait SemanticRewriter<const BITS: usize> {
    type Error;

    fn rewrite(
        &mut self,
        state: &SemanticState<BITS>,
        depth: u32,
    ) -> Result<SemanticRewriteStep<BITS>, Self::Error>;
}

/// One auditable recurrent transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SemanticRewriteTrace {
    pub depth: u32,
    pub delta: SemanticStateDelta,
    pub stability_blocked: bool,
}

/// Why recurrent semantic rewriting stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticHaltReason {
    Stable,
    MaxDepth,
}

/// Completed SBG recurrent rewrite.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticRecurrentOutcome<const BITS: usize> {
    pub state: SemanticState<BITS>,
    pub depth: u32,
    pub halt_reason: SemanticHaltReason,
    pub trace: Vec<SemanticRewriteTrace>,
}

/// Runs bounded recurrent SBG rewriting with exact Boolean stability accounting.
///
/// Stability is evaluated on the sum of V/M/A/R Hamming changes and uses the
/// existing SML `HaltingPolicy::stable_hamming_threshold`. A kernel may block
/// stability-based halting for a cycle, but it can never exceed `max_depth`.
pub fn run_semantic_recurrent<const BITS: usize, K: SemanticRewriter<BITS>>(
    initial: SemanticState<BITS>,
    kernel: &mut K,
    policy: HaltingPolicy,
) -> Result<SemanticRecurrentOutcome<BITS>, SemanticRecurrentRunError<K::Error>> {
    let policy = policy
        .validate()
        .map_err(SemanticRecurrentRunError::Policy)?;
    let mut current = initial;
    let mut trace = Vec::with_capacity(policy.max_depth as usize);

    for depth in 1..=policy.max_depth {
        let step = kernel
            .rewrite(&current, depth)
            .map_err(SemanticRecurrentRunError::Kernel)?;
        let delta = SemanticStateDelta::between(&current, &step.state);
        let stable = depth >= policy.min_depth
            && delta.total() <= policy.stable_hamming_threshold as u128
            && !step.stability_blocked;

        trace.push(SemanticRewriteTrace {
            depth,
            delta,
            stability_blocked: step.stability_blocked,
        });
        current = step.state;

        if stable {
            return Ok(SemanticRecurrentOutcome {
                state: current,
                depth,
                halt_reason: SemanticHaltReason::Stable,
                trace,
            });
        }
    }

    Ok(SemanticRecurrentOutcome {
        state: current,
        depth: policy.max_depth,
        halt_reason: SemanticHaltReason::MaxDepth,
        trace,
    })
}

/// Failure from the SBG recurrent runner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SemanticRecurrentRunError<E> {
    Policy(RecurrentError),
    Kernel(E),
}

#[cfg(test)]
mod tests {
    use super::{
        run_semantic_recurrent, SemanticHaltReason, SemanticRecurrentRunError, SemanticRewriteStep,
        SemanticRewriter, SemanticStateDelta,
    };
    use crate::{
        semantic::{SemanticBitField, SemanticState},
        HaltingPolicy, RecurrentError,
    };

    fn state<const BITS: usize>(
        value: &[usize],
        meaning_mask: &[usize],
        ambiguity: &[usize],
        routing: &[usize],
    ) -> SemanticState<BITS> {
        SemanticState {
            value: SemanticBitField::from_set_indices(value).expect("value"),
            meaning_mask: SemanticBitField::from_set_indices(meaning_mask).expect("mask"),
            ambiguity: SemanticBitField::from_set_indices(ambiguity).expect("ambiguity"),
            routing: SemanticBitField::from_set_indices(routing).expect("routing"),
        }
    }

    #[test]
    fn delta_accounts_for_v_m_a_r_exactly() {
        let previous = state::<16>(&[0, 1], &[2], &[3, 4], &[5]);
        let next = state::<16>(&[1, 6], &[2, 7], &[4], &[5, 8, 9]);

        let delta = SemanticStateDelta::between(&previous, &next);

        assert_eq!(delta.value, 2);
        assert_eq!(delta.meaning_mask, 1);
        assert_eq!(delta.ambiguity, 1);
        assert_eq!(delta.routing, 2);
        assert_eq!(delta.total(), 6);
        assert!(!delta.is_zero());
    }

    struct ChangeThenStable {
        calls: u32,
    }

    impl SemanticRewriter<16> for ChangeThenStable {
        type Error = ();

        fn rewrite(
            &mut self,
            state: &SemanticState<16>,
            _depth: u32,
        ) -> Result<SemanticRewriteStep<16>, Self::Error> {
            self.calls += 1;
            let mut next = state.clone();
            if self.calls == 1 {
                next.value.set(3, true).expect("bit");
                next.routing.set(9, true).expect("bit");
            }
            Ok(SemanticRewriteStep {
                state: next,
                stability_blocked: false,
            })
        }
    }

    #[test]
    fn adaptive_depth_halts_after_boolean_stability() {
        let initial = SemanticState::<16>::zeroed().expect("state");
        let mut kernel = ChangeThenStable { calls: 0 };

        let outcome = run_semantic_recurrent(
            initial,
            &mut kernel,
            HaltingPolicy {
                min_depth: 1,
                max_depth: 8,
                stable_hamming_threshold: 0,
            },
        )
        .expect("run");

        assert_eq!(outcome.depth, 2);
        assert_eq!(outcome.halt_reason, SemanticHaltReason::Stable);
        assert_eq!(outcome.trace.len(), 2);
        assert_eq!(outcome.trace[0].delta.total(), 2);
        assert!(outcome.trace[1].delta.is_zero());
    }

    struct BlockFirstStable {
        calls: u32,
    }

    impl SemanticRewriter<8> for BlockFirstStable {
        type Error = ();

        fn rewrite(
            &mut self,
            state: &SemanticState<8>,
            _depth: u32,
        ) -> Result<SemanticRewriteStep<8>, Self::Error> {
            self.calls += 1;
            Ok(SemanticRewriteStep {
                state: state.clone(),
                stability_blocked: self.calls == 1,
            })
        }
    }

    #[test]
    fn kernel_can_block_one_stability_halt_but_not_depth_bound() {
        let initial = SemanticState::<8>::zeroed().expect("state");
        let mut kernel = BlockFirstStable { calls: 0 };

        let outcome = run_semantic_recurrent(
            initial,
            &mut kernel,
            HaltingPolicy {
                min_depth: 1,
                max_depth: 4,
                stable_hamming_threshold: 0,
            },
        )
        .expect("run");

        assert_eq!(outcome.depth, 2);
        assert_eq!(outcome.halt_reason, SemanticHaltReason::Stable);
        assert!(outcome.trace[0].stability_blocked);
        assert!(!outcome.trace[1].stability_blocked);
    }

    struct ToggleForever;

    impl SemanticRewriter<8> for ToggleForever {
        type Error = ();

        fn rewrite(
            &mut self,
            state: &SemanticState<8>,
            _depth: u32,
        ) -> Result<SemanticRewriteStep<8>, Self::Error> {
            let mut next = state.clone();
            let bit = next.value.get(0).expect("bit");
            next.value.set(0, !bit).expect("bit");
            Ok(SemanticRewriteStep {
                state: next,
                stability_blocked: false,
            })
        }
    }

    #[test]
    fn changing_state_stops_at_hard_max_depth() {
        let initial = SemanticState::<8>::zeroed().expect("state");
        let mut kernel = ToggleForever;

        let outcome = run_semantic_recurrent(
            initial,
            &mut kernel,
            HaltingPolicy {
                min_depth: 1,
                max_depth: 3,
                stable_hamming_threshold: 0,
            },
        )
        .expect("run");

        assert_eq!(outcome.depth, 3);
        assert_eq!(outcome.halt_reason, SemanticHaltReason::MaxDepth);
        assert_eq!(outcome.trace.len(), 3);
        assert!(outcome.trace.iter().all(|entry| entry.delta.total() == 1));
    }

    #[test]
    fn invalid_depth_policy_fails_before_kernel_execution() {
        let initial = SemanticState::<8>::zeroed().expect("state");
        let mut kernel = ToggleForever;
        let result = run_semantic_recurrent(
            initial,
            &mut kernel,
            HaltingPolicy {
                min_depth: 3,
                max_depth: 2,
                stable_hamming_threshold: 0,
            },
        );

        assert_eq!(
            result,
            Err(SemanticRecurrentRunError::Policy(
                RecurrentError::InvalidDepthRange { min: 3, max: 2 }
            ))
        );
    }
}
