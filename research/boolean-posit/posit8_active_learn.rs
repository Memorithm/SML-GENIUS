//! SYNTH-LEARN1: bounded sample-only active learning of posit8 ADD/MUL
//! Boolean decision circuits, followed by native x86-64 GAS code generation.
//! Truth labels are read ONLY for selected membership queries. Validation on
//! reserved/unseen data is audit-only, never fed into candidate construction.
#![forbid(unsafe_code)]

#[path = "posit8_circuit_synth.rs"]
#[allow(dead_code)]
mod arithmetic;

#[path = "posit8_active_bitwise.rs"]
pub mod bitwise;

use arithmetic::Posit8;
use std::collections::{BTreeMap, HashMap};
use std::fmt::Write;

const INITIAL_QUERIES: usize = 64;
const ACTIVE_ROUNDS: usize = 8;
const BATCH_QUERIES: usize = 32;
const MAX_QUERIES: usize = INITIAL_QUERIES + ACTIVE_ROUNDS * BATCH_QUERIES;
const DOMAIN: usize = 65536;
const LEARNED_NODE_CAP: usize = 2 * MAX_QUERIES + 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation {
    Add,
    Mul,
}
impl Operation {
    fn symbol(self) -> &'static str {
        match self {
            Self::Add => "add",
            Self::Mul => "mul",
        }
    }
    fn teacher(self, packed: u16) -> u8 {
        let (a, b) = unpack(packed);
        match self {
            Self::Add => Posit8(a).add(Posit8(b)).0,
            Self::Mul => Posit8(a).mul(Posit8(b)).0,
        }
    }
}
fn unpack(packed: u16) -> (u8, u8) {
    let mut a = 0u8;
    let mut b = 0u8;
    for bit in 0..8 {
        a |= (((packed >> (2 * bit)) & 1) as u8) << bit;
        b |= (((packed >> (2 * bit + 1)) & 1) as u8) << bit;
    }
    (a, b)
}

/// Deterministic, non-affine reserved-set partition. No holdout bit is
/// disclosed as a label to either learner. The partition is frozen in code.
fn is_holdout(input: u16) -> bool {
    let mut x = input ^ (input >> 7);
    x = x.wrapping_mul(0x9e37);
    x ^= x >> 5;
    x = x.wrapping_mul(0x7feb);
    x ^= x >> 7;
    x & 15 == 0
}

