//! Matched native PVP6/SBG diagnostic, not a trained-model throughput claim.
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

fn bank(gates: usize, density: usize, degree: u32, overlap: bool) -> Pvp6Bank<64> {
    let supports: Vec<u16> = (0_u16..64).filter(|m| m.count_ones() <= degree).collect();
    assert!(density <= supports.len());
    Pvp6Bank::new(
        (0..gates)
            .map(|g| {
                let offset = if overlap { 7 } else { g * 17 + g * g + 7 };
                let terms: Vec<_> = (0..density)
                    .map(|i| supports[(offset + i) % supports.len()])
                    .collect();
                Pvp6Gate::from_monomials(&terms).unwrap()
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
        println!("GB_CUDA_11E_RAW scope={scope} {shape} path={name} ns={samples:?}");
        samples.sort_unstable();
        println!(
            "GB_CUDA_11E scope={scope} {shape} path={name} median_ns={} min_ns={} max_ns={} samples={} parity=PASS",
            samples[samples.len() / 2],
            samples[0],
            samples[samples.len() - 1],
            samples.len()
        );
    }
}

fn batch(gates: usize, density: usize, degree: u32, overlap: bool, count: usize) {
    let bank = bank(gates, density, degree, overlap);
    let queries: Vec<_> = (0..count)
        .map(|i| {
            if overlap {
                (i * 13 + i / 11) % 8
            } else {
                (i * 19 + i / 3 + 5) % 64
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
        println!("GB_CUDA_11E_DECISION {shape} path={name} calls={CALLS} ns={values:?}");
    }
}

fn recurrent(overlap: bool) {
    const S: usize = 2048;
    const LANES: usize = 128;
    const GATES: usize = 16;
    const DEPTH: u32 = 8;
    let bank = bank(GATES, 8, 3, overlap);
    let coordinate = |bit| SbgCoordinate {
        field: SbgField::Value,
        bit,
    };
    let inputs = (0..LANES)
        .map(|i| (0..6).map(|j| coordinate((i * GATES + j) % S)).collect())
        .collect();
    let outputs = (0..S).map(coordinate).collect();
    let plan = SbgAnfPlan::<S, 64>::new(inputs, outputs).unwrap();
    let prepared = bank.diagnostic_hybrid_path();
    assert_eq!(prepared, bank.diagnostic_hybrid_scan_path());
    let base = SbgAnfKernel::new(bank, plan, Pvp6PopulationProfile::AffineUniform, DEPTH).unwrap();
    let mut initial = SemanticState::<S>::zeroed().unwrap();
    for i in 0..S {
        initial.value.set(i, (i * 37 + i / 7) % 11 < 5).unwrap();
    }
    initial.routing.set(S - 1, true).unwrap();
    let run = |path| {
        let mut kernel = base.clone().with_execution_path(path);
        let mut state = initial.clone();
        let start = Instant::now();
        for depth in 1..=DEPTH {
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
            "gates={GATES} density=8 degree_cap=3 overlap={overlap} queries={LANES} depth={DEPTH}"
        ),
        &mut times,
    );
}

fn main() {
    for gates in [2, 3, 7, 9, 12, 24, 48] {
        for (density, degree) in [(2, 1), (5, 2), (7, 3), (13, 4), (23, 5)] {
            for overlap in [false, true] {
                for count in [3, 17, 63, 129, 257] {
                    batch(gates, density, degree, overlap, count);
                }
            }
        }
    }
    recurrent(false);
    recurrent(true);
    println!("GB_CUDA_11E_RESULT=PASS");
}
