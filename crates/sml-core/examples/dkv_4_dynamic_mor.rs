//! DKV-4e dynamic MOR recurrent-evidence development panel.
//!
//! Protocol: docs/SML-DKV-4E-DYNAMIC-MOR-PREREGISTRATION.md.

use std::{error::Error, fmt::Write as _};

use sml_core::{
    delta_kv::{DeltaKvMemory, DELTA_KV_ONE},
    delta_kv_adaptive::{
        run_dkv4_actionable_recurrent, run_dkv4_semantic_recurrent, Dkv4ActionableController,
        Dkv4Decision, Dkv4Provenance, Dkv4RefinementCapabilities, Dkv4SemanticController,
        Dkv4SemanticPolicy, Dkv4Signals,
    },
    delta_kv_recurrent::{
        compare_delta_kv_recurrent, DeltaKvRecurrentKernel, DeltaKvRecurrentPolicy,
    },
    delta_kv_semantic::{DeltaKvSemanticKeyEvidence, DeltaKvSemanticProjectionCounters},
    mor::{
        ExpertProposal, MixtureOfRouters, MorDecision, MorPolicy, PreviousRoute, RouterId,
        RouterInput, MOR_SCALE_PPM, MOR_SCORE_CEILING_PPM,
    },
    mor_registry::{MorRegistryPolicy, MorRouterRegistry, RouterSpecialization},
    semantic::{SemanticRewriteTrace, SemanticRewriter, SemanticState, SemanticStateDelta},
    HaltingPolicy, PageId,
};

type R<T> = Result<T, Box<dyn Error>>;
const BITS: usize = 8;

#[derive(Debug, Clone, Copy)]
enum MorFixture {
    Consensus,
    ResolvingMargin,
    PersistentMargin,
    StructuralDisagreement,
}

impl MorFixture {
    const ALL: [Self; 4] = [
        Self::Consensus,
        Self::ResolvingMargin,
        Self::PersistentMargin,
        Self::StructuralDisagreement,
    ];

