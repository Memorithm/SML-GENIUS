//! SYNTH-LEARN1D: actively discover three independently learned Boolean gates
//! and compose them into an exact posit8 multiplication *status* classifier.
//! Status bits: 1=NaR, 2=zero, 4=negative; positive finite is 0.
//! This is NOT a complete posit8 multiplication circuit (magnitude omitted).
#![forbid(unsafe_code)]
#[path = "posit8_active_sign_gates.rs"]
#[allow(dead_code)]
mod known_gates;

use known_gates::Gate;
use std::collections::BTreeSet;
use std::fmt::Write;

const CANDIDATES: [Gate; 9] = [
    Gate::Zero, Gate::One, Gate::SignA, Gate::SignB,
    Gate::NotA, Gate::NotB, Gate::And, Gate::Or, Gate::Xor,
];
const EXAMPLES: [u8; 16] = [
    0, 1, 0x40, 0x7f, 0x80, 0x81, 0xc0, 0xff,
    2, 0x3f, 0x41, 0x82, 0xfd, 0x10, 0xf0, 0x55,
];
const MAX_QUERIES_PER_PREDICATE: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Predicate {
    Nar,
    Zero,
    Negative,
}
impl Predicate {
    fn name(self) -> &'static str {
        match self {
            Self::Nar => "nar",
            Self::Zero => "zero",
            Self::Negative => "negative",
        }
    }
    fn eligible(self, a: u8, b: u8) -> bool {
        match self {
            Self::Nar => true,
            Self::Zero => a != 0x80 && b != 0x80,
            Self::Negative => a != 0 && b != 0 && a != 0x80 && b != 0x80,
        }
    }
    fn inputs(self, a: u8, b: u8) -> (u8, u8) {
        match self {
            Self::Nar => (u8::from(a == 0x80), u8::from(b == 0x80)),
            Self::Zero => (u8::from(a == 0), u8::from(b == 0)),
            Self::Negative => ((a >> 7) & 1, (b >> 7) & 1),
        }
    }
    fn teacher(self, a: u8, b: u8) -> u8 {
        assert!(self.eligible(a, b));
        // This oracle is accessed only on selected training queries and in
        // the separate, read-only final exhaustive audit.
        let raw = known_gates::oracle::Posit8(a)
            .mul(known_gates::oracle::Posit8(b))
            .0;
        match self {
            Self::Nar => u8::from(raw == 0x80),
            Self::Zero => u8::from(raw == 0),
            Self::Negative => (raw >> 7) & 1,
        }
    }
}

