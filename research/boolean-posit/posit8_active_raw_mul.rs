//! SYNTH-LEARN1F: discover raw-posit8 Boolean predicate circuits without
//! giving the classifier engineered isNaR/isZero/sign input features.
//! Stage one: active version-space induction of conjunctions of raw literals.
//! Stage two: active gate selection (OR/XOR) on learned unary outputs.
//! The combined four-class MUL status is not a numeric posit8 product.
#![forbid(unsafe_code)]
#[path = "posit8_circuit_synth.rs"]
#[allow(dead_code)]
mod arithmetic;
#[path = "posit8_active_sign_gates.rs"]
#[allow(dead_code)]
mod gate_library;

use arithmetic::Posit8;
use gate_library::Gate;
use std::collections::BTreeSet;
use std::fmt::Write;

const UNARY_BUDGET: usize = 12;
const BINARY_BUDGET: usize = 16;
const EXAMPLES: [u8; 16] = [
    0, 1, 0x40, 0x7f, 0x80, 0x81, 0xc0, 0xff, 2, 0x3f, 0x41, 0x82, 0xfd, 0x10, 0xf0, 0x55,
];
const GATES: [Gate; 9] = [
    Gate::Zero, Gate::One, Gate::SignA, Gate::SignB, Gate::NotA, Gate::NotB,
    Gate::And, Gate::Or, Gate::Xor,
];

/// Any conjunction of positive/negative input literals, 3^8 = 6561 choices.
/// The learner must infer both the tested bit mask and its required values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MaskPredicate {
    pub mask: u8,
    pub value: u8,
}
impl MaskPredicate {
    pub fn evaluate(self, raw: u8) -> u8 {
        u8::from(raw & self.mask == self.value)
    }
}
fn hypotheses() -> Vec<MaskPredicate> {
    let mut all = Vec::with_capacity(6561);
    for mut trits in 0..6561 {
        let mut mask = 0u8;
        let mut value = 0u8;
        for bit in 0..8 {
            match trits % 3 {
                1 => mask |= 1 << bit,
                2 => {
                    mask |= 1 << bit;
                    value |= 1 << bit;
                }
                _ => {}
            }
            trits /= 3;
        }
        all.push(MaskPredicate { mask, value });
    }
    all
}

#[derive(Clone, Copy, Debug)]
enum UnaryTarget {
    Nar,
    Zero,
    Sign,
}
impl UnaryTarget {
    fn name(self) -> &'static str {
        match self {
            Self::Nar => "nar",
            Self::Zero => "zero",
            Self::Sign => "sign",
        }
    }
    fn teacher(self, raw: u8) -> u8 {
        // The arithmetic result is obtained ONLY for membership queries and
        // the frozen post-training audit, never to select unlabeled inputs.
        let product = Posit8(raw).mul(Posit8(0x40)).0;
        match self {
            Self::Nar => u8::from(product == 0x80),
            Self::Zero => u8::from(product == 0),
            Self::Sign => (product >> 7) & 1,
        }
    }
}