    const fn name(self) -> &'static str {
        match self {
            Self::Consensus => "CONSENSUS",
            Self::ResolvingMargin => "RESOLVING_MARGIN",
            Self::PersistentMargin => "PERSISTENT_MARGIN",
            Self::StructuralDisagreement => "STRUCTURAL_DISAGREEMENT",
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum StateFixture {
    Stable,
    CorrectOnce,
}

impl StateFixture {
    const ALL: [Self; 2] = [Self::Stable, Self::CorrectOnce];

    const fn name(self) -> &'static str {
        match self {
            Self::Stable => "STABLE",
            Self::CorrectOnce => "CORRECT_ONCE",
        }
    }
}

fn router(router_id: u32, page_id: u32, score_ppm: u32) -> RouterInput {
    RouterInput {
        router_id: RouterId(router_id),
        affinity_ppm: MOR_SCALE_PPM,
        reliability_ppm: MOR_SCALE_PPM,
        confidence_ppm: MOR_SCALE_PPM,
        balance_bias_ppm: 0,
        proposals: vec![ExpertProposal {
            page_id: PageId(page_id),
            score_ppm,
        }],
    }
}

fn mor_decision(fixture: MorFixture, previous: &PreviousRoute) -> R<MorDecision> {
    let (specializations, inputs) = match fixture {
        MorFixture::Consensus => (
            (0..4)
                .map(|id| RouterSpecialization {
                    router_id: RouterId(id),
                    experts: vec![PageId(0), PageId(1)],
                })
                .collect::<Vec<_>>(),
            (0..4)
                .map(|id| router(id, 0, MOR_SCALE_PPM))
                .collect::<Vec<_>>(),
        ),
        MorFixture::ResolvingMargin => (
            vec![
                RouterSpecialization {
                    router_id: RouterId(0),
                    experts: vec![PageId(0)],
                },
                RouterSpecialization {
                    router_id: RouterId(1),
                    experts: vec![PageId(1)],
                },
            ],
            vec![
                router(0, 0, MOR_SCALE_PPM),
                router(1, 1, 920_000),
            ],
        ),
        MorFixture::PersistentMargin => (
            vec![
                RouterSpecialization {
                    router_id: RouterId(0),
                    experts: vec![PageId(0)],
                },
                RouterSpecialization {
                    router_id: RouterId(1),
                    experts: vec![PageId(1)],
                },
            ],
            vec![
                router(0, 0, MOR_SCALE_PPM),
                router(1, 1, MOR_SCALE_PPM),
            ],
        ),
        MorFixture::StructuralDisagreement => (
            vec![
                RouterSpecialization {
                    router_id: RouterId(0),
                    experts: vec![PageId(0)],
                },
                RouterSpecialization {
                    router_id: RouterId(1),
                    experts: vec![PageId(0)],
                },
                RouterSpecialization {
                    router_id: RouterId(2),
                    experts: vec![PageId(1)],
                },
                RouterSpecialization {
                    router_id: RouterId(3),
                    experts: vec![PageId(1)],
                },
            ],
            vec![
                router(0, 0, MOR_SCALE_PPM),
                router(1, 0, MOR_SCALE_PPM),
                router(2, 1, MOR_SCALE_PPM),
                router(3, 1, MOR_SCALE_PPM),
            ],
        ),
    };

    let edges = specializations.iter().map(|s| s.experts.len()).sum();
    let max_fanout = specializations
        .iter()
        .map(|s| s.experts.len())
        .max()
        .unwrap_or(1);
    let registry = MorRouterRegistry::new(
        specializations,
        MorRegistryPolicy {
            expert_count: 2,
            max_routers: inputs.len(),
            max_edges: edges,
            max_fanout,
            require_full_coverage: true,
        },
    )?;
    let engine = MixtureOfRouters::new(MorPolicy::default())?;
    Ok(registry.route(&engine, &inputs, &[], previous)?)
}

fn route_margin(decision: &MorDecision) -> u32 {
    match decision.ranked_experts.as_slice() {
        [first, second, ..] => first.final_score_ppm.saturating_sub(second.final_score_ppm),
        [_] => MOR_SCORE_CEILING_PPM,
        [] => 0,
    }
}

fn next_previous(decision: &MorDecision) -> PreviousRoute {
    PreviousRoute {
        primary: decision.ranked_experts.first().map(|expert| expert.page_id),
        selected: decision.selected_experts.clone(),
    }
}

fn format_primary(previous: &PreviousRoute) -> String {
    previous
        .primary
        .map(|page| page.0.to_string())
        .unwrap_or_else(|| "none".to_string())
}

fn format_selected(selected: &[PageId]) -> String {
    if selected.is_empty() {
        return "none".to_string();
    }
    selected
        .iter()
        .map(|page| page.0.to_string())
        .collect::<Vec<_>>()
        .join("|")
}

fn validate_fixture_first_second(fixture: MorFixture) -> R<()> {
    let first = mor_decision(fixture, &PreviousRoute::default())?;
    let previous = next_previous(&first);
    let second = mor_decision(fixture, &previous)?;

    match fixture {
        MorFixture::Consensus => {
            if first.structural_disagreement_ppm != 0
                || first.request_precision_escalation
                || second.request_precision_escalation
            {
                return Err("CONSENSUS dynamic MOR contract not observed".into());
            }
        }
        MorFixture::ResolvingMargin => {
            if first.structural_disagreement_ppm != 0
                || !first.request_precision_escalation
                || route_margin(&first) != 40_000
                || second.request_precision_escalation
                || route_margin(&second) != 70_000
            {
                return Err("RESOLVING_MARGIN dynamic MOR contract not observed".into());
            }
        }
        MorFixture::PersistentMargin => {
            if first.structural_disagreement_ppm != 0
                || !first.request_precision_escalation
                || !second.request_precision_escalation
            {
                return Err("PERSISTENT_MARGIN dynamic MOR contract not observed".into());
            }
        }
        MorFixture::StructuralDisagreement => {
            if first.structural_disagreement_ppm < 250_000
                || second.structural_disagreement_ppm < 250_000
                || !first.request_precision_escalation
                || !second.request_precision_escalation
            {
                return Err("STRUCTURAL_DISAGREEMENT dynamic MOR contract not observed".into());
            }
        }
    }
    Ok(())
}

fn kernel() -> R<DeltaKvRecurrentKernel<BITS>> {
    let memory = DeltaKvMemory::new(1, 1)?;
    let key = DeltaKvSemanticKeyEvidence {
        key_q12: vec![DELTA_KV_ONE],
        selected_experts: vec![PageId(0)],
        provenance: None,
        counters: DeltaKvSemanticProjectionCounters::default(),
        transient_key_bits: 16,
    };
    let mut kernel =
        DeltaKvRecurrentKernel::new(memory, key, vec![0], DeltaKvRecurrentPolicy::default())?;
    kernel.seed_target(&[DELTA_KV_ONE], 4096)?;
    Ok(kernel)
}

fn initial(fixture: StateFixture) -> R<SemanticState<BITS>> {
    let mut state = SemanticState::<BITS>::zeroed()?;
    if matches!(fixture, StateFixture::Stable) {
        state.value.set(0, true)?;
    }
    Ok(state)
}

fn correct(state: &SemanticState<BITS>) -> R<bool> {
    Ok(state.value.get(0)?)
}

#[derive(Debug, Clone)]
struct DynamicCycle {
    depth: u32,
    delta: SemanticStateDelta,
    ambiguity: usize,
    correction_abs_max_q12: u32,
    previous_in: PreviousRoute,
    mor: MorDecision,
    decision: Dkv4Decision,
    previous_out: PreviousRoute,
}

#[derive(Debug, Clone)]
struct DynamicOutcome {
    state: SemanticState<BITS>,
    depth: u32,
    decision: Dkv4Decision,
    cycles: Vec<DynamicCycle>,
}

fn run_dynamic(
    initial: SemanticState<BITS>,
    mut kernel: DeltaKvRecurrentKernel<BITS>,
    fixture: MorFixture,
    provenance: Dkv4Provenance,
) -> R<DynamicOutcome> {
    if matches!(provenance, Dkv4Provenance::Missing) {
        return Ok(DynamicOutcome {
            state: initial,
            depth: 0,
            decision: Dkv4Decision::Abstain(
                sml_core::delta_kv_adaptive::Dkv4Reason::MissingEvidence,
            ),
            cycles: Vec::new(),
        });
    }
    if matches!(provenance, Dkv4Provenance::Conflict) {
        return Ok(DynamicOutcome {
            state: initial,
            depth: 0,
            decision: Dkv4Decision::Abstain(
                sml_core::delta_kv_adaptive::Dkv4Reason::ConflictingEvidence,
            ),
            cycles: Vec::new(),
        });
    }

    let controller = Dkv4ActionableController::new(Dkv4SemanticPolicy::default())?;
    let capabilities = Dkv4RefinementCapabilities::dynamic_mor();
    let mut state = initial;
    let mut previous = PreviousRoute::default();
    let mut cycles = Vec::new();

    for depth in 1..=controller.policy().max_depth {
        let before = kernel.cycle_evidence().len();
        let step = kernel.rewrite(&state, depth)?;
        if kernel.cycle_evidence().len() != before + 1 {
            return Err("dynamic loop DKV evidence drift".into());
        }
        let trace = SemanticRewriteTrace {
            depth,
            delta: SemanticStateDelta::between(&state, &step.state),
            stability_blocked: step.stability_blocked,
        };
        let dkv = kernel
            .cycle_evidence()
            .last()
            .ok_or("dynamic loop missing DKV cycle evidence")?;
        let mor = mor_decision(fixture, &previous)?;
        let signals = Dkv4Signals::from_dkv2b(&trace, dkv, &mor, provenance)?;
        let decision = controller.decide(signals, capabilities)?;
        let previous_out = next_previous(&mor);

        cycles.push(DynamicCycle {
            depth,
            delta: trace.delta,
            ambiguity: dkv.ambiguous_values,
            correction_abs_max_q12: dkv.correction_abs_max_q12,
            previous_in: previous.clone(),
            mor,
            decision,
            previous_out: previous_out.clone(),
        });

        state = step.state;
        if !matches!(decision, Dkv4Decision::Refine(_)) {
            return Ok(DynamicOutcome {
                state,
                depth,
                decision,
                cycles,
            });
        }
        previous = previous_out;
    }

    Err("dynamic loop exhausted without terminal DKV-4 decision".into())
}

pub fn report() -> R<String> {
    let mut out = String::from(
        "PROTOCOL,SML-DKV-4E-DYNAMIC-MOR-PREREGISTRATION\nSTATUS,DEVELOPMENT_NOT_MODEL_QUALIFICATION\n",
    );

    for fixture in MorFixture::ALL {
        validate_fixture_first_second(fixture)?;
        let first_mor = mor_decision(fixture, &PreviousRoute::default())?;
        let second_mor = mor_decision(fixture, &next_previous(&first_mor))?;
        writeln!(
            out,
            "MOR,{},first_margin={},first_disagreement={},first_precision={},second_margin={},second_disagreement={},second_precision={}",
            fixture.name(),
            route_margin(&first_mor),
            first_mor.structural_disagreement_ppm,
            first_mor.request_precision_escalation,
            route_margin(&second_mor),
            second_mor.structural_disagreement_ppm,
            second_mor.request_precision_escalation,
        )?;

        for state_fixture in StateFixture::ALL {
            let initial = initial(state_fixture)?;
            let base = kernel()?;
            let halting = HaltingPolicy {
                min_depth: 1,
                max_depth: 4,
                stable_hamming_threshold: 0,
            };
            let stability =
                compare_delta_kv_recurrent(initial.clone(), base.clone(), halting, 4)?;

            let first = mor_decision(fixture, &PreviousRoute::default())?;

            let mut fixed_kernel = base.clone();
            let fixed = run_dkv4_actionable_recurrent(
                initial.clone(),
                &mut fixed_kernel,
                &first,
                Dkv4Provenance::VerifiedPresent,
                Dkv4ActionableController::new(Dkv4SemanticPolicy::default())?,
                Dkv4RefinementCapabilities::dkv2b_fixed_mor(),
            )?;

            let mut frozen_semantic_kernel = base.clone();
            let frozen_semantic = run_dkv4_semantic_recurrent(
                initial.clone(),
                &mut frozen_semantic_kernel,
                &first,
                Dkv4Provenance::VerifiedPresent,
                Dkv4SemanticController::new(Dkv4SemanticPolicy::default())?,
            )?;

            let dynamic =
                run_dynamic(initial, base, fixture, Dkv4Provenance::VerifiedPresent)?;

            if !correct(&stability.adaptive.state)?
                || !correct(&fixed.state)?
                || !correct(&frozen_semantic.state)?
                || !correct(&dynamic.state)?
            {
                return Err("dynamic MOR panel final Boolean answer drift".into());
            }

            for cycle in &dynamic.cycles {
                writeln!(
                    out,
                    "TRACE,{},{},depth={},delta={},ambiguity={},correction={},prev_primary={},prev_selected={},margin={},disagreement={},precision={},primary={},selected={},decision={:?},next_primary={},next_selected={}",
                    fixture.name(),
                    state_fixture.name(),
                    cycle.depth,
                    cycle.delta.total(),
                    cycle.ambiguity,
                    cycle.correction_abs_max_q12,
                    format_primary(&cycle.previous_in),
                    format_selected(&cycle.previous_in.selected),
                    route_margin(&cycle.mor),
                    cycle.mor.structural_disagreement_ppm,
                    cycle.mor.request_precision_escalation,
                    cycle
                        .mor
                        .ranked_experts
                        .first()
                        .map(|expert| expert.page_id.0.to_string())
                        .unwrap_or_else(|| "none".to_string()),
                    format_selected(&cycle.mor.selected_experts),
                    cycle.decision,
                    format_primary(&cycle.previous_out),
                    format_selected(&cycle.previous_out.selected),
                )?;
            }

            writeln!(
                out,
                "ROW,{},{},stability_depth={},fixed_depth={},fixed_decision={:?},frozen_dkv4c_depth={},frozen_dkv4c_decision={:?},dynamic_depth={},dynamic_decision={:?},final_correct=true",
                fixture.name(),
                state_fixture.name(),
                stability.adaptive.depth,
                fixed.depth,
                fixed.decision,
                frozen_semantic.depth,
                frozen_semantic.decision,
                dynamic.depth,
                dynamic.decision,
            )?;
        }

        let missing = run_dynamic(
            initial(StateFixture::Stable)?,
            kernel()?,
            fixture,
            Dkv4Provenance::Missing,
        )?;
        let unverified = run_dynamic(
            initial(StateFixture::Stable)?,
            kernel()?,
            fixture,
            Dkv4Provenance::Unverified,
        )?;
        if missing.depth != 0
            || matches!(missing.decision, Dkv4Decision::StopCandidate(_))
            || matches!(unverified.decision, Dkv4Decision::StopCandidate(_))
        {
            return Err("dynamic MOR provenance contract drift".into());
        }
        writeln!(
            out,
            "SAFETY,{},missing_depth={},missing_decision={:?},unverified_depth={},unverified_decision={:?}",
            fixture.name(),
            missing.depth,
            missing.decision,
            unverified.depth,
            unverified.decision,
        )?;
    }

    out.push_str("PROMOTION,BLOCKED,dynamic_mor_development_panel\n");
    Ok(out)
}

fn main() -> R<()> {
    print!("{}", report()?);
    Ok(())
}
