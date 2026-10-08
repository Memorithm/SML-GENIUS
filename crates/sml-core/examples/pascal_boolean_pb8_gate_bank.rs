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
    density_count: usize,
    cell_count: usize,
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
        density_count: densities.len(),
        cell_count: cells.len(),
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
    let m8 = deterministic_valid
        && deterministic.density_count == DENSITIES.len()
        && deterministic.cell_count == 144
        && repetitions_valid
        && cells_valid
        && runner_valid;

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
        "pvp6-fixture" => pvp6_fixture(),
        "pvp6-exact-task" => pvp6_exact_task(),
        "pvp6-model-executor" => pvp6_model_executor(),
        "pvp6-task-plan" => pvp6_task_plan(),
        "pvp6-task-benchmark" => pvp6_task_benchmark(),
        other => panic!("unknown PB8 mode: {other}"),
    }
}

// PVP6 v1 extends evidence modes only; the frozen PB8 timing endpoint is unchanged.
// Output word i = repeat * Q + position, with gate g in bit g and bits 16..63 zero.
fn task_words(arm: Arm, gates: &[Gate], addresses: &[usize], q: usize, reuse: usize) -> Vec<u64> {
    assert!(Q_VALUES.contains(&q));
    assert!(REUSE_VALUES.contains(&reuse));
    assert_eq!(gates.len(), GATES);
    assert!(addresses.len() >= q && addresses.iter().all(|&x| x < BITS));
    let mut output = vec![0_u64; q * reuse];
    for (gate_index, gate) in gates.iter().enumerate() {
        let materialized = match arm {
            Arm::Pascal => Some(materialize_pascal(&gate.coefficients)),
            Arm::Generic => Some(materialize_generic(&gate.coefficients)),
            _ => None,
        };
        for repeat in 0..reuse {
            for (position, &x) in addresses.iter().take(q).enumerate() {
                let value = match arm {
                    Arm::Pascal | Arm::Generic => get_bit(materialized.as_ref().unwrap(), x),
                    Arm::Sparse => sparse_eval(&gate.monomials, x),
                    Arm::Submask => submask_eval(&gate.coefficients, x),
                    Arm::Resident => get_bit(&gate.resident_truth, x),
                };
                output[repeat * q + position] |= u64::from(value) << gate_index;
            }
        }
    }
    output
}

// A separate output oracle evaluates each monomial rather than querying transformed tables.
fn task_oracle(gates: &[Gate], addresses: &[usize], q: usize, reuse: usize) -> Vec<u64> {
    let base = addresses
        .iter()
        .take(q)
        .map(|&x| {
            gates.iter().enumerate().fold(0_u64, |word, (g, gate)| {
                word | (u64::from(oracle_eval(&gate.monomials, x)) << g)
            })
        })
        .collect::<Vec<_>>();
    (0..reuse).flat_map(|_| base.iter().copied()).collect()
}

fn exact_words(actual: &[u64], expected: &[u64]) -> bool {
    actual.len() == expected.len() && actual == expected
}

fn pvp6_exact_task() {
    let mut cells = 0;
    let mut comparisons = 0;
    let mut words = 0;
    for density in DENSITIES {
        let gates = generate_bank(density);
        for kind in ScheduleKind::ALL {
            let addresses = schedule(kind);
            // Compute independent monomial values once outside every candidate.
            let oracle = task_oracle(&gates, &addresses, BITS, 1);
            for q in Q_VALUES {
                for reuse in REUSE_VALUES {
                    let expected = (0..reuse)
                        .flat_map(|_| oracle[..q].iter().copied())
                        .collect::<Vec<_>>();
                    for arm in Arm::ALL {
                        let actual = task_words(arm, &gates, &addresses, q, reuse);
                        assert!(exact_words(&actual, &expected), "PVP6 full output mismatch");
                        comparisons += 1;
                        words += actual.len();
                    }
                    cells += 1;
                }
            }
        }
    }
    println!("PVP6_EXACT_TASK,schema=1,cells={cells},arm_comparisons={comparisons},words={words},mismatches=0");
    println!("PROMOTION,BLOCKED,pvp6_fixture_and_exact_task_only");
}