#[derive(Debug)]
pub struct LearnedUnary {
    task: UnaryTarget,
    pub circuit: MaskPredicate,
    pub samples: Vec<(u8, u8)>,
    pub remaining_hypotheses: usize,
    pub unseen_errors: usize,
}
fn learn_unary(task: UnaryTarget) -> LearnedUnary {
    let mut remaining = hypotheses();
    let mut samples = Vec::with_capacity(UNARY_BUDGET);
    let mut queried = BTreeSet::new();

    // Both seeds are queried from the teacher. Their labels are NOT supplied
    // directly; these cover two distinctive encodings for each unary target.
    for raw in [0u8, 0x80] {
        let label = task.teacher(raw);
        assert!(queried.insert(raw));
        samples.push((raw, label));
        remaining.retain(|model| model.evaluate(raw) == label);
    }
    assert!(!remaining.is_empty());
    loop {
        // Maximize disagreement of the surviving Boolean hypotheses using
        // predictions only. No training oracle is consulted for other inputs.
        let mut best = (0usize, None);
        for raw in 0..=u8::MAX {
            if queried.contains(&raw) {
                continue;
            }
            let ones = remaining
                .iter()
                .filter(|model| model.evaluate(raw) != 0)
                .count();
            let disagreement = ones * (remaining.len() - ones);
            if disagreement > best.0 {
                best = (disagreement, Some(raw));
            }
        }
        let Some(raw) = best.1 else {
            break;
        };
        assert!(samples.len() < UNARY_BUDGET, "unary membership budget exceeded");
        let label = task.teacher(raw);
        assert!(queried.insert(raw));
        samples.push((raw, label));
        remaining.retain(|model| model.evaluate(raw) == label);
        assert!(!remaining.is_empty(), "unary Boolean grammar cannot fit teacher");
    }
    let circuit = *remaining
        .iter()
        .min_by_key(|model| (model.mask.count_ones(), model.mask, model.value))
        .expect("fitted raw-bit hypothesis");
    let unseen_errors = (0..=u8::MAX)
        .filter(|raw| !queried.contains(raw))
        .filter(|&raw| circuit.evaluate(raw) != task.teacher(raw))
        .count();
    LearnedUnary {
        task,
        circuit,
        samples,
        remaining_hypotheses: remaining.len(),
        unseen_errors,
    }
}

#[derive(Clone, Copy, Debug)]
enum BinaryTarget {
    Nar,
    Zero,
    Negative,
}
impl BinaryTarget {
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
    fn teacher(self, a: u8, b: u8) -> u8 {
        assert!(self.eligible(a, b));
        let raw = Posit8(a).mul(Posit8(b)).0;
        match self {
            Self::Nar => u8::from(raw == 0x80),
            Self::Zero => u8::from(raw == 0),
            Self::Negative => (raw >> 7) & 1,
        }
    }
    fn features(self, a: u8, b: u8, unary: &[LearnedUnary; 3]) -> (u8, u8) {
        let pred = match self {
            Self::Nar => unary[0].circuit,
            Self::Zero => unary[1].circuit,
            Self::Negative => unary[2].circuit,
        };
        (pred.evaluate(a), pred.evaluate(b))
    }
}
fn eval_gate(gate: Gate, x: u8, y: u8) -> u8 {
    assert!(x <= 1 && y <= 1);
    gate.evaluate(x << 7, y << 7)
}
#[derive(Debug)]
struct LearnedGate {
    task: BinaryTarget,
    gate: Gate,
    samples: Vec<(u8, u8, u8)>,
    unseen_errors: usize,
}
fn learn_gate(task: BinaryTarget, unary: &[LearnedUnary; 3]) -> LearnedGate {
    let pool: Vec<_> = EXAMPLES
        .iter()
        .flat_map(|&a| EXAMPLES.iter().map(move |&b| (a, b)))
        .filter(|&(a, b)| task.eligible(a, b))
        .collect();
    let mut remaining = GATES.to_vec();
    let mut queried = BTreeSet::new();
    let mut samples = Vec::new();
    loop {
        let mut best: Option<(usize, usize, u8, u8)> = None;
        for (i, &(a, b)) in pool.iter().enumerate() {
            if queried.contains(&(a, b)) {
                continue;
            }
            let (x, y) = task.features(a, b, unary);
            let ones = remaining
                .iter()
                .filter(|&&gate| eval_gate(gate, x, y) == 1)
                .count();
            let score = ones * (remaining.len() - ones);
            if best.is_none_or(|candidate| score > candidate.0) {
                best = Some((score, i, a, b));
            }
        }
        let Some((score, _, a, b)) = best else {
            break;
        };
        if score == 0 {
            break;
        }
        assert!(samples.len() < BINARY_BUDGET, "binary teacher budget exceeded");
        let label = task.teacher(a, b);
        let (x, y) = task.features(a, b, unary);
        assert!(queried.insert((a, b)));
        samples.push((a, b, label));
        remaining.retain(|&gate| eval_gate(gate, x, y) == label);
        assert!(!remaining.is_empty(), "composed gate cannot match oracle");
    }
    let gate = remaining[0];
    let mut unseen_errors = 0;
    for a in 0..=u8::MAX {
        for b in 0..=u8::MAX {
            if !task.eligible(a, b) || queried.contains(&(a, b)) {
                continue;
            }
            let (x, y) = task.features(a, b, unary);
            unseen_errors += usize::from(eval_gate(gate, x, y) != task.teacher(a, b));
        }
    }
    LearnedGate {
        task,
        gate,
        samples,
        unseen_errors,
    }
}

