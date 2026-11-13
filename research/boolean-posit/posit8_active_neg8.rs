//! SYNTH-LEARN1G: actively learn an 8-bit recurrent Boolean arithmetic
//! circuit for complete posit8 negation. A separate teacher supplies only
//! selected bit labels for multiplication by negative one; the resulting
//! program is real byte arithmetic, not a four-class status classifier.
#![forbid(unsafe_code)]
#[path = "posit8_circuit_synth.rs"]
#[allow(dead_code)]
mod oracle;
#[path = "posit8_active_sign_gates.rs"]
#[allow(dead_code)]
mod gate_library;

use gate_library::Gate;
use oracle::Posit8;
use std::collections::BTreeSet;
use std::fmt::Write;

const GRAMMAR: [Gate; 9] = [
    Gate::Zero,
    Gate::One,
    Gate::SignA,
    Gate::SignB,
    Gate::NotA,
    Gate::NotB,
    Gate::And,
    Gate::Or,
    Gate::Xor,
];
const PER_STAGE_BUDGET: usize = 8;

#[derive(Clone, Copy, Debug)]
enum Stage {
    Prefix,
    Output,
}
impl Stage {
    fn name(self) -> &'static str {
        match self {
            Self::Prefix => "prefix",
            Self::Output => "negated_bit",
        }
    }
    /// Input to a recurrent stage: raw bit and whether at least one lower
    /// bit has been seen. This feature is supplied during *training*;
    /// *inference* must compute it from previously discovered prefix gates.
    fn features(raw: u8, bit: u8) -> (u8, u8) {
        let current = (raw >> bit) & 1;
        let lower_mask = (1u16 << bit) - 1;
        let previous = u8::from(u16::from(raw) & lower_mask != 0);
        (current, previous)
    }
    fn teacher(self, raw: u8, bit: u8) -> u8 {
        assert!(bit < 8);
        match self {
            Self::Prefix => {
                let lower_mask = (1u16 << (bit + 1)) - 1;
                u8::from(u16::from(raw) & lower_mask != 0)
            }
            Self::Output => {
                // Arithmetic oracle is only queried on the selected labelled
                // examples and in the final read-only exhaustive audit.
                let negated = Posit8(raw).mul(Posit8(0xc0)).0;
                (negated >> bit) & 1
            }
        }
    }
}
fn eval_gate(gate: Gate, x: u8, y: u8) -> u8 {
    assert!(x <= 1 && y <= 1);
    gate.evaluate(x << 7, y << 7)
}

#[derive(Debug)]
struct LearnedStage {
    stage: Stage,
    gate: Gate,
    queries: Vec<(u8, u8, u8)>,
    remaining_candidates: usize,
    unseen_errors: usize,
}
fn discover(stage: Stage) -> LearnedStage {
    let mut hypotheses = GRAMMAR.to_vec();
    let mut queried = BTreeSet::new();
    let mut queries = Vec::new();
    loop {
        let mut best = (0usize, None);
        for raw in 0..=u8::MAX {
            for bit in 0..8u8 {
                if queried.contains(&(raw, bit)) {
                    continue;
                }
                let (x, y) = Stage::features(raw, bit);
                let ones = hypotheses
                    .iter()
                    .filter(|&&candidate| eval_gate(candidate, x, y) != 0)
                    .count();
                let score = ones * (hypotheses.len() - ones);
                if score > best.0 {
                    best = (score, Some((raw, bit)));
                }
            }
        }
        let Some((raw, bit)) = best.1 else {
            break;
        };
        assert!(queries.len() < PER_STAGE_BUDGET, "bit-gate query budget exhausted");
        let label = stage.teacher(raw, bit);
        assert!(queried.insert((raw, bit)));
        queries.push((raw, bit, label));
        let (x, y) = Stage::features(raw, bit);
        hypotheses.retain(|&gate| eval_gate(gate, x, y) == label);
        assert!(!hypotheses.is_empty(), "Boolean gate grammar does not fit bit teacher");
    }
    let gate = hypotheses[0];
    let mut unseen_errors = 0usize;
    for raw in 0..=u8::MAX {
        for bit in 0..8u8 {
            if queried.contains(&(raw, bit)) {
                continue;
            }
            let (x, y) = Stage::features(raw, bit);
            unseen_errors += usize::from(eval_gate(gate, x, y) != stage.teacher(raw, bit));
        }
    }
    LearnedStage {
        stage,
        gate,
        queries,
        remaining_candidates: hypotheses.len(),
        unseen_errors,
    }
}

