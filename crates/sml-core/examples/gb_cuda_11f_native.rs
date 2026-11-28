//! Independent frozen-route holdout. No production selector or threshold change.
use sml_core::{
    pascal_vector_projection::{Pvp6Architecture, Pvp6PopulationProfile, Pvp6Workload},
    pvp6_executor::{Pvp6Bank, Pvp6ExecutionPath, Pvp6Gate},
    pvp6_sbg::{SbgAnfPlan, SbgCoordinate, SbgField},
    pvp6_sbg_kernel::SbgAnfKernel,
    semantic::{SemanticRewriter, SemanticState},
};
use std::{hint::black_box, time::Instant};

const PATHS: [(&str, Pvp6ExecutionPath); 9] = [
    ("selected", Pvp6ExecutionPath::Selected),
    (
        "sparse",
        Pvp6ExecutionPath::Scalar(Pvp6Architecture::SparseAnfScan),
    ),
    ("unshared", Pvp6ExecutionPath::BitslicedUnshared),
    ("shared", Pvp6ExecutionPath::BitslicedShared),
    ("legacy", Pvp6ExecutionPath::BitslicedCost),
    ("selected_repeat", Pvp6ExecutionPath::Selected),
    ("scan", Pvp6ExecutionPath::Selected),
    ("cached", Pvp6ExecutionPath::Selected),
    ("prepared", Pvp6ExecutionPath::Selected),
];

fn next_random(state: &mut u64) -> u64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state
}

fn bank(gates: usize, density: usize, degree: u32, overlap: bool) -> Pvp6Bank<64> {
    let supports: Vec<u16> = (0_u16..64).filter(|m| m.count_ones() <= degree).collect();
    assert!(density <= supports.len());
    Pvp6Bank::new(
        (0..gates)
            .map(|g| {
                let mut shuffled = supports.clone();
                let mut seed = 0x8c1f_39d2_7a65_b40e_u64
                    ^ ((gates as u64) << 32)
                    ^ ((density as u64) << 16)
                    ^ u64::from(degree)
                    ^ if overlap { 0 } else { (g as u64 + 1) << 1 };
                for i in (1..shuffled.len()).rev() {
                    let j = (next_random(&mut seed) % (i as u64 + 1)) as usize;
                    shuffled.swap(i, j);
                }
                Pvp6Gate::from_monomials(&shuffled[..density]).unwrap()
            })
            .collect(),
    )
    .unwrap()
}

fn report(scope: &str, shape: &str, times: &mut [Vec<u128>; 9]) {
    for ((name, _), samples) in PATHS.iter().zip(times) {
        if samples.is_empty() {
            continue;
        }
        println!("GB_CUDA_11F_RAW scope={scope} {shape} path={name} ns={samples:?}");
        samples.sort_unstable();
        println!(
            "GB_CUDA_11F scope={scope} {shape} path={name} median_ns={} min_ns={} max_ns={} samples={} parity=PASS",
            samples[samples.len() / 2],
            samples[0],
            samples[samples.len() - 1],
            samples.len()
        );
    }
}

fn batch(gates: usize, density: usize, degree: u32, overlap: bool, count: usize) {
    let bank = bank(gates, density, degree, overlap);
    let mut seed = 0x52d9_61a8_ef34_0bc7_u64
        ^ ((gates as u64) << 32)
        ^ ((density as u64) << 16)
        ^ count as u64;
    let queries: Vec<_> = (0..count)
        .map(|_| {
            let q = next_random(&mut seed) as usize;
            if overlap {
                ((q & 15) << 2) | 1
            } else {
                q & 63
            }
        })
        .collect();
    let workload = Pvp6Workload::new(
        density as u32,
        count as u32,
        1,
        Pvp6PopulationProfile::AffineUniform,
    );
    let expected = bank.evaluate_on(PATHS[1].1, workload, &queries).unwrap();
    for (g, gate) in bank.gates().iter().enumerate() {
        for (i, &q) in queries.iter().enumerate() {
            let literal = gate.monomials().iter().fold(false, |v, &m| {
                v ^ (0..6).all(|b| m & (1 << b) == 0 || q & (1 << b) != 0)
            });
            assert_eq!(expected.value(g, i), Some(literal));
        }
    }
    let prepared = bank.diagnostic_hybrid_path();
    assert_eq!(prepared, bank.diagnostic_hybrid_scan_path());
    let mut times: [Vec<u128>; 9] = std::array::from_fn(|_| Vec::new());
    for rep in 0..10 {
        for j in 0..9 {
            let i = (rep + j) % 9;
            let start = Instant::now();
            let path = match i {
                4 if gates <= 8 => Pvp6ExecutionPath::Selected,
                6 => bank.diagnostic_hybrid_scan_path(),
                7 => bank.diagnostic_hybrid_path(),
                8 => prepared,
                _ => PATHS[i].1,
            };
            let actual = black_box(bank.evaluate_on(path, workload, &queries).unwrap());
            let elapsed = start.elapsed().as_nanos();
            assert_eq!(actual, expected);
            if rep != 0 {
                times[i].push(elapsed);
            }
        }
    }
    let shape = format!(
        "gates={gates} density={density} degree_cap={degree} overlap={overlap} queries={count}"
    );
    report("batch", &shape, &mut times);
    decision(&bank, &shape);
}

