//! GB-9: bit-sliced gate evaluation inside the real Pvp6Bank / SBG expert path.
//! Engine arms time MOR routing, atlas selection, SBG projection, the bank arm,
//! state write-back and query-major output words. Kernel arms time the bank only;
//! `route_only` times the MOR decision alone, to split the engine overhead.
//! Modes: deterministic, benchmark. No SIMD/GPU, energy or model-quality claim.

use std::hint::black_box;
use std::time::Instant;

use sml_core::{
    mor::{MixtureOfRouters, MorPolicy, PreviousRoute, RouterId},
    mor_registry::{MorRegistryPolicy, MorRouterRegistry, RouterSpecialization},
    pascal_gf2::pascal_gf2_in_place,
    pascal_vector_projection::{Pvp6Architecture, Pvp6PopulationProfile, Pvp6Workload},
    pvp6_executor::{Pvp6Bank, Pvp6ExecutionPath, Pvp6Gate},
    pvp6_sbg::{SbgAnfPlan, SbgCoordinate, SbgField},
    pvp6_sbg_atlas::{SbgExpertAtlas, SbgExpertBatch},
    pvp6_sbg_experts::SbgAnfExpert,
    semantic::{
        SemanticBitField, SemanticExpertSignature, SemanticMorBridge, SemanticMorTopK,
        SemanticRouterSignature, SemanticState,
    },
    PageId,
};

/// State width: 4 fields × 1024 coordinates hold 64 lanes × 64 gates outputs.
const S: usize = 1024;
const DOMAINS: [usize; 3] = [64, 256, 1024];
const DENSITIES: [usize; 3] = [2, 8, 32];
const GATES: [usize; 7] = [1, 2, 4, 8, 16, 32, 64];
const LANES: [usize; 2] = [8, 64];
const PAGES: u32 = 4;
const STATES: usize = 16;
const WARMUPS: usize = 3;
const MEASUREMENTS: usize = 9;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Arm {
    RouteOnly,
    EngineSelected,
    EngineSparseAnf,
    EngineBitslice,
    KernelLutResident,
    KernelSelected,
    KernelSparseAnf,
    KernelBitslice,
}

impl Arm {
    const ALL: [Self; 8] = [
        Self::RouteOnly,
        Self::EngineSelected,
        Self::EngineSparseAnf,
        Self::EngineBitslice,
        Self::KernelLutResident,
        Self::KernelSelected,
        Self::KernelSparseAnf,
        Self::KernelBitslice,
    ];

    const fn label(self) -> &'static str {
        match self {
            Self::RouteOnly => "route_only",
            Self::EngineSelected => "engine_selected",
            Self::EngineSparseAnf => "engine_sparse_anf",
            Self::EngineBitslice => "engine_bitslice",
            Self::KernelLutResident => "kernel_lut_resident",
            Self::KernelSelected => "kernel_selected",
            Self::KernelSparseAnf => "kernel_sparse_anf",
            Self::KernelBitslice => "kernel_bitslice",
        }
    }

    const fn path(self) -> Pvp6ExecutionPath {
        match self {
            Self::RouteOnly
            | Self::EngineSelected
            | Self::KernelSelected
            | Self::KernelLutResident => Pvp6ExecutionPath::Selected,
            Self::EngineSparseAnf | Self::KernelSparseAnf => {
                Pvp6ExecutionPath::Scalar(Pvp6Architecture::SparseAnfScan)
            }
            Self::EngineBitslice | Self::KernelBitslice => Pvp6ExecutionPath::Bitsliced,
        }
    }

    const fn is_engine(self) -> bool {
        matches!(
            self,
            Self::EngineSelected | Self::EngineSparseAnf | Self::EngineBitslice
        )
    }
}

fn lcg(state: &mut u64) -> u64 {
    *state = state
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1_442_695_040_888_963_407);
    *state >> 11
}

fn bank<const D: usize>(seed: u64, gates: usize, density: usize) -> Pvp6Bank<D> {
    let mut state = seed;
    let gates = (0..gates)
        .map(|_| {
            let mut terms = Vec::with_capacity(density);
            while terms.len() < density {
                let mask = (lcg(&mut state) % D as u64) as u16;
                if !terms.contains(&mask) {
                    terms.push(mask);
                }
            }
            Pvp6Gate::from_monomials(&terms).expect("frozen gate")
        })
        .collect();
    Pvp6Bank::new(gates).expect("frozen bank")
}

