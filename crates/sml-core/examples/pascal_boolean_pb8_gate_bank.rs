//! PB-8 model-aligned Boolean/ANF gate-bank experiment.
//!
//! Frozen protocol:
//! docs/SML-PASCAL-BOOLEAN-PB8-MODEL-ALIGNED-GATE-PREREGISTRATION.md

use std::{collections::BTreeSet, env, hint::black_box, time::Instant};

use sml_core::pascal_gf2::pascal_gf2_in_place;

const BITS: usize = 2048;
const WORDS: usize = BITS / 64;
const GATES: usize = 16;
const DENSITIES: [usize; 4] = [4, 32, 256, 1024];
const Q_VALUES: [usize; 4] = [32, 128, 512, 2048];
const REUSE_VALUES: [usize; 3] = [1, 4, 16];
const ZETA_XOR_PER_GATE: u64 = 11_264;
const FULL_DIRECT_READS_PER_GATE: u64 = 177_147;
const WARMUP_REPS: usize = 2;
const MEASURED_REPS: usize = 12;
const EXPECTED_RUNNER: &str = "tarek-scirust-arm64-01";
const BASE_SEED: u64 = 0x5042_3841_4e46_3131;

#[derive(Debug, Clone, Copy)]
struct Stream(u64);

impl Stream {
    const fn new(seed: u64) -> Self {
        Self(seed)
    }

