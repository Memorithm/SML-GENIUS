//! SysV x86-64 GNU assembler lowering for a verified Boolean posit8 ROBDD.
//! Rust owns synthesis and serialization; this module emits textual assembly.
//! No dynamic execution/JIT, writable executable memory, vendor SDK or LUT.
use super::LearnedCircuit;
use std::fmt::Write;

const MAX_LOCAL_BYTES: usize = 16_384;

impl LearnedCircuit {
    /// Generate a stand-alone assembly function, using x86-64 SysV ABI:
    /// u8 fn(u8 a: dil, u8 b: sil) -> al.
    ///
    /// Every ROBDD decision is a branch-free Boolean MUX. The scratch region
    /// is bounded, built entirely from previously computed nodes and never
    /// indexed by untrusted runtime data. The input bits are normalized to 0/1.
    ///
    /// This is a baseline ISA lowering, not evidence of faster inference.
    pub fn emit_sysv_x86_64(&self) -> Result<String, &'static str> {
        let scratch = self
            .nodes
            .len()
            .checked_add(2)
            .ok_or("node count overflow")?;
        let stack_bytes = scratch.checked_add(15).ok_or("stack size overflow")? & !15;
        if stack_bytes == 0 || stack_bytes > MAX_LOCAL_BYTES {
            return Err("stack scratch budget exceeded");
        }
        for (index, node) in self.nodes.iter().enumerate() {
            if node.variable >= 16 || node.low >= index + 2 || node.high >= index + 2 {
                return Err("invalid or cyclic BDD node");
            }
        }
        if self.output_roots.iter().any(|&root| root >= scratch) {
            return Err("invalid output root");
        }

        let name = format!("mem_posit8_asm_{}", self.operation.name());
        let mut asm = String::new();
        writeln!(asm, ".intel_syntax noprefix").unwrap();
        writeln!(asm, ".text").unwrap();
        writeln!(asm, ".p2align 4").unwrap();
        writeln!(asm, ".globl {name}").unwrap();
        writeln!(asm, ".type {name}, @function").unwrap();
        writeln!(asm, "{name}:").unwrap();
        writeln!(asm, "    sub rsp, {stack_bytes}").unwrap();
        writeln!(asm, "    mov BYTE PTR [rsp + 0], 0").unwrap();
        writeln!(asm, "    mov BYTE PTR [rsp + 1], 1").unwrap();

        for (index, node) in self.nodes.iter().enumerate() {
            let id = index + 2;
            let bit = node.variable / 2;
            let input_register = if node.variable & 1 == 0 { "edi" } else { "esi" };
            // eax = low, edx = high XOR low, ecx = the selected input bit.
            // Result: low XOR ((high XOR low) AND selected_bit).
            writeln!(asm, "    movzx eax, BYTE PTR [rsp + {}]", node.low).unwrap();
            writeln!(asm, "    movzx edx, BYTE PTR [rsp + {}]", node.high).unwrap();
            writeln!(asm, "    xor edx, eax").unwrap();
            writeln!(asm, "    mov ecx, {input_register}").unwrap();
            writeln!(asm, "    shr ecx, {bit}").unwrap();
            writeln!(asm, "    and ecx, 1").unwrap();
            writeln!(asm, "    and edx, ecx").unwrap();
            writeln!(asm, "    xor eax, edx").unwrap();
            writeln!(asm, "    mov BYTE PTR [rsp + {id}], al").unwrap();
        }

        writeln!(asm, "    xor r8d, r8d").unwrap();
        for (bit, &root) in self.output_roots.iter().enumerate() {
            writeln!(asm, "    movzx eax, BYTE PTR [rsp + {root}]").unwrap();
            writeln!(asm, "    shl eax, {bit}").unwrap();
            writeln!(asm, "    or r8d, eax").unwrap();
        }
        writeln!(asm, "    mov eax, r8d").unwrap();
        writeln!(asm, "    add rsp, {stack_bytes}").unwrap();
        writeln!(asm, "    ret").unwrap();
        writeln!(asm, ".size {name}, .-{name}").unwrap();
        writeln!(asm, ".section .note.GNU-stack,\"\",@progbits").unwrap();
        Ok(asm)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Operation;

    #[test]
    fn emitted_programs_are_bounded_and_statically_addressed() {
        for operation in [Operation::Add, Operation::Mul] {
            let circuit = LearnedCircuit::synthesize(operation).unwrap();
            let asm = circuit.emit_sysv_x86_64().unwrap();
            assert!(asm.contains(".section .note.GNU-stack"));
            assert!(asm.contains("xor edx, eax"));
            assert!(!asm.contains("call "));
            assert!(!asm.contains("jmp "));
            assert_eq!(asm.matches("    xor edx, eax").count(), circuit.nodes());
            assert_eq!(
                asm.matches("    mov BYTE PTR [rsp + ").count(),
                circuit.nodes() + 2
            );
        }
    }
}