fn plan<const D: usize>(lanes: usize, gates: usize) -> SbgAnfPlan<S, D> {
    let variables = D.trailing_zeros() as usize;
    let inputs = (0..lanes)
        .map(|lane| {
            (0..variables)
                .map(|bit| SbgCoordinate {
                    field: SbgField::Value,
                    bit: lane * variables + bit,
                })
                .collect()
        })
        .collect();
    let fields = [
        SbgField::Meaning,
        SbgField::Ambiguity,
        SbgField::Routing,
        SbgField::Value,
    ];
    let outputs = (0..lanes * gates)
        .map(|i| SbgCoordinate {
            field: fields[i / S],
            bit: i % S,
        })
        .collect();
    SbgAnfPlan::new(inputs, outputs).expect("frozen plan")
}

fn random_field(seed: &mut u64) -> SemanticBitField<S> {
    let indices = (0..S).filter(|_| lcg(seed) & 1 == 1).collect::<Vec<_>>();
    SemanticBitField::from_set_indices(&indices).expect("bounded field")
}

struct Router {
    bridge: SemanticMorBridge<S>,
    registry: MorRouterRegistry,
    engine: MixtureOfRouters,
    previous: PreviousRoute,
}

impl Router {
    fn new(seed: &mut u64) -> Self {
        let experts = (0..PAGES).map(PageId).collect::<Vec<_>>();
        let registry = MorRouterRegistry::new(
            vec![RouterSpecialization {
                router_id: RouterId(0),
                experts: experts.clone(),
            }],
            MorRegistryPolicy {
                expert_count: PAGES,
                max_routers: 1,
                max_edges: PAGES as usize,
                max_fanout: PAGES as usize,
                require_full_coverage: true,
            },
        )
        .expect("frozen registry");
        let bridge = SemanticMorBridge::new(
            vec![SemanticRouterSignature {
                router_id: RouterId(0),
                signature: random_field(seed),
            }],
            experts
                .iter()
                .map(|&page_id| SemanticExpertSignature {
                    page_id,
                    signature: random_field(seed),
                })
                .collect(),
            SemanticMorTopK::One,
        )
        .expect("frozen bridge");
        let engine = MixtureOfRouters::new(MorPolicy {
            max_experts: 1,
            ..MorPolicy::default()
        })
        .expect("frozen MOR policy");
        Self {
            bridge,
            registry,
            engine,
            previous: PreviousRoute::default(),
        }
    }
}

struct Cell<const D: usize> {
    router: Router,
    atlas: SbgExpertAtlas<S, D>,
    states: Vec<SemanticState<S>>,
    /// Per state: routed page index and its projected queries, for kernel arms.
    routed: Vec<(usize, Vec<usize>)>,
    /// Resident truth tables per page and gate, for the LUT control.
    truth: Vec<Vec<Vec<u64>>>,
    workload: Pvp6Workload,
}

impl<const D: usize> Cell<D> {
    fn new(density: usize, gates: usize, lanes: usize) -> Self {
        let mut seed =
            (D as u64) << 32 | (density as u64) << 16 | (gates as u64) << 8 | lanes as u64;
        let router = Router::new(&mut seed);
        let pages = (0..PAGES)
            .map(|id| {
                SbgAnfExpert::new(
                    PageId(id),
                    bank::<D>(lcg(&mut seed), gates, density),
                    plan::<D>(lanes, gates),
                    Pvp6PopulationProfile::AffineUniform,
                )
                .expect("frozen page")
            })
            .collect();
        let atlas = SbgExpertAtlas::new(pages, PAGES as usize).expect("frozen atlas");
        let states = (0..STATES)
            .map(|_| {
                let mut state = SemanticState::zeroed().expect("state");
                state.value = random_field(&mut seed);
                state.routing = random_field(&mut seed);
                state
            })
            .collect::<Vec<_>>();
        let workload = Pvp6Workload::new(
            density as u32,
            lanes as u32,
            1,
            Pvp6PopulationProfile::AffineUniform,
        );
        let routed = states
            .iter()
            .map(|state| {
                let page = route(&router, state)[0].0 as usize;
                let queries = atlas.pages()[page].plan().queries(state).expect("queries");
                (page, queries)
            })
            .collect();
        let truth = atlas
            .pages()
            .iter()
            .map(|page| {
                page.bank()
                    .gates()
                    .iter()
                    .map(|gate| {
                        let mut words = gate.coefficients().to_vec();
                        pascal_gf2_in_place(&mut words).expect("Pascal truth table");
                        words
                    })
                    .collect()
            })
            .collect();
        Self {
            router,
            atlas,
            states,
            routed,
            truth,
            workload,
        }
    }

