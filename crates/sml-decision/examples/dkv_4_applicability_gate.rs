//! DKV-4g independent expert-applicability development panel.
//!
//! Protocol: docs/SML-DKV-4G-APPLICABILITY-PREREGISTRATION.md.

use std::{error::Error, fmt::Write as _};

use sml_core::{
    delta_kv::{DeltaKvMemory, DELTA_KV_ONE},
    delta_kv_adaptive::{
        Dkv4ActionableController, Dkv4Decision, Dkv4Provenance, Dkv4RefinementCapabilities,
        Dkv4SemanticPolicy, Dkv4Signals,
    },
    delta_kv_recurrent::{compare_delta_kv_recurrent, DeltaKvRecurrentKernel, DeltaKvRecurrentPolicy},
    delta_kv_semantic::{DeltaKvSemanticKeyEvidence, DeltaKvSemanticProjectionCounters},
    mor::{
        ExpertProposal, MixtureOfRouters, MorDecision, MorPolicy, PreviousRoute, RouterId,
        RouterInput, MOR_SCALE_PPM,
    },
    mor_registry::{MorRegistryPolicy, MorRouterRegistry, RouterSpecialization},
    semantic::{SemanticRewriteTrace, SemanticRewriter, SemanticState, SemanticStateDelta},
    HaltingPolicy, PageId,
};
use sml_decision::applicability::{
    AllowedExpertSet, ExpertApplicabilityLimits, ExpertApplicabilityOutcome,
};

type R<T> = Result<T, Box<dyn Error>>;
const BITS: usize = 8;

#[derive(Debug, Clone, Copy)]
enum Task {
    AlignedTrue,
    AlignedFalse,
    MisalignedFalse,
    MisalignedTrue,
}

impl Task {
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

    const fn allowed(self) -> PageId {
        match self {
            Self::AlignedTrue | Self::MisalignedTrue => PageId(0),
            Self::AlignedFalse | Self::MisalignedFalse => PageId(1),
        }
    }

    const fn oracle(self) -> bool {
        match self {
            Self::AlignedTrue | Self::MisalignedTrue => true,
            Self::AlignedFalse | Self::MisalignedFalse => false,
        }
    }

