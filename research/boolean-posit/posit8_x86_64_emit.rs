//! Two bounded SysV x86-64 GNU-assembler backends for Boolean posit8 DAGs.
//! The second backend caches the 16 input selectors once per invocation.
//! All instructions are emitted as inert text; GCC assembles them separately.
use super::LearnedCircuit;
use std::fmt::Write;

const MAX_LOCAL_BYTES: usize = 16_384;
const SELECTORS: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Backend {
    Direct,
    CachedSelectors,
}

impl LearnedCircuit {
    /// Baseline: reconstruct each input bit at every Boolean MUX.
    pub fn emit_sysv_x86_64(&self) -> Result<String, &'static str> {
        self.emit_x86_64_backend(Backend::Direct)
    }

    /// Alternative: extract each input bit once, share it among all gates.
    /// This is still a pure bitwise, branchless network, not a runtime LUT.
    pub fn emit_sysv_x86_64_cached(&self) -> Result<String, &'static str> {
        self.emit_x86_64_backend(Backend::CachedSelectors)
    }

    fn emit_x86_64_backend(&self, backend: Backend) -> Result<String, &'static str> {
        let scratch = self
            .nodes
            .len()
            .checked_add(2)
            .ok_or("node count overflow")?;
        let selector_space = if backend == Backend::CachedSelectors {
            SELECTORS
        } else {
            0
        };
        let required = scratch
            .checked_add(selector_space)
            .ok_or("scratch size overflow")?;
        let stack_bytes = required.checked_add(15).ok_or("stack size overflow")? & !15;
        if stack_bytes == 0 || stack_bytes > MAX_LOCAL_BYTES {
            return Err("stack scratch budget exceeded");
        }
        for (index, node) in self.nodes.iter().enumerate() {
            if node.variable >= SELECTORS as u8 || node.low >= index + 2 || node.high >= index + 2 {
                return Err("invalid or cyclic BDD node");
            }
        }
        if self.output_roots.iter().any(|&root| root >= scratch) {
            return Err("invalid output root");
        }

        let symbol = if backend == Backend::CachedSelectors {
            format!("mem_posit8_asm_cached_{}", self.operation.name())
        } else {
            format!("mem_posit8_asm_{}", self.operation.name())
        };
        let mut asm = String::new();
        writeln!(asm, ".intel_syntax noprefix").unwrap();
        writeln!(asm, ".text").unwrap();
        writeln!(asm, ".p2align 4").unwrap();
        writeln!(asm, ".globl {symbol}").unwrap();
        writeln!(asm, ".type {symbol}, @function").unwrap();
        writeln!(asm, "{symbol}:").unwrap();
        writeln!(asm, "    sub rsp, {stack_bytes}").unwrap();
        writeln!(asm, "    mov BYTE PTR [rsp + 0], 0").unwrap();
        writeln!(asm, "    mov BYTE PTR [rsp + 1], 1").unwrap();

        if backend == Backend::CachedSelectors {
            for variable in 0..SELECTORS {
                let source = if variable & 1 == 0 { "edi" } else { "esi" };
                let bit = variable / 2;
                let selector = scratch + variable;
                writeln!(asm, "    mov ecx, {source}").unwrap();
                writeln!(asm, "    shr ecx, {bit}").unwrap();
                writeln!(asm, "    and ecx, 1").unwrap();
                writeln!(asm, "    mov BYTE PTR [rsp + {selector}], cl").unwrap();
            }
        }

        for (index, node) in self.nodes.iter().enumerate() {
            let id = index + 2;
            let bit = node.variable / 2;
            let input_register = if node.variable & 1 == 0 { "edi" } else { "esi" };
            if backend == Backend::CachedSelectors {
                let selector = scratch + usize::from(node.variable);
                if node.low == 0 && node.high == 1 {
                    writeln!(asm, "    mov al, BYTE PTR [rsp + {selector}]").unwrap();
                    writeln!(asm, "    mov BYTE PTR [rsp + {id}], al").unwrap();
                    continue;
                }
                if node.low == 1 && node.high == 0 {
                    writeln!(asm, "    mov al, BYTE PTR [rsp + {selector}]").unwrap();
                    writeln!(asm, "    xor al, 1").unwrap();
                    writeln!(asm, "    mov BYTE PTR [rsp + {id}], al").unwrap();
                    continue;
                }
                // dl is 0/1 after these movzx instructions. An AND with
                // the stored selector preserves zero upper bits in edx.
                writeln!(asm, "    movzx eax, BYTE PTR [rsp + {}]", node.low).unwrap();
                writeln!(asm, "    movzx edx, BYTE PTR [rsp + {}]", node.high).unwrap();
                writeln!(asm, "    xor edx, eax").unwrap();
                writeln!(asm, "    and dl, BYTE PTR [rsp + {selector}]").unwrap();
                writeln!(asm, "    xor eax, edx").unwrap();
                writeln!(asm, "    mov BYTE PTR [rsp + {id}], al").unwrap();
            } else {
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
        writeln!(asm, ".size {symbol}, .-{symbol}").unwrap();
        writeln!(asm, ".section .note.GNU-stack,\"\",@progbits").unwrap();
        Ok(asm)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Operation;

    #[test]
    fn both_emitters_validate_static_bounds_and_branchless_semantics() {
        for operation in [Operation::Add, Operation::Mul] {
            let circuit = LearnedCircuit::synthesize(operation).unwrap();
            let direct = circuit.emit_sysv_x86_64().unwrap();
            let cached = circuit.emit_sysv_x86_64_cached().unwrap();
            for asm in [&direct, &cached] {
                assert!(asm.contains(".section .note.GNU-stack"));
                assert!(!asm.contains("call "));
                assert!(!asm.contains("jmp "));
                assert!(asm.contains("add rsp, "));
            }
            assert_eq!(direct.matches("    xor edx, eax").count(), circuit.nodes());
            assert_eq!(
                direct.matches("    mov BYTE PTR [rsp + ").count(),
                circuit.nodes() + 2
            );
            assert_eq!(
                cached.matches("    mov BYTE PTR [rsp + ").count(),
                circuit.nodes() + 2 + SELECTORS
            );
            assert!(cached.contains("and dl, BYTE PTR"));
            assert!(cached.len() < direct.len());
        }
    }
}