    fn engine(&self, arm: Arm, index: usize) -> SbgExpertBatch<S> {
        let state = &self.states[index % STATES];
        let route = self
            .router
            .bridge
            .route(
                state,
                &self.router.registry,
                &self.router.engine,
                &[],
                &self.router.previous,
            )
            .expect("MOR route");
        self.atlas
            .execute_on(arm.path(), state, &route.decision)
            .expect("exact atlas execution")
    }

    fn kernel(&self, arm: Arm, index: usize) -> Vec<u64> {
        let (page, queries) = &self.routed[index % STATES];
        if arm == Arm::KernelLutResident {
            let truth = &self.truth[*page];
            let words_per_gate = queries.len().div_ceil(64);
            let mut packed = vec![0_u64; truth.len() * words_per_gate];
            for (gate, table) in truth.iter().enumerate() {
                for (lane, &query) in queries.iter().enumerate() {
                    let bit = (table[query / 64] >> (query % 64)) & 1;
                    packed[gate * words_per_gate + lane / 64] |= bit << (lane % 64);
                }
            }
            return packed;
        }
        self.atlas.pages()[*page]
            .bank()
            .evaluate_on(arm.path(), self.workload, queries)
            .expect("exact bank evaluation")
            .packed_words()
            .to_vec()
    }

    /// Checksum of one call: engine folds next-state words and outputs,
    /// kernel folds gate-major packed words. Both are layout-stable.
    fn run(&self, arm: Arm, index: usize) -> u64 {
        if arm == Arm::RouteOnly {
            let pages = route(&self.router, &self.states[black_box(index) % STATES]);
            return fold(0, &pages.iter().map(|p| u64::from(p.0)).collect::<Vec<_>>());
        }
        if arm.is_engine() {
            let batch = self.engine(black_box(arm), black_box(index));
            let state = &batch.state;
            let mut sum = 0_u64;
            for words in [
                state.value.words(),
                state.meaning_mask.words(),
                state.ambiguity.words(),
                state.routing.words(),
            ] {
                sum = fold(sum, words);
            }
            for page in &batch.pages {
                sum = fold(sum, &page.outputs);
            }
            sum
        } else {
            fold(0, &self.kernel(black_box(arm), black_box(index)))
        }
    }

    /// Exact parity on every frozen state, before any timing.
    fn qualify(&self) {
        let selected = self.workload.select().expect("policy");
        assert_eq!(
            selected.architecture,
            Pvp6Architecture::PascalAnfMaterialize,
            "grid must stay on the conservative Pascal policy arm"
        );
        for index in 0..STATES {
            let reference = self.engine(Arm::EngineSelected, index);
            for arm in [Arm::EngineSparseAnf, Arm::EngineBitslice] {
                assert_eq!(self.engine(arm, index), reference, "engine divergence");
            }
            let (page, queries) = &self.routed[index];
            assert_eq!(reference.pages.len(), 1);
            assert_eq!(reference.pages[0].page, PageId(*page as u32));
            assert_eq!(&reference.pages[0].queries, queries);
            let kernel = self.kernel(Arm::KernelSelected, index);
            for arm in [
                Arm::KernelLutResident,
                Arm::KernelSparseAnf,
                Arm::KernelBitslice,
            ] {
                assert_eq!(self.kernel(arm, index), kernel, "kernel divergence");
            }
            let result = self.atlas.pages()[*page]
                .bank()
                .evaluate(self.workload, queries)
                .expect("evaluate");
            assert_eq!(result.packed_words(), kernel);
            assert_eq!(
                result.to_query_major_words().expect("query major"),
                reference.pages[0].outputs,
                "kernel and engine outputs disagree"
            );
            for (gate, terms) in self.atlas.pages()[*page].bank().gates().iter().enumerate() {
                for (lane, &query) in queries.iter().enumerate() {
                    let literal = terms.monomials().iter().fold(false, |parity, &mask| {
                        parity ^ (usize::from(mask) & query == usize::from(mask))
                    });
                    assert_eq!(result.value(gate, lane), Some(literal), "oracle divergence");
                }
            }
        }
    }
}

fn route(router: &Router, state: &SemanticState<S>) -> Vec<PageId> {
    router
        .bridge
        .route(
            state,
            &router.registry,
            &router.engine,
            &[],
            &router.previous,
        )
        .expect("MOR route")
        .decision
        .selected_experts
}

