//! DKV-4d MOR actionability development panel.
//!
//! Protocol: docs/SML-DKV-4D-MOR-ACTIONABILITY-PREREGISTRATION.md.

use std::{error::Error, fmt::Write as _};

use sml_core::{
    delta_kv::{DeltaKvMemory, DELTA_KV_ONE},
    delta_kv_adaptive::{
        run_dkv4_actionable_recurrent, run_dkv4_semantic_recurrent, Dkv4ActionableController,
        Dkv4Decision, Dkv4Provenance, Dkv4RefinementCapabilities, Dkv4SemanticController,
        Dkv4SemanticPolicy,
    },
    delta_kv_recurrent::{
        compare_delta_kv_recurrent, DeltaKvRecurrentKernel, DeltaKvRecurrentPolicy,
    },
    delta_kv_semantic::{DeltaKvSemanticKeyEvidence, DeltaKvSemanticProjectionCounters},
    mor::{
        ExpertProposal, MixtureOfRouters, MorDecision, MorPolicy, PreviousRoute, RouterId,
        RouterInput, MOR_SCALE_PPM,
    },
    mor_registry::{MorRegistryPolicy, MorRouterRegistry, RouterSpecialization},
    semantic::SemanticState,
    HaltingPolicy, PageId,
};

type R<T> = Result<T, Box<dyn Error>>;

const BITS: usize = 8;

#[derive(Debug, Clone, Copy)]
enum MorFixture {
    Consensus,
    MarginEscalation,
    StructuralDisagreement,
}

impl MorFixture {
    const ALL: [Self; 3] = [
        Self::Consensus,
        Self::MarginEscalation,
        Self::StructuralDisagreement,
    ];

