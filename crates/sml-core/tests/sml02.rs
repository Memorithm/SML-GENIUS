use sml_core::{
    tasks::{
        copy_after_marker, development_suite, distractor_rejection, keyed_fact_recall,
        overwrite_bit_recall, two_fact_conjunction, TaskAnswer,
    },
    trace::{run_recurrent_traced, TraceValidationError},
    BooleanWorkspace, HaltingPolicy, PageId, ReasoningKernel, ReasoningStep,
};

struct TraceKernel;

impl ReasoningKernel for TraceKernel {
    type Error = ();

    fn step(
        &mut self,
        workspace: &BooleanWorkspace,
        depth: u32,
    ) -> Result<ReasoningStep, Self::Error> {
        let bits = match depth {
            1 => vec![true, workspace.get(1).unwrap()],
            _ => vec![true, true],
        };
        Ok(ReasoningStep {
            workspace: BooleanWorkspace::from_bits(&bits),
            contradiction: false,
            active_parameters: 10 * depth as u64,
            active_storage_bits: 20 * depth as u128,
            selected_pages: vec![PageId(depth - 1)],
            pairwise_comparisons: 0,
            attention_score_ops: 0,
        })
    }
}

#[test]
fn recurrent_trace_binds_every_internal_cycle() {
    let mut kernel = TraceKernel;
    let traced = run_recurrent_traced(
        BooleanWorkspace::from_bits(&[false, false]),
        &mut kernel,
        HaltingPolicy {
            min_depth: 2,
            max_depth: 8,
            stable_hamming_threshold: 0,
        },
        17,
    )
    .unwrap();

    assert_eq!(traced.outcome.depth, 3);
    assert_eq!(traced.trace.token_index(), 17);
    assert_eq!(traced.trace.len(), 3);
    assert_eq!(traced.trace.records()[0].depth, 1);
    assert_eq!(traced.trace.records()[2].selected_pages, vec![PageId(2)]);
    assert_eq!(traced.trace.records()[0].workspace_hamming, 1);
    assert_eq!(traced.trace.records()[2].workspace_hamming, 0);
    assert_eq!(traced.outcome.counters.active_parameters, 60);
    assert_eq!(traced.outcome.counters.active_storage_bits, 120);
    assert_eq!(traced.trace.validate_non_attention(), Ok(()));
}

struct InvalidCandidateKernel;

impl ReasoningKernel for InvalidCandidateKernel {
    type Error = ();

    fn step(
        &mut self,
        workspace: &BooleanWorkspace,
        _depth: u32,
    ) -> Result<ReasoningStep, Self::Error> {
        Ok(ReasoningStep {
            workspace: workspace.clone(),
            contradiction: false,
            active_parameters: 1,
            active_storage_bits: 1,
            selected_pages: vec![PageId(0)],
            pairwise_comparisons: 1,
            attention_score_ops: 0,
        })
    }
}

#[test]
fn trace_rejects_hidden_pairwise_work() {
    let mut kernel = InvalidCandidateKernel;
    let traced = run_recurrent_traced(
        BooleanWorkspace::from_bits(&[false]),
        &mut kernel,
        HaltingPolicy {
            min_depth: 1,
            max_depth: 1,
            stable_hamming_threshold: 0,
        },
        3,
    )
    .unwrap();

    assert_eq!(
        traced.trace.validate_non_attention(),
        Err(TraceValidationError::PairwiseComparisons { depth: 1, count: 1 })
    );
    assert_eq!(traced.outcome.counters.pairwise_comparisons, 1);
}

#[test]
fn development_task_suite_has_complete_answers() {
    let suite = development_suite(64).unwrap();
    assert_eq!(suite.len(), 384);
    for instance in &suite {
        assert!(instance.oracle_matches().unwrap());
    }
}

#[test]
fn task_generation_is_reproducible() {
    assert_eq!(
        keyed_fact_recall(42, 11).unwrap(),
        keyed_fact_recall(42, 11).unwrap()
    );
    assert_eq!(
        copy_after_marker(42, 7).unwrap(),
        copy_after_marker(42, 7).unwrap()
    );
    assert_eq!(overwrite_bit_recall(42, 7), overwrite_bit_recall(42, 7));
}

#[test]
fn conjunction_and_distractor_oracles_are_independent() {
    let conjunction = two_fact_conjunction(9, 12).unwrap();
    assert_eq!(conjunction.solve().unwrap(), conjunction.expected);

    let rejection = distractor_rejection(9, 12).unwrap();
    assert_eq!(rejection.solve().unwrap(), rejection.expected);
}

#[test]
fn copy_oracle_returns_full_payload_not_a_scalar_proxy() {
    let instance = copy_after_marker(7, 9).unwrap();
    match instance.solve().unwrap() {
        TaskAnswer::Bits(bits) => assert_eq!(bits.len(), 9),
        TaskAnswer::Bit(_) => panic!("copy task returned scalar answer"),
    }
}