fn fold(sum: u64, words: &[u64]) -> u64 {
    words.iter().enumerate().fold(sum, |acc, (i, &word)| {
        acc.rotate_left(7).wrapping_add(word.rotate_left(i as u32))
    })
}

fn iterations(gates: usize) -> usize {
    4096 / gates
}

fn measure<const D: usize>(cell: &Cell<D>, arm: Arm, iterations: usize) -> (u128, u64) {
    let start = Instant::now();
    let mut checksum = 0_u64;
    for index in 0..iterations {
        checksum = checksum.wrapping_add(black_box(cell.run(black_box(arm), index)));
    }
    (start.elapsed().as_nanos(), checksum)
}

fn reference_checksum<const D: usize>(cell: &Cell<D>, arm: Arm, iterations: usize) -> u64 {
    (0..iterations).fold(0_u64, |sum, index| {
        let reference = if arm == Arm::RouteOnly {
            Arm::RouteOnly
        } else if arm.is_engine() {
            Arm::EngineSelected
        } else {
            Arm::KernelSelected
        };
        sum.wrapping_add(cell.run(reference, index))
    })
}

fn execute_domain<const D: usize>(mode: &str) {
    for density in DENSITIES {
        for lanes in LANES {
            for gates in GATES {
                let cell = Cell::<D>::new(density, gates, lanes);
                cell.qualify();
                let iterations = iterations(gates);
                let expected = Arm::ALL.map(|arm| reference_checksum(&cell, arm, iterations));
                let trials = if mode == "deterministic" {
                    1
                } else {
                    WARMUPS + MEASUREMENTS
                };
                for trial in 0..trials {
                    for order in 0..Arm::ALL.len() {
                        let slot = (order + trial) % Arm::ALL.len();
                        let arm = Arm::ALL[slot];
                        let (elapsed, checksum) = if mode == "deterministic" {
                            (
                                0,
                                (0..iterations).fold(0_u64, |sum, index| {
                                    sum.wrapping_add(cell.run(arm, index))
                                }),
                            )
                        } else {
                            measure(&cell, arm, iterations)
                        };
                        assert_eq!(checksum, expected[slot], "timed arm lost exact parity");
                        if mode == "deterministic" {
                            println!(
                                "{D},{density},{lanes},{gates},{},{iterations},{checksum}",
                                arm.label()
                            );
                        } else if trial >= WARMUPS {
                            println!(
                                "{D},{density},{lanes},{gates},{},{},{iterations},{elapsed},{checksum}",
                                arm.label(),
                                trial - WARMUPS
                            );
                        }
                    }
                }
            }
        }
    }
}

fn execute(mode: &str) {
    match mode {
        "deterministic" => {
            println!("domain,density,lanes,gates,method,iterations,checksum")
        }
        "benchmark" => {
            println!("domain,density,lanes,gates,method,repeat,iterations,elapsed_ns,checksum")
        }
        _ => panic!("mode must be deterministic or benchmark"),
    }
    execute_domain::<{ DOMAINS[0] }>(mode);
    execute_domain::<{ DOMAINS[1] }>(mode);
    execute_domain::<{ DOMAINS[2] }>(mode);
}

fn main() {
    let mode = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "deterministic".to_owned());
    execute(&mode);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_frozen_cell_routes_one_page_and_all_arms_match_exactly() {
        fn domain<const D: usize>() {
            for density in DENSITIES {
                for lanes in LANES {
                    for gates in [1, 64] {
                        Cell::<D>::new(density, gates, lanes).qualify();
                    }
                }
            }
        }
        domain::<{ DOMAINS[0] }>();
        domain::<{ DOMAINS[1] }>();
        domain::<{ DOMAINS[2] }>();
    }

    #[test]
    fn routing_visits_more_than_one_page_across_frozen_states() {
        let cell = Cell::<64>::new(2, 4, 8);
        let mut pages = cell
            .routed
            .iter()
            .map(|(page, _)| *page)
            .collect::<Vec<_>>();
        pages.sort_unstable();
        pages.dedup();
        assert!(
            pages.len() > 1,
            "router must not degenerate to a constant page"
        );
    }

    #[test]
    fn frozen_iterations_equalize_call_counts_and_grid_fits_state() {
        for gates in GATES {
            assert_eq!(iterations(gates) * gates, 4096);
            for lanes in LANES {
                assert!(lanes * gates <= 4 * S);
            }
        }
        for domain in DOMAINS {
            assert!(LANES[1] * domain.trailing_zeros() as usize <= S);
        }
    }
}