// Separate decision-only measurement; each raw sample covers 1024 calls.
// The loop/black_box cost is retained in both arms, not subtracted.
fn decision(bank: &Pvp6Bank<64>, shape: &str) {
    const CALLS: usize = 1024;
    let expected = bank.diagnostic_hybrid_path();
    let mut samples: [Vec<u128>; 2] = std::array::from_fn(|_| Vec::new());
    for rep in 0..10 {
        for j in 0..2 {
            let i = (rep + j) % 2;
            let start = Instant::now();
            for _ in 0..CALLS {
                let bank = black_box(bank);
                let route = if i == 0 {
                    bank.diagnostic_hybrid_scan_path()
                } else {
                    bank.diagnostic_hybrid_path()
                };
                black_box(route);
            }
            let elapsed = start.elapsed().as_nanos();
            assert_eq!(bank.diagnostic_hybrid_scan_path(), expected);
            if rep != 0 {
                samples[i].push(elapsed);
            }
        }
    }
    for (name, values) in ["scan", "cached"].into_iter().zip(samples) {
        println!("GB_CUDA_11F_DECISION {shape} path={name} calls={CALLS} ns={values:?}");
    }
}

fn recurrent<const S: usize>(gates: usize, lanes: usize, depth: u32, overlap: bool) {
    assert_eq!(S, gates * lanes);
    let bank = bank(gates, 9, 4, overlap);
    let coordinate = |bit| SbgCoordinate {
        field: SbgField::Value,
        bit,
    };
    let inputs = (0..lanes)
        .map(|i| {
            (0..6)
                .map(|j| coordinate((i * gates + j * 7 + 3) % S))
                .collect()
        })
        .collect();
    let outputs = (0..S).map(coordinate).collect();
    let plan = SbgAnfPlan::<S, 64>::new(inputs, outputs).unwrap();
    let prepared = bank.diagnostic_hybrid_path();
    assert_eq!(prepared, bank.diagnostic_hybrid_scan_path());
    let base = SbgAnfKernel::new(bank, plan, Pvp6PopulationProfile::AffineUniform, depth).unwrap();
    let mut initial = SemanticState::<S>::zeroed().unwrap();
    for i in 0..S {
        initial.value.set(i, (i * 37 + i / 7) % 11 < 5).unwrap();
    }
    initial.routing.set(S - 1, true).unwrap();
    let run = |path| {
        let mut kernel = base.clone().with_execution_path(path);
        let mut state = initial.clone();
        let start = Instant::now();
        for depth in 1..=depth {
            state = kernel.rewrite(&state, depth).unwrap().state;
        }
        let elapsed = start.elapsed().as_nanos();
        (state, kernel, elapsed)
    };
    let (truth, reference, _) = run(PATHS[1].1);
    let mut times: [Vec<u128>; 9] = std::array::from_fn(|_| Vec::new());
    for rep in 0..10 {
        for j in 0..9 {
            let i = (rep + j) % 9;
            if i == 6 || i == 7 {
                continue;
            }
            let path = if i == 8 { prepared } else { PATHS[i].1 };
            let (state, kernel, elapsed) = run(path);
            assert_eq!(state, truth);
            assert_eq!(kernel.cycles(), reference.cycles());
            assert_eq!(kernel.usage(), reference.usage());
            if rep != 0 {
                times[i].push(elapsed);
            }
        }
    }
    report(
        "recurrent",
        &format!(
            "gates={gates} density=9 degree_cap=4 overlap={overlap} queries={lanes} depth={depth}"
        ),
        &mut times,
    );
}

fn main() {
    for gates in [4, 8, 10, 17, 32, 63] {
        for (density, degree) in [(3, 2), (6, 3), (9, 4), (17, 5), (29, 6)] {
            for overlap in [false, true] {
                for count in [5, 31, 64, 66, 191, 513] {
                    batch(gates, density, degree, overlap, count);
                }
            }
        }
    }
    for overlap in [false, true] {
        recurrent::<512>(4, 128, 3, overlap);
        recurrent::<1024>(8, 128, 5, overlap);
        recurrent::<1088>(17, 64, 11, overlap);
        recurrent::<2048>(32, 64, 13, overlap);
    }
    println!("GB_CUDA_11F_RESULT=PASS");
}
