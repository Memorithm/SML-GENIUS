//! Complemented-edge reduced Boolean DAG synthesis for two-input posit8.
//! Independent internal variant: symbolic exact truth-table construction,
//! native x86-64 System V assembly emission and exhaustive parity.
use super::{deinterleave, Operation, DOMAIN, MAX_NODES};
use std::collections::HashMap;
use std::fmt::Write;

const SELECTOR_COUNT: usize = 16;
const MAX_STACK_BYTES: usize = 16_384;

// Encoded references use one complement flag: (node_number << 1) | flip.
// Node number zero means the false terminal. The true terminal is ref 1.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct EdgeNode {
    variable: u8,
    low: usize,
    high: usize,
}
struct EdgeBuilder {
    nodes: Vec<EdgeNode>,
    unique: HashMap<(u8, usize, usize), usize>,
}
impl EdgeBuilder {
    fn visit(
        &mut self,
        expected: &[u8],
        first: usize,
        span: usize,
        output_bit: u8,
    ) -> Result<usize, &'static str> {
        if span == 1 {
            return Ok(usize::from((expected[first] >> output_bit) & 1));
        }
        let half = span / 2;
        let low = self.visit(expected, first, half, output_bit)?;
        let high = self.visit(expected, first + half, half, output_bit)?;
        if low == high {
            return Ok(low);
        }
        // Canonical polarity: always store a non-complemented high edge.
        // Complement both children and the returned reference when high is
        // negated. Sharing works across positive and inverted subfunctions.
        let output_flip = high & 1;
        let low = low ^ output_flip;
        let high = high ^ output_flip;
        let variable = (half.trailing_zeros()) as u8;
        if let Some(&previous) = self.unique.get(&(variable, low, high)) {
            return Ok(previous ^ output_flip);
        }
        if self.nodes.len() >= MAX_NODES {
            return Err("complemented graph node budget exceeded");
        }
        let id = (self.nodes.len() + 1) * 2;
        self.nodes.push(EdgeNode {
            variable,
            low,
            high,
        });
        self.unique.insert((variable, low, high), id);
        Ok(id ^ output_flip)
    }
}

