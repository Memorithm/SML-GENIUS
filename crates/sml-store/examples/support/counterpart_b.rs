//! Counterpart B fixture family. Supplied fixed route is NOT native MOR execution.
#![allow(dead_code)]
use sml_core::{
    delta_kv_semantic::{
        DeltaKvSemanticError, DeltaKvSemanticKeyEvidence, DeltaKvSemanticProjectionPolicy,
        DeltaKvSemanticProjector,
    },
    mor::{MixtureOfRouters, MorDecision, MorPolicy, PreviousRoute, RouterId},
    mor_registry::{MorRegistryPolicy, MorRouterRegistry, RouterSpecialization},
    semantic::{
        SemanticBitField, SemanticExpertSignature, SemanticMorBridge, SemanticMorTopK,
        SemanticRouterSignature, SemanticState,
    },
    PageId,
};
pub type R<T> = Result<T, Box<dyn std::error::Error>>;
pub const BITS: usize = 64;
const POSITIONS: usize = 192;
const MOR_PAGE: u32 = 5;
struct Stream(u64);

impl Stream {
    fn new(base: u64) -> Self {
        Self(base | 1)
    }

    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
}

fn base(seed: u64) -> u64 {
    seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0xA5A5_A5A5_A5A5_A5A5
}

pub fn target_state(seed: u64) -> SemanticState<BITS> {
    let mut stream = Stream::new(base(seed));
    let mut state = SemanticState::<BITS>::zeroed().unwrap();
    for index in 0..BITS {
        state.value.set(index, stream.next() & 1 == 1).unwrap();
    }
    for index in 0..BITS {
        state
            .meaning_mask
            .set(index, stream.next() & 1 == 1)
            .unwrap();
    }
    for index in 0..BITS {
        state.routing.set(index, stream.next() & 1 == 1).unwrap();
    }
    state
}

fn flip_positions(seed: u64, distractor: usize, hamming: usize) -> Vec<usize> {
    let mut stream =
        Stream::new(base(seed) ^ (distractor as u64 + 1).wrapping_mul(0xD6E8_FEB8_6659_FD93));
    let mut pool: Vec<usize> = (0..POSITIONS).collect();
    for k in 0..hamming {
        let j = k + (stream.next() % (POSITIONS - k) as u64) as usize;
        pool.swap(k, j);
    }
    pool[..hamming].to_vec()
}

pub fn distractor_state(seed: u64, distractor: usize, hamming: usize) -> SemanticState<BITS> {
    let mut state = target_state(seed);
    for position in flip_positions(seed, distractor, hamming) {
        let bit = position % BITS;
        let field = match position / BITS {
            0 => &mut state.value,
            1 => &mut state.meaning_mask,
            _ => &mut state.routing,
        };
        let current = field.get(bit).unwrap();
        field.set(bit, !current).unwrap();
    }
    state
}