fn boolean(gate: Gate, first: u8, second: u8) -> u8 {
    assert!(first <= 1 && second <= 1);
    gate.evaluate(first << 7, second << 7)
}
#[derive(Debug)]
struct LearnedPredicate {
    task: Predicate,
    gate: Gate,
    queries: Vec<(u8, u8, u8)>,
    eligible: usize,
    unseen_errors: usize,
}
fn discover(task: Predicate) -> LearnedPredicate {
    let pool: Vec<(u8, u8)> = EXAMPLES
        .iter()
        .flat_map(|&a| EXAMPLES.iter().map(move |&b| (a, b)))
        .filter(|&(a, b)| task.eligible(a, b))
        .collect();
    let mut candidates = CANDIDATES.to_vec();
    let mut queried = BTreeSet::new();
    let mut examples = Vec::new();
    loop {
        let selected = pool.iter().copied().enumerate().filter_map(|(index, (a, b))| {
            if queried.contains(&(a, b)) {
                return None;
            }
            let (x, y) = task.inputs(a, b);
            let ones = candidates
                .iter()
                .filter(|&&gate| boolean(gate, x, y) != 0)
                .count();
            let score = ones * (candidates.len() - ones);
            Some((score, std::cmp::Reverse(index), a, b))
        }).max();
        let Some((score, _, a, b)) = selected else { break };
        if score == 0 {
            break;
        }
        assert!(examples.len() < MAX_QUERIES_PER_PREDICATE);
        let label = task.teacher(a, b);
        examples.push((a, b, label));
        assert!(queried.insert((a, b)));
        let (x, y) = task.inputs(a, b);
        candidates.retain(|&gate| boolean(gate, x, y) == label);
        assert!(!candidates.is_empty(), "frozen Boolean grammar cannot fit oracle");
    }
    // Stable frozen complexity priority: direct wires, then single gates.
    let gate = *candidates.first().expect("at least one consistent hypothesis");
    let mut eligible = 0;
    let mut unseen_errors = 0;
    for a in 0..=u8::MAX {
        for b in 0..=u8::MAX {
            if !task.eligible(a, b) { continue; }
            eligible += 1;
            if !queried.contains(&(a, b)) {
                let (x, y) = task.inputs(a, b);
                unseen_errors += usize::from(boolean(gate, x, y) != task.teacher(a, b));
            }
        }
    }
    LearnedPredicate { task, gate, queries: examples, eligible, unseen_errors }
}
pub struct LearnedMulStatus {
    nar: LearnedPredicate,
    zero: LearnedPredicate,
    negative: LearnedPredicate,
}
impl LearnedMulStatus {
    pub fn train() -> Self {
        Self {
            nar: discover(Predicate::Nar),
            zero: discover(Predicate::Zero),
            negative: discover(Predicate::Negative),
        }
    }
    pub fn total_queries(&self) -> usize {
        self.nar.queries.len() + self.zero.queries.len() + self.negative.queries.len()
    }
    /// Output is a three-bit *classification*, not the posit8 result.
    pub fn classify(&self, a: u8, b: u8) -> u8 {
        if boolean(self.nar.gate, u8::from(a == 0x80), u8::from(b == 0x80)) != 0 {
            1
        } else if boolean(self.zero.gate, u8::from(a == 0), u8::from(b == 0)) != 0 {
            2
        } else if boolean(self.negative.gate, (a >> 7) & 1, (b >> 7) & 1) != 0 {
            4
        } else {
            0
        }
    }
    pub fn oracle_class(a: u8, b: u8) -> u8 {
        let raw = known_gates::oracle::Posit8(a)
            .mul(known_gates::oracle::Posit8(b))
            .0;
        if raw == 0x80 { 1 } else if raw == 0 { 2 }
        else if raw & 0x80 != 0 { 4 } else { 0 }
    }
    pub fn audit_all(&self) -> usize {
        let mut errors = 0;
        for a in 0..=u8::MAX {
            for b in 0..=u8::MAX {
                errors += usize::from(self.classify(a, b) != Self::oracle_class(a, b));
            }
        }
        errors
    }
    /// Generated standalone x86-64 System V implementation of the learned
    /// Boolean gates; computed flags are NOT a substitute for real posit8 MUL.
    pub fn emit_gas(&self) -> String {
        let symbol = "mem_posit8_learned_mul_status";
        let mut code = format!(".intel_syntax noprefix\n.text\n.globl {symbol}\n.type {symbol}, @function\n{symbol}:\n");
        for (step, candidate) in [&self.nar, &self.zero, &self.negative].iter().enumerate() {
            let load = match candidate.task {
                Predicate::Nar => "    cmp dil, 128\n    sete al\n    cmp sil, 128\n    sete dl\n    movzx eax, al\n    movzx edx, dl\n",
                Predicate::Zero => "    test dil, dil\n    sete al\n    test sil, sil\n    sete dl\n    movzx eax, al\n    movzx edx, dl\n",
                Predicate::Negative => "    movzx eax, dil\n    shr eax, 7\n    movzx edx, sil\n    shr edx, 7\n",
            };
            code.push_str(load);
            let gate_asm = match candidate.gate {
                Gate::Zero => "    xor eax, eax\n",
                Gate::One => "    mov eax, 1\n",
                Gate::SignA => "",
                Gate::SignB => "    mov eax, edx\n",
                Gate::NotA => "    xor eax, 1\n",
                Gate::NotB => "    mov eax, edx\n    xor eax, 1\n",
                Gate::And => "    and eax, edx\n",
                Gate::Or => "    or eax, edx\n",
                Gate::Xor => "    xor eax, edx\n",
            };
            code.push_str(gate_asm);
            code.push_str("    test eax, eax\n");
            match step {
                0 => code.push_str("    jnz .Lmul_status_nar\n"),
                1 => code.push_str("    jnz .Lmul_status_zero\n"),
                _ => code.push_str("    shl eax, 2\n    ret\n"),
            }
        }
        code.push_str(".Lmul_status_nar:\n    mov eax, 1\n    ret\n");
        code.push_str(".Lmul_status_zero:\n    mov eax, 2\n    ret\n");
        writeln!(&mut code, ".size {symbol}, .-{symbol}").unwrap();
        code.push_str(".section .note.GNU-stack,\"\",@progbits\n");
        code
    }
    fn report(&self) {
        for candidate in [&self.nar, &self.zero, &self.negative] {
            println!(
                "SYNTH_LEARN1D,task={},gate={:?},teacher_queries={},eligible={},unseen_errors={}",
                candidate.task.name(), candidate.gate, candidate.queries.len(),
                candidate.eligible, candidate.unseen_errors
            );
        }
        println!(
            "SYNTH_LEARN1D_COMPOSED,operation=mul,teacher_queries={},domain=65536,errors={},classes=nar_zero_negative_positive",
            self.total_queries(), self.audit_all()
        );
    }
}
fn main() {
    let learned = LearnedMulStatus::train();
    match std::env::args().nth(1).as_deref() {
        None | Some("--report") => learned.report(),
        Some("--emit-gas") => print!("{}", learned.emit_gas()),
        _ => panic!("use --report or --emit-gas"),
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn learned_gates_are_exact_on_unqueried_operation_subdomains() {
        let learned = LearnedMulStatus::train();
        assert_eq!(learned.nar.gate, Gate::Or);
        assert_eq!(learned.zero.gate, Gate::Or);
        assert_eq!(learned.negative.gate, Gate::Xor);
        assert_eq!(learned.nar.eligible, 65536);
        assert_eq!(learned.zero.eligible, 255 * 255);
        assert_eq!(learned.negative.eligible, 254 * 254);
        for task in [&learned.nar, &learned.zero, &learned.negative] {
            assert_eq!(task.unseen_errors, 0);
            assert!(!task.queries.is_empty());
            assert!(task.queries.len() <= MAX_QUERIES_PER_PREDICATE);
            for &(a, b, label) in &task.queries {
                assert!(task.task.eligible(a, b));
                let (x, y) = task.task.inputs(a, b);
                assert_eq!(label, boolean(task.gate, x, y));
            }
        }
        assert!(learned.total_queries() <= 3 * MAX_QUERIES_PER_PREDICATE);
    }

    #[test]
    fn composed_native_class_matches_all_65536_posit8_mul_statuses() {
        let learned = LearnedMulStatus::train();
        assert_eq!(learned.audit_all(), 0);
        assert_eq!(learned.classify(0x80, 0), 1);
        assert_eq!(learned.classify(0, 0), 2);
        assert_eq!(learned.classify(0x40, 0x81), 4);
        assert_eq!(learned.classify(0xc0, 0x81), 0);
    }

    #[test]
    fn generated_assembly_is_real_machine_logic_not_arithmetic_lookup() {
        let assembly = LearnedMulStatus::train().emit_gas();
        assert!(assembly.contains(".intel_syntax noprefix"));
        assert!(assembly.contains("    or eax, edx"));
        assert!(assembly.contains("    xor eax, edx"));
        assert!(assembly.contains("    sete al"));
        assert!(assembly.contains(".note.GNU-stack"));
        assert!(!assembly.contains(".rodata"));
    }
}
