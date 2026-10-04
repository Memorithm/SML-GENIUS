//! DKV-4i held-out task-family applicability development panel.
//!
//! Protocol: docs/SML-DKV-4I-HELDOUT-FAMILY-PREREGISTRATION.md.
//! The held-out claim applies to the applicability gate, not the base SML candidate.

use std::{error::Error, fmt::Write as _};

use sml_core::{
    candidate::{AssociativeMemoryMode, SmlTaskCandidate, TaskCandidateConfig},
    delta_kv::{DeltaKvMemory, DELTA_KV_ONE},
    delta_kv_adaptive::{
        Dkv4ActionableController, Dkv4Decision, Dkv4Provenance, Dkv4RefinementCapabilities,
        Dkv4SemanticPolicy, Dkv4Signals,
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
    semantic::{SemanticRewriteTrace, SemanticRewriter, SemanticState, SemanticStateDelta},
    tasks::{
        copy_after_marker, delayed_bit_recall, distractor_rejection, keyed_fact_recall,
        overwrite_bit_recall, two_fact_conjunction, TaskEvent, TaskInstance, TaskKind,
    },
    train::TruthTable2,
    HaltingPolicy, PageId,
};
use sml_decision::training::{
    fit_boolean_expert, FitOutcome, FitPolicy, FitReport, LabeledDecision,
};
use sml_rsi::{
    curriculum::{build_verified_curriculum, CurriculumPolicy, CurriculumProposal},
    multi_task_training::{train_multitask_candidate, MultiTaskTrainingPolicy},
};

type R<T> = Result<T, Box<dyn Error>>;
const BITS: usize = 8;
const LIGHT_PAGE: PageId = PageId(0);
const FULL_PAGE: PageId = PageId(1);

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

fn proposal(
    id: &str,
    family: TaskKind,
    start: u64,
    count: u32,
    difficulty: u16,
) -> CurriculumProposal {
    CurriculumProposal {
        proposal_id: id.into(),
        family,
        case_id_start: start,
        case_count: count,
        difficulty,
    }
}

fn base_curriculum() -> R<sml_rsi::curriculum::CurriculumReport> {
    Ok(build_verified_curriculum(
        &[
            proposal("base-recall", TaskKind::DelayedBitRecall, 1_000_000, 64, 7),
            proposal(
                "base-overwrite",
                TaskKind::OverwriteBitRecall,
                1_100_000,
                64,
                7,
            ),
            proposal("base-copy", TaskKind::CopyAfterMarker, 1_200_000, 64, 8),
            proposal(
                "base-conjunction",
                TaskKind::TwoFactConjunction,
                1_300_000,
                64,
                7,
            ),
            proposal("base-keyed", TaskKind::KeyedFactRecall, 1_400_000, 64, 7),
            proposal(
                "base-distractor",
                TaskKind::DistractorRejection,
                1_500_000,
                64,
                7,
            ),
        ],
        CurriculumPolicy::default(),
    )?)
}

fn gate_tasks(start_offset: u64, difficulty: usize) -> R<Vec<TaskInstance>> {
    let specs = [
        (TaskKind::DelayedBitRecall, start_offset),
        (TaskKind::OverwriteBitRecall, start_offset + 100_000),
        (TaskKind::KeyedFactRecall, start_offset + 200_000),
        (TaskKind::TwoFactConjunction, start_offset + 300_000),
    ];
    let mut tasks = Vec::with_capacity(256);
    for (kind, start) in specs {
        for case_id in start..start + 64 {
            let task = match kind {
                TaskKind::DelayedBitRecall => delayed_bit_recall(case_id, difficulty),
                TaskKind::OverwriteBitRecall => overwrite_bit_recall(case_id, difficulty),
                TaskKind::KeyedFactRecall => keyed_fact_recall(case_id, difficulty)?,
                TaskKind::TwoFactConjunction => two_fact_conjunction(case_id, difficulty)?,
                TaskKind::CopyAfterMarker | TaskKind::DistractorRejection => unreachable!(),
            };
            if !task.oracle_matches()? {
                return Err("gate-task oracle mismatch".into());
            }
            tasks.push(task);
        }
    }
    Ok(tasks)
}

fn heldout_tasks() -> R<Vec<TaskInstance>> {
    let mut tasks = Vec::with_capacity(128);
    for case_id in 4_000_000..4_000_064 {
        let length = 1 + (case_id % 16) as usize;
        let task = copy_after_marker(case_id, length)?;
        if !task.oracle_matches()? {
            return Err("copy holdout oracle mismatch".into());
        }
        tasks.push(task);
    }
    for case_id in 4_100_000..4_100_064 {
        let distractors = 1 + (case_id % 16) as usize;
        let task = distractor_rejection(case_id, distractors)?;
        if !task.oracle_matches()? {
            return Err("distractor holdout oracle mismatch".into());
        }
        tasks.push(task);
    }
    Ok(tasks)
}

fn has_fact_event(task: &TaskInstance) -> bool {
    task.events
        .iter()
        .any(|event| matches!(event, TaskEvent::Fact { .. }))
}

fn applicability_features(task: &TaskInstance) -> (bool, bool) {
    (has_fact_event(task), task.case_id & 1 != 0)
}

fn light_operational_label(
    full: &SmlTaskCandidate,
    task: &TaskInstance,
) -> R<(bool, sml_core::candidate::TaskExecutionEvidence)> {
    let evidence = full.execute(task)?;
    Ok((
        evidence.counters.memory_reads == 0 && evidence.counters.memory_writes == 0,
        evidence,
    ))
}

fn gate_samples(full: &SmlTaskCandidate, tasks: &[TaskInstance]) -> R<Vec<LabeledDecision>> {
    tasks
        .iter()
        .map(|task| {
            let (target, _) = light_operational_label(full, task)?;
            let (left, right) = applicability_features(task);
            Ok(LabeledDecision {
                case_id: task.case_id,
                left,
                right,
                target,
            })
        })
        .collect()
}

fn light_gate(report: &FitReport, task: &TaskInstance, has_fact: Option<bool>) -> Option<bool> {
    let left = has_fact?;
    let right = task.case_id & 1 != 0;
    Some(report.selected.evaluate(left, right))
}

fn build_light_candidate(
    learned: &sml_rsi::multi_task_training::MultiTaskTrainingReport,
) -> R<SmlTaskCandidate> {
    let config = TaskCandidateConfig {
        memory_mode: AssociativeMemoryMode::DisabledZeroRead,
        ..MultiTaskTrainingPolicy::default().candidate
    };
    let (candidate, _) = SmlTaskCandidate::train_with_all_micro_ops(
        config,
        learned.conjunction_table,
        learned.overwrite_table,
        learned.recall_table,
        learned.copy_table,
    )?;
    Ok(candidate)
}

fn same_route_trace(
    full: &sml_core::candidate::TaskExecutionEvidence,
    light: &sml_core::candidate::TaskExecutionEvidence,
) -> bool {
    full.trace.len() == light.trace.len()
        && full.trace.iter().zip(&light.trace).all(|(left, right)| {
            left.route_address == right.route_address
                && left.selected_page == right.selected_page
                && left.event == right.event
        })
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
    let other = if favored == LIGHT_PAGE {
        FULL_PAGE
    } else {
        LIGHT_PAGE
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
        &[router(0, favored, MOR_SCALE_PPM), router(1, other, 920_000)],
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

fn validate_geometry(favored: PageId) -> R<(MorDecision, MorDecision)> {
    let first = mor_decision(favored, &PreviousRoute::default())?;
    let second = mor_decision(favored, &next_previous(&first))?;
    if primary(&first)? != favored
        || route_margin(&first) != 40_000
        || !first.request_precision_escalation
        || first.selected_experts.len() != 2
    {
        return Err("first native MOR geometry drift".into());
    }
    if primary(&second)? != favored
        || route_margin(&second) != 70_000
        || second.request_precision_escalation
        || second.selected_experts != vec![favored]
    {
        return Err("second native MOR geometry drift".into());
    }
    Ok((first, second))
}

fn dkv_kernel() -> R<DeltaKvRecurrentKernel<BITS>> {
    let memory = DeltaKvMemory::new(1, 1)?;
    let key = DeltaKvSemanticKeyEvidence {
        key_q12: vec![DELTA_KV_ONE],
        selected_experts: vec![LIGHT_PAGE],
        provenance: None,
        counters: DeltaKvSemanticProjectionCounters::default(),
        transient_key_bits: 16,
    };
    let mut kernel =
        DeltaKvRecurrentKernel::new(memory, key, vec![0], DeltaKvRecurrentPolicy::default())?;
    kernel.seed_target(&[DELTA_KV_ONE], 4096)?;
    Ok(kernel)
}

fn recurrent_initial(fixture: StateFixture) -> R<SemanticState<BITS>> {
    let mut state = SemanticState::<BITS>::zeroed()?;
    if matches!(fixture, StateFixture::Stable) {
        state.value.set(0, true)?;
    }
    Ok(state)
}

#[derive(Debug, Clone)]
struct DynamicOutcome {
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
            Dkv4Provenance::Missing => {
                Dkv4Decision::Abstain(sml_core::delta_kv_adaptive::Dkv4Reason::MissingEvidence)
            }
            Dkv4Provenance::Conflict => {
                Dkv4Decision::Abstain(sml_core::delta_kv_adaptive::Dkv4Reason::ConflictingEvidence)
            }
            _ => unreachable!(),
        };
        return Ok(DynamicOutcome {
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
            return Err("dynamic DKV evidence drift".into());
        }
        let trace = SemanticRewriteTrace {
            depth,
            delta: SemanticStateDelta::between(&state, &step.state),
            stability_blocked: step.stability_blocked,
        };
        let dkv = kernel
            .cycle_evidence()
            .last()
            .ok_or("missing dynamic DKV evidence")?;
        let mor = mor_decision(favored, &previous)?;
        let signals = Dkv4Signals::from_dkv2b(&trace, dkv, &mor, provenance)?;
        let decision = controller.decide(signals, Dkv4RefinementCapabilities::dynamic_mor())?;
        state = step.state;
        let next = next_previous(&mor);

        if !matches!(decision, Dkv4Decision::Refine(_)) {
            return Ok(DynamicOutcome {
                depth,
                decision,
                terminal_mor: Some(mor),
                reads: kernel.counters().reads.saturating_sub(reads_start),
            });
        }
        previous = next;
    }

    Err("dynamic DKV-4i loop exhausted without terminal decision".into())
}

#[derive(Debug, Clone, Copy, Default)]
struct Aggregate {
    rows: u32,
    answered: u32,
    exact_answered: u32,
    errors: u32,
    light_exec: u32,
    full_exec: u32,
    dkv_cycles: u32,
    dkv_reads: u64,
    task_memory_reads: u64,
    task_memory_writes: u64,
}

impl Aggregate {
    fn observe(
        &mut self,
        execution: Option<(&sml_core::candidate::TaskExecutionEvidence, PageId)>,
        dkv_depth: u32,
        dkv_reads: u64,
    ) {
        self.rows += 1;
        self.dkv_cycles += dkv_depth;
        self.dkv_reads += dkv_reads;
        if let Some((evidence, page)) = execution {
            self.answered += 1;
            self.exact_answered += u32::from(evidence.exact);
            self.errors += u32::from(!evidence.exact);
            self.light_exec += u32::from(page == LIGHT_PAGE);
            self.full_exec += u32::from(page == FULL_PAGE);
            self.task_memory_reads += evidence.counters.memory_reads;
            self.task_memory_writes += evidence.counters.memory_writes;
        }
    }
}

fn ppm(num: u32, den: u32) -> Option<u32> {
    (den != 0).then(|| ((num as u128 * MOR_SCALE_PPM as u128) / den as u128) as u32)
}

fn emit_summary(out: &mut String, name: &str, aggregate: Aggregate) -> R<()> {
    let coverage = ppm(aggregate.answered, aggregate.rows).unwrap_or(0);
    let risk = ppm(aggregate.errors, aggregate.answered);
    writeln!(
        out,
        "SUMMARY,{name},rows={},answered={},coverage_ppm={coverage},exact_answered={},errors={},risk_defined={},risk_ppm={},light_exec={},full_exec={},dkv_cycles={},dkv_reads={},task_memory_reads={},task_memory_writes={}",
        aggregate.rows,
        aggregate.answered,
        aggregate.exact_answered,
        aggregate.errors,
        risk.is_some(),
        risk.unwrap_or(0),
        aggregate.light_exec,
        aggregate.full_exec,
        aggregate.dkv_cycles,
        aggregate.dkv_reads,
        aggregate.task_memory_reads,
        aggregate.task_memory_writes,
    )?;
    Ok(())
}

fn execute_page<'a>(
    page: PageId,
    full: &'a SmlTaskCandidate,
    light: &'a SmlTaskCandidate,
    task: &TaskInstance,
) -> R<sml_core::candidate::TaskExecutionEvidence> {
    match page {
        LIGHT_PAGE => Ok(light.execute(task)?),
        FULL_PAGE => Ok(full.execute(task)?),
        _ => Err("unexpected expert page".into()),
    }
}

fn fit_line(out: &mut String, fit: &FitReport) -> R<()> {
    writeln!(
        out,
        "FIT,outcome={:?},selected_code={},training_samples={},validation_samples={},baseline_train_errors={},candidate_train_errors={},baseline_validation_errors={},candidate_validation_errors={},selection_evals={}",
        fit.outcome,
        fit.selected.code(),
        fit.training_samples,
        fit.validation_samples,
        fit.baseline_training_errors,
        fit.candidate_training_errors,
        fit.baseline_validation_errors,
        fit.candidate_validation_errors,
        fit.selection_gate_evaluations,
    )?;
    Ok(())
}

pub fn report() -> R<String> {
    let mut out = String::from(
        "PROTOCOL,SML-DKV-4I-HELDOUT-FAMILY-PREREGISTRATION\nSTATUS,DEVELOPMENT_NOT_MODEL_QUALIFICATION\n",
    );

    let curriculum = base_curriculum()?;
    let learned = train_multitask_candidate(&curriculum, MultiTaskTrainingPolicy::default())?;
    let full = learned.candidate.clone();
    let light = build_light_candidate(&learned)?;

    writeln!(
        out,
        "BASE,verified_tasks={},training_tasks={},development_tasks={},conjunction_code={},overwrite_code={},recall_code={},copy_code={},training_exact={}/{},development_exact={}/{}",
        curriculum.verified_tasks,
        curriculum.training_tasks,
        curriculum.development_tasks,
        learned.conjunction_table.code(),
        learned.overwrite_table.code(),
        learned.recall_table.code(),
        learned.copy_table.code(),
        learned.training_evaluation.exact_tasks,
        learned.training_evaluation.tasks,
        learned.development_evaluation.exact_tasks,
        learned.development_evaluation.tasks,
    )?;

    let gate_train_tasks = gate_tasks(2_000_000, 5)?;
    let gate_dev_tasks = gate_tasks(3_000_000, 11)?;
    let train_samples = gate_samples(&full, &gate_train_tasks)?;
    let dev_samples = gate_samples(&full, &gate_dev_tasks)?;
    let fit = fit_boolean_expert(
        &train_samples,
        &dev_samples,
        TruthTable2::FALSE,
        FitPolicy::default(),
    )?;
    fit_line(&mut out, &fit)?;

    let train_label_true = train_samples.iter().filter(|s| s.target).count();
    let dev_label_true = dev_samples.iter().filter(|s| s.target).count();
    writeln!(
        out,
        "GATE_DATA,training_rows={},development_rows={},training_light_applicable={},development_light_applicable={},training_families=DelayedBitRecall|OverwriteBitRecall|KeyedFactRecall|TwoFactConjunction,heldout_families=CopyAfterMarker|DistractorRejection",
        train_samples.len(),
        dev_samples.len(),
        train_label_true,
        dev_label_true,
    )?;

    if !matches!(fit.outcome, FitOutcome::Accepted(_)) {
        out.push_str("DOWNSTREAM,SKIPPED,applicability_fit_rejected\n");
        out.push_str("PROMOTION,BLOCKED,heldout_family_fit_rejected\n");
        return Ok(out);
    }

    let heldout = heldout_tasks()?;
    let mut dynamic_unguarded = Aggregate::default();
    let mut dynamic_learned = Aggregate::default();
    let mut first_primary_learned = Aggregate::default();
    let mut full_reference = Aggregate::default();
    let mut universal = Aggregate::default();
    let mut copy_learned = Aggregate::default();
    let mut distractor_learned = Aggregate::default();
    let mut full_reference_exact = 0_u32;
    let mut route_trace_mismatches = 0_u32;
    let mut heldout_light_labels = 0_u32;

    for task in &heldout {
        let (_, full_probe) = light_operational_label(&full, task)?;
        full_reference_exact += u32::from(full_probe.exact);
        let operational_light =
            full_probe.counters.memory_reads == 0 && full_probe.counters.memory_writes == 0;
        heldout_light_labels += u32::from(operational_light);
        let favored = if task.case_id & 1 == 0 {
            LIGHT_PAGE
        } else {
            FULL_PAGE
        };
        let (first_mor, second_mor) = validate_geometry(favored)?;

        for fixture in StateFixture::ALL {
            let initial = recurrent_initial(fixture)?;
            let base = dkv_kernel()?;
            let stability = compare_delta_kv_recurrent(
                initial.clone(),
                base.clone(),
                HaltingPolicy {
                    min_depth: 1,
                    max_depth: 4,
                    stable_hamming_threshold: 0,
                },
                4,
            )?;
            let dynamic = run_dynamic(initial, base, favored, Dkv4Provenance::VerifiedPresent)?;
            if !matches!(dynamic.decision, Dkv4Decision::StopCandidate(_)) {
                return Err("held-out dynamic route did not stop candidate".into());
            }
            let terminal = dynamic
                .terminal_mor
                .as_ref()
                .ok_or("missing dynamic terminal MOR")?;
            if terminal.selected_experts.len() != 1 {
                return Err("dynamic held-out route is not singleton".into());
            }
            let dynamic_page = terminal.selected_experts[0];

            let unguarded_evidence = execute_page(dynamic_page, &full, &light, task)?;
            if dynamic_page == LIGHT_PAGE {
                let light_probe = light.execute(task)?;
                if !same_route_trace(&full_probe, &light_probe) {
                    route_trace_mismatches += 1;
                }
            }
            dynamic_unguarded.observe(
                Some((&unguarded_evidence, dynamic_page)),
                dynamic.depth,
                dynamic.reads,
            );

            let dynamic_permit = if dynamic_page == FULL_PAGE {
                Some(true)
            } else {
                light_gate(&fit, task, Some(has_fact_event(task)))
            };
            let learned_execution = if dynamic_permit == Some(true) {
                Some(execute_page(dynamic_page, &full, &light, task)?)
            } else {
                None
            };
            dynamic_learned.observe(
                learned_execution.as_ref().map(|e| (e, dynamic_page)),
                dynamic.depth,
                dynamic.reads,
            );
            match task.kind {
                TaskKind::CopyAfterMarker => copy_learned.observe(
                    learned_execution.as_ref().map(|e| (e, dynamic_page)),
                    dynamic.depth,
                    dynamic.reads,
                ),
                TaskKind::DistractorRejection => distractor_learned.observe(
                    learned_execution.as_ref().map(|e| (e, dynamic_page)),
                    dynamic.depth,
                    dynamic.reads,
                ),
                _ => return Err("unexpected held-out family".into()),
            }

            let first_page = primary(&first_mor)?;
            let first_permit = if first_page == FULL_PAGE {
                Some(true)
            } else {
                light_gate(&fit, task, Some(has_fact_event(task)))
            };
            let first_execution = if first_permit == Some(true) {
                Some(execute_page(first_page, &full, &light, task)?)
            } else {
                None
            };
            first_primary_learned.observe(
                first_execution.as_ref().map(|e| (e, first_page)),
                stability.adaptive.depth,
                stability.adaptive_cycles.len() as u64,
            );

            full_reference.observe(Some((&full_probe, FULL_PAGE)), 0, 0);
            universal.observe(None, 0, 0);

            writeln!(
                out,
                "ROW,family={:?},case_id={},state={},has_fact={},parity={},operational_light={},favored={},first_margin={},first_selected={},second_margin={},second_selected={},dynamic_depth={},dynamic_page={},dynamic_unguarded_exact={},dynamic_permit={},dynamic_learned={},first_primary_depth={},first_page={},first_permit={},first_primary={},full_exact={},full_memory_reads={},full_memory_writes={}",
                task.kind,
                task.case_id,
                fixture.name(),
                has_fact_event(task),
                task.case_id & 1 != 0,
                operational_light,
                favored.0,
                route_margin(&first_mor),
                first_mor.selected_experts.len(),
                route_margin(&second_mor),
                second_mor.selected_experts.len(),
                dynamic.depth,
                dynamic_page.0,
                unguarded_evidence.exact,
                format_option(dynamic_permit),
                format_execution(learned_execution.as_ref()),
                stability.adaptive.depth,
                first_page.0,
                format_option(first_permit),
                format_execution(first_execution.as_ref()),
                full_probe.exact,
                full_probe.counters.memory_reads,
                full_probe.counters.memory_writes,
            )?;
        }
    }

    writeln!(
        out,
        "HOLDOUT,task_instances={},scored_rows={},copy_instances=64,distractor_instances=64,full_reference_exact_instances={},operational_light_instances={},route_trace_mismatches={}",
        heldout.len(),
        heldout.len() * 2,
        full_reference_exact,
        heldout_light_labels,
        route_trace_mismatches,
    )?;

    emit_summary(&mut out, "DYNAMIC_UNGUARDED", dynamic_unguarded)?;
    emit_summary(&mut out, "DYNAMIC_LEARNED", dynamic_learned)?;
    emit_summary(&mut out, "FIRST_PRIMARY_LEARNED", first_primary_learned)?;
    emit_summary(&mut out, "FULL_REFERENCE", full_reference)?;
    emit_summary(&mut out, "UNIVERSAL_ABSTAIN", universal)?;
    emit_summary(&mut out, "COPY_LEARNED", copy_learned)?;
    emit_summary(&mut out, "DISTRACTOR_LEARNED", distractor_learned)?;

    let mut missing_abstain = 0_u32;
    for task in heldout.iter().take(16) {
        let permit = light_gate(&fit, task, None);
        missing_abstain += u32::from(permit.is_none());
        writeln!(
            out,
            "MISSING_FEATURE,family={:?},case_id={},permit={}",
            task.kind,
            task.case_id,
            format_option(permit),
        )?;
    }
    writeln!(
        out,
        "MISSING_FEATURE_SUMMARY,rows=16,abstained={missing_abstain}"
    )?;

    for favored in [LIGHT_PAGE, FULL_PAGE] {
        for provenance in [Dkv4Provenance::Missing, Dkv4Provenance::Conflict] {
            let outcome = run_dynamic(
                recurrent_initial(StateFixture::Stable)?,
                dkv_kernel()?,
                favored,
                provenance,
            )?;
            if outcome.depth != 0
                || outcome.reads != 0
                || matches!(outcome.decision, Dkv4Decision::StopCandidate(_))
            {
                return Err("provenance safety drift".into());
            }
        }
        let unverified = run_dynamic(
            recurrent_initial(StateFixture::Stable)?,
            dkv_kernel()?,
            favored,
            Dkv4Provenance::Unverified,
        )?;
        if matches!(unverified.decision, Dkv4Decision::StopCandidate(_)) {
            return Err("unverified route reached StopCandidate".into());
        }
        writeln!(
            out,
            "SAFETY,favored={},missing_reads=0,conflict_reads=0,unverified_depth={},unverified_decision={:?}",
            favored.0,
            unverified.depth,
            unverified.decision,
        )?;
    }

    out.push_str("PROMOTION,BLOCKED,heldout_family_applicability_development_panel\n");
    Ok(out)
}

fn format_option(value: Option<bool>) -> &'static str {
    match value {
        Some(true) => "true",
        Some(false) => "false",
        None => "unknown",
    }
}

fn format_execution(evidence: Option<&sml_core::candidate::TaskExecutionEvidence>) -> &'static str {
    match evidence {
        Some(e) if e.exact => "exact",
        Some(_) => "wrong",
        None => "abstain",
    }
}

#[allow(dead_code)]
fn main() -> R<()> {
    print!("{}", report()?);
    Ok(())
}
