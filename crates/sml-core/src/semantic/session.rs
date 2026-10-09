//! Owned in-memory continuation for native semantic kernels, including a whole
//! routed expert/Delta-KV pipeline. Not persistence or authenticated storage.
//! Error rollback requires Clone to isolate mutable state, with no external effects.
use super::{
    run_semantic_recurrent, SemanticHaltReason, SemanticRecurrentOutcome,
    SemanticRecurrentRunError, SemanticRewriteStep, SemanticRewriteTrace, SemanticRewriter,
    SemanticState,
};
use crate::{HaltingPolicy, RecurrentError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticSession<K, const S: usize> {
    inner: K,
    state: SemanticState<S>,
    policy: HaltingPolicy,
    trace: Vec<SemanticRewriteTrace>,
    halt: Option<SemanticHaltReason>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnedSemanticCheckpoint<K, const S: usize> {
    session: SemanticSession<K, S>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SemanticSessionError<E> {
    Policy(RecurrentError),
    ZeroWork,
    Overflow,
    Allocation,
    Identity,
    Run(SemanticRecurrentRunError<E>),
}
struct Offset<'a, K> {
    inner: &'a mut K,
    offset: u32,
}
impl<K: SemanticRewriter<S>, const S: usize> SemanticRewriter<S> for Offset<'_, K> {
    type Error = K::Error;
    fn rewrite(
        &mut self,
        state: &SemanticState<S>,
        depth: u32,
    ) -> Result<SemanticRewriteStep<S>, Self::Error> {
        // The validated segment bound guarantees this addition is <= max_depth.
        self.inner.rewrite(state, self.offset + depth)
    }
}
impl<K: SemanticRewriter<S> + Clone, const S: usize> SemanticSession<K, S> {
    /// Start a fresh depth-one session. Kernels with prior history may reject
    /// depth one; resume prior work only through an owned checkpoint.
    pub fn new(
        state: SemanticState<S>,
        inner: K,
        policy: HaltingPolicy,
    ) -> Result<Self, SemanticSessionError<K::Error>> {
        Ok(Self {
            state,
            inner,
            policy: policy.validate().map_err(SemanticSessionError::Policy)?,
            trace: vec![],
            halt: None,
        })
    }
    #[must_use]
    pub fn inner(&self) -> &K {
        &self.inner
    }
    #[must_use]
    pub fn state(&self) -> &SemanticState<S> {
        &self.state
    }
    #[must_use]
    pub fn trace(&self) -> &[SemanticRewriteTrace] {
        &self.trace
    }
    #[must_use]
    pub fn depth(&self) -> u32 {
        self.trace.len() as u32
    }
    #[must_use]
    pub fn policy(&self) -> HaltingPolicy {
        self.policy
    }
    #[must_use]
    pub fn halt_reason(&self) -> Option<SemanticHaltReason> {
        self.halt
    }
    #[must_use]
    pub fn checkpoint(&self) -> OwnedSemanticCheckpoint<K, S> {
        OwnedSemanticCheckpoint {
            session: self.clone(),
        }
    }
    #[must_use]
    pub fn outcome(&self) -> Option<SemanticRecurrentOutcome<S>> {
        self.halt.map(|halt_reason| SemanticRecurrentOutcome {
            state: self.state.clone(),
            depth: self.depth(),
            halt_reason,
            trace: self.trace.clone(),
        })
    }
    /// Advance at most `requested` cycles, using the existing native runner.
    /// A failed segment commits neither partial state nor mutated kernel counters.
    /// A segment boundary is not a stable stop before the original minimum depth.
    pub fn advance(&mut self, requested: u32) -> Result<(), SemanticSessionError<K::Error>> {
        if requested == 0 {
            return Err(SemanticSessionError::ZeroWork);
        }
        if self.halt.is_some() {
            return Ok(());
        }
        let offset = self.depth();
        let remaining = self
            .policy
            .max_depth
            .checked_sub(offset)
            .ok_or(SemanticSessionError::Overflow)?;
        let count = requested.min(remaining);
        if count == 0 {
            return Err(SemanticSessionError::Overflow);
        }
        let segment = HaltingPolicy {
            min_depth: self
                .policy
                .min_depth
                .saturating_sub(offset)
                .max(1)
                .min(count),
            max_depth: count,
            stable_hamming_threshold: self.policy.stable_hamming_threshold,
        };
        let mut inner = self.inner.clone();
        let mut adapter = Offset {
            inner: &mut inner,
            offset,
        };
        let next = run_semantic_recurrent(self.state.clone(), &mut adapter, segment)
            .map_err(SemanticSessionError::Run)?;
        let depth = offset
            .checked_add(next.depth)
            .ok_or(SemanticSessionError::Overflow)?;
        let mut trace = self.trace.clone();
        trace
            .try_reserve(next.trace.len())
            .map_err(|_| SemanticSessionError::Allocation)?;
        for mut entry in next.trace {
            entry.depth = entry
                .depth
                .checked_add(offset)
                .ok_or(SemanticSessionError::Overflow)?;
            trace.push(entry);
        }
        let last = trace.last().ok_or(SemanticSessionError::Overflow)?;
        let halt = if depth >= self.policy.min_depth
            && last.delta.total() <= u128::from(self.policy.stable_hamming_threshold)
            && !last.stability_blocked
        {
            Some(SemanticHaltReason::Stable)
        } else if depth == self.policy.max_depth {
            Some(SemanticHaltReason::MaxDepth)
        } else {
            None
        };
        self.inner = inner;
        self.state = next.state;
        self.trace = trace;
        self.halt = halt;
        Ok(())
    }
}
impl<K: SemanticRewriter<S> + Clone, const S: usize> OwnedSemanticCheckpoint<K, S> {
    #[must_use]
    pub fn state(&self) -> &SemanticState<S> {
        self.session.state()
    }
    #[must_use]
    pub fn depth(&self) -> u32 {
        self.session.depth()
    }
    #[must_use]
    pub fn resume(self) -> SemanticSession<K, S> {
        self.session
    }
    /// Exact owned-kernel equality, including configuration and runtime state.
    /// Not a cryptographic identity check or proof about external dependencies.
    pub fn resume_checked(
        self,
        expected: &K,
    ) -> Result<SemanticSession<K, S>, SemanticSessionError<K::Error>>
    where
        K: PartialEq,
    {
        if &self.session.inner != expected {
            return Err(SemanticSessionError::Identity);
        }
        Ok(self.session)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        pvp6_executor::{Pvp6Bank, Pvp6Gate},
        pvp6_sbg_coupled::fixtures::coupled,
        PageId,
    };
    #[test]
    fn full_expert_mor_memory_pipeline_resumes_with_identical_state_trace_and_counters() {
        for minimum in [1, 4, 8] {
            for split in 1..=8 {
                let initial = SemanticState::zeroed().unwrap();
                let policy = HaltingPolicy {
                    min_depth: minimum,
                    max_depth: 8,
                    stable_hamming_threshold: 0,
                };
                let mut full = coupled();
                let expected = run_semantic_recurrent(initial.clone(), &mut full, policy).unwrap();
                let mut session = SemanticSession::new(initial, coupled(), policy).unwrap();
                session.advance(split).unwrap();
                let snapshot = session.checkpoint();
                assert_eq!(snapshot.state(), session.state());
                assert_eq!(snapshot.depth(), session.depth());
                let mut resumed = snapshot.resume_checked(session.inner()).unwrap();
                resumed.advance(u32::MAX).unwrap();
                assert_eq!(resumed.outcome().unwrap(), expected);
                assert_eq!(resumed.inner(), &full);
                let before = resumed.clone();
                resumed.advance(1).unwrap();
                assert_eq!(resumed, before);
            }
        }
    }
    #[test]
    fn changed_expert_body_cannot_pass_expected_kernel_check() {
        let policy = HaltingPolicy {
            min_depth: 1,
            max_depth: 8,
            stable_hamming_threshold: 0,
        };
        let session =
            SemanticSession::new(SemanticState::zeroed().unwrap(), coupled(), policy).unwrap();
        let mut wrong = coupled();
        wrong
            .replace_expert(
                PageId(0),
                0,
                Pvp6Bank::new(vec![Pvp6Gate::from_monomials(&[1]).unwrap()]).unwrap(),
            )
            .unwrap();
        assert!(matches!(
            session.checkpoint().resume_checked(&wrong),
            Err(SemanticSessionError::Identity)
        ));
        assert_eq!(session.checkpoint().resume(), session);
    }
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct FailAtTwo {
        calls: u32,
    }
    impl SemanticRewriter<8> for FailAtTwo {
        type Error = ();
        fn rewrite(
            &mut self,
            state: &SemanticState<8>,
            depth: u32,
        ) -> Result<SemanticRewriteStep<8>, ()> {
            self.calls += 1;
            if depth == 2 {
                return Err(());
            }
            let mut state = state.clone();
            state.value.set(0, true).unwrap();
            Ok(SemanticRewriteStep {
                state,
                stability_blocked: false,
            })
        }
    }
    #[test]
    fn failed_segment_rolls_back_earlier_successful_cycles_in_that_segment() {
        let policy = HaltingPolicy {
            min_depth: 1,
            max_depth: 8,
            stable_hamming_threshold: 0,
        };
        let mut session = SemanticSession::new(
            SemanticState::zeroed().unwrap(),
            FailAtTwo { calls: 0 },
            policy,
        )
        .unwrap();
        let before = session.clone();
        assert!(matches!(
            session.advance(2),
            Err(SemanticSessionError::Run(_))
        ));
        assert_eq!(session, before);
        session.advance(1).unwrap();
        assert_eq!(session.depth(), 1);
        assert_eq!(session.inner().calls, 1);
        let before = session.clone();
        assert!(session.advance(1).is_err());
        assert_eq!(session, before);
        assert_eq!(session.advance(0), Err(SemanticSessionError::ZeroWork));
        assert_eq!(session, before);
    }
}
