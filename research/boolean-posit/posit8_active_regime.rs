//! REGIME-LEARN1: actively identify two Boolean gates for posit8 regime
//! equality and prefix-continuation, then execute the learned gate identities
//! on actual 7-bit posit payloads. Fixed wiring, bounded teacher queries.
#![forbid(unsafe_code)]

use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Gate {
    And,
    Or,
    Xor,
    Nand,
    Nor,
    Xnor,
}

impl Gate {
    const GRAMMAR: [Self; 6] = [
        Self::And,
        Self::Or,
        Self::Xor,
        Self::Nand,
        Self::Nor,
        Self::Xnor,
    ];

    fn apply(self, a: u8, b: u8) -> u8 {
        assert!(a <= 1 && b <= 1);
        match self {
            Self::And => a & b,
            Self::Or => a | b,
            Self::Xor => a ^ b,
            Self::Nand => a & b ^ 1,
            Self::Nor => (a | b) ^ 1,
            Self::Xnor => a ^ b ^ 1,
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum Teacher {
    Equality,
    Continue,
}

impl Teacher {
    /// Teacher queried only at actively selected cases, never in inference.
    fn answer(self, a: u8, b: u8) -> u8 {
        match self {
            Self::Equality => u8::from(a == b),
            Self::Continue => a & b,
        }
    }
}

fn case_bits(case: usize) -> (u8, u8) {
    ((case as u8) & 1, ((case >> 1) as u8) & 1)
}

fn learn_gate(teacher: Teacher) -> (Gate, usize) {
    let mut candidates = Gate::GRAMMAR.to_vec();
    let mut queried = 0u8;
    let mut queries = 0usize;
    while candidates.len() > 1 {
        let mut selected: Option<(usize, usize)> = None;
        for case in 0..4 {
            if (queried >> case) & 1 != 0 {
                continue;
            }
            let (a, b) = case_bits(case);
            let ones = candidates
                .iter()
                .filter(|&&gate| gate.apply(a, b) != 0)
                .count();
            let disagreement = ones.min(candidates.len() - ones);
            if selected.is_none_or(|(best, _)| disagreement > best) {
                selected = Some((disagreement, case));
            }
        }
        let (disagreement, case) = selected.expect("unqueried candidate must remain");
        assert!(disagreement > 0, "distinct gate candidates must disagree");
        let (a, b) = case_bits(case);
        let answer = teacher.answer(a, b);
        candidates.retain(|&gate| gate.apply(a, b) == answer);
        assert!(!candidates.is_empty(), "teacher outside declared gate grammar");
        queried |= 1 << case;
        queries += 1;
        assert!(queries <= 4, "gate learning budget exceeded");
    }
    (candidates[0], queries)
}

pub struct LearnedRegime {
    equality: Gate,
    continuation: Gate,
    membership_queries: usize,
}

impl LearnedRegime {
    pub fn train() -> Self {
        let (equality, equality_queries) = learn_gate(Teacher::Equality);
        let (continuation, continuation_queries) = learn_gate(Teacher::Continue);
        let learned = Self {
            equality,
            continuation,
            membership_queries: equality_queries + continuation_queries,
        };
        // A posteriori audit of nonqueried inputs: all four truth cases
        // evaluated after active learning, never used to select a gate.
        for a in 0..=1 {
            for b in 0..=1 {
                assert_eq!(learned.equality.apply(a, b), u8::from(a == b));
                assert_eq!(learned.continuation.apply(a, b), a & b);
            }
        }
        learned
    }

    pub fn membership_queries(&self) -> usize {
        self.membership_queries
    }

    /// Return (regime-bit, number of equal leading bits) for a positive
    /// posit magnitude 1..127; tests include the all-ones/zero-run corners.
    /// The topology (XNOR then AND prefix) is prescribed; the two gate
    /// identities are learned through active 2-bit membership queries.
    pub fn run_length(&self, magnitude: u8) -> (u8, u8) {
        assert!((1..=127).contains(&magnitude));
        let first = (magnitude >> 6) & 1;
        let mut prefix = 1u8;
        let mut run = 0u8;
        for pos in (0..=6).rev() {
            let bit = (magnitude >> pos) & 1;
            let equal = self.equality.apply(bit, first);
            prefix = self.continuation.apply(prefix, equal);
            run += prefix;
        }
        debug_assert!((1..=7).contains(&run));
        (first, run)
    }

    pub fn audit_all_magnitudes(&self) -> usize {
        let mut errors = 0usize;
        for magnitude in 1..=127u8 {
            let leading = (magnitude >> 6) & 1;
            let mut expected = 0u8;
            for pos in (0..=6).rev() {
                if (magnitude >> pos) & 1 == leading {
                    expected += 1;
                } else {
                    break;
                }
            }
            errors += usize::from(self.run_length(magnitude) != (leading, expected));
        }
        errors
    }
}

pub fn detector() -> &'static LearnedRegime {
    static INSTANCE: OnceLock<LearnedRegime> = OnceLock::new();
    INSTANCE.get_or_init(LearnedRegime::train)
}

fn main() {
    let learner = detector();
    let errors = learner.audit_all_magnitudes();
    println!(
        "POSIT8_ACTIVE_REGIME,magnitudes=127,learner_queries={},errors={},fixed_topology=true",
        learner.membership_queries(),
        errors
    );
    if errors != 0 {
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn learned_gates_agree_on_complete_two_input_domain() {
        let learner = LearnedRegime::train();
        assert!(learner.membership_queries() > 0);
        assert!(learner.membership_queries() <= 8);
        assert_eq!(learner.equality, Gate::Xnor);
        assert_eq!(learner.continuation, Gate::And);
    }

    #[test]
    fn every_positive_magnitude_has_correct_regime_prefix_length() {
        assert_eq!(detector().audit_all_magnitudes(), 0);
        assert_eq!(detector().run_length(1), (0, 6));
        assert_eq!(detector().run_length(0x40), (1, 1));
        assert_eq!(detector().run_length(0x7f), (1, 7));
    }

    #[test]
    fn statically_cached_learner_is_reused() {
        let a = detector() as *const LearnedRegime;
        let b = detector() as *const LearnedRegime;
        assert_eq!(a, b);
    }
}
