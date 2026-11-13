//! SYNTH-LEARN1J: build an entire 8x8->16-bit unsigned multiplication
//! from the ONE-BIT ADDER GATES discovered by SYNTH-LEARN1I.
//!
//! Shift-and-add topology is explicitly prescribed; no posit8 magnitude,
//! exponent or rounding is modeled. This is a reusable significand primitive.
#![forbid(unsafe_code)]
#[path = "posit8_active_ripple_add.rs"]
#[allow(dead_code)]
mod learned_adder;

use learned_adder::LearnedRippleAdder;
use std::fmt::Write;

pub struct LearnedByteMultiplier {
    adder: LearnedRippleAdder,
}
impl LearnedByteMultiplier {
    pub fn train() -> Self {
        Self {
            adder: LearnedRippleAdder::train(),
        }
    }
    pub fn membership_queries(&self) -> usize {
        self.adder.report.membership_queries
    }
    /// Compose the same eight-stage learned ripple adder twice for a
    /// 16-bit sum; never use host integer ADD for production arithmetic.
    fn add_words(&self, lhs: u16, rhs: u16) -> u16 {
        let lower = self.adder.add_byte(lhs as u8, rhs as u8, 0);
        let upper = self.adder.add_byte(
            (lhs >> 8) as u8,
            (rhs >> 8) as u8,
            (lower >> 8) as u8,
        );
        (lower & 0xff) | ((upper & 0xff) << 8)
    }
    pub fn multiply(&self, a: u8, b: u8) -> u16 {
        let mut accumulator = 0u16;
        for shift in 0..8 {
            let partial = if (b >> shift) & 1 != 0 {
                u16::from(a) << shift
            } else {
                0
            };
            accumulator = self.add_words(accumulator, partial);
        }
        accumulator
    }
    pub fn audit_all_pairs(&self) -> usize {
        let mut mismatches = 0usize;
        for a in 0..=u8::MAX {
            for b in 0..=u8::MAX {
                let expected = u16::from(a) * u16::from(b);
                mismatches += usize::from(self.multiply(a, b) != expected);
            }
        }
        mismatches
    }
    /// Generate a genuine x86-64 SysV multiplication symbol that performs
    /// eight conditional partial products and sixteen linked learned-byte
    /// additions per multiplication. No MUL/IMUL or arithmetic ADD/ADC is
    /// used in the data path; only the already-generated Boolean adder.
    pub fn emit_gas(&self) -> String {
        let symbol = "mem_posit8_learned_u8_mul_u16";
        let mut asm = format!(
            ".intel_syntax noprefix\n.text\n.globl {symbol}\n.type {symbol}, @function\n{symbol}:\n"
        );
        // A System V function enters with rsp % 16 == 8. The 56-byte local
        // frame makes rsp % 16 == 0 across both child function calls.
        // The child is free to overwrite all caller-saved registers.
        asm.push_str("    sub rsp, 56\n");
        asm.push_str("    mov byte ptr [rsp], dil\n    mov byte ptr [rsp+1], sil\n");
        asm.push_str("    mov byte ptr [rsp+2], 0\n    mov byte ptr [rsp+3], 0\n");
        for shift in 0..8 {
            // partial = (a << shift) & -bit(b,shift), stored little endian.
            writeln!(&mut asm, "    movzx eax, byte ptr [rsp]\n    shl eax, {shift}").unwrap();
            writeln!(&mut asm, "    movzx ecx, byte ptr [rsp+1]\n    shr ecx, {shift}").unwrap();
            asm.push_str("    and ecx, 1\n    neg ecx\n    and eax, ecx\n");
            asm.push_str("    mov word ptr [rsp+4], ax\n");
            // Accumulate the 8 low bits and retain the carry-out byte.
            asm.push_str("    mov dil, byte ptr [rsp+2]\n");
            asm.push_str("    mov sil, byte ptr [rsp+4]\n");
            asm.push_str("    xor edx, edx\n    call mem_posit8_learned_ripple_add_u8\n");
            asm.push_str("    mov byte ptr [rsp+2], al\n");
            asm.push_str("    shr eax, 8\n    mov byte ptr [rsp+6], al\n");
            // Accumulate upper 8 bits with carry propagated from low sum.
            asm.push_str("    mov dil, byte ptr [rsp+3]\n");
            asm.push_str("    mov sil, byte ptr [rsp+5]\n");
            asm.push_str("    movzx edx, byte ptr [rsp+6]\n");
            asm.push_str("    call mem_posit8_learned_ripple_add_u8\n");
            asm.push_str("    mov byte ptr [rsp+3], al\n");
        }
        asm.push_str("    movzx eax, byte ptr [rsp+2]\n");
        asm.push_str("    movzx edx, byte ptr [rsp+3]\n    shl edx, 8\n");
        asm.push_str("    or eax, edx\n    lea rsp, [rsp+56]\n    ret\n");
        writeln!(&mut asm, ".size {symbol}, .-{symbol}").unwrap();
        asm.push_str(".section .note.GNU-stack,\"\",@progbits\n");
        asm
    }
    pub fn report(&self) {
        println!(
            "SYNTH_LEARN1J_MULTIPLY,operation=unsigned_u8_mul_u16,teacher_queries={},partial_products=8,learned_add8_calls_per_product=16,input_pairs=65536,errors={}",
            self.membership_queries(),
            self.audit_all_pairs()
        );
    }
}
fn main() {
    let model = LearnedByteMultiplier::train();
    match std::env::args().nth(1).as_deref() {
        None | Some("--report") => model.report(),
        Some("--emit-gas") => print!("{}", model.emit_gas()),
        _ => panic!("valid options: --report or --emit-gas"),
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn learned_adder_composes_into_exact_16_bit_accumulation() {
        let m = LearnedByteMultiplier::train();
        assert!(m.membership_queries() > 0);
        assert!(m.membership_queries() <= 8);
        assert_eq!(m.add_words(0xffff, 1), 0);
        assert_eq!(m.add_words(0xff, 0x100), 0x1ff);
        assert_eq!(m.add_words(0x7fff, 0x8000), 0xffff);
    }
    #[test]
    fn learned_shift_and_add_exact_for_every_byte_product() {
        let m = LearnedByteMultiplier::train();
        assert_eq!(m.audit_all_pairs(), 0);
        assert_eq!(m.multiply(255, 255), 65025);
        assert_eq!(m.multiply(128, 128), 16384);
        assert_eq!(m.multiply(0, 255), 0);
        assert_eq!(m.multiply(1, 255), 255);
    }
    #[test]
    fn native_emitter_links_real_learned_adder_no_multiply_lookup() {
        let asm = LearnedByteMultiplier::train().emit_gas();
        assert!(asm.contains("mem_posit8_learned_u8_mul_u16:"));
        assert_eq!(asm.matches("    call mem_posit8_learned_ripple_add_u8").count(), 16);
        assert!(asm.contains("    mov word ptr [rsp+4], ax"));
        assert!(asm.contains("    lea rsp, [rsp+56]"));
        assert!(!asm.contains("    imul "));
        assert!(!asm.contains("    mul "));
        assert!(!asm.contains(".rodata"));
    }
}
