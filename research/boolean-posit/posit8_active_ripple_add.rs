//! SYNTH-LEARN1I: actively synthesize a five-gate, one-bit full-adder
//! network from *selected* membership labels and unroll its learned gates
//! into a real 8-bit ripple-carry machine-code arithmetic operation.
//!
//! This is exact BYTE integer addition, a future significand primitive.
//! It is NOT posit8 ADD: no regime/exponent/fraction alignment or rounding.
#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::fmt::Write;

const INPUT_CASES: usize = 8;
const GATE_COUNT: usize = 5;
const QUERY_BUDGET: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinaryGate {
    Xor,
    And,
    Or,
    Nand,
    Nor,
    Xnor,
}
impl BinaryGate {
    const GRAMMAR: [Self; 6] = [
        Self::Xor,
        Self::And,
        Self::Or,
        Self::Nand,
        Self::Nor,
        Self::Xnor,
    ];
    fn evaluate(self, a: u8, b: u8) -> u8 {
        assert!(a <= 1 && b <= 1);
        match self {
            Self::Xor => a ^ b,
            Self::And => a & b,
            Self::Or => a | b,
            Self::Nand => (a & b) ^ 1,
            Self::Nor => (a | b) ^ 1,
            Self::Xnor => a ^ b ^ 1,
        }
    }
    fn asm(self) -> &'static str {
        match self {
            Self::Xor => "    xor eax, edx\n",
            Self::And => "    and eax, edx\n",
            Self::Or => "    or eax, edx\n",
            Self::Nand => "    and eax, edx\n    xor eax, 1\n",
            Self::Nor => "    or eax, edx\n    xor eax, 1\n",
            Self::Xnor => "    xor eax, edx\n    xor eax, 1\n",
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct FiveGateAdder {
    gates: [BinaryGate; GATE_COUNT],
}
impl FiveGateAdder {
    /// The network topology is fixed; its five gates are freely selected
    /// from the declared six-gate grammar by the active teacher queries.
    ///
    /// p=G0(a,b), g=G1(a,b), sum=G2(p,cin),
    /// t=G3(p,cin), cout=G4(g,t).
    fn evaluate(self, a: u8, b: u8, carry: u8) -> (u8, u8) {
        let [p, g, s, t, c] = self.gates;
        let propagate = p.evaluate(a, b);
        let generate = g.evaluate(a, b);
        let sum = s.evaluate(propagate, carry);
        let partial = t.evaluate(propagate, carry);
        let carry_out = c.evaluate(generate, partial);
        (sum, carry_out)
    }
    /// A signature is a complete prediction over eight *unlabeled* local
    /// Boolean inputs. No teacher output is read while computing it.
    fn predicted_signature(self) -> u16 {
        let mut result = 0u16;
        for case in 0..INPUT_CASES {
            let (a, b, cin) = decode_case(case);
            let (sum, cout) = self.evaluate(a, b, cin);
            result |= u16::from(sum | (cout << 1)) << (2 * case);
        }
        result
    }
}
fn decode_case(index: usize) -> (u8, u8, u8) {
    assert!(index < INPUT_CASES);
    (
        (index as u8) & 1,
        ((index >> 1) as u8) & 1,
        ((index >> 2) as u8) & 1,
    )
}
fn full_adder_teacher(a: u8, b: u8, carry: u8) -> (u8, u8) {
    assert!(a <= 1 && b <= 1 && carry <= 1);
    let total = u16::from(a) + u16::from(b) + u16::from(carry);
    ((total & 1) as u8, ((total >> 1) & 1) as u8)
}

#[derive(Debug)]
pub struct DiscoveryReport {
    pub membership_queries: usize,
    pub candidate_signatures: usize,
    pub remaining_signatures: usize,
    pub unseen_cases: usize,
    pub unseen_errors: usize,
    pub gates: [BinaryGate; GATE_COUNT],
}
pub struct LearnedRippleAdder {
    learned: FiveGateAdder,
    samples: Vec<(u8, u8, u8, u8, u8)>,
    pub report: DiscoveryReport,
}
impl LearnedRippleAdder {
    pub fn train() -> Self {
        // 6^5 = 7776 candidate gate networks. Multiple networks may induce
        // the same truth function. Retain one representative per signature.
        // This deduplication is based solely on *candidate predictions*.
        let mut candidates = BTreeMap::<u16, FiveGateAdder>::new();
        for id in 0..BinaryGate::GRAMMAR.len().pow(GATE_COUNT as u32) {
            let mut tmp = id;
            let gates = std::array::from_fn(|_| {
                let gate = BinaryGate::GRAMMAR[tmp % BinaryGate::GRAMMAR.len()];
                tmp /= BinaryGate::GRAMMAR.len();
                gate
            });
            let proposal = FiveGateAdder { gates };
            candidates
                .entry(proposal.predicted_signature())
                .or_insert(proposal);
        }
        let candidate_signatures = candidates.len();
        let mut remaining = candidates;
        let mut sample_cases = 0u8;
        let mut samples = Vec::new();
        while remaining.len() > 1 {
            let mut selected = None;
            for case in 0..INPUT_CASES {
                if sample_cases & (1 << case) != 0 {
                    continue;
                }
                let mut histogram = [0usize; 4];
                for &signature in remaining.keys() {
                    let output = ((signature >> (2 * case)) & 3) as usize;
                    histogram[output] += 1;
                }
                let total = remaining.len();
                let disagreement =
                    total * total - histogram.iter().map(|size| size * size).sum::<usize>();
                if selected.is_none_or(|(previous, _)| disagreement > previous) {
                    selected = Some((disagreement, case));
                }
            }
            let Some((disagreement, case)) = selected else {
                break;
            };
            if disagreement == 0 {
                break;
            }
            assert!(samples.len() < QUERY_BUDGET, "membership budget exceeded");
            let (a, b, cin) = decode_case(case);
            // The integer teacher is consulted for *this single new input*.
            let (sum, cout) = full_adder_teacher(a, b, cin);
            let label = u16::from(sum | (cout << 1));
            sample_cases |= 1 << case;
            samples.push((a, b, cin, sum, cout));
            remaining.retain(|&signature, _| ((signature >> (2 * case)) & 3) == label);
            assert!(
                !remaining.is_empty(),
                "full adder not representable in frozen grammar"
            );
        }
        let learned = *remaining.values().next().expect("candidate retained");
        // After hypothesis selection, count errors on inputs NOT queried.
        let mut unseen_cases = 0;
        let mut unseen_errors = 0;
        for case in 0..INPUT_CASES {
            if sample_cases & (1 << case) != 0 {
                continue;
            }
            unseen_cases += 1;
            let (a, b, cin) = decode_case(case);
            unseen_errors +=
                usize::from(learned.evaluate(a, b, cin) != full_adder_teacher(a, b, cin));
        }
        let report = DiscoveryReport {
            membership_queries: samples.len(),
            candidate_signatures,
            remaining_signatures: remaining.len(),
            unseen_cases,
            unseen_errors,
            gates: learned.gates,
        };
        Self {
            learned,
            samples,
            report,
        }
    }
    pub fn add_byte(&self, a: u8, b: u8, carry_in: u8) -> u16 {
        assert!(carry_in <= 1);
        let mut carry = carry_in;
        let mut out = 0u16;
        for bit in 0..8 {
            let (sum, carry_out) = self.learned.evaluate((a >> bit) & 1, (b >> bit) & 1, carry);
            out |= u16::from(sum) << bit;
            carry = carry_out;
        }
        out | (u16::from(carry) << 8)
    }
    pub fn audit_all_pairs(&self) -> usize {
        let mut errors = 0usize;
        for a in 0..=u8::MAX {
            for b in 0..=u8::MAX {
                for carry in 0..=1u8 {
                    let expected = u16::from(a) + u16::from(b) + u16::from(carry);
                    errors += usize::from(self.add_byte(a, b, carry) != expected);
                }
            }
        }
        errors
    }
    /// No machine arithmetic add/adc is emitted: all eight full-adder
    /// stages are lowered from the *learned* five binary Boolean gates.
    pub fn emit_x86_64_gas(&self) -> String {
        let symbol = "mem_posit8_learned_ripple_add_u8";
        let mut asm = format!(
            ".intel_syntax noprefix\n.text\n.globl {symbol}\n.type {symbol}, @function\n{symbol}:\n"
        );
        asm.push_str("    movzx r8d, dil\n    movzx r9d, sil\n");
        asm.push_str("    movzx r10d, dl\n    and r10d, 1\n");
        asm.push_str("    xor r11d, r11d\n");
        let [p, g, s, t, c] = self.learned.gates;
        for bit in 0..8 {
            // Propagate = learned G0(a_i,b_i).
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
            asm.push_str(p.asm());
            asm.push_str("    mov esi, eax\n");
            // Generate = learned G1(a_i,b_i).
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
            asm.push_str(g.asm());
            asm.push_str("    mov edi, eax\n");
            // Output bit = learned G2(propagate, carry).
            asm.push_str("    mov eax, esi\n    mov edx, r10d\n");
            asm.push_str(s.asm());
            writeln!(&mut asm, "    shl eax, {bit}\n    or r11d, eax").unwrap();
            // Next carry = learned G4(generate, G3(propagate,carry)).
            asm.push_str("    mov eax, esi\n    mov edx, r10d\n");
            asm.push_str(t.asm());
            asm.push_str("    mov edx, eax\n    mov eax, edi\n");
            asm.push_str(c.asm());
            asm.push_str("    mov r10d, eax\n");
        }
        asm.push_str("    mov eax, r10d\n    shl eax, 8\n    or eax, r11d\n    ret\n");
        writeln!(&mut asm, ".size {symbol}, .-{symbol}").unwrap();
        asm.push_str(".section .note.GNU-stack,\"\",@progbits\n");
        asm
    }
    pub fn report(&self) {
        println!(
            "SYNTH_LEARN1I_LOCAL,queries={},max_queries={},candidate_functions={},remaining={},unseen_cases={},unseen_errors={},gates={:?}",
            self.samples.len(), QUERY_BUDGET,
            self.report.candidate_signatures, self.report.remaining_signatures,
            self.report.unseen_cases, self.report.unseen_errors,
            self.report.gates
        );
        println!(
            "SYNTH_LEARN1I_BYTE,operation=unsigned_u8_add_with_carry,inputs=131072,teacher_queries={},mismatches={}",
            self.report.membership_queries, self.audit_all_pairs()
        );
    }
}
fn main() {
    let model = LearnedRippleAdder::train();
    match std::env::args().nth(1).as_deref() {
        None | Some("--report") => model.report(),
        Some("--emit-gas") => print!("{}", model.emit_x86_64_gas()),
        _ => panic!("valid arguments: --report, --emit-gas"),
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_queries_produce_an_exact_full_adder() {
        let model = LearnedRippleAdder::train();
        assert!(!model.samples.is_empty());
        assert!(model.samples.len() <= QUERY_BUDGET);
        assert!(model.report.candidate_signatures > 1);
        assert_eq!(model.report.unseen_errors, 0);
        for &(a, b, carry, sum, cout) in &model.samples {
            assert_eq!(model.learned.evaluate(a, b, carry), (sum, cout));
        }
        for case in 0..INPUT_CASES {
            let (a, b, carry) = decode_case(case);
            assert_eq!(
                model.learned.evaluate(a, b, carry),
                full_adder_teacher(a, b, carry)
            );
        }
    }
    #[test]
    fn learned_recurrence_reconstructs_all_byte_additions_with_carry() {
        let model = LearnedRippleAdder::train();
        assert_eq!(model.audit_all_pairs(), 0);
        assert_eq!(model.add_byte(255, 255, 1), 511);
        assert_eq!(model.add_byte(255, 1, 0), 256);
        assert_eq!(model.add_byte(0, 0, 0), 0);
        assert_eq!(model.add_byte(127, 128, 1), 256);
    }
    #[test]
    fn native_emission_has_no_builtin_add_or_lookup_table() {
        let model = LearnedRippleAdder::train();
        let asm = model.emit_x86_64_gas();
        assert!(asm.contains(".intel_syntax noprefix"));
        assert!(asm.contains("mem_posit8_learned_ripple_add_u8:"));
        assert!(asm.contains("    or eax, edx") || asm.contains("    xor eax, edx"));
        assert!(asm.contains("    shl eax, 8"));
        assert!(!asm.contains("    add "));
        assert!(!asm.contains("    adc "));
        assert!(!asm.contains(".rodata"));
    }
}
