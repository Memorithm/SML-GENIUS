//! DKV-4h learned expert-applicability development panel.
//!
//! Protocol: docs/SML-DKV-4H-LEARNED-APPLICABILITY-PREREGISTRATION.md.

use std::{error::Error, fmt::Write as _};

use sml_core::{
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
    semantic::{
        SemanticBitField, SemanticRewriteTrace, SemanticRewriter, SemanticState, SemanticStateDelta,
    },
    HaltingPolicy, PageId,
};
use sml_decision::{
    feature_search::{
        search_feature_pair_expert, FeatureExpertProposal, FeatureSearchOutcome,
        FeatureSearchPolicy, FixedFeatureControl,
    },
    semantic::{
        LabeledSemanticInput, ObservedFields, ProjectionBudget, SemanticFeature, SemanticInput,
        SemanticProjection,
    },
    TruthTable2,
};

type R<T> = Result<T, Box<dyn Error>>;
const BITS: usize = 64;
const SCHEMA_ID: u64 = 84;
const TRAIN_COUNT: usize = 64;
const VALIDATION_COUNT: usize = 64;
const TEST_METADATA_COUNT: usize = 64;

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

#[derive(Debug, Clone, Copy)]
struct Latent {
    a: bool,
    b: bool,
    c: bool,
    d: bool,
}

fn latent(index: usize) -> Latent {
    let pattern = index % 16;
    Latent {
        a: pattern & 1 != 0,
        b: pattern & 2 != 0,
        c: pattern & 4 != 0,
        d: pattern & 8 != 0,
    }
}

fn candidate_projection() -> SemanticProjection<BITS> {
    SemanticProjection::new(
        SCHEMA_ID,
        1,
        vec![
            SemanticFeature::ContextValue(0),
            SemanticFeature::ContextValue(1),
            SemanticFeature::ContextValue(2),
            SemanticFeature::ContextValue(7),
            SemanticFeature::ContextValue(9),
            SemanticFeature::ContextValue(13),
            SemanticFeature::ContextValue(21),
            SemanticFeature::ContextValue(34),
        ],
    )
    .unwrap()
}

fn generated_state(split: u8, index: usize) -> SemanticState<BITS> {
    let bits = latent(index);
    let repeat = index / 16;
    let repeat0 = repeat & 1 != 0;
    let repeat1 = repeat & 2 != 0;
    let split0 = split & 1 != 0;
    let split1 = split & 2 != 0;

    let mut state = SemanticState::<BITS>::zeroed().unwrap();
    state.meaning_mask = SemanticBitField::ones().unwrap();
    for (coordinate, value) in [
        (0, bits.c ^ split0),
        (1, bits.d ^ split1),
        (2, bits.c ^ bits.d ^ repeat0),
        (7, bits.a),
        (9, bits.a ^ bits.c ^ repeat1),
        (13, bits.b),
        (21, bits.b ^ bits.d ^ split0 ^ repeat0),
        (34, (bits.c == bits.d) ^ split1 ^ repeat1),
    ] {
        state.value.set(coordinate, value).unwrap();
    }
    state
}

fn split_states(split: u8) -> Vec<SemanticState<BITS>> {
    (0..64).map(|index| generated_state(split, index)).collect()
}

fn applicable(page: PageId, bits: Latent) -> R<bool> {
    match page {
        PageId(0) => Ok(bits.a == bits.b),
        PageId(1) => Ok(bits.a != bits.b),
        _ => Err("unexpected page in applicability label".into()),
    }
}

fn labeled<'a>(
    states: &'a [SemanticState<BITS>],
    mask: &'a SemanticBitField<BITS>,
    id_base: u64,
    page: PageId,
) -> R<Vec<LabeledSemanticInput<'a, BITS>>> {
    states
        .iter()
        .enumerate()
        .map(|(index, state)| {
            Ok(LabeledSemanticInput {
                input: SemanticInput {
                    record_id: id_base + index as u64,
                    schema_id: SCHEMA_ID,
                    projection_version: 1,
                    state,
                    observed: ObservedFields {
                        value: mask,
                        meaning_mask: mask,
                        ambiguity: mask,
                        routing: mask,
                    },
                },
                target: applicable(page, latent(index))?,
            })
        })
        .collect()
}

