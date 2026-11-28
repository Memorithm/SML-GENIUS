//! Same-source construction control for pre-cache and cached SML revisions.
//! Compile this exact file against both revisions. Gate preparation, input
//! cloning, result destruction and allocation of the output vector are outside
//! the measured interval. Retain the block loop cost; never subtract it.
use sml_core::{
    pascal_vector_projection::{Pvp6Architecture, Pvp6PopulationProfile, Pvp6Workload},
    pvp6_executor::{Pvp6Bank, Pvp6ExecutionPath, Pvp6Gate},
};
use std::{hint::black_box, time::Instant};

const CALLS: usize = 128;
const REPS: usize = 21;

fn fixture(gates: usize, density: usize, degree: u32) -> Vec<Pvp6Gate<64>> {
    let supports: Vec<u16> = (0_u16..64).filter(|m| m.count_ones() <= degree).collect();
    (0..gates)
        .map(|g| {
            let mut shuffled = supports.clone();
            let mut seed = 0x94d8_3b16_c257_ae01_u64 ^ ((g as u64 + 1) << 16);
            for i in (1..shuffled.len()).rev() {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                let j = (seed % (i as u64 + 1)) as usize;
                shuffled.swap(i, j);
            }
            Pvp6Gate::from_monomials(&shuffled[..density]).unwrap()
        })
        .collect()
}

fn run(gates: usize, density: usize, degree: u32) {
    let fixture = fixture(gates, density, degree);
    let mut digest = 0xcbf2_9ce4_8422_2325_u64;
    for gate in &fixture {
        for &mask in gate.monomials() {
            digest ^= u64::from(mask);
            digest = digest.wrapping_mul(0x100_0000_01b3);
        }
    }
    let bank = Pvp6Bank::new(fixture.clone()).unwrap();
    let queries: Vec<_> = (0..67).map(|i| (i * 29 + i / 5 + 11) % 64).collect();
    let workload = Pvp6Workload::new(
        density as u32,
        queries.len() as u32,
        1,
        Pvp6PopulationProfile::AffineUniform,
    );
    let expected = bank
        .evaluate_on(
            Pvp6ExecutionPath::Scalar(Pvp6Architecture::SparseAnfScan),
            workload,
            &queries,
        )
        .unwrap();
    for (g, gate) in fixture.iter().enumerate() {
        for (i, &q) in queries.iter().enumerate() {
            let literal = gate
                .monomials()
                .iter()
                .fold(false, |v, &m| v ^ ((q & usize::from(m)) == usize::from(m)));
            assert_eq!(expected.value(g, i), Some(literal));
        }
    }
    let mut construction = Vec::with_capacity(REPS);
    let mut selected = Vec::with_capacity(REPS);
    for rep in 0..=REPS {
        let inputs: Vec<_> = (0..CALLS).map(|_| fixture.clone()).collect();
        let mut built = Vec::with_capacity(CALLS);
        let start = Instant::now();
        for input in inputs {
            built.push(black_box(Pvp6Bank::new(black_box(input)).unwrap()));
        }
        let elapsed = start.elapsed().as_nanos();
        assert_eq!(built.len(), CALLS);
        for b in &built {
            assert_eq!(b.gates(), fixture.as_slice());
        }
        let start = Instant::now();
        let actual = black_box(
            bank.evaluate_on(Pvp6ExecutionPath::Selected, workload, &queries)
                .unwrap(),
        );
        let selected_elapsed = start.elapsed().as_nanos();
        assert_eq!(actual, expected);
        if rep != 0 {
            construction.push(elapsed);
            selected.push(selected_elapsed);
        }
    }
    println!(
        "GB_CUDA_11F_CONSTRUCT gates={gates} density={density} degree_cap={degree} fixture={digest:016x} calls={CALLS} ns={construction:?} parity=PASS"
    );
    println!(
        "GB_CUDA_11F_SELECTED gates={gates} density={density} degree_cap={degree} fixture={digest:016x} queries=67 ns={selected:?} parity=PASS"
    );
}

fn main() {
    for gates in [4, 8, 10, 17, 32, 63] {
        for (density, degree) in [(3, 2), (6, 3), (9, 4), (17, 5), (29, 6)] {
            run(gates, density, degree);
        }
    }
    println!("GB_CUDA_11F_CONSTRUCTION_RESULT=PASS");
}