fn pvp6_fixture() {
    use std::io::{self, Write};
    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());
    writeln!(
        out,
        "PVP6_FIXTURE,1,K,{BITS},G,{GATES},word_bits,64,bit_order,lsb0"
    )
    .unwrap();
    for density in DENSITIES {
        let gates = generate_bank(density);
        for (g, gate) in gates.iter().enumerate() {
            let truth = materialize_pascal(&gate.coefficients);
            assert!(exact_words(&truth, &gate.resident_truth));
            for (w, truth_word) in truth.iter().enumerate() {
                writeln!(
                    out,
                    "BANK,{density},{g},{w},{:016x},{:016x}",
                    gate.coefficients[w], truth_word
                )
                .unwrap();
            }
        }
    }
    for kind in ScheduleKind::ALL {
        let addresses = schedule(kind);
        for (position, x) in addresses.iter().enumerate() {
            writeln!(out, "QUERY,{},{position},{x}", kind.name()).unwrap();
        }
        for density in DENSITIES {
            let gates = generate_bank(density);
            let expected = task_oracle(&gates, &addresses, BITS, 1);
            for (position, word) in expected.iter().enumerate() {
                writeln!(
                    out,
                    "OUTPUT,{density},{},{position},{word:016x}",
                    kind.name()
                )
                .unwrap();
            }
        }
    }
    writeln!(
        out,
        "END,banks,4,bank_words,2048,query_rows,6144,output_rows,24576"
    )
    .unwrap();
    out.flush().unwrap();
}

#[cfg(test)]
mod pvp6_tests {
    use super::*;

    #[test]
    fn full_words_reject_length_value_and_padding_corruption() {
        assert!(exact_words(&[1, 2], &[1, 2]));
        assert!(!exact_words(&[1], &[1, 2]));
        assert!(!exact_words(&[1, 2, 0], &[1, 2]));
        assert!(!exact_words(&[1, 3], &[1, 2]));
        assert!(!exact_words(&[1 | (1_u64 << 63), 2], &[1, 2]));
    }

    #[test]
    fn query_schedules_are_complete_unique_and_bounded() {
        for kind in ScheduleKind::ALL {
            let addresses = schedule(kind);
            assert_eq!(addresses.len(), BITS);
            assert_eq!(
                addresses.iter().copied().collect::<BTreeSet<_>>(),
                (0..BITS).collect()
            );
        }
    }

    #[test]
    fn all_arms_match_complete_query_outputs_and_reuse_order() {
        for density in DENSITIES {
            let gates = generate_bank(density);
            for kind in ScheduleKind::ALL {
                let addresses = schedule(kind);
                let expected = task_oracle(&gates, &addresses, 32, 4);
                for arm in Arm::ALL {
                    assert_eq!(task_words(arm, &gates, &addresses, 32, 4), expected);
                }
                assert_eq!(&expected[..32], &expected[32..64]);
                assert!(expected.iter().all(|word| word >> GATES == 0));
            }
        }
    }

    #[test]
    fn changed_coefficients_require_new_truth_and_corruption_is_detected() {
        let mut gates = generate_bank(4);
        let addresses = schedule(ScheduleKind::AffineUniform);
        let before = task_words(Arm::Pascal, &gates, &addresses, 32, 1);
        gates[0].coefficients[0] ^= 1;
        if let Some(index) = gates[0].monomials.iter().position(|&mask| mask == 0) {
            gates[0].monomials.remove(index);
        } else {
            gates[0].monomials.push(0);
        }
        let expected = task_oracle(&gates, &addresses, 32, 1);
        assert!(!exact_words(&before, &expected));
        assert_eq!(task_words(Arm::Pascal, &gates, &addresses, 32, 1), expected);
        assert!(!exact_words(
            &task_words(Arm::Resident, &gates, &addresses, 32, 1),
            &expected
        ));
        gates[0].resident_truth = oracle_truth(&gates[0].monomials);
        assert_eq!(
            task_words(Arm::Resident, &gates, &addresses, 32, 1),
            expected
        );
    }
}

const PVP6_WARMUPS: usize = 5;
const PVP6_REPETITIONS: usize = 20;

fn pvp6_timing_cells() -> Vec<(usize, ScheduleKind, usize, usize)> {
    let mut cells = physical_cells()
        .into_iter()
        .map(|c| (c.density, c.schedule, c.q, c.reuse))
        .collect::<Vec<_>>();
    for kind in ScheduleKind::ALL {
        cells.push((4, kind, 32, 1));
    }
    cells.push((1024, ScheduleKind::AffineUniform, 2048, 16));
    let mut seen = BTreeSet::new();
    cells.retain(|(d, k, q, r)| seen.insert((*d, k.name(), *q, *r)));
    cells
}

