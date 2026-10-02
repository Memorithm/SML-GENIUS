//! Independent DKV-3a-R1-A DEVELOPMENT harness. Not a production context model.
//! Protocol: docs/SML-DKV-3A-R1-A-PREREGISTRATION.md.
//! Uses actual native MOR, DKV-2a and DKV-2b. Raw authority never repairs a latent score.
//! Shared shadow/source instrumentation is reported, never hidden as free inference.

use std::{error::Error, fmt::Write as _};

use sml_core::{
    context_ledger::{ContextSegmentRef, ContextStreamId, RawContextLedger},
    delta_kv::{DeltaKvCounters, DeltaKvMemory, DELTA_KV_ONE},
    delta_kv_recurrent::{DeltaKvRecurrentKernel, DeltaKvRecurrentPolicy},
    delta_kv_semantic::{
        DeltaKvSemanticKeyEvidence, DeltaKvSemanticProjectionPolicy, DeltaKvSemanticProjector,
        DeltaKvSourceProvenance,
    },
    memory::{BoundedAssociativeMemory, ReadOutcome, ReplacementPolicy, WriteOutcome},
    mor::{MixtureOfRouters, MorPolicy, PreviousRoute, RouterId},
    mor_registry::{MorRegistryPolicy, MorRouterRegistry, RouterSpecialization},
    semantic::{
        run_semantic_recurrent, SemanticBitField, SemanticExpertSignature, SemanticMorBridge,
        SemanticMorTopK, SemanticRecurrentOutcome, SemanticRewriteStep, SemanticRewriter,
        SemanticRouterSignature, SemanticState,
    },
    BooleanWorkspace, ExecutionCounters, HaltingPolicy, PageId,
};
use sml_store::{
    delta_kv_context::{ContextError, ContextLimits, DeltaKvContextMemory, VerificationLimits},
    raw_context::{
        build_raw_segment_artifact, parse_raw_segment_artifact, sha256_raw_bytes,
        RawContextStoreLimits,
    },
};

type R<T> = Result<T, Box<dyn Error>>;
const BITS: usize = 64;
const SCRATCH: [usize; 2] = [48, 49];
const OUTPUT: [usize; 2] = [50, 51];
const MAX_DEPTH: u32 = 4;
const PROTOCOL_COMMIT: &str = "d7bf16fb879dc9f822522a2c1d7173fb2f8d42e4";
const BASE_COMMIT: &str = "9a8828ba3f009822f040b01d6e1dca05cc85489e";

