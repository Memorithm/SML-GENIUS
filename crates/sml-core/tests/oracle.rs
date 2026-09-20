use sml_core::{
    active_fraction_within_ppm, run_recurrent, BooleanCircuit, BooleanWorkspace, DirectRouteTable,
    ExecutionCounters, Gate, HaltingPolicy, PageId, ParameterBudget, ReasoningKernel,
    ReasoningStep, RunError, Wire, INITIAL_ACTIVE_PPM, SML_25B_INITIAL_BUDGET,
};

#[test]
fn sml_25b_initial_budget_is_exact() {
    let budget = SML_25B_INITIAL_BUDGET;
    assert_eq!(budget.total_parameters(), 25_000_000_000);
    assert_eq!(budget.boolean_fraction_ppm(), 850_000);
    assert_eq!(budget.storage_bits(), 31_250_000_000);
    assert_eq!(budget.storage_bytes_ceil(), 3_906_250_000);
}

#[test]
fn empty_budget_is_well_defined() {
    let budget = ParameterBudget::default();
    assert_eq!(budget.total_parameters(), 0);
    assert_eq!(budget.storage_bits(), 0);
    assert_eq!(budget.boolean_fraction_ppm(), 0);
}

#[test]
fn boolean_oracle_composes_without_pairwise_scoring() {
    let circuit = BooleanCircuit::new(
        3,
        vec![
            Gate::Xor(Wire::Input(0), Wire::Input(1)),
            Gate::Not(Wire::Input(2)),
            Gate::And(Wire::Gate(0), Wire::Gate(1)),
        ],
        vec![Wire::Gate(2)],
    );

    let mut counters = ExecutionCounters::default();
    assert_eq!(
        circuit.evaluate(&[true, false, false], &mut counters),
        Ok(vec![true])
    );
    assert_eq!(counters.boolean_ops, 3);
}

#[test]
fn direct_router_uses_workspace_address() {
    let router =
        DirectRouteTable::new(2, vec![PageId(10), PageId(11), PageId(12), PageId(13)]).unwrap();
    let workspace = BooleanWorkspace::from_bits(&[true, true, false, true]);
    let mut counters = ExecutionCounters::default();

    assert_eq!(router.route(&workspace, &mut counters), Ok(PageId(13)));
    assert_eq!(counters.route_lookups, 1);
}

struct ConvergingKernel;

impl ReasoningKernel for ConvergingKernel {
    type Error = ();

    fn step(
        &mut self,
        workspace: &BooleanWorkspace,
        depth: u32,
    ) -> Result<ReasoningStep, Self::Error> {
        let bits = if depth == 1 {
            vec![true, workspace.get(1).unwrap()]
        } else {
            vec![true, true]
        };
        Ok(ReasoningStep {
            workspace: BooleanWorkspace::from_bits(&bits),
            contradiction: false,
            active_parameters: 100,
            active_storage_bits: 100,
        })
    }
}

#[test]
fn recurrent_depth_halts_only_after_stability() {
    let mut kernel = ConvergingKernel;
    let outcome = run_recurrent(
        BooleanWorkspace::from_bits(&[false, false]),
        &mut kernel,
        HaltingPolicy {
            min_depth: 2,
            max_depth: 8,
            stable_hamming_threshold: 0,
        },
    )
    .unwrap();

    assert_eq!(outcome.depth, 3);
    assert_eq!(outcome.counters.recurrent_cycles, 3);
    assert_eq!(outcome.counters.active_parameters, 300);
    assert_eq!(outcome.counters.active_storage_bits, 300);
}

#[test]
fn invalid_depth_policy_fails_closed() {
    let mut kernel = ConvergingKernel;
    let result = run_recurrent(
        BooleanWorkspace::from_bits(&[false, false]),
        &mut kernel,
        HaltingPolicy {
            min_depth: 4,
            max_depth: 2,
            stable_hamming_threshold: 0,
        },
    );
    assert!(matches!(result, Err(RunError::Runner(_))));
}

#[test]
fn one_percent_activity_gate_is_exact() {
    assert!(active_fraction_within_ppm(
        250_000_000,
        25_000_000_000,
        INITIAL_ACTIVE_PPM
    ));
    assert!(!active_fraction_within_ppm(
        250_000_001,
        25_000_000_000,
        INITIAL_ACTIVE_PPM
    ));
}
