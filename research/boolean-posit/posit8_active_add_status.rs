//! SYNTH-LEARN1E: active learning of a composed Boolean status classifier
//! for posit8 ADD, with explicitly provided raw-bit magnitude comparisons.
//! Output is status {1 NaR, 2 zero, 4 negative, 0 positive}, not a posit8 sum.
#![forbid(unsafe_code)]
#[path = "posit8_active_sign_gates.rs"]
#[allow(dead_code)]
mod learned_gates;
#[path = "posit8_circuit_synth.rs"]
#[allow(dead_code)]
mod arithmetic;
use arithmetic::Posit8;
use learned_gates::Gate;
use std::collections::BTreeSet;
use std::fmt::Write;

const CANDIDATES: [Gate; 9] = [
    Gate::Zero, Gate::One, Gate::SignA, Gate::SignB,
    Gate::NotA, Gate::NotB, Gate::And, Gate::Or, Gate::Xor,
];
const SYMBOLS: [u8; 16] = [
    0, 1, 0x40, 0x7f, 0x80, 0x81, 0xc0, 0xff,
    2, 0x3f, 0x41, 0x82, 0xfd, 0x10, 0xf0, 0x55,
];
const MAX_QUERIES: usize = 16;

#[derive(Clone, Copy, Debug)]
enum Task { Nar, Zero, Negative }
impl Task {
    fn name(self) -> &'static str {
        match self { Self::Nar => "nar", Self::Zero => "zero", Self::Negative => "negative" }
    }
    fn eligible(self, a: u8, b: u8) -> bool {
        match self {
            Self::Nar => true,
            Self::Zero => a != 0x80 && b != 0x80,
            Self::Negative => {
                a != 0x80 && b != 0x80 && b != a.wrapping_neg()
            }
        }
    }
    fn magnitude(raw: u8) -> u8 {
        if raw & 0x80 != 0 { raw.wrapping_neg() } else { raw }
    }
    fn feature_pair(self, a: u8, b: u8) -> (u8, u8) {
        match self {
            Self::Nar => (u8::from(a == 0x80), u8::from(b == 0x80)),
            Self::Zero => (
                u8::from(b == a.wrapping_neg()),
                u8::from(a == 0 && b == 0)
            ),
            Self::Negative => {
                let mag_a = Self::magnitude(a);
                let mag_b = Self::magnitude(b);
                (
                    u8::from(a & 0x80 != 0 && mag_a >= mag_b),
                    u8::from(b & 0x80 != 0 && mag_b > mag_a)
                )
            }
        }
    }
    fn teacher(self, a: u8, b: u8) -> u8 {
        assert!(self.eligible(a, b));
        let result = Posit8(a).add(Posit8(b)).0;
        match self {
            Self::Nar => u8::from(result == 0x80),
            Self::Zero => u8::from(result == 0),
            Self::Negative => u8::from(result & 0x80 != 0),
        }
    }
}
fn gate_value(gate: Gate, x: u8, y: u8) -> u8 {
    assert!(x <= 1 && y <= 1);
    gate.evaluate(x << 7, y << 7)
}
#[derive(Debug)]
struct LearnedRule {
    task: Task,
    gate: Gate,
    examples: Vec<(u8, u8, u8)>,
    eligible_count: usize,
    unseen_errors: usize,
}
fn discover(task: Task) -> LearnedRule {
    let candidates_pool: Vec<_> = SYMBOLS
        .iter()
        .flat_map(|&a| SYMBOLS.iter().map(move |&b| (a, b)))
        .filter(|&(a, b)| task.eligible(a, b))
        .collect();
    let mut remaining = CANDIDATES.to_vec();
    let mut observations = Vec::new();
    let mut queried = BTreeSet::new();
    loop {
        let select = candidates_pool.iter().copied().enumerate()
            .filter_map(|(index, (a, b))| {
                if queried.contains(&(a, b)) { return None; }
                let (x, y) = task.feature_pair(a, b);
                let ones = remaining.iter().filter(|&&gate| gate_value(gate, x, y) == 1).count();
                Some((ones * (remaining.len() - ones), std::cmp::Reverse(index), a, b))
            })
            .max();
        let Some((score, _, a, b)) = select else { break };
        if score == 0 { break; }
        assert!(observations.len() < MAX_QUERIES, "query budget exhausted");
        let label = task.teacher(a, b);
        let (x, y) = task.feature_pair(a, b);
        assert!(queried.insert((a, b)));
        observations.push((a, b, label));
        remaining.retain(|&gate| gate_value(gate, x, y) == label);
        assert!(!remaining.is_empty(), "frozen candidate grammar failed");
    }
    // Deterministic complexity / enumeration priority. Equivalent gates
    // on inaccessible feature combinations are not identifiable.
    let gate = *remaining.first().expect("consistent candidate");
    let mut eligible_count = 0usize;
    let mut unseen_errors = 0usize;
    for a in 0..=u8::MAX {
        for b in 0..=u8::MAX {
            if !task.eligible(a, b) { continue; }
            eligible_count += 1;
            if !queried.contains(&(a, b)) {
                let (x, y) = task.feature_pair(a, b);
                unseen_errors += usize::from(gate_value(gate, x, y) != task.teacher(a, b));
            }
        }
    }
    LearnedRule { task, gate, examples: observations, eligible_count, unseen_errors }
}