    fn next(&mut self) -> u64 {
        let mut state = self.0;
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        self.0 = state;
        state
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScheduleKind {
    AffineUniform,
    LowPopcount,
    HighPopcount,
}

impl ScheduleKind {
    const ALL: [Self; 3] = [Self::AffineUniform, Self::LowPopcount, Self::HighPopcount];

    const fn name(self) -> &'static str {
        match self {
            Self::AffineUniform => "AFFINE_UNIFORM",
            Self::LowPopcount => "LOW_POPCOUNT",
            Self::HighPopcount => "HIGH_POPCOUNT",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Arm {
    Pascal,
    Sparse,
    Submask,
    Generic,
    Resident,
}

impl Arm {
    const ALL: [Self; 5] = [
        Self::Pascal,
        Self::Sparse,
        Self::Submask,
        Self::Generic,
        Self::Resident,
    ];

    const fn name(self) -> &'static str {
        match self {
            Self::Pascal => "PASCAL_ANF_MATERIALIZE",
            Self::Sparse => "SPARSE_ANF_SCAN",
            Self::Submask => "SUBMASK_COEFFICIENT_QUERY",
            Self::Generic => "GENERIC_TRUTH_MATERIALIZE",
            Self::Resident => "RESIDENT_TRUTH_TABLE",
        }
    }

    const fn index(self) -> usize {
        match self {
            Self::Pascal => 0,
            Self::Sparse => 1,
            Self::Submask => 2,
            Self::Generic => 3,
            Self::Resident => 4,
        }
    }
}

#[derive(Debug, Clone)]
struct Gate {
    coefficients: Vec<u64>,
    monomials: Vec<u16>,
    resident_truth: Vec<u64>,
}

#[derive(Debug, Clone)]
struct GateTables {
    pascal: Vec<u64>,
    sparse: Vec<u64>,
    submask: Vec<u64>,
    generic: Vec<u64>,
    resident: Vec<u64>,
}

#[derive(Debug, Clone)]
struct DensityEvidence {
    density: usize,
    gates: Vec<Gate>,
    tables: Vec<GateTables>,
    coefficient_collisions: usize,
    pascal_mismatches: u64,
    sparse_mismatches: u64,
    submask_mismatches: u64,
    generic_mismatches: u64,
    resident_mismatches: u64,
    pascal_generic_table_mismatches: u64,
}

#[derive(Debug, Clone)]
struct CellEvidence {
    density: usize,
    schedule: ScheduleKind,
    q: usize,
    reuse: usize,
    gate_evaluations: u64,
    checksums: [u64; 5],
    mismatches: [u64; 5],
    b1_subset_tests: u64,
    b2_coefficient_reads: u64,
    a_zeta_xor: u64,
    c_materialization_reads: u64,
    lookup_count: u64,
}

#[derive(Debug)]
struct DeterministicEvidence {
    densities: Vec<DensityEvidence>,
    cells: Vec<CellEvidence>,
    m1: bool,
    m2: bool,
    m3: bool,
    m4: bool,
}

fn get_bit(words: &[u64], index: usize) -> bool {
    words[index / 64] & (1_u64 << (index % 64)) != 0
}

fn set_bit(words: &mut [u64], index: usize) {
    words[index / 64] |= 1_u64 << (index % 64);
}

fn schedule(kind: ScheduleKind) -> Vec<usize> {
    match kind {
        ScheduleKind::AffineUniform => (0..BITS).map(|index| (405 * index + 17) % BITS).collect(),
        ScheduleKind::LowPopcount => {
            let mut addresses = (0..BITS).collect::<Vec<_>>();
            addresses.sort_by_key(|&address| (address.count_ones(), address));
            addresses
        }
        ScheduleKind::HighPopcount => {
            let mut addresses = (0..BITS).collect::<Vec<_>>();
            addresses.sort_by(|left, right| {
                right
                    .count_ones()
                    .cmp(&left.count_ones())
                    .then_with(|| left.cmp(right))
            });
            addresses
        }
    }
}

fn generate_gate(density: usize, gate_index: usize) -> Gate {
    let mut stream = Stream::new(
        BASE_SEED
            ^ ((density as u64) << 32)
            ^ (gate_index as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15),
    );
    let mut selected = BTreeSet::new();
    while selected.len() < density {
        selected.insert((stream.next() as usize % BITS) as u16);
    }
    let monomials = selected.into_iter().collect::<Vec<_>>();
    let mut coefficients = vec![0_u64; WORDS];
    for &mask in &monomials {
        set_bit(&mut coefficients, mask as usize);
    }
    let resident_truth = oracle_truth(&monomials);
    Gate {
        coefficients,
        monomials,
        resident_truth,
    }
}

fn generate_bank(density: usize) -> Vec<Gate> {
    (0..GATES)
        .map(|gate| generate_gate(density, gate))
        .collect()
}

fn oracle_eval(monomials: &[u16], x: usize) -> bool {
    let mut value = false;
    for &mask in monomials {
        let mask = mask as usize;
        if mask & x == mask {
            value ^= true;
        }
    }
    value
}

fn sparse_eval(monomials: &[u16], x: usize) -> bool {
    monomials
        .iter()
        .copied()
        .filter(|&mask| (mask as usize) & x == mask as usize)
        .fold(false, |value, _| !value)
}

fn submask_eval(coefficients: &[u64], x: usize) -> bool {
    let mut value = false;
    let mut submask = x;
    loop {
        value ^= get_bit(coefficients, submask);
        if submask == 0 {
            break;
        }
        submask = (submask - 1) & x;
    }
    value
}

fn generic_eval(coefficients: &[u64], x: usize) -> bool {
    let mut value = false;
    let mut current = x;
    loop {
        if get_bit(coefficients, current) {
            value = !value;
        }
        if current == 0 {
            return value;
        }
        current = (current - 1) & x;
    }
}

fn truth_from<F>(mut evaluate: F) -> Vec<u64>
where
    F: FnMut(usize) -> bool,
{
    let mut truth = vec![0_u64; WORDS];
    for x in 0..BITS {
        if evaluate(x) {
            set_bit(&mut truth, x);
        }
    }
    truth
}

fn oracle_truth(monomials: &[u16]) -> Vec<u64> {
    truth_from(|x| oracle_eval(monomials, x))
}

fn materialize_pascal(coefficients: &[u64]) -> Vec<u64> {
    let mut truth = coefficients.to_vec();
    let stats = pascal_gf2_in_place(&mut truth).expect("PB8 Pascal transform");
    assert_eq!(stats.bits, BITS);
    assert_eq!(stats.logical_xor_ops, ZETA_XOR_PER_GATE);
    assert_eq!(stats.state_bytes, BITS / 8);
    assert_eq!(stats.scratch_words, 0);
    truth
}

fn materialize_sparse(monomials: &[u16]) -> Vec<u64> {
    truth_from(|x| sparse_eval(monomials, x))
}

fn materialize_submask(coefficients: &[u64]) -> Vec<u64> {
    truth_from(|x| submask_eval(coefficients, x))
}

fn materialize_generic(coefficients: &[u64]) -> Vec<u64> {
    truth_from(|x| generic_eval(coefficients, x))
}

fn bit_mismatches(left: &[u64], right: &[u64]) -> u64 {
    left.iter()
        .zip(right)
        .map(|(a, b)| (a ^ b).count_ones() as u64)
        .sum()
}

fn build_density_evidence(density: usize) -> DensityEvidence {
    let gates = generate_bank(density);
    let unique = gates
        .iter()
        .map(|gate| gate.monomials.clone())
        .collect::<BTreeSet<_>>()
        .len();
    let coefficient_collisions = GATES - unique;

    let mut tables = Vec::with_capacity(GATES);
    let mut pascal_mismatches = 0_u64;
    let mut sparse_mismatches = 0_u64;
    let mut submask_mismatches = 0_u64;
    let mut generic_mismatches = 0_u64;
    let mut resident_mismatches = 0_u64;
    let mut pascal_generic_table_mismatches = 0_u64;

    for gate in &gates {
        let resident = gate.resident_truth.clone();
        let pascal = materialize_pascal(&gate.coefficients);
        let sparse = materialize_sparse(&gate.monomials);
        let submask = materialize_submask(&gate.coefficients);
        let generic = materialize_generic(&gate.coefficients);

        pascal_mismatches += bit_mismatches(&pascal, &resident);
        sparse_mismatches += bit_mismatches(&sparse, &resident);
        submask_mismatches += bit_mismatches(&submask, &resident);
        generic_mismatches += bit_mismatches(&generic, &resident);
        resident_mismatches += bit_mismatches(&gate.resident_truth, &resident);
        pascal_generic_table_mismatches += bit_mismatches(&pascal, &generic);

        tables.push(GateTables {
            pascal,
            sparse,
            submask,
            generic,
            resident,
        });
    }

    DensityEvidence {
        density,
        gates,
        tables,
        coefficient_collisions,
        pascal_mismatches,
        sparse_mismatches,
        submask_mismatches,
        generic_mismatches,
        resident_mismatches,
        pascal_generic_table_mismatches,
    }
}

fn direct_reads(addresses: &[usize], q: usize) -> u64 {
    addresses
        .iter()
        .take(q)
        .map(|address| 1_u64 << address.count_ones())
        .sum()
}

fn checksum_push(
    checksum: u64,
    value: bool,
    gate: usize,
    reuse: usize,
    position: usize,
    x: usize,
) -> u64 {
    let token = ((gate as u64) << 52)
        ^ ((reuse as u64) << 44)
        ^ ((position as u64) << 24)
        ^ (x as u64)
        ^ if value {
            0xd6e8_feb8_6659_fd93
        } else {
            0xa5a3_564e_27f1_886b
        };
    checksum.rotate_left(11).wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ token
}

fn table_for_arm(tables: &GateTables, arm: Arm) -> &[u64] {
    match arm {
        Arm::Pascal => &tables.pascal,
        Arm::Sparse => &tables.sparse,
        Arm::Submask => &tables.submask,
        Arm::Generic => &tables.generic,
        Arm::Resident => &tables.resident,
    }
}

fn cell_checksum(
    density: &DensityEvidence,
    arm: Arm,
    addresses: &[usize],
    q: usize,
    reuse: usize,
) -> u64 {
    let mut checksum = 0_u64;
    for (gate_index, tables) in density.tables.iter().enumerate() {
        let truth = table_for_arm(tables, arm);
        for repeat in 0..reuse {
            for (position, &x) in addresses.iter().take(q).enumerate() {
                checksum =
                    checksum_push(checksum, get_bit(truth, x), gate_index, repeat, position, x);
            }
        }
    }
    checksum
}

fn run_deterministic(emit: bool) -> DeterministicEvidence {
    let densities = DENSITIES
        .into_iter()
        .map(build_density_evidence)
        .collect::<Vec<_>>();
    let mut cells = Vec::new();

    for density in &densities {
        for kind in ScheduleKind::ALL {
            let addresses = schedule(kind);
            for q in Q_VALUES {
                for reuse in REUSE_VALUES {
                    let checksums = std::array::from_fn(|index| {
                        cell_checksum(density, Arm::ALL[index], &addresses, q, reuse)
                    });
                    let oracle_checksum = checksums[Arm::Resident.index()];
                    let mismatches =
                        std::array::from_fn(|index| u64::from(checksums[index] != oracle_checksum));
                    let gate_evaluations = (GATES * q * reuse) as u64;
                    cells.push(CellEvidence {
                        density: density.density,
                        schedule: kind,
                        q,
                        reuse,
                        gate_evaluations,
                        checksums,
                        mismatches,
                        b1_subset_tests: gate_evaluations * density.density as u64,
                        b2_coefficient_reads: direct_reads(&addresses, q)
                            * GATES as u64
                            * reuse as u64,
                        a_zeta_xor: ZETA_XOR_PER_GATE * GATES as u64,
                        c_materialization_reads: FULL_DIRECT_READS_PER_GATE * GATES as u64,
                        lookup_count: gate_evaluations,
                    });
                }
            }
        }
    }

    let m1 = densities.iter().all(|density| {
        density.coefficient_collisions == 0
            && density.pascal_mismatches == 0
            && density.sparse_mismatches == 0
            && density.submask_mismatches == 0
            && density.generic_mismatches == 0
            && density.resident_mismatches == 0
            && density.pascal_generic_table_mismatches == 0
    }) && cells.iter().all(|cell| {
        cell.mismatches.iter().all(|&count| count == 0)
            && cell
                .checksums
                .iter()
                .all(|&checksum| checksum == cell.checksums[0])
    });

    let m2 = densities.iter().all(|density| {
        GATES * (BITS / 8) == 4096
            && density.gates.iter().all(|gate| {
                gate.coefficients.len() * 8 == BITS / 8
                    && gate.resident_truth.len() * 8 == BITS / 8
                    && gate.monomials.len() * 2 == density.density * 2
            })
    });

    let sparse_favorable = {
        let b1 = (GATES * 4 * 32) as u64;
        let a = ZETA_XOR_PER_GATE * GATES as u64 + (GATES * 32) as u64;
        b1 < a
    };
    let dense_favorable = {
        let b1 = (GATES * 1024 * 2048 * 16) as u64;
        let a = ZETA_XOR_PER_GATE * GATES as u64 + (GATES * 2048 * 16) as u64;
        a < b1
    };
    let m3 = sparse_favorable && dense_favorable;

    let m4 = Q_VALUES.into_iter().all(|q| {
        let low = direct_reads(&schedule(ScheduleKind::LowPopcount), q);
        let affine = direct_reads(&schedule(ScheduleKind::AffineUniform), q);
        let high = direct_reads(&schedule(ScheduleKind::HighPopcount), q);
        low <= affine && affine <= high
    });

    if emit {
        println!(
            "PB8_DETERMINISTIC_BEGIN,protocol=1,bits={BITS},gates={GATES},densities=4|32|256|1024,schedules=AFFINE_UNIFORM|LOW_POPCOUNT|HIGH_POPCOUNT,q=32|128|512|2048,reuse=1|4|16,cells={}",
            cells.len()
        );
        for density in &densities {
            println!(
                "PB8_DENSITY,density={},coefficient_collisions={},pascal_mismatches={},sparse_mismatches={},submask_mismatches={},generic_mismatches={},resident_mismatches={},pascal_generic_table_mismatches={},packed_bytes_per_gate={},sparse_mask_bytes_per_gate={}",
                density.density,
                density.coefficient_collisions,
                density.pascal_mismatches,
                density.sparse_mismatches,
                density.submask_mismatches,
                density.generic_mismatches,
                density.resident_mismatches,
                density.pascal_generic_table_mismatches,
                BITS / 8,
                density.density * 2
            );
        }
        for cell in &cells {
            println!(
                "PB8_CELL,density={},schedule={},q={},reuse={},gate_evaluations={},a_mismatches={},b1_mismatches={},b2_mismatches={},c_mismatches={},d_mismatches={},a_checksum={:016x},b1_checksum={:016x},b2_checksum={:016x},c_checksum={:016x},d_checksum={:016x},b1_subset_tests={},b2_coefficient_reads={},a_zeta_xor={},c_materialization_reads={},lookup_count={},truth_bytes_bank={},sparse_mask_bytes_bank={}",
                cell.density,
                cell.schedule.name(),
                cell.q,
                cell.reuse,
                cell.gate_evaluations,
                cell.mismatches[Arm::Pascal.index()],
                cell.mismatches[Arm::Sparse.index()],
                cell.mismatches[Arm::Submask.index()],
                cell.mismatches[Arm::Generic.index()],
                cell.mismatches[Arm::Resident.index()],
                cell.checksums[Arm::Pascal.index()],
                cell.checksums[Arm::Sparse.index()],
                cell.checksums[Arm::Submask.index()],
                cell.checksums[Arm::Generic.index()],
                cell.checksums[Arm::Resident.index()],
                cell.b1_subset_tests,
                cell.b2_coefficient_reads,
                cell.a_zeta_xor,
                cell.c_materialization_reads,
                cell.lookup_count,
                GATES * (BITS / 8),
                GATES * cell.density * 2
            );
        }
        println!(
            "PB8_GATE,M1_EXACT_MODEL_ALIGNED_SEMANTICS,{}",
            if m1 { "HOLDS" } else { "REFUTED" }
        );
        println!(
            "PB8_GATE,M2_REPRESENTATION_ACCOUNTING,{}",
            if m2 { "HOLDS" } else { "REFUTED" }
        );
        println!(
            "PB8_GATE,M3_COMPETENT_SPARSE_CONTROL_BOUNDARY,{}",
            if m3 { "HOLDS" } else { "REFUTED" }
        );
        println!(
            "PB8_GATE,M4_SUBMASK_POPULATION_ORDERING,{}",
            if m4 { "HOLDS" } else { "REFUTED" }
        );
        println!("PB8_DETERMINISTIC_DECISION,PENDING_PHYSICAL_M5_M6_M7");
        println!("PROMOTION,BLOCKED,pb8_requires_physical_model_aligned_gate");
    }

    DeterministicEvidence {
        densities,
        cells,
        m1,
        m2,
        m3,
        m4,
    }
}

fn run_arm(arm: Arm, gates: &[Gate], addresses: &[usize], q: usize, reuse: usize) -> u64 {
    let mut checksum = 0_u64;
    for (gate_index, gate) in gates.iter().enumerate() {
        match arm {
            Arm::Pascal => {
                let truth = materialize_pascal(black_box(&gate.coefficients));
                for repeat in 0..reuse {
                    for (position, &x) in addresses.iter().take(q).enumerate() {
                        checksum = checksum_push(
                            checksum,
                            get_bit(&truth, x),
                            gate_index,
                            repeat,
                            position,
                            x,
                        );
                    }
                }
            }
            Arm::Sparse => {
                for repeat in 0..reuse {
                    for (position, &x) in addresses.iter().take(q).enumerate() {
                        checksum = checksum_push(
                            checksum,
                            sparse_eval(black_box(&gate.monomials), x),
                            gate_index,
                            repeat,
                            position,
                            x,
                        );
                    }
                }
            }
            Arm::Submask => {
                for repeat in 0..reuse {
                    for (position, &x) in addresses.iter().take(q).enumerate() {
                        checksum = checksum_push(
                            checksum,
                            submask_eval(black_box(&gate.coefficients), x),
                            gate_index,
                            repeat,
                            position,
                            x,
                        );
                    }
                }
            }
            Arm::Generic => {
                let truth = materialize_generic(black_box(&gate.coefficients));
                for repeat in 0..reuse {
                    for (position, &x) in addresses.iter().take(q).enumerate() {
                        checksum = checksum_push(
                            checksum,
                            get_bit(&truth, x),
                            gate_index,
                            repeat,
                            position,
                            x,
                        );
                    }
                }
            }
            Arm::Resident => {
                let truth = black_box(&gate.resident_truth);
                for repeat in 0..reuse {
                    for (position, &x) in addresses.iter().take(q).enumerate() {
                        checksum = checksum_push(
                            checksum,
                            get_bit(truth, x),
                            gate_index,
                            repeat,
                            position,
                            x,
                        );
                    }
                }
            }
        }
    }
    checksum
}

fn percentile(values: &[u128], numerator: usize, denominator: usize) -> u128 {
    assert!(!values.is_empty());
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let rank = (numerator * sorted.len()).div_ceil(denominator);
    sorted[rank.saturating_sub(1)]
}

fn median(values: &[u128]) -> u128 {
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let count = sorted.len();
    if count % 2 == 0 {
        (sorted[count / 2 - 1] + sorted[count / 2]) / 2
    } else {
        sorted[count / 2]
    }
}

fn format_times(values: &[u128]) -> String {
    values
        .iter()
        .map(u128::to_string)
        .collect::<Vec<_>>()
        .join("|")
}

fn arm_order(offset: usize) -> Vec<Arm> {
    (0..Arm::ALL.len())
        .map(|index| Arm::ALL[(index + offset) % Arm::ALL.len()])
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Panel {
    P0,
    P1,
    P2,
}

impl Panel {
    const fn name(self) -> &'static str {
        match self {
            Self::P0 => "P0",
            Self::P1 => "P1",
            Self::P2 => "P2",
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct PhysicalCell {
    panel: Panel,
    density: usize,
    schedule: ScheduleKind,
    q: usize,
    reuse: usize,
}

fn physical_cells() -> Vec<PhysicalCell> {
    let mut cells = Vec::new();
    for density in DENSITIES {
        cells.push(PhysicalCell {
            panel: Panel::P0,
            density,
            schedule: ScheduleKind::AffineUniform,
            q: 512,
            reuse: 1,
        });
    }
    for q in [128_usize, 512, 2048] {
        for reuse in REUSE_VALUES {
            cells.push(PhysicalCell {
                panel: Panel::P1,
                density: 256,
                schedule: ScheduleKind::AffineUniform,
                q,
                reuse,
            });
        }
    }
    for schedule in [ScheduleKind::LowPopcount, ScheduleKind::HighPopcount] {
        for q in [128_usize, 512] {
            cells.push(PhysicalCell {
                panel: Panel::P2,
                density: 256,
                schedule,
                q,
                reuse: 1,
            });
        }
    }
    cells
}

#[derive(Debug)]
struct BenchResult {
    cell: PhysicalCell,
    times: [Vec<u128>; 5],
    medians: [u128; 5],
    p95s: [u128; 5],
    checksums: [u64; 5],
    checksums_equal: bool,
}

fn benchmark_cell(cell: PhysicalCell) -> BenchResult {
    let gates = generate_bank(cell.density);
    let addresses = schedule(cell.schedule);

    for warmup in 0..WARMUP_REPS {
        for arm in arm_order(warmup) {
            black_box(run_arm(arm, &gates, &addresses, cell.q, cell.reuse));
        }
    }

    let mut times: [Vec<u128>; 5] = std::array::from_fn(|_| Vec::with_capacity(MEASURED_REPS));
    let mut checksums: [Vec<u64>; 5] = std::array::from_fn(|_| Vec::with_capacity(MEASURED_REPS));

    for repetition in 0..MEASURED_REPS {
        for arm in arm_order(repetition) {
            let start = Instant::now();
            let checksum = run_arm(arm, &gates, &addresses, cell.q, cell.reuse);
            let elapsed = start.elapsed().as_nanos();
            assert!(elapsed > 0);
            black_box(checksum);
            times[arm.index()].push(elapsed);
            checksums[arm.index()].push(checksum);
        }
    }

    let medians = std::array::from_fn(|index| median(&times[index]));
    let p95s = std::array::from_fn(|index| percentile(&times[index], 95, 100));
    let first_checksums =
        std::array::from_fn(|index| checksums[index].first().copied().unwrap_or_default());
    let stable = (0..Arm::ALL.len()).all(|index| {
        checksums[index]
            .iter()
            .all(|&value| value == first_checksums[index])
    });
    let checksums_equal = stable
        && first_checksums
            .iter()
            .all(|&value| value == first_checksums[0]);

    BenchResult {
        cell,
        times,
        medians,
        p95s,
        checksums: first_checksums,
        checksums_equal,
    }
}

fn ten_percent_faster(candidate: u128, control: u128) -> bool {
    10_u128 * candidate <= 9_u128 * control
}

fn benchmark() {
    let deterministic = run_deterministic(false);
    let runner = env::var("RUNNER_NAME").unwrap_or_else(|_| "UNKNOWN".to_string());
    let arch = env::consts::ARCH;
    let cells = physical_cells();

    println!(
        "PB8_BENCH_BEGIN,protocol=1,runner={},arch={},bits={BITS},gates={GATES},warmups={WARMUP_REPS},measured_reps={MEASURED_REPS},cells={}",
        runner,
        arch,
        cells.len()
    );

    let mut results = Vec::with_capacity(cells.len());
    for cell in cells {
        let result = benchmark_cell(cell);
        let gate_evaluations = (GATES * cell.q * cell.reuse) as u128;
        for arm in Arm::ALL {
            let index = arm.index();
            let throughput = gate_evaluations * 1_000_000_000_u128 / result.medians[index].max(1);
            println!(
                "PB8_BENCH_ARM,panel={},density={},schedule={},q={},reuse={},arm={},times_ns={},median_ns={},p95_ns={},gate_evals_per_second={},checksum={:016x}",
                cell.panel.name(),
                cell.density,
                cell.schedule.name(),
                cell.q,
                cell.reuse,
                arm.name(),
                format_times(&result.times[index]),
                result.medians[index],
                result.p95s[index],
                throughput,
                result.checksums[index]
            );
        }
        println!(
            "PB8_BENCH_COMPARE,panel={},density={},schedule={},q={},reuse={},a_ns={},b1_ns={},b2_ns={},c_ns={},d_ns={},checksums_equal={}",
            cell.panel.name(),
            cell.density,
            cell.schedule.name(),
            cell.q,
            cell.reuse,
            result.medians[Arm::Pascal.index()],
            result.medians[Arm::Sparse.index()],
            result.medians[Arm::Submask.index()],
            result.medians[Arm::Generic.index()],
            result.medians[Arm::Resident.index()],
            u8::from(result.checksums_equal)
        );
        results.push(result);
    }

    let p0 = |density: usize| {
        results.iter().find(|result| {
            result.cell.panel == Panel::P0
                && result.cell.density == density
                && result.cell.q == 512
                && result.cell.reuse == 1
        })
    };

    let m5 = DENSITIES.into_iter().all(|density| {
        p0(density).is_some_and(|result| {
            result.checksums_equal
                && ten_percent_faster(
                    result.medians[Arm::Pascal.index()],
                    result.medians[Arm::Generic.index()],
                )
        })
    });

    let m6 = [256_usize, 1024].into_iter().all(|density| {
        p0(density).is_some_and(|result| {
            let faster = result.medians[Arm::Sparse.index()]
                .min(result.medians[Arm::Submask.index()])
                .min(result.medians[Arm::Generic.index()]);
            result.checksums_equal
                && ten_percent_faster(result.medians[Arm::Pascal.index()], faster)
        })
    });

    let m7_cells = [(128_usize, 16_usize), (2048_usize, 1_usize)];
    let m7 = m7_cells.into_iter().all(|(q, reuse)| {
        results
            .iter()
            .find(|result| {
                result.cell.panel == Panel::P1
                    && result.cell.density == 256
                    && result.cell.schedule == ScheduleKind::AffineUniform
                    && result.cell.q == q
                    && result.cell.reuse == reuse
            })
            .is_some_and(|result| {
                let faster = result.medians[Arm::Sparse.index()]
                    .min(result.medians[Arm::Submask.index()])
                    .min(result.medians[Arm::Generic.index()]);
                result.checksums_equal
                    && ten_percent_faster(result.medians[Arm::Pascal.index()], faster)
            })
    });

    let deterministic_valid =
        deterministic.m1 && deterministic.m2 && deterministic.m3 && deterministic.m4;
    let repetitions_valid = results.iter().all(|result| {
        result
            .times
            .iter()
            .all(|times| times.len() == MEASURED_REPS && times.iter().all(|&elapsed| elapsed > 0))
    });
    let cells_valid = results.len() == 17
        && results.iter().all(|result| {
            result.checksums_equal
                && Q_VALUES.contains(&result.cell.q)
                && REUSE_VALUES.contains(&result.cell.reuse)
                && DENSITIES.contains(&result.cell.density)
        });
    let runner_valid = runner == EXPECTED_RUNNER && arch == "aarch64";
    let m8 = deterministic_valid && repetitions_valid && cells_valid && runner_valid;

    println!(
        "PB8_GATE,M5_SAME_SOURCE_MATERIALIZATION_UTILITY,{}",
        if m5 { "HOLDS" } else { "REFUTED" }
    );
    println!(
        "PB8_GATE,M6_DENSE_GATE_BANK_TASK_UTILITY,{}",
        if m6 { "HOLDS" } else { "REFUTED" }
    );
    println!(
        "PB8_GATE,M7_AMORTIZED_TASK_UTILITY,{}",
        if m7 { "HOLDS" } else { "REFUTED" }
    );
    println!(
        "PB8_GATE,M8_BENCHMARK_SANITY,{}",
        if m8 { "HOLDS" } else { "INVALID" }
    );

    let decision = if !m8 {
        "INVALID"
    } else if deterministic_valid && m5 && m6 && m7 {
        "RETAIN_MODEL_ALIGNED_GATE_UTILITY"
    } else {
        "RETAIN_MIXED_MODEL_ALIGNED_GATE_SURFACE"
    };
    println!("PB8_DECISION,{decision}");
    println!("PROMOTION,BLOCKED,pb8_model_aligned_gate_only");
}

fn main() {
    let mode = env::args()
        .nth(1)
        .unwrap_or_else(|| "deterministic".to_string());
    match mode.as_str() {
        "deterministic" => {
            run_deterministic(true);
        }
        "benchmark" => benchmark(),
        other => panic!("unknown PB8 mode: {other}"),
    }
}