#[derive(Clone, Copy, Debug)]
enum Gate {
    Constant(u8),
    Select {
        variable: u8,
        low: usize,
        high: usize,
    },
}
#[derive(Debug)]
pub struct Circuit {
    nodes: Vec<Gate>,
    root: usize,
}
struct TreeBuilder {
    nodes: Vec<Gate>,
    constants: HashMap<u8, usize>,
    shared: HashMap<(u8, usize, usize), usize>,
    prefer_high: bool,
}
impl TreeBuilder {
    fn constant(&mut self, value: u8) -> usize {
        if let Some(&index) = self.constants.get(&value) {
            return index;
        }
        let index = self.nodes.len();
        self.nodes.push(Gate::Constant(value));
        self.constants.insert(value, index);
        index
    }
    /// Select an input bit maximizing post-split class concentration, with
    /// a deterministic secondary variable preference for the committee.
    fn build(&mut self, samples: &[(u16, u8)], remaining: u16) -> usize {
        let first = samples[0].1;
        if samples.iter().all(|&(_, output)| output == first) {
            return self.constant(first);
        }
        let mut best = None;
        for variable in 0..16u8 {
            if remaining & (1u16 << variable) == 0 {
                continue;
            }
            let mut hist = [[0u16; 256]; 2];
            let mut sizes = [0usize; 2];
            for &(packed, label) in samples {
                let half = usize::from((packed >> variable) & 1);
                hist[half][usize::from(label)] += 1;
                sizes[half] += 1;
            }
            if sizes[0] == 0 || sizes[1] == 0 {
                continue;
            }
            let concentration: u64 = hist
                .iter()
                .flat_map(|counts| counts.iter())
                .map(|&v| u64::from(v) * u64::from(v))
                .sum();
            let preference = if self.prefer_high {
                variable
            } else {
                15 - variable
            };
            let candidate = (concentration, preference, variable);
            if best.is_none_or(|previous| candidate > previous) {
                best = Some(candidate);
            }
        }
        let Some((_, _, variable)) = best else {
            // Unique membership queries cannot assign conflicting labels
            // to the same input. This fallback is never needed on valid data.
            return self.constant(first);
        };
        let mut low = Vec::new();
        let mut high = Vec::new();
        for &row in samples {
            if (row.0 >> variable) & 1 != 0 {
                high.push(row);
            } else {
                low.push(row);
            }
        }
        let next = remaining & !(1u16 << variable);
        let low_ref = self.build(&low, next);
        let high_ref = self.build(&high, next);
        if low_ref == high_ref {
            return low_ref;
        }
        if let Some(&id) = self.shared.get(&(variable, low_ref, high_ref)) {
            return id;
        }
        let index = self.nodes.len();
        self.nodes.push(Gate::Select {
            variable,
            low: low_ref,
            high: high_ref,
        });
        self.shared.insert((variable, low_ref, high_ref), index);
        index
    }
}
impl Circuit {
    pub fn learn(samples: &BTreeMap<u16, u8>, prefer_high: bool) -> Self {
        assert!(!samples.is_empty());
        assert!(samples.len() <= MAX_QUERIES);
        let rows: Vec<_> = samples.iter().map(|(&key, &label)| (key, label)).collect();
        let mut learner = TreeBuilder {
            nodes: Vec::new(),
            constants: HashMap::new(),
            shared: HashMap::new(),
            prefer_high,
        };
        let root = learner.build(&rows, u16::MAX);
        assert!(learner.nodes.len() <= LEARNED_NODE_CAP);
        Self {
            nodes: learner.nodes,
            root,
        }
    }
    pub fn evaluate(&self, packed: u16) -> u8 {
        let mut at = self.root;
        loop {
            match self.nodes[at] {
                Gate::Constant(label) => return label,
                Gate::Select {
                    variable,
                    low,
                    high,
                } => {
                    at = if (packed >> variable) & 1 == 0 {
                        low
                    } else {
                        high
                    };
                }
            }
        }
    }
    pub fn gates(&self) -> usize {
        self.nodes
            .iter()
            .filter(|node| matches!(node, Gate::Select { .. }))
            .count()
    }
    pub fn depth(&self) -> usize {
        fn walk(circuit: &Circuit, id: usize, memo: &mut [Option<usize>]) -> usize {
            if let Some(value) = memo[id] {
                return value;
            }
            let depth = match circuit.nodes[id] {
                Gate::Constant(_) => 0,
                Gate::Select { low, high, .. } => {
                    1 + walk(circuit, low, memo).max(walk(circuit, high, memo))
                }
            };
            memo[id] = Some(depth);
            depth
        }
        walk(self, self.root, &mut vec![None; self.nodes.len()])
    }
    /// Native function uses SysV x86-64 arguments dil/sil for a and b. A
    /// direct bit test and conditional jump traverses the learned gate DAG.
    /// There is NO posit arithmetic routine or runtime result lookup.
    pub fn emit_x86_64_gas(&self, operation: Operation) -> String {
        let symbol = format!("mem_posit8_active_{}", operation.symbol());
        let mut code = format!(
            ".intel_syntax noprefix\n.text\n.globl {symbol}\n.type {symbol}, @function\n{symbol}:\n"
        );
        writeln!(&mut code, "    jmp .L{symbol}_node_{}", self.root).unwrap();
        for (id, node) in self.nodes.iter().enumerate() {
            writeln!(&mut code, ".L{symbol}_node_{id}:").unwrap();
            match *node {
                Gate::Constant(result) => {
                    writeln!(&mut code, "    mov eax, {}", result).unwrap();
                    code.push_str("    ret\n");
                }
                Gate::Select {
                    variable,
                    low,
                    high,
                } => {
                    let argument = if variable % 2 == 0 { "dil" } else { "sil" };
                    let mask = 1u8 << (variable / 2);
                    writeln!(&mut code, "    test {argument}, {mask}").unwrap();
                    writeln!(&mut code, "    jnz .L{symbol}_node_{high}").unwrap();
                    writeln!(&mut code, "    jmp .L{symbol}_node_{low}").unwrap();
                }
            }
        }
        writeln!(&mut code, ".size {symbol}, .-{symbol}").unwrap();
        code.push_str(".section .note.GNU-stack,\"\",@progbits\n");
        code
    }
}