pub struct LearnedAddStatus {
    nar: LearnedRule,
    zero: LearnedRule,
    negative: LearnedRule,
}
impl LearnedAddStatus {
    pub fn train() -> Self {
        Self {
            nar: discover(Task::Nar),
            zero: discover(Task::Zero),
            negative: discover(Task::Negative),
        }
    }
    pub fn query_count(&self) -> usize {
        self.nar.examples.len() + self.zero.examples.len() + self.negative.examples.len()
    }
    /// Four-way classification, not posit8 ADD magnitude.
    pub fn classify(&self, a: u8, b: u8) -> u8 {
        let (x, y) = Task::Nar.feature_pair(a, b);
        if gate_value(self.nar.gate, x, y) == 1 { return 1; }
        let (x, y) = Task::Zero.feature_pair(a, b);
        if gate_value(self.zero.gate, x, y) == 1 { return 2; }
        let (x, y) = Task::Negative.feature_pair(a, b);
        if gate_value(self.negative.gate, x, y) == 1 { 4 } else { 0 }
    }
    pub fn oracle_status(a: u8, b: u8) -> u8 {
        let raw = Posit8(a).add(Posit8(b)).0;
        if raw == 0x80 { 1 }
        else if raw == 0 { 2 }
        else if raw & 0x80 != 0 { 4 }
        else { 0 }
    }
    pub fn audit(&self) -> usize {
        let mut errors = 0;
        for a in 0..=u8::MAX {
            for b in 0..=u8::MAX {
                errors += usize::from(self.classify(a, b) != Self::oracle_status(a, b));
            }
        }
        errors
    }
    fn gate_asm(gate: Gate) -> &'static str {
        match gate {
            Gate::Zero => "    xor eax, eax\n",
            Gate::One => "    mov eax, 1\n",
            Gate::SignA => "",
            Gate::SignB => "    mov eax, edx\n",
            Gate::NotA => "    xor eax, 1\n",
            Gate::NotB => "    mov eax, edx\n    xor eax, 1\n",
            Gate::And => "    and eax, edx\n",
            Gate::Or => "    or eax, edx\n",
            Gate::Xor => "    xor eax, edx\n",
        }
    }
    /// Emit one stand-alone native x86-64 SysV four-way ADD-status function.
    pub fn emit_gas(&self) -> String {
        let symbol = "mem_posit8_learned_add_status";
        let mut asm = format!(".intel_syntax noprefix\n.text\n.globl {symbol}\n.type {symbol}, @function\n{symbol}:\n");
        // NaR features: each input is the raw 0x80 sentinel.
        asm.push_str("    cmp dil, 128\n    sete al\n    cmp sil, 128\n    sete dl\n    movzx eax, al\n    movzx edx, dl\n");
        asm.push_str(Self::gate_asm(self.nar.gate));
        asm.push_str("    test eax, eax\n    jnz .Ladd_status_nar\n");
        // Zero features: b == -a (including both zero), and both zero.
        asm.push_str("    mov al, dil\n    neg al\n    cmp al, sil\n    sete al\n    movzx eax, al\n");
        asm.push_str("    test dil, dil\n    sete dl\n    test sil, sil\n    sete cl\n    and dl, cl\n    movzx edx, dl\n");
        asm.push_str(Self::gate_asm(self.zero.gate));
        asm.push_str("    test eax, eax\n    jnz .Ladd_status_zero\n");
        // Raw posit8 finite encodings are monotone in magnitude after two's
        // complement on negative words; both operands are now non-NaR.
        asm.push_str("    movzx r9d, dil\n    test dil, 128\n    jz .Ladd_mag_a\n    neg r9b\n.Ladd_mag_a:\n");
        asm.push_str("    movzx r10d, sil\n    test sil, 128\n    jz .Ladd_mag_b\n    neg r10b\n.Ladd_mag_b:\n");
        // first feature: negative(a) AND magnitude(a)>=magnitude(b)
        asm.push_str("    xor eax, eax\n    test dil, 128\n    jz .Ladd_dom_a\n    cmp r9b, r10b\n    jb .Ladd_dom_a\n    mov eax, 1\n.Ladd_dom_a:\n");
        // second feature: negative(b) AND magnitude(b)>magnitude(a)
        asm.push_str("    xor edx, edx\n    test sil, 128\n    jz .Ladd_dom_b\n    cmp r10b, r9b\n    jbe .Ladd_dom_b\n    mov edx, 1\n.Ladd_dom_b:\n");
        asm.push_str(Self::gate_asm(self.negative.gate));
        asm.push_str("    shl eax, 2\n    ret\n");
        asm.push_str(".Ladd_status_nar:\n    mov eax, 1\n    ret\n");
        asm.push_str(".Ladd_status_zero:\n    mov eax, 2\n    ret\n");
        writeln!(&mut asm, ".size {symbol}, .-{symbol}").unwrap();
        asm.push_str(".section .note.GNU-stack,\"\",@progbits\n");
        asm
    }
    fn report(&self) {
        for learned in [&self.nar, &self.zero, &self.negative] {
            println!(
                "SYNTH_LEARN1E,task={},gate={:?},teacher_queries={},eligible={},unseen_errors={}",
                learned.task.name(), learned.gate, learned.examples.len(),
                learned.eligible_count, learned.unseen_errors
            );
        }
        println!(
            "SYNTH_LEARN1E_COMPOSED,operation=add,queries={},domain=65536,errors={},classes=nar_zero_negative_positive",
            self.query_count(), self.audit()
        );
    }
}
fn main() {
    let circuit = LearnedAddStatus::train();
    match std::env::args().nth(1).as_deref() {
        None | Some("--report") => circuit.report(),
        Some("--emit-gas") => print!("{}", circuit.emit_gas()),
        _ => panic!("valid options: --report, --emit-gas"),
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discover_gates_with_bounded_teacher_queries_and_no_unseen_errors() {
        let learner = LearnedAddStatus::train();
        assert_eq!(learner.nar.gate, Gate::Or);
        assert_eq!(learner.zero.gate, Gate::SignA);
        assert_eq!(learner.negative.gate, Gate::Or);
        assert_eq!(learner.nar.eligible_count, 65536);
        assert_eq!(learner.zero.eligible_count, 255 * 255);
        assert_eq!(learner.negative.eligible_count, 255 * 254);
        for learned in [&learner.nar, &learner.zero, &learner.negative] {
            assert_eq!(learned.unseen_errors, 0);
            assert!(!learned.examples.is_empty());
            assert!(learned.examples.len() <= MAX_QUERIES);
            for &(a, b, expected) in &learned.examples {
                let (x, y) = learned.task.feature_pair(a, b);
                assert_eq!(gate_value(learned.gate, x, y), expected);
            }
        }
        assert!(learner.query_count() <= 3 * MAX_QUERIES);
    }

    #[test]
    fn full_domain_add_status_classification_is_exact() {
        let model = LearnedAddStatus::train();
        assert_eq!(model.audit(), 0);
        assert_eq!(model.classify(0, 0), 2);
        assert_eq!(model.classify(0x40, 0xc0), 2);
        assert_eq!(model.classify(0x40, 0x40), 0);
        assert_eq!(model.classify(0xc0, 0xc0), 4);
        assert_eq!(model.classify(0x80, 0x40), 1);
    }

    #[test]
    fn emitter_uses_native_boolean_composition_without_result_lut() {
        let asm = LearnedAddStatus::train().emit_gas();
        assert!(asm.contains(".intel_syntax noprefix"));
        assert!(asm.contains("    or eax, edx"));
        assert!(asm.contains("    cmp r9b, r10b"));
        assert!(asm.contains(".note.GNU-stack"));
        assert!(!asm.contains(".rodata"));
    }
}
