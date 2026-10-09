//! Declared synthetic full-loop fixture. ANF controls evaluate literals directly;
//! native MOR, the qualified key projector and fixed-point memory are shared.
use sml_core::{
    delta_kv::{DeltaKvCounters, DeltaKvMemory, DELTA_KV_SCALE},
    delta_kv_dynamic::DeltaKvDynamicRekeyKernel,
    delta_kv_recurrent::DeltaKvRecurrentPolicy,
    delta_kv_semantic::{
        DeltaKvSemanticProjectionPolicy, DeltaKvSemanticProjector, DeltaKvSourceProvenance,
    },
    mor::{MixtureOfRouters, MorPolicy, PreviousRoute, RouterId},
    mor_registry::{MorRegistryPolicy, MorRouterRegistry, RouterSpecialization},
    pascal_vector_projection::Pvp6PopulationProfile,
    pvp6_executor::{Pvp6Bank, Pvp6Gate},
    pvp6_sbg::{SbgAnfPlan, SbgCoordinate, SbgField},
    pvp6_sbg_atlas::{SbgExpertAtlas, SbgExpertWork},
    pvp6_sbg_coupled::{CoupledSbgError, CoupledSbgLoop},
    pvp6_sbg_expert_loop::RoutedExpertSemantic,
    pvp6_sbg_experts::SbgAnfExpert,
    pvp6_sbg_kernel::{SbgAnfKernel, SbgAnfKernelError},
    pvp6_sbg_routing::{RoutedSemantic, SbgRoutingContext},
    semantic::session::SemanticSession,
    semantic::{
        run_semantic_recurrent, SemanticBitField, SemanticExpertSignature, SemanticHaltReason,
        SemanticMorBridge, SemanticMorTopK, SemanticRecurrentOutcome, SemanticRewriteStep,
        SemanticRewriteTrace, SemanticRewriter, SemanticRouterSignature, SemanticState,
        SemanticStateDelta,
    },
    HaltingPolicy, PageId,
};
use std::fmt::Write as _;
pub(super) type Kernel = CoupledSbgLoop<SbgAnfKernel<8, 64>, 8, 64, 8>;
#[derive(Debug, Clone, PartialEq, Eq)]
struct Scheduled {
    kernel: Kernel,
    update: bool,
}
impl SemanticRewriter<8> for Scheduled {
    type Error = CoupledSbgError<SbgAnfKernelError>;
    fn rewrite(
        &mut self,
        state: &SemanticState<8>,
        depth: u32,
    ) -> Result<SemanticRewriteStep<8>, Self::Error> {
        let mut candidate = self.kernel.clone();
        if self.update && depth == 3 {
            candidate.replace_expert(PageId(0), 0, bank(1, &[0, 2]))?;
        }
        let result = candidate.rewrite(state, depth)?;
        self.kernel = candidate;
        Ok(result)
    }
}
fn mix(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9e3779b97f4a7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d049bb133111eb);
    x ^ (x >> 31)
}
pub(super) fn initial(seed: u64) -> SemanticState<8> {
    SemanticState {
        value: SemanticBitField::from_words(vec![mix(seed) & 255]).unwrap(),
        meaning_mask: SemanticBitField::from_words(vec![mix(seed ^ 1) & 255]).unwrap(),
        ambiguity: SemanticBitField::from_words(vec![mix(seed ^ 2) & 255]).unwrap(),
        routing: SemanticBitField::from_words(vec![mix(seed ^ 3) & 255]).unwrap(),
    }
}
fn context() -> SbgRoutingContext<8> {
    let registry = MorRouterRegistry::new(
        vec![RouterSpecialization {
            router_id: RouterId(0),
            experts: vec![PageId(0), PageId(1)],
        }],
        MorRegistryPolicy {
            expert_count: 2,
            max_routers: 1,
            max_edges: 2,
            max_fanout: 2,
            require_full_coverage: true,
        },
    )
    .unwrap();
    let bridge = SemanticMorBridge::new(
        vec![SemanticRouterSignature {
            router_id: RouterId(0),
            signature: SemanticBitField::from_set_indices(&[0, 1, 2, 3]).unwrap(),
        }],
        vec![
            SemanticExpertSignature {
                page_id: PageId(0),
                signature: SemanticBitField::zeroed().unwrap(),
            },
            SemanticExpertSignature {
                page_id: PageId(1),
                signature: SemanticBitField::ones().unwrap(),
            },
        ],
        SemanticMorTopK::One,
    )
    .unwrap();
    SbgRoutingContext {
        bridge,
        registry,
        engine: MixtureOfRouters::new(MorPolicy::default()).unwrap(),
        runtime_hints: vec![],
    }
}
fn sources() -> Vec<SbgCoordinate> {
    [
        (SbgField::Value, 0),
        (SbgField::Value, 1),
        (SbgField::Meaning, 0),
        (SbgField::Meaning, 1),
        (SbgField::Ambiguity, 0),
        (SbgField::Routing, 0),
    ]
    .into_iter()
    .map(|(field, bit)| SbgCoordinate { field, bit })
    .collect()
}
fn bank(gates: usize, terms: &[u16]) -> Pvp6Bank<64> {
    Pvp6Bank::new(vec![Pvp6Gate::from_monomials(terms).unwrap(); gates]).unwrap()
}
fn page(id: u32) -> SbgAnfExpert<8, 64> {
    let plan = SbgAnfPlan::new(
        vec![sources()],
        vec![SbgCoordinate {
            field: SbgField::Value,
            bit: 0,
        }],
    )
    .unwrap();
    SbgAnfExpert::new(
        PageId(id),
        bank(1, if id == 0 { &[0, 1] } else { &[1, 2] }),
        plan,
        Pvp6PopulationProfile::AffineUniform,
    )
    .unwrap()
}
fn projector() -> DeltaKvSemanticProjector<8> {
    DeltaKvSemanticProjector::new(
        DeltaKvSemanticProjectionPolicy {
            semantic_weight: 1,
            mor_weight: 32,
        },
        17,
    )
    .unwrap()
}
fn provenance() -> DeltaKvSourceProvenance {
    DeltaKvSourceProvenance::new(77, vec![1000, 1001]).unwrap()
}
fn memory(seed: u64, target: i16) -> DeltaKvMemory {
    let mut memory = DeltaKvMemory::new(8, 1).unwrap();
    let ctx = context();
    let mut seed_counters = DeltaKvCounters::default();
    // Four declared seeding writes, before inference; no online learning claim.
    for route_bit in [false, true] {
        for value in [false, true] {
            let mut state = initial(seed);
            state.value.set(0, value).unwrap();
            state.value.set(1, route_bit).unwrap();
            state.routing = if route_bit {
                SemanticBitField::ones().unwrap()
            } else {
                SemanticBitField::zeroed().unwrap()
            };
            let route = ctx
                .bridge
                .route(
                    &state,
                    &ctx.registry,
                    &ctx.engine,
                    &[],
                    &PreviousRoute::default(),
                )
                .unwrap();
            let key = projector()
                .project(&state, &route.decision, Some(&provenance()))
                .unwrap();
            memory
                .update(
                    &key.key_q12,
                    &[if route_bit { -target } else { target }],
                    DELTA_KV_SCALE as u16,
                    &mut seed_counters,
                )
                .unwrap();
        }
    }
    memory
}
pub(super) fn kernel(seed: u64, target: i16, limit: u32) -> Kernel {
    let plan = SbgAnfPlan::new(
        vec![sources()],
        (0..8)
            .map(|bit| SbgCoordinate {
                field: SbgField::Routing,
                bit,
            })
            .collect(),
    )
    .unwrap();
    let inner = SbgAnfKernel::new(
        bank(8, &[2]),
        plan,
        Pvp6PopulationProfile::AffineUniform,
        limit,
    )
    .unwrap();
    let ctx = context();
    let first = ctx
        .bridge
        .route(
            &initial(seed),
            &ctx.registry,
            &ctx.engine,
            &[],
            &PreviousRoute::default(),
        )
        .unwrap();
    let routed = RoutedSemantic::new(inner, ctx, limit).unwrap();
    let experts = RoutedExpertSemantic::new(
        routed,
        SbgExpertAtlas::new(vec![page(0), page(1)], 2).unwrap(),
        SbgExpertWork {
            pages: u64::from(limit),
            gate_queries: u64::from(limit),
            retained_words: u64::from(limit) * 2,
        },
    )
    .unwrap();
    let memory = DeltaKvDynamicRekeyKernel::new(
        memory(seed, target),
        projector(),
        first.decision,
        Some(provenance()),
        vec![1],
        DeltaKvRecurrentPolicy {
            ambiguity_threshold_q12: 128,
        },
    )
    .unwrap();
    CoupledSbgLoop::new(experts, memory).unwrap()
}
// Independent ANF control: no SbgAnfPlan, PVP executor or kernel call.
fn literals(state: &SemanticState<8>) -> [bool; 6] {
    [
        state.value.get(0).unwrap(),
        state.value.get(1).unwrap(),
        state.meaning_mask.get(0).unwrap(),
        state.meaning_mask.get(1).unwrap(),
        state.ambiguity.get(0).unwrap(),
        state.routing.get(0).unwrap(),
    ]
}
fn address(state: &SemanticState<8>) -> usize {
    literals(state)
        .iter()
        .enumerate()
        .fold(0, |q, (i, &v)| q | (usize::from(v) << i))
}
fn literal_eval(state: &SemanticState<8>, terms: &[u16]) -> bool {
    let input = literals(state);
    terms.iter().fold(false, |parity, mask| {
        parity ^ (0..6).all(|i| mask & (1 << i) == 0 || input[i])
    })
}
fn state_text(state: &SemanticState<8>) -> String {
    format!(
        "{:02x}:{:02x}:{:02x}:{:02x}",
        state.value.words()[0],
        state.meaning_mask.words()[0],
        state.ambiguity.words()[0],
        state.routing.words()[0]
    )
}
#[derive(Default)]
struct Counts {
    cases: usize,
    cycles: usize,
    key_changes: usize,
    route_changes: usize,
}
fn case(
    report: &mut String,
    counts: &mut Counts,
    seed: u64,
    target: i16,
    limit: u32,
    update: bool,
    fixed: bool,
) {
    let id = counts.cases;
    let policy = HaltingPolicy {
        min_depth: if fixed { limit } else { 1 },
        max_depth: limit,
        stable_hamming_threshold: 0,
    };
    writeln!(
        report,
        "CASE,{id},seed={seed:016x},target={target},limit={limit},update={},fixed={}",
        u8::from(update),
        u8::from(fixed)
    )
    .unwrap();
    let initial = initial(seed);
    let base = Scheduled {
        kernel: kernel(seed, target, limit),
        update,
    };
    let mut actual = base.clone();
    let ctx = context();
    let raw_memory = memory(seed, target);
    let weights = raw_memory
        .weights_q12()
        .iter()
        .map(i16::to_string)
        .collect::<Vec<_>>()
        .join(":");
    writeln!(report, "MEMORY,{id},weights={weights}").unwrap();
    let mut read_counters = DeltaKvCounters::default();
    let mut previous = PreviousRoute::default();
    let mut prior_key = None;
    let mut state = initial.clone();
    let mut trace = vec![];
    let mut halt = SemanticHaltReason::MaxDepth;
    for depth in 1..=limit {
        let input = state.clone();
        let inner_query = address(&input);
        let route_value = literal_eval(&input, &[2]);
        for bit in 0..8 {
            state.routing.set(bit, route_value).unwrap();
        }
        let route = ctx
            .bridge
            .route(&state, &ctx.registry, &ctx.engine, &[], &previous)
            .unwrap();
        assert_eq!(route.decision.selected_experts.len(), 1);
        let selected = route.decision.selected_experts[0];
        let revision = u64::from(selected == PageId(0) && update && depth >= 3);
        let terms: &[u16] = match (selected.0, revision) {
            (0, 0) => &[0, 1],
            (0, 1) => &[0, 2],
            (1, 0) => &[1, 2],
            _ => panic!("unexpected selected expert"),
        };
        let expert_query = address(&state);
        let expert_value = literal_eval(&state, terms);
        state.value.set(0, expert_value).unwrap();
        let post_expert = state.clone();
        let projected = projector()
            .project(&state, &route.decision, Some(&provenance()))
            .unwrap();
        let read = raw_memory
            .read(&projected.key_q12, &mut read_counters)
            .unwrap();
        let blocked = read[0].unsigned_abs() <= 128;
        if blocked {
            state.ambiguity.set(1, true).unwrap();
        } else {
            state.value.set(1, read[0] > 0).unwrap();
            state.ambiguity.set(1, false).unwrap();
        }
        let result = actual.rewrite(&input, depth).unwrap();
        assert_eq!(result.state, state);
        assert_eq!(result.stability_blocked, blocked);
        let cycle = actual.kernel.cycles().last().unwrap();
        assert_eq!(cycle.post_expert, post_expert);
        assert_eq!(cycle.output, state);
        assert_eq!(cycle.selected, route.decision.selected_experts);
        assert_eq!(cycle.memory.key_q12, projected.key_q12);
        assert_eq!(cycle.memory.read_values_q12, read);
        assert_eq!(cycle.memory.projection, projected.counters);
        assert_eq!(cycle.provenance, Some(provenance()));
        let key_changed = prior_key
            .as_ref()
            .is_some_and(|key| key != &projected.key_q12);
        assert_eq!(cycle.memory.key_changed, key_changed);
        let native_route = actual.kernel.experts().routed().cycles().last().unwrap();
        assert_eq!(native_route.previous, previous);
        assert_eq!(native_route.route, route);
        let body = &actual.kernel.experts().cycles().last().unwrap().pages[0];
        assert_eq!(body.page, selected);
        assert_eq!(body.revision, revision);
        assert_eq!(body.queries, vec![expert_query]);
        assert_eq!(body.outputs, vec![u64::from(expert_value)]);
        let inner = actual
            .kernel
            .experts()
            .routed()
            .inner()
            .cycles()
            .last()
            .unwrap();
        assert_eq!(inner.queries, vec![inner_query]);
        assert_eq!(inner.outputs, vec![if route_value { 255 } else { 0 }]);
        assert_eq!(actual.kernel.memory().counters(), read_counters);
        assert_eq!(actual.kernel.memory_usage().read_macs, u64::from(depth) * 8);
        let delta = SemanticStateDelta::between(&input, &state);
        assert_eq!(cycle.delta, delta);
        trace.push(SemanticRewriteTrace {
            depth,
            delta,
            stability_blocked: blocked,
        });
        let key = projected
            .key_q12
            .iter()
            .map(i16::to_string)
            .collect::<Vec<_>>()
            .join(":");
        writeln!(report,"FRAME,{id},{depth},page={},revision={revision},input={},expert={},output={},key={key},read={},blocked={},read_macs=8,projected_coordinates=32",selected.0,state_text(&input),state_text(&post_expert),state_text(&state),read[0],u8::from(blocked)).unwrap();
        counts.cycles += 1;
        counts.key_changes += usize::from(key_changed);
        counts.route_changes += usize::from(previous.primary.is_some_and(|page| page != selected));
        previous = PreviousRoute {
            primary: Some(selected),
            selected: route.decision.selected_experts,
        };
        prior_key = Some(projected.key_q12);
        if depth >= policy.min_depth && delta.is_zero() && !blocked {
            halt = SemanticHaltReason::Stable;
            break;
        }
    }
    let expected = SemanticRecurrentOutcome {
        state,
        depth: trace.len() as u32,
        halt_reason: halt,
        trace,
    };
    let mut native = base.clone();
    let outcome = run_semantic_recurrent(initial.clone(), &mut native, policy).unwrap();
    assert_eq!(outcome, expected);
    assert_eq!(native, actual);
    let mut session = SemanticSession::new(initial, base, policy).unwrap();
    session.advance((limit / 2).max(1)).unwrap();
    let expected_kernel = session.inner().clone();
    let mut resumed = session
        .checkpoint()
        .resume_checked(&expected_kernel)
        .unwrap();
    resumed.advance(u32::MAX).unwrap();
    assert_eq!(resumed.outcome().unwrap(), outcome);
    assert_eq!(resumed.inner(), &native);
    counts.cases += 1;
}
/// The complete report is returned only after all exact checks succeed.
pub fn qualification_report() -> String {
    let mut report=String::from("A4_CLOSED_LOOP,schema=1,S=8,D=64,Q=8,source_id=77,raw=1000:1001,seed_updates=4,scope=synthetic_correctness\n");
    let mut counts = Counts::default();
    for seed in [0, 1, 0x4144434c, u64::MAX] {
        for target in [-2000, 0, 2000] {
            for limit in [1, 2, 4, 8] {
                for update in [false, true] {
                    case(&mut report, &mut counts, seed, target, limit, update, true);
                }
            }
        }
    }
    assert_eq!((counts.cases, counts.cycles), (96, 360));
    for seed in [0, 1, 0x4144434c, u64::MAX] {
        for target in [-2000, 0, 2000] {
            case(&mut report, &mut counts, seed, target, 8, false, false);
        }
    }
    assert_eq!(counts.cases, 108);
    writeln!(report,"A4_CLOSED_SUMMARY,cases={},cycles={},key_changes={},route_changes={},full_pipeline_resumes=108,mismatches=0",counts.cases,counts.cycles,counts.key_changes,counts.route_changes).unwrap();
    report.push_str("PROMOTION,BLOCKED,no_learning_speed_or_persistent_checkpoint_claim\n");
    report
}