    const fn aligned(self) -> bool {
        self.favored().0 == self.allowed().0
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
    let registry = MorRouterRegistry::new(
        vec![
            RouterSpecialization {
                router_id: RouterId(0),
                experts: vec![favored],
            },
            RouterSpecialization {
                router_id: RouterId(1),
                experts: vec![other],
            },
        ],
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
    Ok(registry.route(
        &engine,
        &[
            router(0, favored, MOR_SCALE_PPM),
            router(1, other, 920_000),
        ],
        &[],
        previous,
    )?)
}

fn route_margin(decision: &MorDecision) -> u32 {
    match decision.ranked_experts.as_slice() {
        [first, second, ..] => first.final_score_ppm.saturating_sub(second.final_score_ppm),
        [_] => 2_000_000,
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

fn validate_geometry(task: Task) -> R<(MorDecision, MorDecision)> {
    let first = mor_decision(task.favored(), &PreviousRoute::default())?;
    let second = mor_decision(task.favored(), &next_previous(&first))?;
    if primary(&first)? != task.favored()
        || route_margin(&first) != 40_000
        || !first.request_precision_escalation
        || first.selected_experts.len() != 2
    {
        return Err("first MOR geometry drift".into());
    }
    if primary(&second)? != task.favored()
        || route_margin(&second) != 70_000
        || second.request_precision_escalation
        || second.selected_experts != vec![task.favored()]
    {
        return Err("second MOR geometry drift".into());
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
struct DynamicOutcome {
    state: SemanticState<BITS>,
    depth: u32,
    decision: Dkv4Decision,
    terminal_mor: Option<MorDecision>,
    reads: u64,
}

fn run_dynamic(
    initial: SemanticState<BITS>,
    mut kernel: DeltaKvRecurrentKernel<BITS>,
    favored: PageId,
    provenance: Dkv4Provenance,
) -> R<DynamicOutcome> {
    let reads_start = kernel.counters().reads;
    if matches!(
        provenance,
        Dkv4Provenance::Missing | Dkv4Provenance::Conflict
    ) {
        let decision = match provenance {
            Dkv4Provenance::Missing => Dkv4Decision::Abstain(
                sml_core::delta_kv_adaptive::Dkv4Reason::MissingEvidence,
            ),
            Dkv4Provenance::Conflict => Dkv4Decision::Abstain(
                sml_core::delta_kv_adaptive::Dkv4Reason::ConflictingEvidence,
            ),
            _ => unreachable!(),
        };
        return Ok(DynamicOutcome {
            state: initial,
            depth: 0,
            decision,
            terminal_mor: None,
            reads: 0,
        });
    }

    let controller = Dkv4ActionableController::new(Dkv4SemanticPolicy::default())?;
    let mut state = initial;
    let mut previous = PreviousRoute::default();

    for depth in 1..=controller.policy().max_depth {
        let before = kernel.cycle_evidence().len();
        let step = kernel.rewrite(&state, depth)?;
        if kernel.cycle_evidence().len() != before + 1 {
            return Err("DKV cycle evidence drift".into());
        }
        let trace = SemanticRewriteTrace {
            depth,
            delta: SemanticStateDelta::between(&state, &step.state),
            stability_blocked: step.stability_blocked,
        };
        let dkv = kernel.cycle_evidence().last().ok_or("missing DKV evidence")?;
        let mor = mor_decision(favored, &previous)?;
        let signals = Dkv4Signals::from_dkv2b(&trace, dkv, &mor, provenance)?;
        let decision =
            controller.decide(signals, Dkv4RefinementCapabilities::dynamic_mor())?;
        let next = next_previous(&mor);
        state = step.state;

        if !matches!(decision, Dkv4Decision::Refine(_)) {
            return Ok(DynamicOutcome {
                state,
                depth,
                decision,
                terminal_mor: Some(mor),
                reads: kernel.counters().reads.saturating_sub(reads_start),
            });
        }
        previous = next;
    }

    Err("dynamic loop exhausted without terminal decision".into())
}

fn allowed_set(task: Task) -> R<AllowedExpertSet> {
    Ok(AllowedExpertSet::new(
        vec![task.allowed()],
        ExpertApplicabilityLimits {
            expert_count: 2,
            max_allowed_experts: 1,
        },
    )?)
}

fn screen(
    set: &AllowedExpertSet,
    page: PageId,
    proposal: bool,
) -> Option<bool> {
    match set.screen(page, proposal) {
        ExpertApplicabilityOutcome::Allowed(value) => Some(value),
        ExpertApplicabilityOutcome::Abstain(_) => None,
    }
}

fn dynamic_proposal(outcome: &DynamicOutcome) -> R<Option<(PageId, bool)>> {
    if !matches!(outcome.decision, Dkv4Decision::StopCandidate(_)) {
        return Ok(None);
    }
    let mor = outcome
        .terminal_mor
        .as_ref()
        .ok_or("terminal MOR missing")?;
    if mor.selected_experts.len() != 1 {
        return Ok(None);
    }
    let page = mor.selected_experts[0];
    Ok(Some((page, routed_answer(&outcome.state, page)?)))
}

#[derive(Debug, Clone, Copy, Default)]
struct Aggregate {
    rows: u32,
    answered: u32,
    correct_answered: u32,
    errors: u32,
    abstained_not_allowed: u32,
    cycles: u32,
    reads: u64,
}

impl Aggregate {
    fn observe(
        &mut self,
        answer: Option<bool>,
        oracle: bool,
        not_allowed: bool,
        cycles: u32,
        reads: u64,
    ) {
        self.rows += 1;
        self.cycles += cycles;
        self.reads += reads;
        self.abstained_not_allowed += u32::from(not_allowed);
        if let Some(answer) = answer {
            self.answered += 1;
            self.correct_answered += u32::from(answer == oracle);
            self.errors += u32::from(answer != oracle);
        }
    }
}

fn ppm(num: u32, den: u32) -> Option<u32> {
    (den != 0).then(|| ((num as u128 * MOR_SCALE_PPM as u128) / den as u128) as u32)
}

fn emit_summary(out: &mut String, name: &str, a: Aggregate) -> R<()> {
    let coverage = ppm(a.answered, a.rows).unwrap_or(0);
    let risk = ppm(a.errors, a.answered);
    writeln!(
        out,
        "SUMMARY,{name},rows={},answered={},coverage_ppm={coverage},correct_answered={},errors={},risk_defined={},risk_ppm={},abstained_not_allowed={},cycles={},reads={}",
        a.rows,
        a.answered,
        a.correct_answered,
        a.errors,
        risk.is_some(),
        risk.unwrap_or(0),
        a.abstained_not_allowed,
        a.cycles,
        a.reads,
    )?;
    Ok(())
}

pub fn report() -> R<String> {
    let mut out = String::from(
        "PROTOCOL,SML-DKV-4G-APPLICABILITY-PREREGISTRATION\nSTATUS,DEVELOPMENT_NOT_MODEL_QUALIFICATION\n",
    );
    let mut dynamic_unguarded = Aggregate::default();
    let mut dynamic_guarded = Aggregate::default();
    let mut primary_guarded = Aggregate::default();
    let mut universal = Aggregate::default();

    for task in Task::ALL {
        let (first_mor, second_mor) = validate_geometry(task)?;
        let allowed = allowed_set(task)?;
        writeln!(
            out,
            "TASK,{},favored={},allowed={},aligned={},oracle={},first_margin={},first_selected={},second_margin={},second_selected={}",
            task.name(),
            task.favored().0,
            task.allowed().0,
            task.aligned(),
            task.oracle(),
            route_margin(&first_mor),
            first_mor.selected_experts.len(),
            route_margin(&second_mor),
            second_mor.selected_experts.len(),
        )?;

        for fixture in StateFixture::ALL {
            let start = initial(fixture)?;
            let base = kernel()?;
            let stability = compare_delta_kv_recurrent(
                start.clone(),
                base.clone(),
                HaltingPolicy {
                    min_depth: 1,
                    max_depth: 4,
                    stable_hamming_threshold: 0,
                },
                4,
            )?;
            let dynamic =
                run_dynamic(start, base, task.favored(), Dkv4Provenance::VerifiedPresent)?;
            let (dynamic_page, unguarded_answer) = dynamic_proposal(&dynamic)?
                .ok_or("dynamic DKV-4g fixture did not produce singleton proposal")?;
            let guarded_answer = screen(&allowed, dynamic_page, unguarded_answer);
            let dynamic_not_allowed = guarded_answer.is_none();

            let first_page = primary(&first_mor)?;
            let first_answer = routed_answer(&stability.adaptive.state, first_page)?;
            let first_guarded = screen(&allowed, first_page, first_answer);
            let first_not_allowed = first_guarded.is_none();

            dynamic_unguarded.observe(
                Some(unguarded_answer),
                task.oracle(),
                false,
                dynamic.depth,
                dynamic.reads,
            );
            dynamic_guarded.observe(
                guarded_answer,
                task.oracle(),
                dynamic_not_allowed,
                dynamic.depth,
                dynamic.reads,
            );
            primary_guarded.observe(
                first_guarded,
                task.oracle(),
                first_not_allowed,
                stability.adaptive.depth,
                stability.adaptive_cycles.len() as u64,
            );
            universal.observe(None, task.oracle(), false, 0, 0);

            writeln!(
                out,
                "ROW,{},{},oracle={},favored={},allowed={},dynamic_page={},dynamic_depth={},dynamic_unguarded={},dynamic_guarded={},dynamic_allowed={},primary_page={},primary_depth={},primary_guarded={},primary_allowed={},final_v={}",
                task.name(),
                fixture.name(),
                task.oracle(),
                task.favored().0,
                task.allowed().0,
                dynamic_page.0,
                dynamic.depth,
                unguarded_answer,
                format_answer(guarded_answer),
                !dynamic_not_allowed,
                first_page.0,
                stability.adaptive.depth,
                format_answer(first_guarded),
                !first_not_allowed,
                dynamic.state.value.get(0)?,
            )?;
        }

        for provenance in [Dkv4Provenance::Missing, Dkv4Provenance::Conflict] {
            let safety =
                run_dynamic(initial(StateFixture::Stable)?, kernel()?, task.favored(), provenance)?;
            if safety.depth != 0
                || safety.reads != 0
                || matches!(safety.decision, Dkv4Decision::StopCandidate(_))
            {
                return Err("fail-closed provenance drift".into());
            }
        }
        let unverified = run_dynamic(
            initial(StateFixture::Stable)?,
            kernel()?,
            task.favored(),
            Dkv4Provenance::Unverified,
        )?;
        if matches!(unverified.decision, Dkv4Decision::StopCandidate(_)) {
            return Err("unverified path stopped candidate".into());
        }
        writeln!(
            out,
            "SAFETY,{},missing_reads=0,conflict_reads=0,unverified_depth={},unverified_decision={:?}",
            task.name(),
            unverified.depth,
            unverified.decision,
        )?;
    }

    emit_summary(&mut out, "DYNAMIC_UNGUARDED", dynamic_unguarded)?;
    emit_summary(&mut out, "DYNAMIC_APPLICABILITY", dynamic_guarded)?;
    emit_summary(&mut out, "FIRST_PRIMARY_APPLICABILITY", primary_guarded)?;
    emit_summary(&mut out, "UNIVERSAL_ABSTAIN", universal)?;
    out.push_str("PROMOTION,BLOCKED,independent_applicability_development_panel\n");
    Ok(out)
}

fn format_answer(answer: Option<bool>) -> &'static str {
    match answer {
        Some(true) => "true",
        Some(false) => "false",
        None => "abstain",
    }
}

#[allow(dead_code)]
fn main() -> R<()> {
    print!("{}", report()?);
    Ok(())
}