fn pvp6_order(process: usize, cell: usize, repetition: usize) -> Vec<Arm> {
    let offset = (process - 1 + cell + repetition % 5) % 5;
    (0..5)
        .map(|position| {
            let step = if (repetition / 5) % 2 == 0 {
                position
            } else {
                (5 - position) % 5
            };
            Arm::ALL[(offset + step) % 5]
        })
        .collect()
}

fn pvp6_task_plan() {
    println!("PVP6_TASK_PLAN,protocol=1,cells=20,arms=5,processes=10,warmups=5,repetitions=20");
    for (cell, (density, kind, q, reuse)) in pvp6_timing_cells().into_iter().enumerate() {
        println!(
            "CELL,{cell},{density},{},{q},{reuse},output_words={}",
            kind.name(),
            q * reuse
        );
    }
    println!("END,measured_per_process=2000,warmup_per_process=500");
}

fn pvp6_task_benchmark() {
    let process = env::var("PVP6_PROCESS_ID")
        .expect("required process ID")
        .parse::<usize>()
        .expect("integer process ID");
    assert!((1..=10).contains(&process));
    let source = env::var("PVP6_SOURCE_REV").expect("required pinned source revision");
    assert!(
        source.len() == 40
            && source
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
    );
    println!("PVP6_TASK_BEGIN,protocol=1,process={process},source={source},arch={},cells=20,arms=5,warmups=5,repetitions=20,admission=shared_CPU_diagnostic", env::consts::ARCH);
    let mut measured = 0;
    let mut warmups = 0;
    let mut words = 0_usize;
    for (cell, (density, kind, q, reuse)) in pvp6_timing_cells().into_iter().enumerate() {
        let gates = generate_bank(density);
        let addresses = schedule(kind);
        let expected = task_oracle(&gates, &addresses, q, reuse);
        let mut times: [Vec<u128>; 5] =
            std::array::from_fn(|_| Vec::with_capacity(PVP6_REPETITIONS));
        for repetition in 0..PVP6_WARMUPS + PVP6_REPETITIONS {
            let warmup = repetition < PVP6_WARMUPS;
            let relative = if warmup {
                repetition
            } else {
                repetition - PVP6_WARMUPS
            };
            for (position, arm) in pvp6_order(process, cell, relative).into_iter().enumerate() {
                let start = Instant::now();
                let actual = task_words(
                    black_box(arm),
                    black_box(&gates),
                    black_box(&addresses),
                    q,
                    reuse,
                );
                black_box(&actual);
                let elapsed = start.elapsed().as_nanos();
                let exact = exact_words(&actual, &expected);
                let mismatches = actual.iter().zip(&expected).filter(|(a, b)| a != b).count();
                println!("PVP6_SAMPLE,process={process},cell={cell},density={density},schedule={},q={q},reuse={reuse},warmup={},repetition={relative},position={position},arm={},ns={elapsed},words={},mismatches={mismatches},exact={}",kind.name(),u8::from(warmup),arm.name(),actual.len(),u8::from(exact));
                assert!(exact && elapsed > 0, "PVP6 timed output or clock failure");
                words += actual.len();
                if warmup {
                    warmups += 1;
                } else {
                    measured += 1;
                    times[arm.index()].push(elapsed);
                }
            }
        }
        for arm in Arm::ALL {
            println!("PVP6_SUMMARY,process={process},cell={cell},arm={},samples={},median_ns={},p95_ns={},lifecycle={}",arm.name(),times[arm.index()].len(),median(&times[arm.index()]),percentile(&times[arm.index()],95,100),if arm == Arm::Resident { "resident_lookup" } else { "activation_plus_query" });
        }
    }
    assert_eq!((measured, warmups), (2000, 500));
    println!("PVP6_TASK_END,process={process},measured={measured},warmups={warmups},verified_words={words},mismatches=0");
    println!("PROMOTION,BLOCKED,synthetic_shared_CPU_diagnostic_only");
}

#[cfg(test)]
mod pvp6_timing_tests {
    use super::*;

    #[test]
    fn panels_are_unique_and_preserve_negative_boundary() {
        let cells = pvp6_timing_cells();
        assert_eq!(cells.len(), 20);
        let keys = cells
            .iter()
            .map(|(d, k, q, r)| (*d, k.name(), *q, *r))
            .collect::<BTreeSet<_>>();
        assert_eq!(keys.len(), 20);
        for kind in ScheduleKind::ALL {
            assert!(keys.contains(&(4, kind.name(), 32, 1)));
        }
        assert!(keys.contains(&(1024, "AFFINE_UNIFORM", 2048, 16)));
    }

