//! SYNTH-LEARN1H: actively choose the Boolean gates of an 8-stage byte
//! comparator; compose it with SYNTH-LEARN1G learnt unary posit magnitude.
//! The five recurrent stages have a supplied state topology, not a freely
//! inferred program structure. An actual x86-64 ABI function is emitted.
#![forbid(unsafe_code)]
#[path = "posit8_circuit_synth.rs"]
#[allow(dead_code)]
mod arithmetic;
#[path = "posit8_active_sign_gates.rs"]
#[allow(dead_code)]
mod gate_library;
#[path = "posit8_active_neg8.rs"]
#[allow(dead_code)]
mod negation;

use arithmetic::Posit8;
use gate_library::Gate;
use std::collections::BTreeSet;
use std::fmt::Write;

const STAGE_BUDGET: usize = 4;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Candidate {
    Existing(Gate),
    AAndNotB,
    Xnor,
}
impl Candidate {
    fn grammar() -> [Self; 11] {
        [
            Self::Existing(Gate::Zero),
            Self::Existing(Gate::One),
            Self::Existing(Gate::SignA),
            Self::Existing(Gate::SignB),
            Self::Existing(Gate::NotA),
            Self::Existing(Gate::NotB),
            Self::Existing(Gate::And),
            Self::Existing(Gate::Or),
            Self::Existing(Gate::Xor),
            Self::AAndNotB,
            Self::Xnor,
        ]
    }
    fn evaluate(self, x: u8, y: u8) -> u8 {
        assert!(x <= 1 && y <= 1);
        match self {
            Self::Existing(gate) => gate.evaluate(x << 7, y << 7),
            Self::AAndNotB => x & (y ^ 1),
            Self::Xnor => x ^ y ^ 1,
        }
    }
    fn native_asm(self) -> &'static str {
        match self {
            Self::Existing(Gate::Zero) => "    xor eax, eax\n",
            Self::Existing(Gate::One) => "    mov eax, 1\n",
            Self::Existing(Gate::SignA) => "",
            Self::Existing(Gate::SignB) => "    mov eax, edx\n",
            Self::Existing(Gate::NotA) => "    xor eax, 1\n",
            Self::Existing(Gate::NotB) => "    mov eax, edx\n    xor eax, 1\n",
            Self::Existing(Gate::And) => "    and eax, edx\n",
            Self::Existing(Gate::Or) => "    or eax, edx\n",
            Self::Existing(Gate::Xor) => "    xor eax, edx\n",
            Self::AAndNotB => "    not edx\n    and eax, edx\n",
            Self::Xnor => "    xor eax, edx\n    xor eax, 1\n",
        }
    }
}
#[derive(Clone, Copy, Debug)]
enum Stage {
    BitGreater,
    BitEqual,
    GreaterQualified,
    GreaterMerge,
    EqualMerge,
}
impl Stage {
    fn name(self) -> &'static str {
        match self {
            Self::BitGreater => "bit_greater",
            Self::BitEqual => "bit_equal",
            Self::GreaterQualified => "greater_qualified",
            Self::GreaterMerge => "greater_merge",
            Self::EqualMerge => "equal_merge",
        }
    }
    /// All candidate inputs are Boolean. Stage targets are **local comparator
    /// transition semantics**, not the entire 8-bit ordering function.
    fn teacher(self, a: u8, b: u8) -> u8 {
        match self {
            Self::BitGreater => u8::from(a > b),
            Self::BitEqual => u8::from(a == b),
            Self::GreaterQualified => a & b,
            Self::GreaterMerge => a | b,
            Self::EqualMerge => a & b,
        }
    }
}
#[derive(Debug)]
struct LearnedGate {
    stage: Stage,
    gate: Candidate,
    queries: Vec<(u8, u8, u8)>,
    unseen_errors: usize,
}
fn discover(stage: Stage) -> LearnedGate {
    let mut options = Candidate::grammar().to_vec();
    let mut queried = BTreeSet::new();
    let mut queries = Vec::new();
    loop {
        let mut best = (0usize, None);
        for a in 0..=1u8 {
            for b in 0..=1u8 {
                if queried.contains(&(a, b)) {
                    continue;
                }
                let ones = options
                    .iter()
                    .filter(|&&candidate| candidate.evaluate(a, b) == 1)
                    .count();
                let disagreement = ones * (options.len() - ones);
                if disagreement > best.0 {
                    best = (disagreement, Some((a, b)));
                }
            }
        }
        let Some((a, b)) = best.1 else {
            break;
        };
        assert!(queries.len() < STAGE_BUDGET);
        let label = stage.teacher(a, b);
        assert!(queried.insert((a, b)));
        queries.push((a, b, label));
        options.retain(|&gate| gate.evaluate(a, b) == label);
        assert!(
            !options.is_empty(),
            "fixed comparator grammar does not cover teacher"
        );
    }
    let gate = options[0];
    let mut unseen_errors = 0usize;
    for a in 0..=1u8 {
        for b in 0..=1u8 {
            if !queried.contains(&(a, b)) {
                unseen_errors += usize::from(gate.evaluate(a, b) != stage.teacher(a, b));
            }
        }
    }
    LearnedGate {
        stage,
        gate,
        queries,
        unseen_errors,
    }
}