#[derive(Debug)]
pub struct ComplementCircuit {
    operation: Operation,
    nodes: Vec<EdgeNode>,
    roots: [usize; 8],
}
impl ComplementCircuit {
    pub fn synthesize(operation: Operation) -> Result<Self, &'static str> {
        let mut expected = vec![0u8; DOMAIN];
        for (encoded, output) in expected.iter_mut().enumerate() {
            let (a, b) = deinterleave(encoded as u16);
            *output = operation.oracle(a, b);
        }
        let mut builder = EdgeBuilder {
            nodes: Vec::new(),
            unique: HashMap::new(),
        };
        let mut roots = [0usize; 8];
        for (bit, root) in roots.iter_mut().enumerate() {
            *root = builder.visit(&expected, 0, DOMAIN, bit as u8)?;
        }
        Ok(Self {
            operation,
            nodes: builder.nodes,
            roots,
        })
    }

    pub fn nodes(&self) -> usize {
        self.nodes.len()
    }

    fn eval_edge(&self, mut edge: usize, input: u16) -> u8 {
        let mut negated = 0u8;
        loop {
            negated ^= (edge & 1) as u8;
            let id = edge >> 1;
            if id == 0 {
                return negated;
            }
            let node = self.nodes[id - 1];
            edge = if (input >> node.variable) & 1 == 0 {
                node.low
            } else {
                node.high
            };
        }
    }

    pub fn evaluate(&self, a: u8, b: u8) -> u8 {
        let input = super::interleave(a, b);
        self.roots
            .iter()
            .enumerate()
            .fold(0u8, |out, (bit, &root)| {
                out | (self.eval_edge(root, input) << bit)
            })
    }

    pub fn verify_exhaustive(&self) -> Result<(), &'static str> {
        for code in 0..=65535u16 {
            let (a, b) = deinterleave(code);
            if self.evaluate(a, b) != self.operation.oracle(a, b) {
                return Err("complemented graph differs from posit8 oracle");
            }
        }
        Ok(())
    }

    /// Emit an actual branchless x86-64 SysV function. Each node resides in
    /// one stack byte; the 16 input selectors are extracted once at function
    /// entry. Complemented edges are implemented as XOR 1 on the input/output
    /// of a MUX. All scratch accesses have statically validated offsets.
    pub fn emit_sysv_x86_64(&self) -> Result<String, &'static str> {
        let scratch = self.nodes.len().checked_add(1).ok_or("scratch overflow")?;
        let bytes = scratch
            .checked_add(SELECTOR_COUNT)
            .and_then(|sum| sum.checked_add(15))
            .ok_or("stack byte overflow")?
            & !15;
        if bytes > MAX_STACK_BYTES {
            return Err("stack budget exceeded");
        }
        for (index, node) in self.nodes.iter().enumerate() {
            let own_id = index + 1;
            if node.variable >= SELECTOR_COUNT as u8
                || node.low >> 1 >= own_id
                || node.high >> 1 >= own_id
                || node.high & 1 != 0
            {
                return Err("invalid complemented graph");
            }
        }
        if self.roots.iter().any(|&root| root >> 1 >= scratch) {
            return Err("invalid output root");
        }
        let name = format!("mem_posit8_ce_{}", self.operation.name());
        let mut out = String::new();
        writeln!(out, ".intel_syntax noprefix").unwrap();
        writeln!(out, ".text").unwrap();
        writeln!(out, ".p2align 4").unwrap();
        writeln!(out, ".globl {name}").unwrap();
        writeln!(out, ".type {name}, @function").unwrap();
        writeln!(out, "{name}:").unwrap();
        writeln!(out, "    sub rsp, {bytes}").unwrap();
        writeln!(out, "    mov BYTE PTR [rsp + 0], 0").unwrap();
        for bit in 0..SELECTOR_COUNT {
            let input_reg = if bit & 1 == 0 { "edi" } else { "esi" };
            let bit_in_operand = bit / 2;
            let selector = scratch + bit;
            writeln!(out, "    mov ecx, {input_reg}").unwrap();
            writeln!(out, "    shr ecx, {bit_in_operand}").unwrap();
            writeln!(out, "    and ecx, 1").unwrap();
            writeln!(out, "    mov BYTE PTR [rsp + {selector}], cl").unwrap();
        }
        for (index, node) in self.nodes.iter().enumerate() {
            let id = index + 1;
            let selector = scratch + usize::from(node.variable);
            let low_id = node.low >> 1;
            let high_id = node.high >> 1;
            if node.low == 1 && node.high == 0 {
                writeln!(out, "    mov al, BYTE PTR [rsp + {selector}]").unwrap();
                writeln!(out, "    xor al, 1").unwrap();
                writeln!(out, "    mov BYTE PTR [rsp + {id}], al").unwrap();
                continue;
            }
            writeln!(out, "    movzx eax, BYTE PTR [rsp + {low_id}]").unwrap();
            if node.low & 1 != 0 {
                writeln!(out, "    xor eax, 1").unwrap();
            }
            writeln!(out, "    movzx edx, BYTE PTR [rsp + {high_id}]").unwrap();
            writeln!(out, "    xor edx, eax").unwrap();
            writeln!(out, "    and dl, BYTE PTR [rsp + {selector}]").unwrap();
            writeln!(out, "    xor eax, edx").unwrap();
            writeln!(out, "    mov BYTE PTR [rsp + {id}], al").unwrap();
        }
        writeln!(out, "    xor r8d, r8d").unwrap();
        for (bit, root) in self.roots.iter().enumerate() {
            let id = root >> 1;
            writeln!(out, "    movzx eax, BYTE PTR [rsp + {id}]").unwrap();
            if root & 1 != 0 {
                writeln!(out, "    xor eax, 1").unwrap();
            }
            writeln!(out, "    shl eax, {bit}").unwrap();
            writeln!(out, "    or r8d, eax").unwrap();
        }
        writeln!(out, "    mov eax, r8d").unwrap();
        writeln!(out, "    add rsp, {bytes}").unwrap();
        writeln!(out, "    ret").unwrap();
        writeln!(out, ".size {name}, .-{name}").unwrap();
        writeln!(out, ".section .note.GNU-stack,\"\",@progbits").unwrap();
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LearnedCircuit;

    #[test]
    fn complement_edges_save_nodes_and_keep_all_exact_outputs() {
        for operation in [Operation::Add, Operation::Mul] {
            let baseline = LearnedCircuit::synthesize(operation).unwrap();
            let complemented = ComplementCircuit::synthesize(operation).unwrap();
            assert!(complemented.nodes() < baseline.nodes());
            complemented.verify_exhaustive().unwrap();
        }
    }

    #[test]
    fn assembly_is_bounded_and_has_no_indirect_control_flow() {
        for operation in [Operation::Add, Operation::Mul] {
            let graph = ComplementCircuit::synthesize(operation).unwrap();
            let code = graph.emit_sysv_x86_64().unwrap();
            assert!(code.contains(".note.GNU-stack"));
            assert!(code.contains("and dl, BYTE PTR"));
            assert!(!code.contains("jmp "));
            assert!(!code.contains("call "));
            assert_eq!(
                code.matches("    mov BYTE PTR [rsp + ").count(),
                graph.nodes() + 1 + SELECTOR_COUNT
            );
        }
    }
}