pub struct LearnedRawMulStatus {
    unary: [LearnedUnary; 3],
    gates: [LearnedGate; 3],
}
impl LearnedRawMulStatus {
    pub fn train() -> Self {
        let unary = [
            learn_unary(UnaryTarget::Nar),
            learn_unary(UnaryTarget::Zero),
            learn_unary(UnaryTarget::Sign),
        ];
        let gates = [
            learn_gate(BinaryTarget::Nar, &unary),
            learn_gate(BinaryTarget::Zero, &unary),
            learn_gate(BinaryTarget::Negative, &unary),
        ];
        Self { unary, gates }
    }
    pub fn teacher_queries(&self) -> usize {
        self.unary.iter().map(|x| x.samples.len()).sum::<usize>()
            + self.gates.iter().map(|x| x.samples.len()).sum::<usize>()
    }
    pub fn classify(&self, a: u8, b: u8) -> u8 {
        for (index, &code) in [1u8, 2, 4].iter().enumerate() {
            let (x, y) = self.gates[index].task.features(a, b, &self.unary);
            if eval_gate(self.gates[index].gate, x, y) != 0 {
                return code;
            }
        }
        0
    }
    pub fn oracle_status(a: u8, b: u8) -> u8 {
        let raw = Posit8(a).mul(Posit8(b)).0;
        if raw == 0x80 {
            1
        } else if raw == 0 {
            2
        } else if raw & 0x80 != 0 {
            4
        } else {
            0
        }
    }
    pub fn audit_full(&self) -> usize {
        let mut mismatches = 0;
        for a in 0..=u8::MAX {
            for b in 0..=u8::MAX {
                mismatches += usize::from(self.classify(a, b) != Self::oracle_status(a, b));
            }
        }
        mismatches
    }
    /// Generate a *standalone* native SysV x86-64 function from learned
    /// masks AND learned OR/XOR gates. No runtime oracle or answer lookup.
    pub fn emit_x86_64_gas(&self) -> String {
        let symbol = "mem_posit8_learned_raw_mul_status";
        let mut asm = format!(
            ".intel_syntax noprefix\n.text\n.globl {symbol}\n.type {symbol}, @function\n{symbol}:\n"
        );
        for (index, unary) in self.unary.iter().enumerate() {
            // Each raw-byte predicate is compiled as a masked compare. The
            // mask and value come from version-space learning, not constants.
            for (source, dest, bitreg) in [
                ("dil", "eax", "al"),
                ("sil", "edx", "dl"),
            ] {
                writeln!(&mut asm, "    movzx {dest}, {source}").unwrap();
                writeln!(&mut asm, "    and {dest}, {}", unary.circuit.mask).unwrap();
                writeln!(&mut asm, "    cmp {dest}, {}", unary.circuit.value).unwrap();
                writeln!(&mut asm, "    sete {bitreg}").unwrap();
                writeln!(&mut asm, "    movzx {dest}, {bitreg}").unwrap();
            }
            match self.gates[index].gate {
                Gate::Zero => asm.push_str("    xor eax, eax\n"),
                Gate::One => asm.push_str("    mov eax, 1\n"),
                Gate::SignA => {}
                Gate::SignB => asm.push_str("    mov eax, edx\n"),
                Gate::NotA => asm.push_str("    xor eax, 1\n"),
                Gate::NotB => asm.push_str("    mov eax, edx\n    xor eax, 1\n"),
                Gate::And => asm.push_str("    and eax, edx\n"),
                Gate::Or => asm.push_str("    or eax, edx\n"),
                Gate::Xor => asm.push_str("    xor eax, edx\n"),
            }
            match index {
                0 => asm.push_str("    test eax, eax\n    jnz .Lraw_mul_nar\n"),
                1 => asm.push_str("    test eax, eax\n    jnz .Lraw_mul_zero\n"),
                _ => asm.push_str("    shl eax, 2\n    ret\n"),
            }
        }
        asm.push_str(".Lraw_mul_nar:\n    mov eax, 1\n    ret\n");
        asm.push_str(".Lraw_mul_zero:\n    mov eax, 2\n    ret\n");
        writeln!(&mut asm, ".size {symbol}, .-{symbol}").unwrap();
        asm.push_str(".section .note.GNU-stack,\"\",@progbits\n");
        asm
    }
    fn report(&self) {
        for unary in &self.unary {
            println!(
                "SYNTH_LEARN1F_RAW,task={},mask={:#04x},value={:#04x},queries={},remaining={},unseen_errors={}",
                unary.task.name(), unary.circuit.mask, unary.circuit.value,
                unary.samples.len(), unary.remaining_hypotheses, unary.unseen_errors
            );
        }
        for gate in &self.gates {
            println!(
                "SYNTH_LEARN1F_GATE,task={},gate={:?},queries={},unseen_errors={}",
                gate.task.name(), gate.gate, gate.samples.len(), gate.unseen_errors
            );
        }
        println!(
            "SYNTH_LEARN1F_COMPOSED,op=mul,teacher_queries={},domain=65536,errors={},class_status_only=true",
            self.teacher_queries(), self.audit_full()
        );
    }
}
fn main() {
    let model = LearnedRawMulStatus::train();
    match std::env::args().nth(1).as_deref() {
        None | Some("--report") => model.report(),
        Some("--emit-gas") => print!("{}", model.emit_x86_64_gas()),
        _ => panic!("valid options: --report, --emit-gas"),
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eight_raw_bits_are_learned_without_feature_oracle_labels() {
        let model = LearnedRawMulStatus::train();
        let expected = [
            (0xff, 0x80),
            (0xff, 0x00),
            (0x80, 0x80),
        ];
        for (learner, &(mask, value)) in model.unary.iter().zip(expected.iter()) {
            assert_eq!(learner.circuit, MaskPredicate { mask, value });
            assert_eq!(learner.samples.len(), 9);
            assert!(learner.samples.len() <= UNARY_BUDGET);
            assert_eq!(learner.remaining_hypotheses, 1);
            assert_eq!(learner.unseen_errors, 0);
        }
    }

    #[test]
    fn learned_boolean_composition_is_exact_on_full_mul_domain() {
        let model = LearnedRawMulStatus::train();
        assert_eq!(model.gates[0].gate, Gate::Or);
        assert_eq!(model.gates[1].gate, Gate::Or);
        assert_eq!(model.gates[2].gate, Gate::Xor);
        assert!(model.gates.iter().all(|g| g.unseen_errors == 0));
        assert_eq!(model.audit_full(), 0);
        assert_eq!(model.classify(0x80, 0), 1);
        assert_eq!(model.classify(0, 0), 2);
        assert_eq!(model.classify(0x81, 0x40), 4);
        assert_eq!(model.classify(0xc0, 0x81), 0);
    }

    #[test]
    fn native_program_uses_learned_masks_and_no_answer_table() {
        let asm = LearnedRawMulStatus::train().emit_x86_64_gas();
        assert!(asm.contains("    and eax, 255"));
        assert!(asm.contains("    cmp eax, 128"));
        assert!(asm.contains("    and eax, 128"));
        assert!(asm.contains("    xor eax, edx"));
        assert!(asm.contains(".note.GNU-stack"));
        assert!(!asm.contains(".rodata"));
    }
}