pub struct LearnedNegation {
    prefix: LearnedStage,
    output: LearnedStage,
}
impl LearnedNegation {
    pub fn train() -> Self {
        Self {
            prefix: discover(Stage::Prefix),
            output: discover(Stage::Output),
        }
    }
    pub fn query_count(&self) -> usize {
        self.prefix.queries.len() + self.output.queries.len()
    }
    /// Full 8-bit arithmetic transformation, not an answer lookup table.
    pub fn negate(&self, raw: u8) -> u8 {
        let mut prefix = 0u8;
        let mut result = 0u8;
        for bit in 0..8 {
            let input = (raw >> bit) & 1;
            result |= eval_gate(self.output.gate, input, prefix) << bit;
            prefix = eval_gate(self.prefix.gate, input, prefix);
        }
        result
    }
    /// Exposes a reusable finite-magnitude primitive. NaR is not a number,
    /// and 0x80 is only a sentinel, so its "magnitude" is not meaningful.
    pub fn abs_encoding(&self, raw: u8) -> Option<u8> {
        if raw == 0x80 {
            None
        } else if raw & 0x80 != 0 {
            Some(self.negate(raw))
        } else {
            Some(raw)
        }
    }
    pub fn audit_all_negations(&self) -> usize {
        (0..=u8::MAX)
            .filter(|&raw| self.negate(raw) != Posit8(raw).mul(Posit8(0xc0)).0)
            .count()
    }
    pub fn audit_all_magnitudes(&self) -> usize {
        (0..=u8::MAX)
            .filter(|&raw| {
                let expected = if raw == 0x80 {
                    None
                } else if raw & 0x80 == 0 {
                    Some(raw)
                } else {
                    Some(Posit8(raw).mul(Posit8(0xc0)).0)
                };
                self.abs_encoding(raw) != expected
            })
            .count()
    }
    fn asm_gate(gate: Gate) -> &'static str {
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
    /// Two genuine x86-64 ABI functions in one generated GAS source: exact
    /// negation and a finite-input magnitude wrapper. The Boolean recurrence
    /// is unrolled using the gates selected by teacher membership queries.
    pub fn emit_gas(&self) -> String {
        let symbol = "mem_posit8_active_neg8";
        let absolute = "mem_posit8_active_abs8";
        let mut asm = format!(
            ".intel_syntax noprefix\n.text\n.globl {symbol}\n.type {symbol}, @function\n{symbol}:\n"
        );
        asm.push_str("    movzx r8d, dil\n    xor ecx, ecx\n    xor r11d, r11d\n");
        for bit in 0..8 {
            writeln!(&mut asm, "    mov eax, r8d\n    shr eax, {bit}\n    and eax, 1").unwrap();
            asm.push_str("    mov edx, ecx\n");
            asm.push_str(Self::asm_gate(self.output.gate));
            writeln!(&mut asm, "    shl eax, {bit}\n    or r11d, eax").unwrap();
            writeln!(&mut asm, "    mov eax, r8d\n    shr eax, {bit}\n    and eax, 1").unwrap();
            asm.push_str("    mov edx, ecx\n");
            asm.push_str(Self::asm_gate(self.prefix.gate));
            asm.push_str("    mov ecx, eax\n");
        }
        asm.push_str("    mov eax, r11d\n    ret\n");
        writeln!(&mut asm, ".size {symbol}, .-{symbol}").unwrap();
        writeln!(
            &mut asm,
            ".globl {absolute}\n.type {absolute}, @function\n{absolute}:"
        )
        .unwrap();
        // Sentinel treatment is a wrapper contract, NOT a learned gate.
        asm.push_str("    cmp dil, 128\n    je .Labs_invalid\n");
        asm.push_str("    test dil, 128\n    jnz mem_posit8_active_neg8\n");
        asm.push_str("    movzx eax, dil\n    ret\n");
        asm.push_str(".Labs_invalid:\n    mov eax, 128\n    ret\n");
        writeln!(&mut asm, ".size {absolute}, .-{absolute}").unwrap();
        asm.push_str(".section .note.GNU-stack,\"\",@progbits\n");
        asm
    }
    fn report(&self) {
        for stage in [&self.prefix, &self.output] {
            println!(
                "SYNTH_LEARN1G_GATE,stage={},gate={:?},teacher_queries={},remaining={},unseen_errors={}",
                stage.stage.name(),
                stage.gate,
                stage.queries.len(),
                stage.remaining_candidates,
                stage.unseen_errors
            );
        }
        println!(
            "SYNTH_LEARN1G_BYTE,operation=neg8,teacher_queries={},domain=256,neg_errors={},abs_errors={}",
            self.query_count(),
            self.audit_all_negations(),
            self.audit_all_magnitudes()
        );
    }
}
fn main() {
    let result = LearnedNegation::train();
    match std::env::args().nth(1).as_deref() {
        None | Some("--report") => result.report(),
        Some("--emit-gas") => print!("{}", result.emit_gas()),
        _ => panic!("valid flags: --report or --emit-gas"),
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actively_learns_prefix_or_and_negation_xor() {
        let model = LearnedNegation::train();
        assert_eq!(model.prefix.gate, Gate::Or);
        assert_eq!(model.output.gate, Gate::Xor);
        assert_eq!(model.prefix.unseen_errors, 0);
        assert_eq!(model.output.unseen_errors, 0);
        assert!(model.prefix.queries.len() <= PER_STAGE_BUDGET);
        assert!(model.output.queries.len() <= PER_STAGE_BUDGET);
        assert!(model.prefix.remaining_candidates >= 1);
        assert!(model.output.remaining_candidates >= 1);
        assert!(model.query_count() > 0);
    }

    #[test]
    fn learned_arithmetic_negation_exact_for_every_posit8_byte() {
        let model = LearnedNegation::train();
        assert_eq!(model.audit_all_negations(), 0);
        assert_eq!(model.negate(0x00), 0x00);
        assert_eq!(model.negate(0x40), 0xc0);
        assert_eq!(model.negate(0xc0), 0x40);
        assert_eq!(model.negate(0x80), 0x80);
        assert_eq!(model.audit_all_magnitudes(), 0);
        assert_eq!(model.abs_encoding(0x80), None);
        assert_eq!(model.abs_encoding(0xc0), Some(0x40));
    }

    #[test]
    fn generated_machine_program_is_unrolled_boolean_recurrence() {
        let asm = LearnedNegation::train().emit_gas();
        assert!(asm.contains("mem_posit8_active_neg8:"));
        assert!(asm.contains("mem_posit8_active_abs8:"));
        assert!(asm.contains("    xor eax, edx"));
        assert!(asm.contains("    or eax, edx"));
        assert!(asm.contains("    shl eax, 7"));
        assert!(asm.contains(".note.GNU-stack"));
        assert!(!asm.contains(".rodata"));
    }
}