    const fn name(self) -> &'static str {
        match self {
            Self::Consensus => "CONSENSUS",
            Self::MarginEscalation => "MARGIN_ESCALATION",
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

fn router(router_id: u32, page_id: u32) -> RouterInput {
    RouterInput {
        router_id: RouterId(router_id),
        affinity_ppm: MOR_SCALE_PPM,
        reliability_ppm: MOR_SCALE_PPM,
        confidence_ppm: MOR_SCALE_PPM,
        balance_bias_ppm: 0,
        proposals: vec![ExpertProposal {
            page_id: PageId(page_id),
            score_ppm: MOR_SCALE_PPM,
        }],
    }
}

fn mor_decision(fixture: MorFixture) -> R<MorDecision> {
    let (specializations, inputs) = match fixture {
        MorFixture::Consensus => (
            (0..4)
                .map(|id| RouterSpecialization {
                    router_id: RouterId(id),
                    experts: vec![PageId(0), PageId(1)],
                })
                .collect::<Vec<_>>(),
            (0..4).map(|id| router(id, 0)).collect::<Vec<_>>(),
        ),
        MorFixture::MarginEscalation => (
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
            vec![router(0, 0), router(1, 1)],
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
            vec![router(0, 0), router(1, 0), router(2, 1), router(3, 1)],
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
    Ok(registry.route(&engine, &inputs, &[], &PreviousRoute::default())?)
}

fn validate_fixture(fixture: MorFixture, decision: &MorDecision) -> R<()> {
    match fixture {
        MorFixture::Consensus => {
            if decision.structural_disagreement_ppm != 0 || decision.request_precision_escalation {
                return Err("CONSENSUS MOR signal contract not observed".into());
            }
        }
        MorFixture::MarginEscalation => {
            if decision.structural_disagreement_ppm != 0 || !decision.request_precision_escalation {
                return Err("MARGIN_ESCALATION MOR signal contract not observed".into());
            }
        }
        MorFixture::StructuralDisagreement => {
            if decision.structural_disagreement_ppm < 250_000
                || !decision.request_precision_escalation
            {
                return Err("STRUCTURAL_DISAGREEMENT MOR signal contract not observed".into());
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

pub fn report() -> R<String> {
    let mut out = String::from(
        "PROTOCOL,SML-DKV-4D-MOR-ACTIONABILITY-PREREGISTRATION\nSTATUS,DEVELOPMENT_NOT_MODEL_QUALIFICATION\n",
    );

    for mor_fixture in MorFixture::ALL {
        let mor = mor_decision(mor_fixture)?;
        validate_fixture(mor_fixture, &mor)?;
        writeln!(
            out,
            "MOR,{},{},{},{},{},{}",
            mor_fixture.name(),
            mor.active_routers.len(),
            mor.ranked_experts.len(),
            mor.selected_experts.len(),
            mor.structural_disagreement_ppm,
            mor.request_precision_escalation,
        )?;

        for state_fixture in StateFixture::ALL {
            let initial = initial(state_fixture)?;
            let base = kernel()?;
            let halting = HaltingPolicy {
                min_depth: 1,
                max_depth: 4,
                stable_hamming_threshold: 0,
            };
            let stability = compare_delta_kv_recurrent(initial.clone(), base.clone(), halting, 4)?;

            let mut semantic_kernel = base.clone();
            let semantic = run_dkv4_semantic_recurrent(
                initial.clone(),
                &mut semantic_kernel,
                &mor,
                Dkv4Provenance::VerifiedPresent,
                Dkv4SemanticController::new(Dkv4SemanticPolicy::default())?,
            )?;

            let mut fixed_mor_kernel = base.clone();
            let fixed_mor = run_dkv4_actionable_recurrent(
                initial.clone(),
                &mut fixed_mor_kernel,
                &mor,
                Dkv4Provenance::VerifiedPresent,
                Dkv4ActionableController::new(Dkv4SemanticPolicy::default())?,
                Dkv4RefinementCapabilities::dkv2b_fixed_mor(),
            )?;

            let mut dynamic_mor_kernel = base;
            let dynamic_mor = run_dkv4_actionable_recurrent(
                initial,
                &mut dynamic_mor_kernel,
                &mor,
                Dkv4Provenance::VerifiedPresent,
                Dkv4ActionableController::new(Dkv4SemanticPolicy::default())?,
                Dkv4RefinementCapabilities::dynamic_mor(),
            )?;

            let stability_correct = correct(&stability.adaptive.state)?;
            let semantic_correct = correct(&semantic.state)?;
            let fixed_correct = correct(&fixed_mor.state)?;
            let dynamic_correct = correct(&dynamic_mor.state)?;

            if !(stability_correct && semantic_correct && fixed_correct && dynamic_correct) {
                return Err("recurrent fixture did not reach the positive target".into());
            }
            if dynamic_mor.depth != semantic.depth || dynamic_mor.decision != semantic.decision {
                return Err("dynamic MOR capability did not reproduce DKV-4c".into());
            }
            if fixed_mor.depth > semantic.depth {
                return Err("fixed-MOR actionability increased recurrent depth".into());
            }

            writeln!(
                out,
                "ROW,{},{},disagreement={},precision={},stability_depth={},semantic_depth={},semantic_decision={:?},fixed_mor_depth={},fixed_mor_decision={:?},dynamic_mor_depth={},dynamic_mor_decision={:?},final_correct=true",
                mor_fixture.name(),
                state_fixture.name(),
                mor.structural_disagreement_ppm,
                mor.request_precision_escalation,
                stability.adaptive.depth,
                semantic.depth,
                semantic.decision,
                fixed_mor.depth,
                fixed_mor.decision,
                dynamic_mor.depth,
                dynamic_mor.decision,
            )?;
        }

        let mut missing_kernel = kernel()?;
        let before_reads = missing_kernel.counters().reads;
        let missing = run_dkv4_actionable_recurrent(
            initial(StateFixture::Stable)?,
            &mut missing_kernel,
            &mor,
            Dkv4Provenance::Missing,
            Dkv4ActionableController::new(Dkv4SemanticPolicy::default())?,
            Dkv4RefinementCapabilities::dkv2b_fixed_mor(),
        )?;
        let missing_reads = missing_kernel.counters().reads.saturating_sub(before_reads);

        let mut unverified_kernel = kernel()?;
        let unverified = run_dkv4_actionable_recurrent(
            initial(StateFixture::Stable)?,
            &mut unverified_kernel,
            &mor,
            Dkv4Provenance::Unverified,
            Dkv4ActionableController::new(Dkv4SemanticPolicy::default())?,
            Dkv4RefinementCapabilities::dkv2b_fixed_mor(),
        )?;

        if missing_reads != 0
            || matches!(missing.decision, Dkv4Decision::StopCandidate(_))
            || matches!(unverified.decision, Dkv4Decision::StopCandidate(_))
        {
            return Err("fail-closed provenance contract drift".into());
        }

        writeln!(
            out,
            "SAFETY,{},missing_reads={},missing_decision={:?},unverified_depth={},unverified_decision={:?}",
            mor_fixture.name(),
            missing_reads,
            missing.decision,
            unverified.depth,
            unverified.decision,
        )?;
    }

    out.push_str("PROMOTION,BLOCKED,mor_actionability_development_panel\n");
    Ok(out)
}

#[allow(dead_code)]
fn main() -> R<()> {
    print!("{}", report()?);
    Ok(())
}
