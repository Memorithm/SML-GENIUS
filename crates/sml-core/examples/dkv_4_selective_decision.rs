//! DKV-4f selective downstream decision development panel.
//!
//! Protocol: docs/SML-DKV-4F-SELECTIVE-DECISION-PREREGISTRATION.md.

use std::{error::Error, fmt::Write as _};

use sml_core::{
    delta_kv::{DeltaKvMemory, DELTA_KV_ONE},
    delta_kv_adaptive::{
        run_dkv4_semantic_recurrent, Dkv4ActionableController, Dkv4Decision, Dkv4Provenance,
        Dkv4RefinementCapabilities, Dkv4SemanticController, Dkv4SemanticPolicy, Dkv4Signals,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TaskFamily {
    AlignedTrue,
    AlignedFalse,
    MisalignedFalse,
    MisalignedTrue,
}

impl TaskFamily {
    const ALL: [Self; 4] = [
        Self::AlignedTrue,
        Self::AlignedFalse,
        Self::MisalignedFalse,
        Self::MisalignedTrue,
    ];

    const fn name(self) -> &'static str {
        match self {
            Self::AlignedTrue => "ALIGNED_TRUE",
            Self::AlignedFalse => "ALIGNED_FALSE",
            Self::MisalignedFalse => "MISALIGNED_FALSE",
            Self::MisalignedTrue => "MISALIGNED_TRUE",
        }
    }

    const fn favored(self) -> PageId {
        match self {
            Self::AlignedTrue | Self::MisalignedFalse => PageId(0),
            Self::AlignedFalse | Self::MisalignedTrue => PageId(1),
        }
    }

    const fn oracle(self) -> bool {
        match self {
            Self::AlignedTrue | Self::MisalignedTrue => true,
            Self::AlignedFalse | Self::MisalignedFalse => false,
        }
    }

    const fn aligned(self) -> bool {
        matches!(self, Self::AlignedTrue | Self::AlignedFalse)
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

fn router(router_id: u32, page_id: PageId, score_ppm: u32) -> RouterInput {
    RouterInput {
        router_id: RouterId(router_id),
        affinity_ppm: MOR_SCALE_PPM,
        reliability_ppm: MOR_SCALE_PPM,
        confidence_ppm: MOR_SCALE_PPM,
        balance_bias_ppm: 0,
        proposals: vec![ExpertProposal { page_id, score_ppm }],
    }
}

fn mor_decision(favored: PageId, previous: &PreviousRoute) -> R<MorDecision> {
    let other = if favored == PageId(0) {
        PageId(1)
    } else {
        PageId(0)
    };
    let specializations = vec![
        RouterSpecialization {
            router_id: RouterId(0),
            experts: vec![favored],
        },
        RouterSpecialization {
            router_id: RouterId(1),
            experts: vec![other],
        },
    ];
    let inputs = vec![
        router(0, favored, MOR_SCALE_PPM),
        router(1, other, 920_000),
    ];

    let registry = MorRouterRegistry::new(
        specializations,
        MorRegistryPolicy {
            expert_count: 2,
            max_routers: 2,
            max_edges: 2,
            max_fanout: 1,
            require_full_coverage: true,
        },
    )?;
    let engine = MixtureOfRouters::new(MorPolicy {
        base_expert_mass_threshold_ppm: 530_000,
        ..MorPolicy::default()
    })?;
    Ok(registry.route(&engine, &inputs, &[], previous)?)
}

fn route_margin(decision: &MorDecision) -> u32 {
    match decision.ranked_experts.as_slice() {
        [first, second, ..] => first.final_score_ppm.saturating_sub(second.final_score_ppm),
        [_] => MOR_SCORE_CEILING_PPM,
        [] => 0,
    }
}

fn primary(decision: &MorDecision) -> R<PageId> {
    decision
        .ranked_experts
        .first()
        .map(|expert| expert.page_id)
        .ok_or_else(|| "missing MOR primary".into())
}

fn next_previous(decision: &MorDecision) -> PreviousRoute {
    PreviousRoute {
        primary: decision.ranked_experts.first().map(|expert| expert.page_id),
        selected: decision.selected_experts.clone(),
    }
}

fn validate_route_geometry(task: TaskFamily) -> R<(MorDecision, MorDecision)> {
    let first = mor_decision(task.favored(), &PreviousRoute::default())?;
    let second = mor_decision(task.favored(), &next_previous(&first))?;

    if primary(&first)? != task.favored()
        || first.ranked_experts.len() != 2
        || route_margin(&first) != 40_000
        || !first.request_precision_escalation
        || first.selected_experts.len() != 2
    {
        return Err(format!("{} first-route frozen geometry refuted", task.name()).into());
    }
    if primary(&second)? != task.favored()
        || second.ranked_experts.len() != 2
        || route_margin(&second) != 70_000
        || second.request_precision_escalation
        || second.selected_experts != vec![task.favored()]
    {
        return Err(format!("{} second-route frozen geometry refuted", task.name()).into());
    }
    Ok((first, second))
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

fn expert_output(page: PageId) -> R<bool> {
    match page {
        PageId(0) => Ok(true),
        PageId(1) => Ok(false),
        _ => Err("unexpected expert page".into()),
    }
}

fn routed_answer(state: &SemanticState<BITS>, page: PageId) -> R<bool> {
    Ok(state.value.get(0)? && expert_output(page)?)
}

#[derive(Debug, Clone)]
struct DynamicCycle {
    depth: u32,
    mor: MorDecision,
    decision: Dkv4Decision,
}

#[derive(Debug, Clone)]
struct DynamicOutcome {
    state: SemanticState<BITS>,
    depth: u32,
    decision: Dkv4Decision,
    terminal_mor: Option<MorDecision>,
    cycles: Vec<DynamicCycle>,
    reads_delta: u64,
}

fn run_dynamic(
    initial: SemanticState<BITS>,
    mut kernel: DeltaKvRecurrentKernel<BITS>,
    favored: PageId,
    provenance: Dkv4Provenance,
) -> R<DynamicOutcome> {
    let reads_start = kernel.counters().reads;
    if matches!(provenance, Dkv4Provenance::Missing | Dkv4Provenance::Conflict) {
        let decision = if matches!(provenance, Dkv4Provenance::Missing) {
            Dkv4Decision::Abstain(sml_core::delta_kv_adaptive::Dkv4Reason::MissingEvidence)
        } else {
            Dkv4Decision::Abstain(sml_core::delta_kv_adaptive::Dkv4Reason::ConflictingEvidence)
        };
        return Ok(DynamicOutcome {
            state: initial,
            depth: 0,
            decision,
            terminal_mor: None,
            cycles: Vec::new(),
            reads_delta: 0,
        });
    }

    let controller = Dkv4ActionableController::new(Dkv4SemanticPolicy::default())?;
    let mut state = initial;
    let mut previous = PreviousRoute::default();
    let mut cycles = Vec::new();

    for depth in 1..=controller.policy().max_depth {
        let before = kernel.cycle_evidence().len();
        let step = kernel.rewrite(&state, depth)?;
        if kernel.cycle_evidence().len() != before + 1 {
            return Err("dynamic DKV cycle evidence drift".into());
        }
        let trace = SemanticRewriteTrace {
            depth,
            delta: SemanticStateDelta::between(&state, &step.state),
            stability_blocked: step.stability_blocked,
        };
        let dkv = kernel
            .cycle_evidence()
            .last()
            .ok_or("dynamic DKV evidence missing")?;
        let mor = mor_decision(favored, &previous)?;
        let signals = Dkv4Signals::from_dkv2b(&trace, dkv, &mor, provenance)?;
        let decision = controller.decide(signals, Dkv4RefinementCapabilities::dynamic_mor())?;
        let previous_out = next_previous(&mor);

        cycles.push(DynamicCycle {
            depth,
            mor: mor.clone(),
            decision,
        });
        state = step.state;

        if !matches!(decision, Dkv4Decision::Refine(_)) {
            return Ok(DynamicOutcome {
                state,
                depth,
                decision,
                terminal_mor: Some(mor),
                cycles,
                reads_delta: kernel.counters().reads.saturating_sub(reads_start),
            });
        }
        previous = previous_out;
    }

    Err("dynamic selective loop exhausted without terminal decision".into())
}

#[derive(Debug, Clone, Copy, Default)]
struct Aggregate {
    rows: u32,
    answered: u32,
    correct_answered: u32,
    errors: u32,
    cycles: u32,
    reads: u64,
}

impl Aggregate {
    fn observe(&mut self, answered: bool, correct: bool, cycles: u32, reads: u64) {
        self.rows += 1;
        self.cycles += cycles;
        self.reads += reads;
        if answered {
            self.answered += 1;
            self.correct_answered += u32::from(correct);
            self.errors += u32::from(!correct);
        }
    }
}

fn selective_answer(
    decision: Dkv4Decision,
    mor: &MorDecision,
    state: &SemanticState<BITS>,
) -> R<Option<bool>> {
    if !matches!(decision, Dkv4Decision::StopCandidate(_)) || mor.selected_experts.len() != 1 {
        return Ok(None);
    }
    Ok(Some(routed_answer(state, mor.selected_experts[0])?))
}

fn answer_correct(answer: Option<bool>, oracle: bool) -> bool {
    answer == Some(oracle)
}

pub fn report() -> R<String> {
    let mut out = String::from(
        "PROTOCOL,SML-DKV-4F-SELECTIVE-DECISION-PREREGISTRATION\nSTATUS,DEVELOPMENT_NOT_MODEL_QUALIFICATION\n",
    );

    let mut stability_selective = Aggregate::default();
    let mut frozen = Aggregate::default();
    let mut dynamic = Aggregate::default();
    let mut first_primary = Aggregate::default();
    let mut dynamic_aligned = Aggregate::default();
    let mut dynamic_misaligned = Aggregate::default();

    for task in TaskFamily::ALL {
        let (first_mor, second_mor) = validate_route_geometry(task)?;
        writeln!(
            out,
            "MOR,{},favored={},oracle={},first_margin={},first_precision={},first_selected={},second_margin={},second_precision={},second_selected={}",
            task.name(),
            task.favored().0,
            task.oracle(),
            route_margin(&first_mor),
            first_mor.request_precision_escalation,
            first_mor.selected_experts.len(),
            route_margin(&second_mor),
            second_mor.request_precision_escalation,
            second_mor.selected_experts.len(),
        )?;

        for state_fixture in StateFixture::ALL {
            let start = initial(state_fixture)?;
            let base = kernel()?;
            let halting = HaltingPolicy {
                min_depth: 1,
                max_depth: 4,
                stable_hamming_threshold: 0,
            };
            let stability = compare_delta_kv_recurrent(start.clone(), base.clone(), halting, 4)?;
            let stability_reads = stability.adaptive_cycles.len() as u64;

            let stability_answer = if first_mor.selected_experts.len() == 1 {
                Some(routed_answer(
                    &stability.adaptive.state,
                    first_mor.selected_experts[0],
                )?)
            } else {
                None
            };
            let stability_correct = answer_correct(stability_answer, task.oracle());
            stability_selective.observe(
                stability_answer.is_some(),
                stability_correct,
                stability.adaptive.depth,
                stability_reads,
            );

            let mut frozen_kernel = base.clone();
            let frozen_out = run_dkv4_semantic_recurrent(
                start.clone(),
                &mut frozen_kernel,
                &first_mor,
                Dkv4Provenance::VerifiedPresent,
                Dkv4SemanticController::new(Dkv4SemanticPolicy::default())?,
            )?;
            let frozen_answer = selective_answer(
                frozen_out.decision,
                &first_mor,
                &frozen_out.state,
            )?;
            let frozen_correct = answer_correct(frozen_answer, task.oracle());
            frozen.observe(
                frozen_answer.is_some(),
                frozen_correct,
                frozen_out.depth,
                frozen_out.cycles.len() as u64,
            );

            let dynamic_out =
                run_dynamic(start, base, task.favored(), Dkv4Provenance::VerifiedPresent)?;
            let terminal_mor = dynamic_out
                .terminal_mor
                .as_ref()
                .ok_or("dynamic terminal MOR missing")?;
            let dynamic_answer =
                selective_answer(dynamic_out.decision, terminal_mor, &dynamic_out.state)?;
            let dynamic_correct = answer_correct(dynamic_answer, task.oracle());
            dynamic.observe(
                dynamic_answer.is_some(),
                dynamic_correct,
                dynamic_out.depth,
                dynamic_out.reads_delta,
            );
            if task.aligned() {
                dynamic_aligned.observe(
                    dynamic_answer.is_some(),
                    dynamic_correct,
                    dynamic_out.depth,
                    dynamic_out.reads_delta,
                );
            } else {
                dynamic_misaligned.observe(
                    dynamic_answer.is_some(),
                    dynamic_correct,
                    dynamic_out.depth,
                    dynamic_out.reads_delta,
                );
            }

            let first_primary_page = primary(&first_mor)?;
            let aggressive_answer = Some(routed_answer(
                &stability.adaptive.state,
                first_primary_page,
            )?);
            let aggressive_correct = answer_correct(aggressive_answer, task.oracle());
            first_primary.observe(
                true,
                aggressive_correct,
                stability.adaptive.depth,
                stability_reads,
            );

            if dynamic_out.cycles.len() < 2
                || dynamic_out.cycles[0].depth != 1
                || dynamic_out.cycles[0].mor.selected_experts.len() != 2
                || !dynamic_out.cycles[0].mor.request_precision_escalation
                || dynamic_out.cycles[1].mor.selected_experts != vec![task.favored()]
                || dynamic_out.cycles[1].mor.request_precision_escalation
            {
                return Err("dynamic route transition contract drift".into());
            }

            writeln!(
                out,
                "ROW,{},{},oracle={},stability_depth={},stability_answer={},stability_correct={},frozen_depth={},frozen_decision={:?},frozen_answer={},frozen_correct={},dynamic_depth={},dynamic_decision={:?},dynamic_terminal_margin={},dynamic_terminal_selected={},dynamic_answer={},dynamic_correct={},first_primary_depth={},first_primary_answer={},first_primary_correct={}",
                task.name(),
                state_fixture.name(),
                task.oracle(),
                stability.adaptive.depth,
                format_answer(stability_answer),
                stability_correct,
                frozen_out.depth,
                frozen_out.decision,
                format_answer(frozen_answer),
                frozen_correct,
                dynamic_out.depth,
                dynamic_out.decision,
                route_margin(terminal_mor),
                terminal_mor.selected_experts.len(),
                format_answer(dynamic_answer),
                dynamic_correct,
                stability.adaptive.depth,
                format_answer(aggressive_answer),
                aggressive_correct,
            )?;
        }

        let missing =
            run_dynamic(initial(StateFixture::Stable)?, kernel()?, task.favored(), Dkv4Provenance::Missing)?;
        let conflict =
            run_dynamic(initial(StateFixture::Stable)?, kernel()?, task.favored(), Dkv4Provenance::Conflict)?;
        let unverified =
            run_dynamic(initial(StateFixture::Stable)?, kernel()?, task.favored(), Dkv4Provenance::Unverified)?;
        if missing.depth != 0
            || missing.reads_delta != 0
            || conflict.depth != 0
            || conflict.reads_delta != 0
            || matches!(missing.decision, Dkv4Decision::StopCandidate(_))
            || matches!(conflict.decision, Dkv4Decision::StopCandidate(_))
            || matches!(unverified.decision, Dkv4Decision::StopCandidate(_))
        {
            return Err("selective decision provenance contract drift".into());
        }
        writeln!(
            out,
            "SAFETY,{},missing_reads={},missing_decision={:?},conflict_reads={},conflict_decision={:?},unverified_depth={},unverified_decision={:?}",
            task.name(),
            missing.reads_delta,
            missing.decision,
            conflict.reads_delta,
            conflict.decision,
            unverified.depth,
            unverified.decision,
        )?;
    }

    emit_summary(&mut out, "STABILITY_SELECTIVE", stability_selective)?;
    emit_summary(&mut out, "FROZEN_DKV4C_SELECTIVE", frozen)?;
    emit_summary(&mut out, "DYNAMIC_DKV4F_SELECTIVE", dynamic)?;
    emit_summary(&mut out, "FIRST_PRIMARY", first_primary)?;
    emit_summary(&mut out, "DYNAMIC_ALIGNED", dynamic_aligned)?;
    emit_summary(&mut out, "DYNAMIC_MISALIGNED", dynamic_misaligned)?;
    out.push_str("PROMOTION,BLOCKED,selective_downstream_decision_development_panel\n");
    Ok(out)
}

fn emit_summary(out: &mut String, arm: &str, a: Aggregate) -> R<()> {
    let risk_ppm = if a.answered == 0 {
        0
    } else {
        ((a.errors as u128 * MOR_SCALE_PPM as u128) / a.answered as u128) as u32
    };
    writeln!(
        out,
        "SUMMARY,{arm},rows={},answered={},correct_answered={},errors={},risk_ppm={},cycles={},reads={}",
        a.rows,
        a.answered,
        a.correct_answered,
        a.errors,
        risk_ppm,
        a.cycles,
        a.reads,
    )?;
    Ok(())
}

fn format_answer(answer: Option<bool>) -> &'static str {
    match answer {
        Some(true) => "true",
        Some(false) => "false",
        None => "abstain",
    }
}

fn main() -> R<()> {
    print!("{}", report()?);
    Ok(())
}