fn baseline() -> FixedFeatureControl {
    FixedFeatureControl {
        feature_slots: [0, 1],
        table: TruthTable2::AND,
    }
}

#[derive(Debug, Clone)]
struct FrozenGate {
    page: PageId,
    proposal: FeatureExpertProposal,
    projection: SemanticProjection<BITS>,
}

impl FrozenGate {
    fn from_proposal(page: PageId, proposal: FeatureExpertProposal) -> R<Self> {
        Ok(Self {
            page,
            proposal,
            projection: SemanticProjection::new(
                SCHEMA_ID,
                100 + page.0 as u64,
                vec![proposal.left_feature, proposal.right_feature],
            )?,
        })
    }

    fn evaluate(
        &self,
        record_id: u64,
        state: &SemanticState<BITS>,
        observed: ObservedFields<'_, BITS>,
    ) -> R<Option<bool>> {
        let (record, _) = self.projection.project(SemanticInput {
            record_id,
            schema_id: SCHEMA_ID,
            projection_version: self.projection.version(),
            state,
            observed,
        })?;
        if record.known & 0b11 != 0b11 {
            return Ok(None);
        }
        let left = record.values & 1 != 0;
        let right = record.values & 2 != 0;
        Ok(Some(self.proposal.table.evaluate(left, right)))
    }
}

fn gate_for<'a>(page: PageId, page0: &'a FrozenGate, page1: &'a FrozenGate) -> R<&'a FrozenGate> {
    match page {
        PageId(0) => Ok(page0),
        PageId(1) => Ok(page1),
        _ => Err("unexpected routed page".into()),
    }
}

fn feature_coordinate(feature: SemanticFeature) -> usize {
    match feature {
        SemanticFeature::ContextValue(index)
        | SemanticFeature::Meaning(index)
        | SemanticFeature::Ambiguity(index)
        | SemanticFeature::Routing(index) => index,
    }
}