fn boxed(e: impl std::fmt::Debug) -> Box<dyn Error> {
    format!("{e:?}").into()
}
fn ensure(condition: bool, message: &str) -> R<()> {
    if condition {
        Ok(())
    } else {
        Err(message.into())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Family {
    Isolated,
    Old,
    Recent,
    Overwrite,
    Conflict,
    Missing,
    NearOld,
    NearRecent,
}
impl Family {
    const ALL: [Self; 8] = [
        Self::Isolated,
        Self::Old,
        Self::Recent,
        Self::Overwrite,
        Self::Conflict,
        Self::Missing,
        Self::NearOld,
        Self::NearRecent,
    ];
    fn name(self) -> &'static str {
        match self {
            Self::Isolated => "isolated",
            Self::Old => "old",
            Self::Recent => "recent",
            Self::Overwrite => "overwrite",
            Self::Conflict => "conflict",
            Self::Missing => "missing",
            Self::NearOld => "near_old",
            Self::NearRecent => "near_recent",
        }
    }
    fn near(self) -> bool {
        matches!(self, Self::NearOld | Self::NearRecent)
    }
}

#[derive(Clone, Debug)]
struct Fact {
    id: u64,
    subject: u64,
    group: u32,
    bits: [bool; 2],
    prior: Option<u64>,
}
impl Fact {
    fn bytes(&self) -> Vec<u8> {
        format!(
            "{}\t{}\t{}\t{}\t{}\t{}",
            self.id,
            self.subject,
            self.group,
            u8::from(self.bits[0]),
            u8::from(self.bits[1]),
            self.prior
                .map_or_else(|| "-".to_string(), |id| id.to_string())
        )
        .into_bytes()
    }
}

fn parse(bytes: &[u8], reference: &ContextSegmentRef) -> R<Fact> {
    let fields: Vec<_> = std::str::from_utf8(bytes)?.split('\t').collect();
    ensure(fields.len() == 6, "raw grammar arity")?;
    let bit = |s: &str| -> R<bool> {
        match s {
            "0" => Ok(false),
            "1" => Ok(true),
            _ => Err("noncanonical Boolean".into()),
        }
    };
    let fact = Fact {
        id: fields[0].parse()?,
        subject: fields[1].parse()?,
        group: fields[2].parse()?,
        bits: [bit(fields[3])?, bit(fields[4])?],
        prior: if fields[5] == "-" {
            None
        } else {
            Some(fields[5].parse()?)
        },
    };
    ensure(
        fact.id != 0 && fact.subject != 0 && fact.group <= 1,
        "raw identity",
    )?;
    ensure(
        reference.semantic_record_ids() == [fact.id],
        "record/source binding",
    )?;
    Ok(fact)
}

fn fixture(family: Family, seed: u64, group: u32) -> (Vec<Fact>, Option<[bool; 2]>) {
    let n = (seed + u64::from(group)) % 4;
    let target = [n & 1 != 0, n & 2 != 0];
    let opposite = [!target[0], !target[1]];
    let mut raw: Vec<(u64, [bool; 2], Option<u64>)> = Vec::new();
    let distractors = || (2..=8).map(|subject| (subject, opposite, None));
    match family {
        Family::Isolated => raw.push((1, target, None)),
        Family::Old | Family::NearOld => {
            raw.push((1, target, None));
            raw.extend(distractors());
        }
        Family::Recent | Family::NearRecent => {
            raw.extend(distractors());
            raw.push((1, target, None));
        }
        Family::Overwrite => {
            raw.push((1, opposite, None));
            raw.extend(distractors());
            raw.push((1, target, Some(1)));
        }
        Family::Conflict => {
            raw.push((1, opposite, None));
            raw.push((1, target, None));
        }
        Family::Missing => raw.extend(distractors()),
    }
    let facts = raw
        .into_iter()
        .enumerate()
        .map(|(i, (subject, bits, prior))| Fact {
            id: i as u64 + 1,
            subject,
            group,
            bits,
            prior,
        })
        .collect();
    let expected = if matches!(family, Family::Conflict | Family::Missing) {
        None
    } else {
        Some(target)
    };
    (facts, expected)
}

fn mix(mut x: u64) -> u64 {
    x ^= x >> 30;
    x = x.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}
fn signature(group: u32) -> R<SemanticBitField<BITS>> {
    ensure(group <= 1, "unknown routing group")?;
    SemanticBitField::from_set_indices(
        &(0..BITS)
            .filter(|i| *i % 2 == group as usize)
            .collect::<Vec<_>>(),
    )
    .map_err(boxed)
}

/// Crucial information boundary: NO source payload, label or expected answer argument.
fn address(subject: u64, group: u32, near: bool) -> R<SemanticState<BITS>> {
    ensure((1..=8).contains(&subject), "undeclared subject")?;
    let mut state = SemanticState::zeroed().map_err(boxed)?;
    let identity = mix(if near { 1 } else { subject });
    for i in 0..48 {
        state
            .value
            .set(i, identity & (1 << i) != 0)
            .map_err(boxed)?;
        state.meaning_mask.set(i, true).map_err(boxed)?;
    }
    if near && subject != 1 {
        for i in [(subject as usize * 7) % 48, (subject as usize * 11) % 48] {
            let old = state.value.get(i).map_err(boxed)?;
            state.value.set(i, !old).map_err(boxed)?;
        }
    }
    for i in SCRATCH.into_iter().chain(OUTPUT) {
        state.ambiguity.set(i, true).map_err(boxed)?;
    }
    state.routing = signature(group)?;
    Ok(state)
}

struct Router {
    bridge: SemanticMorBridge<BITS>,
    registry: MorRouterRegistry,
    engine: MixtureOfRouters,
}
impl Router {
    fn new() -> R<Self> {
        let registry = MorRouterRegistry::new(
            (0..2)
                .map(|id| RouterSpecialization {
                    router_id: RouterId(id),
                    experts: vec![PageId(id)],
                })
                .collect(),
            MorRegistryPolicy {
                expert_count: 2,
                max_routers: 2,
                max_edges: 2,
                max_fanout: 1,
                require_full_coverage: true,
            },
        )
        .map_err(boxed)?;
        let bridge = SemanticMorBridge::new(
            vec![
                SemanticRouterSignature {
                    router_id: RouterId(0),
                    signature: signature(0)?,
                },
                SemanticRouterSignature {
                    router_id: RouterId(1),
                    signature: signature(1)?,
                },
            ],
            vec![
                SemanticExpertSignature {
                    page_id: PageId(0),
                    signature: signature(0)?,
                },
                SemanticExpertSignature {
                    page_id: PageId(1),
                    signature: signature(1)?,
                },
            ],
            SemanticMorTopK::One,
        )
        .map_err(boxed)?;
        let engine = MixtureOfRouters::new(MorPolicy {
            max_experts: 1,
            ..MorPolicy::default()
        })
        .map_err(boxed)?;
        Ok(Self {
            bridge,
            registry,
            engine,
        })
    }
    fn project<const K: usize>(
        &self,
        state: &SemanticState<BITS>,
        seed: u64,
        provenance: Option<&DeltaKvSourceProvenance>,
    ) -> R<DeltaKvSemanticKeyEvidence> {
        let route = self
            .bridge
            .route(
                state,
                &self.registry,
                &self.engine,
                &[],
                &PreviousRoute::default(),
            )
            .map_err(boxed)?;
        ensure(
            route.decision.selected_experts.len() == 1,
            "MOR must admit exactly one expert",
        )?;
        DeltaKvSemanticProjector::<K>::new(DeltaKvSemanticProjectionPolicy::default(), seed)
            .map_err(boxed)?
            .project(state, &route.decision, provenance)
            .map_err(boxed)
    }
}

fn q12(bits: [bool; 2]) -> [i16; 2] {
    bits.map(|b| if b { DELTA_KV_ONE } else { -DELTA_KV_ONE })
}
fn limits() -> ContextLimits {
    ContextLimits {
        max_writes: 16,
        max_sources_per_write: 4,
        max_source_links: 32,
        max_ledger_segments: 16,
        max_ledger_semantic_links: 32,
        max_element_bytes: 1 << 20,
        max_staging_bytes: 1 << 20,
    }
}
fn raw_limits() -> RawContextStoreLimits {
    RawContextStoreLimits {
        max_file_bytes: 1 << 16,
        max_raw_bytes: 1 << 16,
        max_semantic_links: 32,
    }
}
fn direct_id(subject: u64, group: u32) -> u64 {
    2 * subject + u64::from(group)
}

struct World {
    initial: SemanticState<BITS>,
    query: DeltaKvSemanticKeyEvidence,
    memory: DeltaKvMemory,
    source: DeltaKvContextMemory,
    direct: BoundedAssociativeMemory,
    ledger: RawContextLedger,
    artifacts: Vec<Vec<u8>>,
    projected: Vec<(u64, Vec<i16>)>,
    setup_numeric: DeltaKvCounters,
    setup_direct: ExecutionCounters,
    projection_coordinates: u64,
    expected: Option<[bool; 2]>,
}
impl World {
    fn build<const K: usize>(family: Family, seed: u64, group: u32) -> R<Self> {
        let (facts, expected) = fixture(family, seed, group);
        ensure(facts.len() <= 16, "write budget")?;
        let router = Router::new()?;
        let stream = ContextStreamId::new(1).map_err(boxed)?;
        let mut ledger = RawContextLedger::new(stream);
        let mut source = DeltaKvContextMemory::new(stream, K, 2, limits())?;
        let mut memory = DeltaKvMemory::new(K, 2)?;
        let mut direct = BoundedAssociativeMemory::new(32, 2, ReplacementPolicy::ReplaceExisting)
            .map_err(boxed)?;
        let mut numeric = DeltaKvCounters::default();
        let mut direct_counters = ExecutionCounters::default();
        let mut artifacts = Vec::new();
        let mut projected = Vec::new();
        let mut projection_coordinates = 0;
        for raw_fact in facts {
            let bytes = raw_fact.bytes();
            let id = sml_core::context_ledger::ContextSegmentId::new(stream, raw_fact.id)
                .map_err(boxed)?;
            // This panel does not tokenize language: token counts intentionally remain zero.
            let reference = ContextSegmentRef::new(
                id,
                ledger.stats().raw_bytes,
                bytes.len() as u64,
                0,
                0,
                ledger.stats().segments,
                sha256_raw_bytes(&bytes)?,
                vec![raw_fact.id],
            )
            .map_err(boxed)?;
            let artifact = build_raw_segment_artifact(&reference, &bytes)?;
            let loaded = parse_raw_segment_artifact(&artifact, raw_limits())?;
            let fact = parse(&loaded.raw_bytes, &loaded.reference)?;
            ledger.append(reference).map_err(boxed)?;
            let state = address(fact.subject, fact.group, family.near())?;
            let provenance =
                DeltaKvSourceProvenance::new(fact.id, vec![id.segment_id]).map_err(boxed)?;
            let key = router.project::<K>(&state, seed, Some(&provenance))?;
            ensure(
                key.selected_experts == [PageId(group)],
                "unexpected actual MOR decision",
            )?;
            projection_coordinates += key.counters.semantic_coordinates_read;
            let target = q12(fact.bits);
            source.write(&ledger, fact.id, &key.key_q12, &target, 4096, &[id])?;
            memory.update(&key.key_q12, &target, 4096, &mut numeric)?;
            let write = direct
                .write(
                    direct_id(fact.subject, fact.group),
                    BooleanWorkspace::from_bits(&fact.bits),
                    &mut direct_counters,
                )
                .map_err(boxed)?;
            ensure(
                matches!(
                    write,
                    WriteOutcome::Inserted { .. } | WriteOutcome::Updated { .. }
                ),
                "unintended direct collision",
            )?;
            projected.push((fact.subject, key.key_q12));
            artifacts.push(artifact);
        }
        let initial = address(1, group, family.near())?;
        let query = router.project::<K>(&initial, seed, None)?;
        ensure(
            query.selected_experts == [PageId(group)],
            "query not routed by expected MOR page",
        )?;
        projection_coordinates += query.counters.semantic_coordinates_read;
        let mut diagnostic = DeltaKvCounters::default();
        ensure(
            source.read(&query.key_q12)? == memory.read(&query.key_q12, &mut diagnostic)?,
            "source/numeric shadow mismatch",
        )?;
        Ok(Self {
            initial,
            query,
            memory,
            source,
            direct,
            ledger,
            artifacts,
            projected,
            setup_numeric: numeric,
            setup_direct: direct_counters,
            projection_coordinates,
            expected,
        })
    }
    fn raw_all(&self) -> R<Vec<(ContextSegmentRef, Vec<u8>)>> {
        self.artifacts
            .iter()
            .map(|artifact| {
                let loaded = parse_raw_segment_artifact(artifact, raw_limits())?;
                Ok((loaded.reference, loaded.raw_bytes))
            })
            .collect()
    }
    fn verified_sources(&self) -> R<Vec<(ContextSegmentRef, Vec<u8>)>> {
        let query = match self.source.query(&self.query.key_q12) {
            Ok(query) => query,
            Err(ContextError::NoWriteEvidence) => return Ok(Vec::new()),
            Err(e) => return Err(e.into()),
        };
        let receipt = query.verify(
            &self.ledger,
            VerificationLimits {
                max_sources: 16,
                max_raw_bytes: 1 << 16,
            },
            |r| {
                parse_raw_segment_artifact(
                    &self.artifacts[(r.identity.segment_id - 1) as usize],
                    raw_limits(),
                )
                .map(|s| s.raw_bytes)
            },
        )?;
        ensure(receipt.key() == self.query.key_q12, "receipt query binding")?;
        Ok(receipt
            .sources()
            .iter()
            .map(|s| (s.reference().clone(), s.bytes().to_vec()))
            .collect())
    }
}

/// Independently inspect raw bytes. No expected-answer argument or lookup table.
fn resolve(
    sources: &[(ContextSegmentRef, Vec<u8>)],
    subject: u64,
    group: u32,
) -> R<Option<[bool; 2]>> {
    let mut active: Vec<Fact> = Vec::new();
    let mut previous_order = None;
    for (reference, bytes) in sources {
        if let Some(previous) = previous_order {
            ensure(reference.order_index > previous, "source order")?;
        }
        previous_order = Some(reference.order_index);
        let fact = parse(bytes, reference)?;
        if fact.subject != subject || fact.group != group {
            continue;
        }
        if let Some(prior) = fact.prior {
            let index = active
                .iter()
                .position(|f| f.id == prior)
                .ok_or("supersession not an active predecessor")?;
            ensure(prior < fact.id, "future supersession")?;
            active.remove(index);
        }
        active.push(fact);
    }
    match active.last() {
        Some(last) if active.iter().all(|f| f.bits == last.bits) => Ok(Some(last.bits)),
        _ => Ok(None),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Arm {
    None,
    Direct,
    DirectFixed,
    Dkv,
    DkvFixed,
    Cached,
}
impl Arm {
    const ALL: [Self; 6] = [
        Self::None,
        Self::Direct,
        Self::DirectFixed,
        Self::Dkv,
        Self::DkvFixed,
        Self::Cached,
    ];
    fn name(self) -> &'static str {
        match self {
            Self::None => "none_adaptive",
            Self::Direct => "direct_adaptive",
            Self::DirectFixed => "direct_fixed",
            Self::Dkv => "dkv_adaptive",
            Self::DkvFixed => "dkv_fixed",
            Self::Cached => "dkv_cached",
        }
    }
    fn fixed(self) -> bool {
        matches!(self, Self::DirectFixed | Self::DkvFixed)
    }
}

enum Input {
    None,
    Direct {
        memory: BoundedAssociativeMemory,
        key: u64,
        counters: ExecutionCounters,
    },
    Dkv {
        kernel: DeltaKvRecurrentKernel<BITS>,
        cached: bool,
        scratch: Option<([bool; 2], [bool; 2])>,
    },
}
struct Kernel {
    input: Input,
    page: u32,
}
impl Kernel {
    fn new(world: &World, arm: Arm) -> R<Self> {
        let page = world.query.selected_experts[0].0;
        let input = match arm {
            Arm::None => Input::None,
            Arm::Direct | Arm::DirectFixed => Input::Direct {
                memory: world.direct.clone(),
                key: direct_id(1, page),
                counters: ExecutionCounters::default(),
            },
            Arm::Dkv | Arm::DkvFixed | Arm::Cached => Input::Dkv {
                kernel: DeltaKvRecurrentKernel::new(
                    world.memory.clone(),
                    world.query.clone(),
                    SCRATCH.to_vec(),
                    DeltaKvRecurrentPolicy::default(),
                )
                .map_err(boxed)?,
                cached: arm == Arm::Cached,
                scratch: None,
            },
        };
        Ok(Self { input, page })
    }
    fn counts(&self) -> (u64, u64, u64) {
        match &self.input {
            Input::None => (0, 0, 0),
            Input::Direct { counters, .. } => (0, 0, counters.memory_reads),
            Input::Dkv { kernel, .. } => (kernel.counters().reads, kernel.counters().read_macs, 0),
        }
    }
    fn first_values(&self) -> Vec<i32> {
        match &self.input {
            Input::Dkv { kernel, .. } => kernel
                .cycle_evidence()
                .first()
                .map(|e| e.read_values_q12.clone())
                .unwrap_or_default(),
            _ => Vec::new(),
        }
    }
}
fn pair(state: &SemanticState<BITS>, coordinates: [usize; 2], ambiguity: bool) -> R<[bool; 2]> {
    let field = if ambiguity {
        &state.ambiguity
    } else {
        &state.value
    };
    Ok([
        field.get(coordinates[0]).map_err(boxed)?,
        field.get(coordinates[1]).map_err(boxed)?,
    ])
}
fn set_scratch(state: &mut SemanticState<BITS>, values: [bool; 2], ambiguity: [bool; 2]) -> R<()> {
    for j in 0..2 {
        state.value.set(SCRATCH[j], values[j]).map_err(boxed)?;
        state
            .ambiguity
            .set(SCRATCH[j], ambiguity[j])
            .map_err(boxed)?;
    }
    Ok(())
}
impl SemanticRewriter<BITS> for Kernel {
    type Error = Box<dyn Error>;
    fn rewrite(&mut self, state: &SemanticState<BITS>, depth: u32) -> R<SemanticRewriteStep<BITS>> {
        let mut next = state.clone();
        match &mut self.input {
            Input::None => {}
            Input::Direct {
                memory,
                key,
                counters,
            } => match memory.read(*key, counters) {
                ReadOutcome::Hit { value, .. } => set_scratch(
                    &mut next,
                    [
                        value.get(0).ok_or("direct payload")?,
                        value.get(1).ok_or("direct payload")?,
                    ],
                    [false, false],
                )?,
                _ => set_scratch(&mut next, pair(state, SCRATCH, false)?, [true, true])?,
            },
            Input::Dkv {
                kernel,
                cached,
                scratch,
            } => {
                if *cached && scratch.is_some() {
                    let (values, ambiguity) = scratch.as_ref().ok_or("cache invariant")?;
                    set_scratch(&mut next, *values, *ambiguity)?;
                } else {
                    next = kernel.rewrite(state, depth).map_err(boxed)?.state;
                    *scratch = Some((pair(&next, SCRATCH, false)?, pair(&next, SCRATCH, true)?));
                }
            }
        }
        // Consume PREVIOUS state only: this is a two-stage synthetic circuit,
        // not a claim of learned reasoning or improved memory from more cycles.
        let unknown = pair(state, SCRATCH, true)?.into_iter().any(|b| b);
        if unknown {
            for i in OUTPUT {
                next.ambiguity.set(i, true).map_err(boxed)?;
            }
        } else {
            let [x, y] = pair(state, SCRATCH, false)?;
            let result = match self.page {
                0 => [x ^ y, x & y],
                1 => [x | y, !(x ^ y)],
                _ => return Err("unadmitted expert".into()),
            };
            for j in 0..2 {
                next.value.set(OUTPUT[j], result[j]).map_err(boxed)?;
                next.ambiguity.set(OUTPUT[j], false).map_err(boxed)?;
            }
        }
        let blocked = pair(&next, SCRATCH, true)?
            .into_iter()
            .chain(pair(&next, OUTPUT, true)?)
            .any(|b| b);
        Ok(SemanticRewriteStep {
            state: next,
            stability_blocked: blocked,
        })
    }
}

/// Evaluation oracle: independently enumerated complete two-input truth tables.
fn oracle(payload: Option<[bool; 2]>, page: u32) -> Option<[bool; 2]> {
    let [x, y] = payload?;
    let index = usize::from(x) + 2 * usize::from(y);
    let table = if page == 0 {
        [[false, false], [true, false], [true, false], [false, true]]
    } else {
        [[false, true], [true, false], [true, false], [true, true]]
    };
    Some(table[index])
}
fn latent(state: &SemanticState<BITS>) -> R<Option<[bool; 2]>> {
    if pair(state, OUTPUT, true)?.into_iter().any(|b| b) {
        Ok(None)
    } else {
        Ok(Some(pair(state, OUTPUT, false)?))
    }
}
fn bits_text(bits: Option<[bool; 2]>) -> String {
    bits.map_or_else(
        || "unknown".to_string(),
        |b| format!("{}{}", u8::from(b[0]), u8::from(b[1])),
    )
}
fn dot(a: &[i16], b: &[i16]) -> i128 {
    a.iter()
        .zip(b)
        .map(|(&x, &y)| i128::from(x) * i128::from(y))
        .sum()
}
fn key_id(key: &[i16]) -> R<String> {
    let bytes: Vec<u8> = key.iter().flat_map(|v| v.to_le_bytes()).collect();
    Ok(sha256_raw_bytes(&bytes)?.hex())
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Score {
    arm: Arm,
    correct: bool,
    wrong_known: bool,
    unknown: bool,
    depth: u32,
    reads: u64,
    macs: u64,
}
struct Run {
    outcome: SemanticRecurrentOutcome<BITS>,
    score: Score,
    direct_reads: u64,
    first_values: Vec<i32>,
}
fn execute(world: &World, arm: Arm) -> R<Run> {
    let mut kernel = Kernel::new(world, arm)?;
    let outcome = run_semantic_recurrent(
        world.initial.clone(),
        &mut kernel,
        HaltingPolicy {
            min_depth: if arm.fixed() { MAX_DEPTH } else { 1 },
            max_depth: MAX_DEPTH,
            stable_hamming_threshold: 0,
        },
    )
    .map_err(boxed)?;
    ensure(outcome.depth <= MAX_DEPTH, "depth bound")?;
    ensure(
        outcome.state.meaning_mask == world.initial.meaning_mask
            && outcome.state.routing == world.initial.routing,
        "M/R mutated",
    )?;
    for i in (0..48).chain(52..BITS) {
        ensure(
            outcome.state.value.get(i).map_err(boxed)?
                == world.initial.value.get(i).map_err(boxed)?,
            "identity mutated",
        )?;
        ensure(
            outcome.state.ambiguity.get(i).map_err(boxed)?
                == world.initial.ambiguity.get(i).map_err(boxed)?,
            "unrelated ambiguity mutated",
        )?;
    }
    if let Input::Dkv { kernel: native, .. } = &kernel.input {
        ensure(
            native.key_evidence() == &world.query,
            "key changed inside recurrence",
        )?;
    }
    let got = latent(&outcome.state)?;
    // The expected answer is read only after recurrence has completed.
    let gold = oracle(world.expected, kernel.page);
    let (reads, macs, direct_reads) = kernel.counts();
    ensure(
        macs == reads * world.query.key_q12.len() as u64 * 2,
        "matrix read accounting",
    )?;
    if arm == Arm::Cached {
        ensure(reads == 1, "cached arm must read exactly once")?;
    }
    let score = Score {
        arm,
        correct: got == gold,
        wrong_known: got.is_some() && got != gold,
        unknown: got.is_none(),
        depth: outcome.depth,
        reads,
        macs,
    };
    Ok(Run {
        outcome,
        score,
        direct_reads,
        first_values: kernel.first_values(),
    })
}

fn one_config<const K: usize>(
    family: Family,
    seed: u64,
    group: u32,
    text: &mut String,
    scores: &mut Vec<Score>,
) -> R<()> {
    let world = World::build::<K>(family, seed, group)?;
    let query_id = key_id(&world.query.key_q12)?;
    writeln!(
        text,
        "KEY,{K},{seed},{group},{},{query_id},{}",
        family.name(),
        world
            .query
            .key_q12
            .iter()
            .map(i16::to_string)
            .collect::<Vec<_>>()
            .join(";")
    )?;
    let mut collisions = 0;
    for (i, (subject, key)) in world.projected.iter().enumerate() {
        writeln!(
            text,
            "GRAM,{K},{seed},{group},{},{i},{subject},{},{},{}",
            family.name(),
            dot(key, &world.query.key_q12),
            dot(key, key),
            dot(&world.query.key_q12, &world.query.key_q12)
        )?;
        for (other_subject, other_key) in &world.projected[..i] {
            if subject != other_subject && key == other_key {
                collisions += 1;
            }
        }
    }
    let all = world.raw_all()?;
    let verified = world.verified_sources()?;
    let raw_answer = resolve(&all, 1, group)?;
    let verified_answer = resolve(&verified, 1, group)?;
    ensure(raw_answer == world.expected, "raw fixture oracle mismatch")?;
    let source_correct = verified_answer == world.expected;
    let source_bytes: usize = verified.iter().map(|(_, bytes)| bytes.len()).sum();
    let plan = world.source.element_plan();
    let numeric_bits = world.memory.semantic_bits();
    let source_counters = world.source.context_counters();
    writeln!(text,
        "SETUP,{K},{seed},{group},{},writes={},mor_calls={},projection_coordinates={},native_ingest_macs={},shadow_ingest_macs={},diagnostic_reads=2,numeric_matrix_instances=2,matrix_bits_each={},source_retained_elements={},source_staging_elements={},direct_bits={},direct_writes={},raw_artifact_bytes={},projected_collisions={},verified_segments={},verified_bytes={},source_correct={}",
        family.name(), world.projected.len(), world.projected.len()+1, world.projection_coordinates,
        world.setup_numeric.read_macs, source_counters.ingest_prediction_macs, numeric_bits,
        plan.max_retained_element_bytes, plan.max_write_staging_element_bytes, world.direct.semantic_bits(),
        world.setup_direct.memory_writes, world.artifacts.iter().map(Vec::len).sum::<usize>(), collisions,
        verified.len(), source_bytes, u8::from(source_correct))?;
    let mut runs = Vec::new();
    for arm in Arm::ALL {
        let run = execute(&world, arm)?;
        let score = &run.score;
        writeln!(
            text,
            "ROW,{K},{seed},{group},{},{},{},{},{},{},{},{},{:?},{},{},{},{}",
            family.name(),
            arm.name(),
            bits_text(oracle(world.expected, group)),
            bits_text(latent(&run.outcome.state)?),
            u8::from(score.correct),
            u8::from(score.wrong_known),
            u8::from(score.unknown),
            score.depth,
            run.outcome.halt_reason,
            score.reads,
            score.macs,
            run.direct_reads,
            run.first_values
                .iter()
                .map(i32::to_string)
                .collect::<Vec<_>>()
                .join(";")
        )?;
        for trace in &run.outcome.trace {
            writeln!(
                text,
                "TRACE,{K},{seed},{group},{},{},{},{},{},{},{},{}",
                family.name(),
                arm.name(),
                trace.depth,
                trace.delta.value,
                trace.delta.meaning_mask,
                trace.delta.ambiguity,
                trace.delta.routing,
                u8::from(trace.stability_blocked)
            )?;
        }
        scores.push(score.clone());
        runs.push(run);
    }
    let fresh = &runs[3];
    let cached = &runs[5];
    ensure(
        fresh.outcome == cached.outcome,
        "static-key fresh/cached mismatch",
    )?;
    ensure(
        runs[3].outcome.state == runs[4].outcome.state,
        "fixed/adaptive static path mismatch",
    )?;
    Ok(())
}

fn run_grid() -> R<(String, Vec<Score>)> {
    let mut text =
        format!("PROTOCOL,{PROTOCOL_COMMIT}\nBASE,{BASE_COMMIT}\nSTATUS,DEVELOPMENT_ONLY\n");
    text.push_str("ROW_HEADER,dimension,seed,group,family,arm,gold,latent,correct,wrong_known,unknown,depth,halt,fresh_reads,read_macs,direct_reads,first_q12\n");
    text.push_str("TRACE_HEADER,dimension,seed,group,family,arm,depth,delta_v,delta_m,delta_a,delta_r,stability_blocked\n");
    text.push_str("GRAM_HEADER,dimension,seed,group,family,write_index,subject,dot_query_write,norm_write_sq,norm_query_sq\n");
    let mut scores = Vec::new();
    for seed in 0..3 {
        for group in 0..2 {
            for family in Family::ALL {
                one_config::<32>(family, seed, group, &mut text, &mut scores)?;
                one_config::<64>(family, seed, group, &mut text, &mut scores)?;
                one_config::<128>(family, seed, group, &mut text, &mut scores)?;
            }
        }
    }
    ensure(scores.len() == 864, "grid completeness")?;
    for arm in Arm::ALL {
        let rows: Vec<_> = scores.iter().filter(|r| r.arm == arm).collect();
        writeln!(text, "SUMMARY,{},rows={},correct={},wrong_known={},unknown={},cycles={},fresh_reads={},read_macs={}",
            arm.name(), rows.len(), rows.iter().filter(|r| r.correct).count(), rows.iter().filter(|r| r.wrong_known).count(),
            rows.iter().filter(|r| r.unknown).count(), rows.iter().map(|r| u64::from(r.depth)).sum::<u64>(),
            rows.iter().map(|r| r.reads).sum::<u64>(), rows.iter().map(|r| r.macs).sum::<u64>())?;
    }
    text.push_str(
        "PROMOTION,BLOCKED,physical_resources_and_independent_context_qualification_missing\n",
    );
    ensure(text.len() < 8 * 1024 * 1024, "report budget")?;
    Ok((text, scores))
}

#[cfg_attr(test, allow(dead_code))]
fn main() -> R<()> {
    use std::io::Write;
    let (text, _) = run_grid()?;
    std::io::stdout().lock().write_all(text.as_bytes())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn native_mor_routes_both_real_pages_and_projection_ignores_provenance() -> R<()> {
        let router = Router::new()?;
        for group in 0..2 {
            let state = address(1, group, false)?;
            let plain = router.project::<32>(&state, 0, None)?;
            let proof = DeltaKvSourceProvenance::new(99, vec![123]).map_err(boxed)?;
            let with = router.project::<32>(&state, 0, Some(&proof))?;
            assert_eq!(plain.selected_experts, vec![PageId(group)]);
            assert_eq!(plain.key_q12, with.key_q12);
            assert_eq!(plain.counters.semantic_coordinates_read, 256);
            assert_eq!(plain.counters.mor_selected_experts_read, 1);
        }
        assert_ne!(
            router
                .project::<32>(&address(1, 0, false)?, 0, None)?
                .key_q12,
            router
                .project::<32>(&address(1, 1, false)?, 0, None)?
                .key_q12
        );
        Ok(())
    }

    #[test]
    fn no_payload_enters_address_and_all_four_tasks_work_when_isolated() -> R<()> {
        let router = Router::new()?;
        let key = router
            .project::<32>(&address(1, 0, false)?, 0, None)?
            .key_q12;
        for seed in 0..4 {
            for group in 0..2 {
                let world = World::build::<32>(Family::Isolated, seed, group)?;
                for arm in [Arm::Direct, Arm::Dkv, Arm::Cached] {
                    let run = execute(&world, arm)?;
                    assert!(run.score.correct);
                    assert_eq!(run.score.depth, 3);
                }
                // Payload changes are ingested into values, never into the address API.
                assert_eq!(
                    router
                        .project::<32>(&address(1, 0, false)?, 0, None)?
                        .key_q12,
                    key
                );
            }
        }
        Ok(())
    }

    #[test]
    fn cached_and_fresh_have_identical_recurrent_states_not_just_answers() -> R<()> {
        for family in Family::ALL {
            let world = World::build::<64>(family, 1, 0)?;
            let fresh = execute(&world, Arm::Dkv)?;
            let cached = execute(&world, Arm::Cached)?;
            assert_eq!(fresh.outcome, cached.outcome);
            assert_eq!(cached.score.reads, 1);
            assert_eq!(fresh.score.reads, u64::from(fresh.score.depth));
        }
        Ok(())
    }

    #[test]
    fn source_resolution_does_not_rescue_or_enter_latent_recurrence() -> R<()> {
        let world = World::build::<32>(Family::Isolated, 0, 0)?;
        assert!(resolve(&world.verified_sources()?, 1, 0)?.is_some());
        let no_memory = execute(&world, Arm::None)?;
        assert_eq!(latent(&no_memory.outcome.state)?, None);
        assert!(!no_memory.score.correct);
        assert_eq!(no_memory.score.depth, MAX_DEPTH);
        Ok(())
    }

    #[test]
    fn typed_conflict_overwrite_and_missing_are_independent_raw_controls() -> R<()> {
        for (family, known) in [
            (Family::Conflict, false),
            (Family::Missing, false),
            (Family::Overwrite, true),
        ] {
            let world = World::build::<64>(family, 0, 1)?;
            assert_eq!(resolve(&world.raw_all()?, 1, 1)?.is_some(), known);
            assert_eq!(resolve(&world.verified_sources()?, 1, 1)?, world.expected);
        }
        Ok(())
    }

    #[test]
    fn key_has_unknown_scratch_and_no_numeric_authority() -> R<()> {
        let state = address(1, 0, false)?;
        assert_eq!(pair(&state, SCRATCH, true)?, [true, true]);
        assert_eq!(pair(&state, OUTPUT, true)?, [true, true]);
        assert_eq!(pair(&state, SCRATCH, false)?, [false, false]);
        Ok(())
    }

    #[test]
    fn actual_projected_geometry_is_recorded_without_orthogonality_claim() -> R<()> {
        let world = World::build::<32>(Family::NearOld, 0, 0)?;
        let addresses: BTreeSet<_> = (1..=8)
            .map(|s| {
                let state = address(s, 0, true).unwrap();
                (0..48)
                    .map(|i| state.value.get(i).unwrap())
                    .collect::<Vec<_>>()
            })
            .collect();
        assert_eq!(addresses.len(), 8);
        assert_eq!(world.projected.len(), 8);
        for (_, key) in &world.projected {
            assert!(dot(key, key) > 0);
        }
        Ok(())
    }

    #[test]
    fn invalid_routing_and_address_reject_before_execution() {
        assert!(signature(2).is_err());
        assert!(address(0, 0, false).is_err());
        assert!(address(9, 0, false).is_err());
    }

    #[test]
    fn full_development_grid_is_deterministic_and_never_promotes() -> R<()> {
        let (a, rows) = run_grid()?;
        let (b, _) = run_grid()?;
        assert_eq!(a, b);
        assert_eq!(rows.len(), 864);
        assert_eq!(a.lines().filter(|l| l.starts_with("ROW,")).count(), 864);
        assert!(a.contains("PROMOTION,BLOCKED,"));
        Ok(())
    }
}
