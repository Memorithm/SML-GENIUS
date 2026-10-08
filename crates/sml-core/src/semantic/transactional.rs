//! Transactional composition of two native semantic rewriters.
//! Clone must isolate mutable state: external side effects are not rolled back.
use crate::semantic::{SemanticRewriteStep, SemanticRewriter, SemanticState};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticChain<A, B> {
    first: A,
    second: B,
}
impl<A, B> SemanticChain<A, B> {
    pub fn new(first: A, second: B) -> Self {
        Self { first, second }
    }
    #[must_use]
    pub fn first(&self) -> &A {
        &self.first
    }
    #[must_use]
    pub fn second(&self) -> &B {
        &self.second
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SemanticChainError<A, B> {
    First(A),
    Second(B),
}
impl<const S: usize, A: SemanticRewriter<S> + Clone, B: SemanticRewriter<S> + Clone>
    SemanticRewriter<S> for SemanticChain<A, B>
{
    type Error = SemanticChainError<A::Error, B::Error>;
    fn rewrite(
        &mut self,
        state: &SemanticState<S>,
        depth: u32,
    ) -> Result<SemanticRewriteStep<S>, Self::Error> {
        let mut first = self.first.clone();
        let mut second = self.second.clone();
        let first_step = first
            .rewrite(state, depth)
            .map_err(SemanticChainError::First)?;
        let second_step = second
            .rewrite(&first_step.state, depth)
            .map_err(SemanticChainError::Second)?;
        let blocked = first_step.stability_blocked || second_step.stability_blocked;
        self.first = first;
        self.second = second;
        Ok(SemanticRewriteStep {
            state: second_step.state,
            stability_blocked: blocked,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct Stage {
        calls: u32,
        write: bool,
        fail: bool,
        blocked: bool,
    }
    impl SemanticRewriter<8> for Stage {
        type Error = &'static str;
        fn rewrite(
            &mut self,
            state: &SemanticState<8>,
            _: u32,
        ) -> Result<SemanticRewriteStep<8>, Self::Error> {
            self.calls += 1;
            if self.fail {
                return Err("fixture-failure");
            }
            let mut next = state.clone();
            if self.write {
                next.value.set(0, true).unwrap();
            } else {
                next.routing.set(0, state.value.get(0).unwrap()).unwrap();
            }
            Ok(SemanticRewriteStep {
                state: next,
                stability_blocked: self.blocked,
            })
        }
    }
    fn stage(write: bool, fail: bool, blocked: bool) -> Stage {
        Stage {
            calls: 0,
            write,
            fail,
            blocked,
        }
    }
    #[test]
    fn second_reads_first_state_and_either_stage_can_block_stability() {
        for (a, b) in [(false, false), (true, false), (false, true), (true, true)] {
            let mut chain = SemanticChain::new(stage(true, false, a), stage(false, false, b));
            let input = SemanticState::zeroed().unwrap();
            let out = chain.rewrite(&input, 1).unwrap();
            assert_eq!(out.state.value.get(0), Ok(true));
            assert_eq!(out.state.routing.get(0), Ok(true));
            assert_eq!(out.stability_blocked, a || b);
            assert_eq!(chain.first().calls, 1);
            assert_eq!(chain.second().calls, 1);
            assert_eq!(input.value.count_ones(), 0);
        }
    }
    #[test]
    fn either_failure_rolls_back_both_owned_stages() {
        for (a, b) in [(true, false), (false, true)] {
            let mut chain = SemanticChain::new(stage(true, a, false), stage(false, b, false));
            let before = chain.clone();
            let result = chain.rewrite(&SemanticState::zeroed().unwrap(), 1);
            assert!(if a {
                matches!(result, Err(SemanticChainError::First(_)))
            } else {
                matches!(result, Err(SemanticChainError::Second(_)))
            });
            assert_eq!(chain, before);
        }
    }
    #[test]
    fn order_is_explicit_and_is_not_assumed_commutative() {
        let mut chain = SemanticChain::new(stage(false, false, false), stage(true, false, false));
        let out = chain.rewrite(&SemanticState::zeroed().unwrap(), 1).unwrap();
        assert_eq!(out.state.value.get(0), Ok(true));
        assert_eq!(out.state.routing.get(0), Ok(false));
    }
}
