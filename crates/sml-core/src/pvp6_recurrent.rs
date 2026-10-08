//! A4 bounded recurrent ANF-bank substitution: exact state/output trajectories.
//! This is a deterministic execution reference, not a trained recurrent model.
use crate::pascal_vector_projection::Pvp6Workload;
use crate::pvp6_executor::{Pvp6Bank, Pvp6ExecutionError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecurrentStep<const BITS: usize> {
    pub workload: Pvp6Workload,
    pub queries: Vec<usize>,
    /// None retains the previous bank; Some replaces it before this step.
    pub replacement: Option<Pvp6Bank<BITS>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecurrentTrace {
    pub outputs: Vec<Vec<u64>>,
    /// State is the exact previous query-major output, not a learned state.
    pub final_state: Vec<u64>,
}

/// Reject the entire trajectory if any step is invalid. The initial bank is immutable.
pub fn execute<const BITS: usize>(
    initial: &Pvp6Bank<BITS>,
    steps: &[RecurrentStep<BITS>],
    max_steps: usize,
) -> Result<RecurrentTrace, Pvp6RecurrentError> {
    if steps.is_empty() || steps.len() > max_steps {
        return Err(Pvp6RecurrentError::InvalidDepth);
    }
    let mut bank = initial.clone();
    let mut outputs = Vec::new();
    outputs
        .try_reserve_exact(steps.len())
        .map_err(|_| Pvp6RecurrentError::Allocation)?;
    let mut state = Vec::new();
    for step in steps {
        if let Some(replacement) = &step.replacement {
            bank = replacement.clone();
        }
        let batch = bank
            .evaluate(step.workload, &step.queries)
            .map_err(Pvp6RecurrentError::Execution)?;
        state = batch
            .to_query_major_words()
            .map_err(Pvp6RecurrentError::Execution)?;
        outputs.push(state.clone());
    }
    Ok(RecurrentTrace {
        outputs,
        final_state: state,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pvp6RecurrentError {
    InvalidDepth,
    Allocation,
    Execution(Pvp6ExecutionError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pascal_vector_projection::Pvp6PopulationProfile;
    use crate::pvp6_executor::Pvp6Gate;

    fn bank(terms: &[u16]) -> Pvp6Bank<64> {
        Pvp6Bank::new(vec![Pvp6Gate::from_monomials(terms).unwrap()]).unwrap()
    }
    fn oracle(terms: &[u16], query: usize) -> u64 {
        u64::from(terms.iter().fold(false, |v, &mask| {
            v ^ ((usize::from(mask) & query) == usize::from(mask))
        }))
    }
    #[test]
    fn full_trajectory_matches_independent_oracle_after_bank_changes() {
        let a = [0, 1, 3, 7];
        let b = [1, 2, 3, 7];
        for depth in [1, 2, 4, 8] {
            let steps: Vec<_> = (0..depth)
                .map(|t| RecurrentStep {
                    workload: Pvp6Workload::new(
                        4,
                        32,
                        if t % 2 == 0 { 1 } else { 4 },
                        Pvp6PopulationProfile::AffineUniform,
                    ),
                    queries: (0..32).map(|q| (q * 7 + t) % 64).collect(),
                    replacement: if t % 2 == 1 {
                        Some(bank(&b))
                    } else if t > 0 {
                        Some(bank(&a))
                    } else {
                        None
                    },
                })
                .collect();
            let before = bank(&a);
            let trace = execute(&before, &steps, 8).unwrap();
            assert_eq!(trace.outputs.len(), depth);
            for (t, output) in trace.outputs.iter().enumerate() {
                let terms = if t % 2 == 0 { &a } else { &b };
                let repeat = if t % 2 == 0 { 1 } else { 4 };
                let expected: Vec<_> = (0..repeat)
                    .flat_map(|_| (0..32).map(|q| oracle(terms, (q * 7 + t) % 64)))
                    .collect();
                assert_eq!(output, &expected);
            }
            assert_eq!(trace.final_state, *trace.outputs.last().unwrap());
            assert_eq!(before, bank(&a));
        }
    }
    #[test]
    fn invalid_depth_and_update_fail_closed() {
        let a = bank(&[0, 1, 3, 7]);
        assert_eq!(execute(&a, &[], 8), Err(Pvp6RecurrentError::InvalidDepth));
        let step = RecurrentStep {
            workload: Pvp6Workload::new(4, 1, 1, Pvp6PopulationProfile::AffineUniform),
            queries: vec![64],
            replacement: None,
        };
        assert_eq!(
            execute(&a, std::slice::from_ref(&step), 0),
            Err(Pvp6RecurrentError::InvalidDepth)
        );
        assert!(matches!(
            execute(&a, &[step], 1),
            Err(Pvp6RecurrentError::Execution(_))
        ));
        let wrong = RecurrentStep {
            workload: Pvp6Workload::new(4, 1, 1, Pvp6PopulationProfile::AffineUniform),
            queries: vec![1],
            replacement: Some(bank(&[0])),
        };
        assert!(matches!(
            execute(&a, &[wrong], 1),
            Err(Pvp6RecurrentError::Execution(
                Pvp6ExecutionError::DensityMismatch { .. }
            ))
        ));
    }
}