#[derive(Debug)]
pub struct ActiveReport {
    pub operation: Operation,
    pub teacher_queries: usize,
    pub committee_disagreements: usize,
    pub counterexamples: usize,
    pub train_errors: usize,
    pub holdout_errors: usize,
    pub holdout_size: usize,
    pub full_errors: usize,
    pub nodes: usize,
    pub gates: usize,
    pub depth: usize,
}
pub struct ActiveOutcome {
    pub circuit: Circuit,
    pub report: ActiveReport,
    pub samples: BTreeMap<u16, u8>,
}
fn permuted(index: u16, salt: u16) -> u16 {
    index.wrapping_mul(40503).wrapping_add(salt)
}
fn eligible(input: u16, known: &BTreeMap<u16, u8>) -> bool {
    !is_holdout(input) && !known.contains_key(&input)
}
fn acquire(
    known: &BTreeMap<u16, u8>,
    main: Option<&Circuit>,
    shadow: Option<&Circuit>,
    count: usize,
    salt: u16,
) -> Vec<u16> {
    let mut disagreement = Vec::new();
    let mut exploration = Vec::new();
    for step in 0..DOMAIN {
        let candidate = permuted(step as u16, salt);
        if !eligible(candidate, known) {
            continue;
        }
        if let (Some(a), Some(b)) = (main, shadow) {
            if a.evaluate(candidate) != b.evaluate(candidate) {
                disagreement.push(candidate);
            } else {
                exploration.push(candidate);
            }
        } else {
            exploration.push(candidate);
        }
    }
    let disagreement_quota = (count * 3) / 4;
    let mut batch = Vec::new();
    batch.extend(disagreement.iter().take(disagreement_quota).copied());
    batch.extend(exploration.iter().take(count - batch.len()).copied());
    if batch.len() < count {
        batch.extend(
            disagreement
                .into_iter()
                .skip(disagreement_quota)
                .take(count - batch.len()),
        );
    }
    batch
}
pub fn learn_active(operation: Operation) -> ActiveOutcome {
    let mut samples = BTreeMap::new();
    for input in acquire(&samples, None, None, INITIAL_QUERIES, 12345) {
        samples.insert(input, operation.teacher(input));
    }
    let mut counterexamples = 0usize;
    let mut committee_disagreements = 0usize;
    for round in 0..ACTIVE_ROUNDS {
        let primary = Circuit::learn(&samples, false);
        let secondary = Circuit::learn(&samples, true);
        let batch = acquire(
            &samples,
            Some(&primary),
            Some(&secondary),
            BATCH_QUERIES,
            5431u16.wrapping_add((round * 4099) as u16),
        );
        assert_eq!(batch.len(), BATCH_QUERIES);
        for packed in batch {
            if primary.evaluate(packed) != secondary.evaluate(packed) {
                committee_disagreements += 1;
            }
            let answer = operation.teacher(packed);
            if primary.evaluate(packed) != answer {
                counterexamples += 1;
            }
            assert!(samples.insert(packed, answer).is_none());
        }
    }
    assert_eq!(samples.len(), MAX_QUERIES);
    let circuit = Circuit::learn(&samples, false);
    let train_errors = samples
        .iter()
        .filter(|&(input, answer)| circuit.evaluate(*input) != *answer)
        .count();
    let mut holdout_size = 0usize;
    let mut holdout_errors = 0usize;
    let mut full_errors = 0usize;
    // Audit oracle access is NEVER used to alter the trained circuit.
    for id in 0..=u16::MAX {
        let predicted = circuit.evaluate(id);
        let actual = operation.teacher(id);
        if predicted != actual {
            full_errors += 1;
            if is_holdout(id) {
                holdout_errors += 1;
            }
        }
        if is_holdout(id) {
            holdout_size += 1;
        }
    }
    let report = ActiveReport {
        operation,
        teacher_queries: samples.len(),
        committee_disagreements,
        counterexamples,
        train_errors,
        holdout_errors,
        holdout_size,
        full_errors,
        nodes: circuit.nodes.len(),
        gates: circuit.gates(),
        depth: circuit.depth(),
    };
    ActiveOutcome {
        circuit,
        samples,
        report,
    }
}
fn main() {
    let flag = std::env::args().nth(1);
    let bitwise_action = match flag.as_deref() {
        Some("--emit-bitwise-x86-add") | Some("--report-bitwise-add") => Some(Operation::Add),
        Some("--emit-bitwise-x86-mul") | Some("--report-bitwise-mul") => Some(Operation::Mul),
        _ => None,
    };
    if let Some(op) = bitwise_action {
        let outcome = bitwise::learn_active_bitwise(op);
        if flag.as_deref().is_some_and(|name| name.starts_with("--emit-bitwise")) {
            print!("{}", outcome.circuit.emit_x86_64_gas(op));
        } else {
            let r = &outcome.report;
            println!(
                "SYNTH_LEARN1_BITWISE,op={},queries={},budget={},committee_disagreements={},counterexamples={},train_errors={},holdout_errors={},holdout_size={},full_errors={},nodes={},gates={},depth={},exact={}",
                r.operation.symbol(),
                r.teacher_queries,
                MAX_QUERIES,
                r.committee_disagreements,
                r.counterexamples,
                r.train_errors,
                r.holdout_errors,
                r.holdout_size,
                r.full_errors,
                r.nodes,
                r.gates,
                r.depth,
                r.full_errors == 0
            );
        }
        return;
    }
    let operation = match flag.as_deref() {
        Some("--emit-x86-add") | Some("--report-add") => Some(Operation::Add),
        Some("--emit-x86-mul") | Some("--report-mul") => Some(Operation::Mul),
        None => None,
        _ => panic!("valid flags: --emit-x86-add, --emit-x86-mul, --report-add, --report-mul"),
    };
    let operations = match operation {
        Some(op) => vec![op],
        None => vec![Operation::Add, Operation::Mul],
    };
    for op in operations {
        let outcome = learn_active(op);
        if matches!(flag.as_deref(), Some("--emit-x86-add" | "--emit-x86-mul")) {
            print!("{}", outcome.circuit.emit_x86_64_gas(op));
        } else {
            let r = &outcome.report;
            println!(
                "SYNTH_LEARN1,op={},queries={},budget={},committee_disagreements={},counterexamples={},train_errors={},holdout_errors={},holdout_size={},full_errors={},nodes={},gates={},depth={},exact={}",
                r.operation.symbol(),
                r.teacher_queries,
                MAX_QUERIES,
                r.committee_disagreements,
                r.counterexamples,
                r.train_errors,
                r.holdout_errors,
                r.holdout_size,
                r.full_errors,
                r.nodes,
                r.gates,
                r.depth,
                r.full_errors == 0
            );
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_queries_are_unique_and_never_include_holdout() {
        let model = learn_active(Operation::Add);
        assert_eq!(model.samples.len(), MAX_QUERIES);
        assert!(model.samples.keys().all(|&input| !is_holdout(input)));
        assert!(model.report.holdout_size > 0);
        assert_eq!(model.report.train_errors, 0);
        assert!(model.report.gates > 0);
        assert!(model.report.depth <= 16);
    }

    #[test]
    fn active_multiplication_preserves_every_membership_label() {
        let model = learn_active(Operation::Mul);
        assert_eq!(model.report.train_errors, 0);
        for (&input, &expected) in &model.samples {
            assert_eq!(model.circuit.evaluate(input), expected);
        }
        assert!(model.report.counterexamples > 0);
    }

    #[test]
    fn asm_is_native_direct_branching_not_runtime_table() {
        for op in [Operation::Add, Operation::Mul] {
            let model = learn_active(op);
            let asm = model.circuit.emit_x86_64_gas(op);
            assert!(asm.contains(".intel_syntax noprefix"));
            assert!(asm.contains("    test dil, "));
            assert!(asm.contains("    test sil, "));
            assert!(asm.contains("    jnz "));
            assert!(asm.contains(".note.GNU-stack"));
            assert!(!asm.contains(".rodata"));
        }
    }
}
