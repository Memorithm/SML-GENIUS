//! A2 Development-only projection ablation. No accepted sourced answer or action.
//! Protocol is committed before this implementation; R1-A is not modified.
use sml_core::{
    delta_kv::{DeltaKvCounters, DeltaKvMemory, DELTA_KV_ONE},
    delta_kv_recurrent::{DeltaKvRecurrentKernel, DeltaKvRecurrentPolicy},
    delta_kv_semantic::{
        DeltaKvSemanticError, DeltaKvSemanticKeyEvidence, DeltaKvSemanticProjectionPolicy,
        DeltaKvSemanticProjector,
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
use sml_store::raw_context::sha256_raw_bytes;
use std::{error::Error, fmt::Write as _};

type R<T> = Result<T, Box<dyn Error>>;
const BITS: usize = 64;
const SCRATCH: [usize; 2] = [48, 49];
const OUTPUT: [usize; 2] = [50, 51];
const PROTOCOL: &str = "473b3f48bccd594dd2b39a8903f315f0ec9d5ff5";
const BASE: &str = "ffb34484d9c95848e016aa34a3348666a92e7dca";
fn err(e: impl std::fmt::Debug) -> Box<dyn Error> {
    format!("{e:?}").into()
}
fn require(ok: bool, message: &str) -> R<()> {
    if ok {
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
    fn answerable(self) -> bool {
        !matches!(self, Self::Conflict | Self::Missing)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Standard,
    Contrast,
    Direct,
}
impl Mode {
    const ALL: [Self; 3] = [Self::Standard, Self::Contrast, Self::Direct];
    fn name(self) -> &'static str {
        match self {
            Self::Standard => "standard",
            Self::Contrast => "contrast",
            Self::Direct => "direct",
        }
    }
}
#[derive(Clone, Copy)]
struct Fact {
    id: u64,
    subject: u64,
    bits: [bool; 2],
    prior: Option<u64>,
}
fn data(family: Family, payload: u8) -> Vec<Fact> {
    let target = [payload & 1 != 0, payload & 2 != 0];
    let opposite = target.map(|b| !b);
    let mut raw = Vec::new();
    let distractors = || (2..=8).map(|s| (s, opposite, None));
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
    raw.into_iter()
        .enumerate()
        .map(|(i, (subject, bits, prior))| Fact {
            id: i as u64 + 1,
            subject,
            bits,
            prior,
        })
        .collect()
}
fn mix(mut x: u64) -> u64 {
    x ^= x >> 30;
    x = x.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}
fn signature(group: u32) -> R<SemanticBitField<BITS>> {
    require(group < 2, "routing group")?;
    SemanticBitField::from_set_indices(
        &(0..BITS)
            .filter(|i| *i % 2 == group as usize)
            .collect::<Vec<_>>(),
    )
    .map_err(err)
}
/// Address-only signature: payload and evaluator truth cannot be arguments.
fn address(subject: u64, group: u32, near: bool) -> R<SemanticState<BITS>> {
    require((1..=8).contains(&subject), "subject range")?;
    let mut s = SemanticState::zeroed().map_err(err)?;
    let identity = mix(if near { 1 } else { subject });
    for i in 0..48 {
        s.value.set(i, identity & (1_u64 << i) != 0).map_err(err)?;
        s.meaning_mask.set(i, true).map_err(err)?;
    }
    if near && subject != 1 {
        for i in [(subject as usize * 7) % 48, (subject as usize * 11) % 48] {
            let old = s.value.get(i).map_err(err)?;
            s.value.set(i, !old).map_err(err)?;
        }
    }
    for i in SCRATCH.into_iter().chain(OUTPUT) {
        s.ambiguity.set(i, true).map_err(err)?;
    }
    s.routing = signature(group)?;
    Ok(s)
}
fn complement(s: &SemanticState<BITS>) -> R<SemanticState<BITS>> {
    let mut t = s.clone();
    for i in 0..48 {
        t.value.set(i, !s.value.get(i).map_err(err)?).map_err(err)?;
    }
    Ok(t)
}
#[derive(Clone, Copy, Debug, Default)]
struct ProjectionWork {
    mor_calls: u64,
    calls: u64,
    coordinates: u64,
    buckets: u64,
    contrast_coordinates: u64,
    zero_calls: u64,
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
        .map_err(err)?;
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
        .map_err(err)?;
        let engine = MixtureOfRouters::new(MorPolicy {
            max_experts: 1,
            ..MorPolicy::default()
        })
        .map_err(err)?;
        Ok(Self {
            bridge,
            registry,
            engine,
        })
    }
    fn route(
        &self,
        s: &SemanticState<BITS>,
        work: &mut ProjectionWork,
    ) -> R<sml_core::mor::MorDecision> {
        let route = self
            .bridge
            .route(
                s,
                &self.registry,
                &self.engine,
                &[],
                &PreviousRoute::default(),
            )
            .map_err(err)?;
        require(
            route.decision.selected_experts.len() == 1,
            "actual MOR admission",
        )?;
        work.mor_calls += 1;
        Ok(route.decision)
    }
    fn key<const K: usize>(
        &self,
        s: &SemanticState<BITS>,
        seed: u64,
        mode: Mode,
        work: &mut ProjectionWork,
    ) -> R<Option<DeltaKvSemanticKeyEvidence>> {
        require(mode != Mode::Direct, "no projection for direct arm")?;
        let decision = self.route(s, work)?;
        let projector =
            DeltaKvSemanticProjector::<K>::new(DeltaKvSemanticProjectionPolicy::default(), seed)
                .map_err(err)?;
        let mut apply = |state: &SemanticState<BITS>| -> R<Option<DeltaKvSemanticKeyEvidence>> {
            work.calls += 1;
            // Under the pinned projector, zero-key rejection occurs AFTER all
            // four field loops and the admitted MOR page. Account it as work.
            work.coordinates += 4 * BITS as u64;
            work.buckets += 4 * BITS as u64 + 1;
            match projector.project(state, &decision, None) {
                Ok(p) => {
                    require(
                        p.counters.semantic_coordinates_read == 4 * BITS as u64
                            && p.counters.bucket_accumulations == 4 * BITS as u64 + 1,
                        "projector cost drift",
                    )?;
                    Ok(Some(p))
                }
                Err(DeltaKvSemanticError::ZeroProjectedKey) => {
                    work.zero_calls += 1;
                    Ok(None)
                }
                Err(e) => Err(err(e)),
            }
        };
        let first = apply(s)?;
        if mode == Mode::Standard {
            return Ok(first);
        }
        // Execute both planned projections even if the first was rejected.
        let second = apply(&complement(s)?)?;
        work.contrast_coordinates += K as u64;
        let (Some(mut p), Some(q)) = (first, second) else {
            return Ok(None);
        };
        match contrast_key(&p.key_q12, &q.key_q12)? {
            Some(k) => {
                p.key_q12 = k;
                Ok(Some(p))
            }
            None => Ok(None),
        }
    }
}
fn contrast_key(a: &[i16], b: &[i16]) -> R<Option<Vec<i16>>> {
    require(a.len() == b.len() && !a.is_empty(), "contrast shape")?;
    let mut out = Vec::with_capacity(a.len());
    for (&x, &y) in a.iter().zip(b) {
        require(
            [-4096, 0, 4096].contains(&x) && [-4096, 0, 4096].contains(&y),
            "projector alphabet",
        )?;
        let difference = i32::from(x) - i32::from(y);
        require(difference % 2 == 0, "contrast exact division")?;
        out.push(i16::try_from(difference / 2)?);
    }
    Ok(if out.iter().all(|&v| v == 0) {
        None
    } else {
        Some(out)
    })
}
fn q12(bits: [bool; 2]) -> [i16; 2] {
    bits.map(|b| if b { DELTA_KV_ONE } else { -DELTA_KV_ONE })
}
fn direct_id(subject: u64, group: u32) -> u64 {
    2 * subject + u64::from(group)
}
fn pair(s: &SemanticState<BITS>, coords: [usize; 2], ambiguity: bool) -> R<[bool; 2]> {
    let f = if ambiguity { &s.ambiguity } else { &s.value };
    Ok([
        f.get(coords[0]).map_err(err)?,
        f.get(coords[1]).map_err(err)?,
    ])
}
fn set_scratch(s: &mut SemanticState<BITS>, bits: [bool; 2], amb: [bool; 2]) -> R<()> {
    for j in 0..2 {
        s.value.set(SCRATCH[j], bits[j]).map_err(err)?;
        s.ambiguity.set(SCRATCH[j], amb[j]).map_err(err)?;
    }
    Ok(())
}
enum Input {
    Missing,
    Direct {
        memory: BoundedAssociativeMemory,
        key: u64,
        counters: ExecutionCounters,
    },
    Dkv {
        kernel: DeltaKvRecurrentKernel<BITS>,
        cache: Option<([bool; 2], [bool; 2])>,
        cached: bool,
    },
}
struct Kernel {
    input: Input,
    page: u32,
}
impl SemanticRewriter<BITS> for Kernel {
    type Error = Box<dyn Error>;
    fn rewrite(&mut self, s: &SemanticState<BITS>, depth: u32) -> R<SemanticRewriteStep<BITS>> {
        let mut next = s.clone();
        match &mut self.input {
            Input::Missing => {}
            Input::Direct {
                memory,
                key,
                counters,
            } => match memory.read(*key, counters) {
                ReadOutcome::Hit { value, .. } => set_scratch(
                    &mut next,
                    [
                        value.get(0).ok_or("payload")?,
                        value.get(1).ok_or("payload")?,
                    ],
                    [false, false],
                )?,
                _ => set_scratch(&mut next, pair(s, SCRATCH, false)?, [true, true])?,
            },
            Input::Dkv {
                kernel,
                cache,
                cached,
            } => {
                if *cached && cache.is_some() {
                    let (bits, amb) = cache.as_ref().ok_or("cache")?;
                    set_scratch(&mut next, *bits, *amb)?;
                } else {
                    next = kernel.rewrite(s, depth).map_err(err)?.state;
                    *cache = Some((pair(&next, SCRATCH, false)?, pair(&next, SCRATCH, true)?));
                }
            }
        }
        if pair(s, SCRATCH, true)?.into_iter().any(|b| b) {
            for c in OUTPUT {
                next.ambiguity.set(c, true).map_err(err)?;
            }
        } else {
            let [x, y] = pair(s, SCRATCH, false)?;
            let result = match self.page {
                0 => [x ^ y, x & y],
                1 => [x | y, !(x ^ y)],
                _ => return Err("unadmitted page".into()),
            };
            for j in 0..2 {
                next.value.set(OUTPUT[j], result[j]).map_err(err)?;
                next.ambiguity.set(OUTPUT[j], false).map_err(err)?;
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
struct Episode {
    initial: SemanticState<BITS>,
    page: u32,
    input: Input,
    projection: ProjectionWork,
    ingest: DeltaKvCounters,
    direct_ingest: ExecutionCounters,
    keys: Vec<(u64, Option<Vec<i16>>)>,
    query: Option<Vec<i16>>,
    matrix_bits: u128,
    direct_bits: u128,
    rejected_writes: usize,
}
impl Episode {
    fn build<const K: usize>(
        family: Family,
        seed: u64,
        group: u32,
        payload: u8,
        mode: Mode,
    ) -> R<Self> {
        let facts = data(family, payload);
        require(facts.len() <= 16, "record limit")?;
        let initial = address(1, group, family.near())?;
        let router = Router::new()?;
        let mut projection = ProjectionWork::default();
        let mut ingest = DeltaKvCounters::default();
        let mut direct_ingest = ExecutionCounters::default();
        let mut keys = Vec::new();
        let mut rejected_writes = 0;
        let (input, query, matrix_bits, direct_bits, page) = if mode == Mode::Direct {
            let mut memory =
                BoundedAssociativeMemory::new(32, 2, ReplacementPolicy::ReplaceExisting)
                    .map_err(err)?;
            for fact in &facts {
                let state = address(fact.subject, group, family.near())?;
                let admitted = router.route(&state, &mut projection)?;
                require(admitted.selected_experts == [PageId(group)], "write page")?;
                let outcome = memory
                    .write(
                        direct_id(fact.subject, group),
                        BooleanWorkspace::from_bits(&fact.bits),
                        &mut direct_ingest,
                    )
                    .map_err(err)?;
                require(
                    matches!(
                        outcome,
                        WriteOutcome::Inserted { .. } | WriteOutcome::Updated { .. }
                    ),
                    "direct collision",
                )?;
            }
            let admitted = router.route(&initial, &mut projection)?;
            let page = admitted.selected_experts[0].0;
            let bits = memory.semantic_bits();
            (
                Input::Direct {
                    memory,
                    key: direct_id(1, group),
                    counters: ExecutionCounters::default(),
                },
                None,
                0,
                bits,
                page,
            )
        } else {
            let mut memory = DeltaKvMemory::new(K, 2)?;
            for fact in &facts {
                let state = address(fact.subject, group, family.near())?;
                let key = router.key::<K>(&state, seed, mode, &mut projection)?;
                if let Some(key) = key {
                    require(key.selected_experts == [PageId(group)], "write page")?;
                    memory.update(&key.key_q12, &q12(fact.bits), 4096, &mut ingest)?;
                    keys.push((fact.subject, Some(key.key_q12)));
                } else {
                    rejected_writes += 1;
                    keys.push((fact.subject, None));
                }
            }
            let key = router.key::<K>(&initial, seed, mode, &mut projection)?;
            let bits = memory.semantic_bits();
            match key {
                Some(key) => {
                    require(key.selected_experts == [PageId(group)], "query page")?;
                    let page = key.selected_experts[0].0;
                    let query = Some(key.key_q12.clone());
                    // A setup matrix plus its owned-kernel clone exist briefly.
                    let kernel = DeltaKvRecurrentKernel::new(
                        memory.clone(),
                        key,
                        SCRATCH.to_vec(),
                        DeltaKvRecurrentPolicy::default(),
                    )
                    .map_err(err)?;
                    (
                        Input::Dkv {
                            kernel,
                            cache: None,
                            cached: true,
                        },
                        query,
                        bits,
                        0,
                        page,
                    )
                }
                None => (Input::Missing, None, bits, 0, group),
            }
        };
        require(page == group, "actual query page")?;
        Ok(Self {
            initial,
            page,
            input,
            projection,
            ingest,
            direct_ingest,
            keys,
            query,
            matrix_bits,
            direct_bits,
            rejected_writes,
        })
    }
}
struct Execution {
    outcome: SemanticRecurrentOutcome<BITS>,
    reads: u64,
    macs: u64,
    direct_reads: u64,
    first: Vec<i32>,
}
fn execute(
    initial: SemanticState<BITS>,
    page: u32,
    input: Input,
    query: Option<&[i16]>,
) -> R<Execution> {
    let mut kernel = Kernel { input, page };
    let outcome = run_semantic_recurrent(
        initial.clone(),
        &mut kernel,
        HaltingPolicy {
            min_depth: 1,
            max_depth: 4,
            stable_hamming_threshold: 0,
        },
    )
    .map_err(err)?;
    require(
        outcome.depth <= 4
            && outcome.state.meaning_mask == initial.meaning_mask
            && outcome.state.routing == initial.routing,
        "recurrence invariant",
    )?;
    for i in (0..48).chain(52..BITS) {
        require(
            outcome.state.value.get(i).map_err(err)? == initial.value.get(i).map_err(err)?
                && outcome.state.ambiguity.get(i).map_err(err)?
                    == initial.ambiguity.get(i).map_err(err)?,
            "address changed",
        )?;
    }
    let (reads, macs, direct_reads, first) = match &kernel.input {
        Input::Missing => (0, 0, 0, Vec::new()),
        Input::Direct { counters, .. } => (0, 0, counters.memory_reads, Vec::new()),
        Input::Dkv { kernel, cached, .. } => {
            require(
                Some(kernel.key_evidence().key_q12.as_slice()) == query,
                "query mutated",
            )?;
            if *cached {
                require(kernel.counters().reads == 1, "cached read count")?;
            }
            let c = kernel.counters();
            require(
                c.read_macs == c.reads * kernel.key_evidence().key_q12.len() as u64 * 2,
                "query accounting",
            )?;
            (
                c.reads,
                c.read_macs,
                0,
                kernel.cycle_evidence()[0].read_values_q12.clone(),
            )
        }
    };
    Ok(Execution {
        outcome,
        reads,
        macs,
        direct_reads,
        first,
    })
}
fn oracle(family: Family, payload: u8, page: u32) -> Option<[bool; 2]> {
    if !family.answerable() {
        return None;
    }
    let table = if page == 0 {
        [[false, false], [true, false], [true, false], [false, true]]
    } else {
        [[false, true], [true, false], [true, false], [true, true]]
    };
    Some(table[payload as usize])
}
fn latent(s: &SemanticState<BITS>) -> R<Option<[bool; 2]>> {
    Ok(if pair(s, OUTPUT, true)?.into_iter().any(|b| b) {
        None
    } else {
        Some(pair(s, OUTPUT, false)?)
    })
}
fn bit_text(v: Option<[bool; 2]>) -> String {
    v.map_or_else(
        || "unknown".into(),
        |b| format!("{}{}", u8::from(b[0]), u8::from(b[1])),
    )
}
fn dot(a: &[i16], b: &[i16]) -> i128 {
    a.iter()
        .zip(b)
        .map(|(&x, &y)| i128::from(x) * i128::from(y))
        .sum()
}
fn key_text(k: Option<&[i16]>) -> String {
    k.map_or_else(
        || "rejected".into(),
        |v| v.iter().map(i16::to_string).collect::<Vec<_>>().join(";"),
    )
}
#[derive(Clone, Debug)]
struct Row {
    mode: Mode,
    family: Family,
    correct: bool,
    wrong_known: bool,
    unknown: bool,
    aliases: usize,
    query_aliases: usize,
    rejected: usize,
    query_rejected: bool,
    projection_calls: u64,
    coordinates: u64,
    ingest_macs: u64,
    query_macs: u64,
    far_rho_sum: f64,
    far_rho_count: u64,
}
fn one<const K: usize>(
    family: Family,
    seed: u64,
    group: u32,
    payload: u8,
    mode: Mode,
    text: &mut String,
) -> R<Row> {
    let e = Episode::build::<K>(family, seed, group, payload, mode)?;
    let id = format!(
        "{K},{seed},{group},{payload},{},{}",
        family.name(),
        mode.name()
    );
    let mut serial = String::new();
    for f in data(family, payload) {
        writeln!(
            serial,
            "{}:{}:{}:{:?}:{:?}",
            f.id, f.subject, group, f.bits, f.prior
        )?;
    }
    let source_hash = sha256_raw_bytes(serial.as_bytes())?.hex();
    let mut aliases = 0;
    let mut query_aliases = 0;
    let mut far_rho_sum = 0.0;
    let mut far_rho_count = 0;
    for (i, (subject, key)) in e.keys.iter().enumerate() {
        writeln!(
            text,
            "KEY,{id},write,{i},{subject},{}",
            key_text(key.as_deref())
        )?;
        if let Some(key) = key {
            for (other, old) in &e.keys[..i] {
                if subject != other && old.as_ref() == Some(key) {
                    aliases += 1;
                }
            }
            if let Some(query) = &e.query {
                if *subject != 1 && key == query {
                    query_aliases += 1;
                }
                let d = dot(key, query);
                let nk = dot(key, key);
                let nq = dot(query, query);
                writeln!(text, "GRAM,{id},{i},{subject},{d},{nk},{nq}")?;
                if *subject != 1 && !family.near() {
                    far_rho_sum += (d as f64) * (d as f64) / ((nk as f64) * (nq as f64));
                    far_rho_count += 1;
                }
            }
        }
    }
    writeln!(text, "KEY,{id},query,0,1,{}", key_text(e.query.as_deref()))?;
    let query_rejected = mode != Mode::Direct && e.query.is_none();
    writeln!(text,"SETUP,{id},input_sha256={source_hash},writes={},mor_calls={},projector_calls={},projector_coordinates={},bucket_accumulations={},contrast_coordinates={},zero_projector_calls={},rejected_writes={},query_rejected={},ingest_macs={},weight_updates={},saturations={},matrix_bits={},direct_bits={},direct_writes={},matrix_instances_peak={}",data(family,payload).len(),e.projection.mor_calls,e.projection.calls,e.projection.coordinates,e.projection.buckets,e.projection.contrast_coordinates,e.projection.zero_calls,e.rejected_writes,u8::from(query_rejected),e.ingest.read_macs,e.ingest.weight_updates,e.ingest.saturations,e.matrix_bits,e.direct_bits,e.direct_ingest.memory_writes,if mode==Mode::Direct{0}else if query_rejected{1}else{2})?;
    let run = execute(e.initial, e.page, e.input, e.query.as_deref())?;
    // Gold is consulted only AFTER the full recurrent episode finishes.
    let gold = oracle(family, payload, group);
    let got = latent(&run.outcome.state)?;
    let correct = got == gold;
    let wrong_known = got.is_some() && got != gold;
    let unknown = got.is_none();
    writeln!(
        text,
        "ROW,{id},{},{},{},{},{},{},{:?},{},{},{},{},{},{},{},{}",
        bit_text(gold),
        bit_text(got),
        u8::from(correct),
        u8::from(wrong_known),
        u8::from(unknown),
        run.outcome.depth,
        run.outcome.halt_reason,
        run.reads,
        run.macs,
        run.direct_reads,
        aliases,
        query_aliases,
        e.rejected_writes,
        u8::from(query_rejected),
        run.first
            .iter()
            .map(i32::to_string)
            .collect::<Vec<_>>()
            .join(";")
    )?;
    for t in run.outcome.trace {
        writeln!(
            text,
            "TRACE,{id},{},{},{},{},{},{}",
            t.depth,
            t.delta.value,
            t.delta.meaning_mask,
            t.delta.ambiguity,
            t.delta.routing,
            u8::from(t.stability_blocked)
        )?;
    }
    Ok(Row {
        mode,
        family,
        correct,
        wrong_known,
        unknown,
        aliases,
        query_aliases,
        rejected: e.rejected_writes,
        query_rejected,
        projection_calls: e.projection.calls,
        coordinates: e.projection.coordinates,
        ingest_macs: e.ingest.read_macs,
        query_macs: run.macs,
        far_rho_sum,
        far_rho_count,
    })
}
fn grid() -> R<(String, Vec<Row>)> {
    let mut text = format!("PROTOCOL,{PROTOCOL}\nBASE,{BASE}\nSTATUS,DEVELOPMENT_ONLY\n");
    text.push_str("ROW_HEADER,dimension,seed,group,payload,family,arm,gold,latent,correct,wrong_known,unknown,depth,halt,fresh_reads,query_macs,direct_reads,cross_subject_write_aliases,query_aliases,rejected_writes,query_rejected,first_q12\n");
    text.push_str("GRAM_HEADER,dimension,seed,group,payload,family,arm,write_index,subject,dot,norm_write_sq,norm_query_sq\n");
    let mut rows = Vec::new();
    for seed in 0..3 {
        for group in 0..2 {
            for payload in 0..4 {
                for family in Family::ALL {
                    for mode in Mode::ALL {
                        rows.push(one::<32>(family, seed, group, payload, mode, &mut text)?);
                        rows.push(one::<64>(family, seed, group, payload, mode, &mut text)?);
                        rows.push(one::<128>(family, seed, group, payload, mode, &mut text)?);
                        require(text.len() <= 16 * 1024 * 1024, "report budget")?;
                    }
                }
            }
        }
    }
    require(rows.len() == 1728, "grid completeness")?;
    for mode in Mode::ALL {
        let r: Vec<_> = rows.iter().filter(|r| r.mode == mode).collect();
        writeln!(text,"SUMMARY,{},rows={},correct={},wrong_known={},unknown={},write_aliases={},query_aliases={},rejected_writes={},rejected_queries={},projector_calls={},projector_coordinates={},ingest_macs={},query_macs={},answerable_correct={},answerable_rows={},far_rho_sq_sum={:.12},far_rho_count={}",mode.name(),r.len(),r.iter().filter(|r|r.correct).count(),r.iter().filter(|r|r.wrong_known).count(),r.iter().filter(|r|r.unknown).count(),r.iter().map(|r|r.aliases).sum::<usize>(),r.iter().map(|r|r.query_aliases).sum::<usize>(),r.iter().map(|r|r.rejected).sum::<usize>(),r.iter().filter(|r|r.query_rejected).count(),r.iter().map(|r|r.projection_calls).sum::<u64>(),r.iter().map(|r|r.coordinates).sum::<u64>(),r.iter().map(|r|r.ingest_macs).sum::<u64>(),r.iter().map(|r|r.query_macs).sum::<u64>(),r.iter().filter(|r|r.family.answerable()&&r.correct).count(),r.iter().filter(|r|r.family.answerable()).count(),r.iter().map(|r|r.far_rho_sum).sum::<f64>(),r.iter().map(|r|r.far_rho_count).sum::<u64>())?;
        for family in Family::ALL {
            let f: Vec<_> = r.iter().filter(|r| r.family == family).collect();
            writeln!(
                text,
                "FAMILY,{},{},rows={},correct={},wrong_known={},unknown={}",
                mode.name(),
                family.name(),
                f.len(),
                f.iter().filter(|r| r.correct).count(),
                f.iter().filter(|r| r.wrong_known).count(),
                f.iter().filter(|r| r.unknown).count()
            )?;
        }
    }
    text.push_str(
        "PROMOTION,BLOCKED,latent_projection_pilot_not_source_safe_context_qualification\n",
    );
    Ok((text, rows))
}
pub fn report() -> R<String> {
    Ok(grid()?.0)
}
pub fn conformance(which: u8) -> R<()> {
    match which {
        0 => {
            for g in 0..2 {
                for n in [false, true] {
                    for subject in 1..=8 {
                        let a = address(subject, g, n)?;
                        let b = complement(&a)?;
                        require(complement(&b)? == a, "involution")?;
                        require(
                            a.meaning_mask == b.meaning_mask
                                && a.routing == b.routing
                                && a.ambiguity == b.ambiguity,
                            "complement fields",
                        )?;
                    }
                }
            }
        }
        1 => {
            let r = Router::new()?;
            for g in 0..2 {
                let s = address(1, g, false)?;
                for seed in 0..3 {
                    let mut w = ProjectionWork::default();
                    let p = r
                        .key::<32>(&s, seed, Mode::Standard, &mut w)?
                        .ok_or("standard key")?;
                    require(
                        p.selected_experts == [PageId(g)] && w.calls == 1 && w.coordinates == 256,
                        "MOR projection",
                    )?;
                    let decision = r.route(&s, &mut ProjectionWork::default())?;
                    let native = DeltaKvSemanticProjector::<32>::new(
                        DeltaKvSemanticProjectionPolicy::default(),
                        seed,
                    )
                    .map_err(err)?
                    .project(&s, &decision, None)
                    .map_err(err)?;
                    require(native == p, "standard differs from DKV-2a")?;
                }
            }
        }
        2 => {
            require(
                contrast_key(&[4096, 0, -4096], &[-4096, 4096, 0])?
                    == Some(vec![4096, -2048, -2048]),
                "integer contrast",
            )?;
            require(
                contrast_key(&[4096, 0], &[4096, 0])?.is_none(),
                "zero rejection",
            )?;
            require(
                contrast_key(&[1], &[0]).is_err() && contrast_key(&[], &[]).is_err(),
                "malformed contrast",
            )?;
        }
        3 => {
            let r = Router::new()?;
            for seed in 0..3 {
                let s = address(2, 1, true)?;
                let p = r.key::<64>(&s, seed, Mode::Contrast, &mut ProjectionWork::default())?;
                let q = r.key::<64>(
                    &complement(&s)?,
                    seed,
                    Mode::Contrast,
                    &mut ProjectionWork::default(),
                )?;
                match (p, q) {
                    (Some(p), Some(q)) => require(
                        p.key_q12.iter().map(|v| -v).collect::<Vec<_>>() == q.key_q12,
                        "antisymmetry",
                    )?,
                    (None, None) => {}
                    _ => return Err("zero symmetry".into()),
                }
            }
        }
        4 => {
            for mode in [Mode::Standard, Mode::Contrast] {
                for family in Family::ALL {
                    for seed in 0..3 {
                        let e = Episode::build::<32>(family, seed, 0, 3, mode)?;
                        let f = Episode::build::<32>(family, seed, 0, 3, mode)?;
                        let mut fresh = f.input;
                        if let Input::Dkv { cached, .. } = &mut fresh {
                            *cached = false;
                        }
                        let a = execute(e.initial, e.page, e.input, e.query.as_deref())?;
                        let b = execute(f.initial, f.page, fresh, f.query.as_deref())?;
                        require(a.outcome == b.outcome, "cached/fresh mismatch")?;
                        require(a.reads <= 1 && b.reads >= a.reads, "read bound")?;
                    }
                }
            }
        }
        5 => {
            for mode in Mode::ALL {
                for g in 0..2 {
                    for payload in 0..4 {
                        let e = Episode::build::<64>(Family::Isolated, 0, g, payload, mode)?;
                        let r = execute(e.initial, e.page, e.input, e.query.as_deref())?;
                        require(
                            latent(&r.outcome.state)? == oracle(Family::Isolated, payload, g),
                            "isolated equivalence",
                        )?;
                    }
                }
            }
        }
        6 => {
            let r = Router::new()?;
            for mode in [Mode::Standard, Mode::Contrast] {
                let a = Episode::build::<32>(Family::Isolated, 0, 0, 0, mode)?;
                for payload in 1..4 {
                    let b = Episode::build::<32>(Family::Isolated, 0, 0, payload, mode)?;
                    require(
                        a.query == b.query && a.keys == b.keys,
                        "payload leaked into address",
                    )?;
                }
                let mut w = ProjectionWork::default();
                let _ = r.key::<32>(&address(1, 0, false)?, 0, mode, &mut w)?;
                require(
                    w.calls == if mode == Mode::Contrast { 2 } else { 1 },
                    "projection cost",
                )?;
            }
        }
        7 => {
            let (a, rows) = grid()?;
            let (b, _) = grid()?;
            require(a == b && rows.len() == 1728, "determinism/completeness")?;
            for mode in Mode::ALL {
                require(
                    rows.iter().filter(|r| r.mode == mode).count() == 576,
                    "arm completeness",
                )?;
            }
        }
        _ => return Err("unknown conformance check".into()),
    }
    Ok(())
}
fn main() -> R<()> {
    print!("{}", report()?);
    Ok(())
}