fn normal_observed<'a>(mask: &'a SemanticBitField<BITS>) -> ObservedFields<'a, BITS> {
    ObservedFields {
        value: mask,
        meaning_mask: mask,
        ambiguity: mask,
        routing: mask,
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

fn recurrence_state(
    metadata: &SemanticState<BITS>,
    fixture: StateFixture,
) -> R<SemanticState<BITS>> {
    let mut state = metadata.clone();
    state
        .value
        .set(0, matches!(fixture, StateFixture::Stable))?;
    Ok(state)
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
            Dkv4Provenance::Missing => {
                Dkv4Decision::Abstain(sml_core::delta_kv_adaptive::Dkv4Reason::MissingEvidence)
            }
            Dkv4Provenance::Conflict => {
                Dkv4Decision::Abstain(sml_core::delta_kv_adaptive::Dkv4Reason::ConflictingEvidence)
            }
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
            .ok_or("missing dynamic DKV cycle evidence")?;
        let mor = mor_decision(favored, &previous)?;
        let signals = Dkv4Signals::from_dkv2b(&trace, dkv, &mor, provenance)?;
        let decision = controller.decide(signals, Dkv4RefinementCapabilities::dynamic_mor())?;
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

    Err("dynamic DKV-4h loop exhausted without terminal decision".into())
}

fn expected_page(bits: Latent) -> PageId {
    if bits.a == bits.b {
        PageId(0)
    } else {
        PageId(1)
    }
}

fn other_page(page: PageId) -> PageId {
    if page == PageId(0) {
        PageId(1)
    } else {
        PageId(0)
    }
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
    cycles: u32,
    reads: u64,
}

impl Aggregate {
    fn observe(&mut self, answer: Option<bool>, oracle: bool, cycles: u32, reads: u64) {
        self.rows += 1;
        self.cycles += cycles;
        self.reads += reads;
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

fn emit_summary(out: &mut String, name: &str, aggregate: Aggregate) -> R<()> {
    let coverage = ppm(aggregate.answered, aggregate.rows).unwrap_or(0);
    let risk = ppm(aggregate.errors, aggregate.answered);
    writeln!(
        out,
        "SUMMARY,{name},rows={},answered={},coverage_ppm={coverage},correct_answered={},errors={},risk_defined={},risk_ppm={},cycles={},reads={}",
        aggregate.rows,
        aggregate.answered,
        aggregate.correct_answered,
        aggregate.errors,
        risk.is_some(),
        risk.unwrap_or(0),
        aggregate.cycles,
        aggregate.reads,
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

fn report_fit(
    out: &mut String,
    page: PageId,
    report: &sml_decision::feature_search::FeatureSearchReport,
) -> R<()> {
    writeln!(
        out,
        "FIT,page={},outcome={:?},left_slot={},right_slot={},left_feature={:?},right_feature={:?},table_code={},train_samples={},validation_samples={},candidate_features={},candidate_pairs={},selection_bound={},selection_executed={},baseline_train_errors={},selected_train_errors={},baseline_validation_errors={},selected_validation_errors={},validation_missing={},validation_coverage={}",
        page.0,
        report.outcome,
        report.selected.left_slot,
        report.selected.right_slot,
        report.selected.left_feature,
        report.selected.right_feature,
        report.selected.table.code(),
        report.training_samples,
        report.validation_samples,
        report.candidate_features,
        report.candidate_pairs,
        report.selection_evaluations_bound,
        report.selection_evaluations_executed,
        report.baseline_training_errors,
        report.selected_training_errors,
        report.baseline_validation_errors,
        report.selected_validation_errors,
        report.selected_validation_missing,
        report.selected_validation_coverage,
    )?;
    Ok(())
}

pub fn report() -> R<String> {
    let mut out = String::from(
        "PROTOCOL,SML-DKV-4H-LEARNED-APPLICABILITY-PREREGISTRATION\nSTATUS,DEVELOPMENT_NOT_MODEL_QUALIFICATION\n",
    );

    let projection = candidate_projection();
    let mask = SemanticBitField::<BITS>::ones()?;
    let train_states = split_states(0);
    let validation_states = split_states(1);
    let test_states = split_states(2);

    let train0 = labeled(&train_states, &mask, 10_000, PageId(0))?;
    let validation0 = labeled(&validation_states, &mask, 20_000, PageId(0))?;
    let train1 = labeled(&train_states, &mask, 30_000, PageId(1))?;
    let validation1 = labeled(&validation_states, &mask, 40_000, PageId(1))?;

    if train0.len() != TRAIN_COUNT
        || validation0.len() != VALIDATION_COUNT
        || test_states.len() != TEST_METADATA_COUNT
    {
        return Err("split cardinality drift".into());
    }

    let page0 = search_feature_pair_expert(
        &projection,
        &train0,
        &validation0,
        baseline(),
        FeatureSearchPolicy::default(),
        ProjectionBudget::default(),
    )?;
    let page1 = search_feature_pair_expert(
        &projection,
        &train1,
        &validation1,
        baseline(),
        FeatureSearchPolicy::default(),
        ProjectionBudget::default(),
    )?;

    writeln!(
        out,
        "SPLIT,train_records={},validation_records={},test_metadata_records={},train_id_ranges=10000-10063|30000-30063,validation_id_ranges=20000-20063|40000-40063,test_id_range=50000-50127",
        TRAIN_COUNT,
        VALIDATION_COUNT,
        TEST_METADATA_COUNT,
    )?;
    report_fit(&mut out, PageId(0), &page0)?;
    report_fit(&mut out, PageId(1), &page1)?;

    if !matches!(page0.outcome, FeatureSearchOutcome::Accepted)
        || !matches!(page1.outcome, FeatureSearchOutcome::Accepted)
    {
        out.push_str("DOWNSTREAM,SKIPPED,fit_not_accepted\n");
        out.push_str("PROMOTION,BLOCKED,learned_applicability_fit_rejected\n");
        return Ok(out);
    }

    let gate0 = FrozenGate::from_proposal(PageId(0), page0.selected)?;
    let gate1 = FrozenGate::from_proposal(PageId(1), page1.selected)?;
    let normal_mask = SemanticBitField::<BITS>::ones()?;

    let mut dynamic_unguarded = Aggregate::default();
    let mut dynamic_learned = Aggregate::default();
    let mut first_primary_learned = Aggregate::default();
    let mut universal = Aggregate::default();
    let mut learned_aligned = Aggregate::default();
    let mut learned_misaligned = Aggregate::default();

    let mut row_id = 50_000_u64;
    for (metadata_index, metadata) in test_states.iter().enumerate() {
        let bits = latent(metadata_index);
        let expected = expected_page(bits);
        let favored = if bits.c {
            expected
        } else {
            other_page(expected)
        };
        let aligned = favored == expected;
        let oracle = expert_output(expected)?;
        let (first_mor, second_mor) = validate_geometry(favored)?;

        for fixture in StateFixture::ALL {
            let initial = recurrence_state(metadata, fixture)?;
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
            let (dynamic_page, dynamic_answer) = dynamic_proposal(&dynamic)?
                .ok_or("dynamic learned-applicability row did not produce singleton proposal")?;
            let gate = gate_for(dynamic_page, &gate0, &gate1)?;
            let permit = gate.evaluate(row_id, &dynamic.state, normal_observed(&normal_mask))?;
            let learned_answer = if permit == Some(true) {
                Some(dynamic_answer)
            } else {
                None
            };

            let first_page = primary(&first_mor)?;
            let first_gate = gate_for(first_page, &gate0, &gate1)?;
            let first_permit = first_gate.evaluate(
                row_id + 1_000_000,
                &stability.adaptive.state,
                normal_observed(&normal_mask),
            )?;
            let first_answer = if first_permit == Some(true) {
                Some(routed_answer(&stability.adaptive.state, first_page)?)
            } else {
                None
            };

            dynamic_unguarded.observe(Some(dynamic_answer), oracle, dynamic.depth, dynamic.reads);
            dynamic_learned.observe(learned_answer, oracle, dynamic.depth, dynamic.reads);
            first_primary_learned.observe(
                first_answer,
                oracle,
                stability.adaptive.depth,
                stability.adaptive_cycles.len() as u64,
            );
            universal.observe(None, oracle, 0, 0);
            if aligned {
                learned_aligned.observe(learned_answer, oracle, dynamic.depth, dynamic.reads);
            } else {
                learned_misaligned.observe(learned_answer, oracle, dynamic.depth, dynamic.reads);
            }

            writeln!(
                out,
                "ROW,id={row_id},metadata_index={metadata_index},state={},a={},b={},c={},d={},expected_page={},favored_page={},aligned={},oracle={},first_margin={},first_selected={},second_margin={},second_selected={},dynamic_depth={},dynamic_page={},dynamic_unguarded={},learned_permit={},dynamic_learned={},first_primary_depth={},first_primary_page={},first_permit={},first_primary_learned={},final_v={}",
                fixture.name(),
                bits.a,
                bits.b,
                bits.c,
                bits.d,
                expected.0,
                favored.0,
                aligned,
                oracle,
                route_margin(&first_mor),
                first_mor.selected_experts.len(),
                route_margin(&second_mor),
                second_mor.selected_experts.len(),
                dynamic.depth,
                dynamic_page.0,
                dynamic_answer,
                format_option_bool(permit),
                format_answer(learned_answer),
                stability.adaptive.depth,
                first_page.0,
                format_option_bool(first_permit),
                format_answer(first_answer),
                dynamic.state.value.get(0)?,
            )?;
            row_id += 1;
        }
    }

    emit_summary(&mut out, "DYNAMIC_UNGUARDED", dynamic_unguarded)?;
    emit_summary(&mut out, "DYNAMIC_LEARNED", dynamic_learned)?;
    emit_summary(&mut out, "FIRST_PRIMARY_LEARNED", first_primary_learned)?;
    emit_summary(&mut out, "UNIVERSAL_ABSTAIN", universal)?;
    emit_summary(&mut out, "LEARNED_ALIGNED", learned_aligned)?;
    emit_summary(&mut out, "LEARNED_MISALIGNED", learned_misaligned)?;

    let mut missing_count = 0_u32;
    for metadata_index in 0..16 {
        let metadata = &test_states[metadata_index];
        let bits = latent(metadata_index);
        let expected = expected_page(bits);
        let favored = if bits.c {
            expected
        } else {
            other_page(expected)
        };
        let (_, second_mor) = validate_geometry(favored)?;
        let page = second_mor.selected_experts[0];
        let gate = gate_for(page, &gate0, &gate1)?;

        let mut value_observed = SemanticBitField::<BITS>::ones()?;
        let meaning_observed = SemanticBitField::<BITS>::ones()?;
        let ambiguity_observed = SemanticBitField::<BITS>::ones()?;
        let routing_observed = SemanticBitField::<BITS>::ones()?;
        let missing_coordinate = feature_coordinate(gate.proposal.left_feature);
        value_observed.set(missing_coordinate, false)?;

        let permit = gate.evaluate(
            2_000_000 + metadata_index as u64,
            metadata,
            ObservedFields {
                value: &value_observed,
                meaning_mask: &meaning_observed,
                ambiguity: &ambiguity_observed,
                routing: &routing_observed,
            },
        )?;
        missing_count += u32::from(permit.is_none());
        writeln!(
            out,
            "MISSING_METADATA,index={metadata_index},page={},missing_coordinate={},permit={}",
            page.0,
            missing_coordinate,
            format_option_bool(permit),
        )?;
    }
    writeln!(
        out,
        "MISSING_METADATA_SUMMARY,rows=16,abstained={missing_count}"
    )?;

    for favored in [PageId(0), PageId(1)] {
        for provenance in [Dkv4Provenance::Missing, Dkv4Provenance::Conflict] {
            let outcome = run_dynamic(
                recurrence_state(&test_states[0], StateFixture::Stable)?,
                dkv_kernel()?,
                favored,
                provenance,
            )?;
            if outcome.depth != 0
                || outcome.reads != 0
                || matches!(outcome.decision, Dkv4Decision::StopCandidate(_))
            {
                return Err("fail-closed provenance drift".into());
            }
        }
        let unverified = run_dynamic(
            recurrence_state(&test_states[0], StateFixture::Stable)?,
            dkv_kernel()?,
            favored,
            Dkv4Provenance::Unverified,
        )?;
        if matches!(unverified.decision, Dkv4Decision::StopCandidate(_)) {
            return Err("unverified path reached StopCandidate".into());
        }
        writeln!(
            out,
            "SAFETY,favored_page={},missing_reads=0,conflict_reads=0,unverified_depth={},unverified_decision={:?}",
            favored.0,
            unverified.depth,
            unverified.decision,
        )?;
    }

    out.push_str("PROMOTION,BLOCKED,learned_applicability_development_panel\n");
    Ok(out)
}

fn format_option_bool(value: Option<bool>) -> &'static str {
    match value {
        Some(true) => "true",
        Some(false) => "false",
        None => "unknown",
    }
}

#[allow(dead_code)]
fn main() -> R<()> {
    print!("{}", report()?);
    Ok(())
}