    #[test]
    fn every_arm_occupies_every_position_four_times() {
        for process in 1..=10 {
            for cell in 0..20 {
                let mut positions = [[0; 5]; 5];
                for repetition in 0..20 {
                    let order = pvp6_order(process, cell, repetition);
                    assert_eq!(
                        order
                            .iter()
                            .map(|a| a.index())
                            .collect::<BTreeSet<_>>()
                            .len(),
                        5
                    );
                    for (position, arm) in order.into_iter().enumerate() {
                        positions[arm.index()][position] += 1;
                    }
                }
                assert!(positions.iter().flatten().all(|&count| count == 4));
            }
        }
    }
}

// Destination requalification of the public model executor, not a timing mode.
// Uses the original generator and independent task oracle; old modes are unchanged.
fn pvp6_model_executor() {
    use sml_core::pascal_vector_projection::{
        Pvp6Architecture, Pvp6PopulationProfile, Pvp6Workload,
    };
    use sml_core::pvp6_executor::{Pvp6Bank, Pvp6Gate};

    let mut cells = 0;
    let mut query_words = 0;
    let mut packed_words = 0;
    let mut coefficient_words = 0;
    let mut arms = [0_usize; 3];
    let mut measured_coordinates = 0;
    for density in DENSITIES {
        let gates = generate_bank(density);
        let bank = Pvp6Bank::<BITS>::new(
            gates
                .iter()
                .map(|gate| Pvp6Gate::from_monomials(&gate.monomials).expect("canonical gate"))
                .collect(),
        )
        .expect("canonical bank");
        for (actual, expected) in bank.gates().iter().zip(&gates) {
            assert_eq!(actual.coefficients(), expected.coefficients);
            coefficient_words += actual.coefficients().len();
        }
        for kind in ScheduleKind::ALL {
            let profile = match kind {
                ScheduleKind::AffineUniform => Pvp6PopulationProfile::AffineUniform,
                ScheduleKind::LowPopcount => Pvp6PopulationProfile::LowPopcount,
                ScheduleKind::HighPopcount => Pvp6PopulationProfile::HighPopcount,
            };
            let addresses = schedule(kind);
            let oracle = task_oracle(&gates, &addresses, BITS, 1);
            for q in Q_VALUES {
                for reuse in REUSE_VALUES {
                    let workload =
                        Pvp6Workload::new(density as u32, q as u32, reuse as u32, profile);
                    let result = bank
                        .evaluate(workload, &addresses[..q])
                        .expect("model evaluation");
                    assert_eq!(result.gate_count(), GATES);
                    assert_eq!(result.query_count(), q * reuse);
                    assert_eq!(result.selection(), workload.select().unwrap());
                    assert!(result.selection().diagnostic_only);
                    measured_coordinates += usize::from(result.selection().measured_on_thor);
                    arms[match result.selection().architecture {
                        Pvp6Architecture::PascalAnfMaterialize => 0,
                        Pvp6Architecture::SparseAnfScan => 1,
                        Pvp6Architecture::SubmaskCoefficientQuery => 2,
                    }] += 1;
                    let expected = (0..reuse)
                        .flat_map(|_| oracle[..q].iter().copied())
                        .collect::<Vec<_>>();
                    let actual = result.to_query_major_words().expect("canonical projection");
                    assert!(
                        exact_words(&actual, &expected),
                        "model canonical output mismatch"
                    );
                    assert!(actual.iter().all(|word| word >> GATES == 0));
                    let per_gate = (q * reuse).div_ceil(64);
                    let mut expected_packed = vec![0_u64; GATES * per_gate];
                    for (query, &word) in expected.iter().enumerate() {
                        for gate in 0..GATES {
                            if word & (1_u64 << gate) != 0 {
                                expected_packed[gate * per_gate + query / 64] |=
                                    1_u64 << (query % 64);
                            }
                        }
                    }
                    assert_eq!(
                        result.packed_words(),
                        expected_packed,
                        "model packed output or padding mismatch"
                    );
                    query_words += actual.len();
                    packed_words += result.packed_words().len();
                    cells += 1;
                }
            }
        }
    }
    assert_eq!(arms, [141, 2, 1]);
    assert_eq!(measured_coordinates, 20);
    println!("PVP6_MODEL_EXECUTOR,schema=1,cells={cells},query_words={query_words},packed_words={packed_words},coefficient_words={coefficient_words},mismatches=0");
    println!("PVP6_MODEL_SELECTION,pascal=141,sparse=2,submask=1,measured_coordinates=20,diagnostic_only=144");
    println!("PROMOTION,BLOCKED,model_executor_synthetic_correctness_only");
}