pub struct LearnedMagnitudeComparator {
    gates: [LearnedGate; 5],
    magnitude: negation::LearnedNegation,
}
impl LearnedMagnitudeComparator {
    pub fn train() -> Self {
        Self {
            gates: [
                discover(Stage::BitGreater),
                discover(Stage::BitEqual),
                discover(Stage::GreaterQualified),
                discover(Stage::GreaterMerge),
                discover(Stage::EqualMerge),
            ],
            magnitude: negation::LearnedNegation::train(),
        }
    }
    pub fn teacher_queries(&self) -> usize {
        self.gates
            .iter()
            .map(|gate| gate.queries.len())
            .sum::<usize>()
            + self.magnitude.query_count()
    }
    /// 0 if a<b, 1 if a>b, 2 if a==b. A total order on raw bytes.
    pub fn compare_unsigned(&self, a: u8, b: u8) -> u8 {
        let mut greater = 0u8;
        let mut equal = 1u8;
        for bit in (0..8).rev() {
            let bit_a = (a >> bit) & 1;
            let bit_b = (b >> bit) & 1;
            let local_greater = self.gates[0].gate.evaluate(bit_a, bit_b);
            let local_equal = self.gates[1].gate.evaluate(bit_a, bit_b);
            let qualified = self.gates[2].gate.evaluate(equal, local_greater);
            greater = self.gates[3].gate.evaluate(greater, qualified);
            equal = self.gates[4].gate.evaluate(equal, local_equal);
        }
        greater | (equal << 1)
    }
    /// Finite posit magnitude comparison. 3 signals NaR on either input.
    pub fn compare_posit_magnitudes(&self, a: u8, b: u8) -> u8 {
        match (
            self.magnitude.abs_encoding(a),
            self.magnitude.abs_encoding(b),
        ) {
            (Some(first), Some(second)) => self.compare_unsigned(first, second),
            _ => 3,
        }
    }
    pub fn oracle_posit_magnitude(a: u8, b: u8) -> u8 {
        match (Posit8(a).decode_units(), Posit8(b).decode_units()) {
            (Some(first), Some(second)) => {
                let first = first.abs();
                let second = second.abs();
                if first > second {
                    1
                } else if first == second {
                    2
                } else {
                    0
                }
            }
            _ => 3,
        }
    }
    pub fn audit_all_byte_pairs(&self) -> usize {
        let mut errors = 0usize;
        for a in 0..=u8::MAX {
            for b in 0..=u8::MAX {
                let expected = if a > b {
                    1
                } else if a == b {
                    2
                } else {
                    0
                };
                errors += usize::from(self.compare_unsigned(a, b) != expected);
            }
        }
        errors
    }
    pub fn audit_all_posit_pairs(&self) -> usize {
        let mut errors = 0usize;
        for a in 0..=u8::MAX {
            for b in 0..=u8::MAX {
                errors += usize::from(
                    self.compare_posit_magnitudes(a, b) != Self::oracle_posit_magnitude(a, b),
                );
            }
        }
        errors
    }
    /// Two real native SysV functions. The magnitude wrapper *calls* the
    /// previously compiled SYNTH-LEARN1G unary magnitude function.
    pub fn emit_gas(&self) -> String {
        let byte_symbol = "mem_posit8_learned_byte_cmp";
        let magnitude_symbol = "mem_posit8_learned_posit_mag_cmp";
        let mut asm = format!(
            ".intel_syntax noprefix\n.text\n.globl {byte_symbol}\n.type {byte_symbol}, @function\n{byte_symbol}:\n"
        );
        asm.push_str("    movzx r8d, dil\n    movzx r9d, sil\n");
        asm.push_str("    xor r10d, r10d\n    mov r11d, 1\n");
        for bit in (0..8).rev() {
            // Greater-of-bits flag, followed by the learned prefix-qualified
            // current-greater OR previous-greater recurrence.
            writeln!(
                &mut asm,
                "    mov eax, r8d\n    shr eax, {bit}\n    and eax, 1"
            )
            .unwrap();
            writeln!(
                &mut asm,
                "    mov edx, r9d\n    shr edx, {bit}\n    and edx, 1"
            )
            .unwrap();
            asm.push_str(self.gates[0].gate.native_asm());
            asm.push_str("    mov edx, eax\n    mov eax, r11d\n");
            asm.push_str(self.gates[2].gate.native_asm());
            asm.push_str("    mov edx, eax\n    mov eax, r10d\n");
            asm.push_str(self.gates[3].gate.native_asm());
            asm.push_str("    mov r10d, eax\n");
            // Equality-of-bits followed by the learned prefix AND.
            writeln!(
                &mut asm,
                "    mov eax, r8d\n    shr eax, {bit}\n    and eax, 1"
            )
            .unwrap();
            writeln!(
                &mut asm,
                "    mov edx, r9d\n    shr edx, {bit}\n    and edx, 1"
            )
            .unwrap();
            asm.push_str(self.gates[1].gate.native_asm());
            asm.push_str("    mov edx, eax\n    mov eax, r11d\n");
            asm.push_str(self.gates[4].gate.native_asm());
            asm.push_str("    mov r11d, eax\n");
        }
        asm.push_str("    mov eax, r11d\n    shl eax, 1\n");
        asm.push_str("    or eax, r10d\n    ret\n");
        writeln!(&mut asm, ".size {byte_symbol}, .-{byte_symbol}").unwrap();
        writeln!(
            &mut asm,
            ".globl {magnitude_symbol}\n.type {magnitude_symbol}, @function\n{magnitude_symbol}:"
        )
        .unwrap();
        asm.push_str("    cmp dil, 128\n    je .Lmag_invalid\n");
        asm.push_str("    cmp sil, 128\n    je .Lmag_invalid\n");
        // At SysV entry rsp%16=8. Reserve 24 bytes so two calls are aligned.
        asm.push_str("    sub rsp, 24\n    mov [rsp], sil\n");
        asm.push_str("    call mem_posit8_active_abs8\n");
        asm.push_str("    mov [rsp+1], al\n");
        asm.push_str("    mov dil, [rsp]\n");
        asm.push_str("    call mem_posit8_active_abs8\n");
        asm.push_str("    mov sil, al\n    mov dil, [rsp+1]\n");
        asm.push_str("    add rsp, 24\n");
        asm.push_str("    jmp mem_posit8_learned_byte_cmp\n");
        asm.push_str(".Lmag_invalid:\n    mov eax, 3\n    ret\n");
        writeln!(&mut asm, ".size {magnitude_symbol}, .-{magnitude_symbol}").unwrap();
        asm.push_str(".section .note.GNU-stack,\"\",@progbits\n");
        asm
    }
    fn report(&self) {
        for gate in &self.gates {
            println!(
                "SYNTH_LEARN1H_GATE,stage={},gate={:?},teacher_queries={},unseen_errors={}",
                gate.stage.name(),
                gate.gate,
                gate.queries.len(),
                gate.unseen_errors
            );
        }
        println!(
            "SYNTH_LEARN1H_COMPOSED,unary_queries={},total_queries={},raw_pairs=65536,raw_errors={},posit_pairs=65536,posit_errors={}",
            self.magnitude.query_count(),
            self.teacher_queries(),
            self.audit_all_byte_pairs(),
            self.audit_all_posit_pairs()
        );
    }
}
fn main() {
    let model = LearnedMagnitudeComparator::train();
    match std::env::args().nth(1).as_deref() {
        None | Some("--report") => model.report(),
        Some("--emit-gas") => print!("{}", model.emit_gas()),
        _ => panic!("valid arguments: --report or --emit-gas"),
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn five_comparison_gates_are_actively_selected() {
        let model = LearnedMagnitudeComparator::train();
        assert_eq!(model.gates[0].gate, Candidate::AAndNotB);
        assert_eq!(model.gates[1].gate, Candidate::Xnor);
        assert_eq!(model.gates[2].gate, Candidate::Existing(Gate::And));
        assert_eq!(model.gates[3].gate, Candidate::Existing(Gate::Or));
        assert_eq!(model.gates[4].gate, Candidate::Existing(Gate::And));
        for gate in &model.gates {
            assert!(gate.queries.len() <= STAGE_BUDGET);
            assert!(!gate.queries.is_empty());
            assert_eq!(gate.unseen_errors, 0);
        }
    }

    #[test]
    fn learned_comparison_exact_on_every_posit8_pair() {
        let model = LearnedMagnitudeComparator::train();
        assert_eq!(model.audit_all_byte_pairs(), 0);
        assert_eq!(model.audit_all_posit_pairs(), 0);
        assert_eq!(model.compare_unsigned(1, 0), 1);
        assert_eq!(model.compare_unsigned(0, 1), 0);
        assert_eq!(model.compare_unsigned(127, 127), 2);
        assert_eq!(model.compare_posit_magnitudes(0x80, 0x40), 3);
        assert_eq!(model.compare_posit_magnitudes(0xc0, 0x40), 2);
    }

    #[test]
    fn emitted_code_uses_learned_gates_and_native_magnitude_function() {
        let asm = LearnedMagnitudeComparator::train().emit_gas();
        assert!(asm.contains("mem_posit8_learned_byte_cmp:"));
        assert!(asm.contains("mem_posit8_learned_posit_mag_cmp:"));
        assert!(asm.contains("    call mem_posit8_active_abs8"));
        assert!(asm.contains("    not edx"));
        assert!(asm.contains("    xor eax, 1"));
        assert!(asm.contains(".note.GNU-stack"));
        assert!(!asm.contains(".rodata"));
    }
}