pub fn supplied_decision() -> MorDecision {
    MorDecision {
        active_routers: Vec::new(),
        ranked_experts: Vec::new(),
        selected_experts: vec![PageId(MOR_PAGE)],
        noise_ppm: 0,
        structural_disagreement_ppm: 0,
        expert_mass_threshold_ppm: 0,
        request_precision_escalation: false,
        hysteresis_retained_previous_primary: false,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteMode {
    Supplied,
    NativeFixed,
    NativeDual,
}
impl RouteMode {
    pub const ALL: [Self; 3] = [Self::Supplied, Self::NativeFixed, Self::NativeDual];
    pub fn name(self) -> &'static str {
        match self {
            Self::Supplied => "supplied",
            Self::NativeFixed => "native_fixed",
            Self::NativeDual => "native_dual",
        }
    }
}
fn boxed(e: impl std::fmt::Debug) -> Box<dyn std::error::Error> {
    format!("{e:?}").into()
}
pub fn route(state: &SemanticState<BITS>, mode: RouteMode) -> R<MorDecision> {
    if mode == RouteMode::Supplied {
        return Ok(supplied_decision());
    }
    let pages = if mode == RouteMode::NativeFixed {
        vec![5]
    } else {
        vec![0, 1]
    };
    let sig = |page: u32| -> R<SemanticBitField<BITS>> {
        let bits = (0..BITS)
            .filter(|i| mode == RouteMode::NativeFixed || *i % 2 == page as usize)
            .collect::<Vec<_>>();
        SemanticBitField::from_set_indices(&bits).map_err(boxed)
    };
    let registry = MorRouterRegistry::new(
        pages
            .iter()
            .map(|&p| RouterSpecialization {
                router_id: RouterId(p),
                experts: vec![PageId(p)],
            })
            .collect(),
        MorRegistryPolicy {
            expert_count: 6,
            max_routers: 2,
            max_edges: 2,
            max_fanout: 1,
            require_full_coverage: false,
        },
    )
    .map_err(boxed)?;
    let routers = pages
        .iter()
        .map(|&p| {
            Ok(SemanticRouterSignature {
                router_id: RouterId(p),
                signature: sig(p)?,
            })
        })
        .collect::<R<Vec<_>>>()?;
    let experts = pages
        .iter()
        .map(|&p| {
            Ok(SemanticExpertSignature {
                page_id: PageId(p),
                signature: sig(p)?,
            })
        })
        .collect::<R<Vec<_>>>()?;
    let bridge = SemanticMorBridge::new(routers, experts, SemanticMorTopK::One).map_err(boxed)?;
    let engine = MixtureOfRouters::new(MorPolicy {
        max_experts: 1,
        ..MorPolicy::default()
    })
    .map_err(boxed)?;
    let result = bridge
        .route(state, &registry, &engine, &[], &PreviousRoute::default())
        .map_err(boxed)?
        .decision;
    if result.selected_experts.len() != 1 || result.active_routers.is_empty() {
        return Err("native routing evidence missing".into());
    }
    Ok(result)
}
#[derive(Debug, Clone, Copy, Default)]
pub struct Work {
    pub mor_calls: u64,
    pub projector_calls: u64,
    pub coordinates: u64,
    pub contrast_coordinates: u64,
}
pub fn project<const K: usize>(
    state: &SemanticState<BITS>,
    seed: u64,
    mode: RouteMode,
    contrast: bool,
    work: &mut Work,
) -> R<Option<DeltaKvSemanticKeyEvidence>> {
    let decision = route(state, mode)?;
    work.mor_calls += u64::from(mode != RouteMode::Supplied);
    let p = DeltaKvSemanticProjector::<K>::new(DeltaKvSemanticProjectionPolicy::default(), seed)
        .map_err(boxed)?;
    let mut apply = |s: &SemanticState<BITS>| -> R<Option<DeltaKvSemanticKeyEvidence>> {
        work.projector_calls += 1;
        work.coordinates += 4 * BITS as u64;
        match p.project(s, &decision, None) {
            Ok(v) => Ok(Some(v)),
            Err(DeltaKvSemanticError::ZeroProjectedKey) => Ok(None),
            Err(e) => Err(boxed(e)),
        }
    };
    let first = apply(state)?;
    if !contrast {
        return Ok(first);
    }
    let mut complement = state.clone();
    for i in 0..48 {
        let b = complement.value.get(i).map_err(boxed)?;
        complement.value.set(i, !b).map_err(boxed)?;
    }
    let second = apply(&complement)?;
    let (Some(mut a), Some(b)) = (first, second) else {
        return Ok(None);
    };
    work.contrast_coordinates += K as u64;
    for (x, y) in a.key_q12.iter_mut().zip(b.key_q12) {
        *x = ((i32::from(*x) - i32::from(y)) / 2) as i16;
    }
    if a.key_q12.iter().all(|x| *x == 0) {
        return Ok(None);
    }
    // a.counters describes the original first call; Work records aggregate calls.
    Ok(Some(a))
}
