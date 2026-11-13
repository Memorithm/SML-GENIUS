//! Clean-room exact Boolean circuit synthesis for 8-bit posit ADD and MUL.
//! Reduced ordered BDDs, learned only from the complete 65,536-pair oracle;
//! not gradient descent, not independent validation of Posit Standard 2022.
#![forbid(unsafe_code)]

#[path = "posit8_circuit_synth.rs"]
#[allow(dead_code)]
mod source;

#[path = "posit8_x86_64_emit.rs"]
mod x86_64_emit;

#[path = "posit8_complement_edges.rs"]
mod complement_edges;

use source::Posit8;
use std::collections::HashMap;
use std::fmt::Write;

const DOMAIN: usize = 1 << 16;
const MAX_NODES: usize = 16000;
const BITS: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation {
    Add,
    Mul,
}
impl Operation {
    fn name(self) -> &'static str {
        match self {
            Self::Add => "add",
            Self::Mul => "mul",
        }
    }
    fn oracle(self, a: u8, b: u8) -> u8 {
        match self {
            Self::Add => Posit8(a).add(Posit8(b)).0,
            Self::Mul => Posit8(a).mul(Posit8(b)).0,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Branch {
    variable: u8,
    low: usize,
    high: usize,
}
#[derive(Debug)]
pub struct LearnedCircuit {
    operation: Operation,
    nodes: Vec<Branch>,
    output_roots: [usize; BITS],
}
struct Builder {
    nodes: Vec<Branch>,
    unique: HashMap<(u8, usize, usize), usize>,
}
impl Builder {
    fn visit(
        &mut self,
        expected: &[u8],
        start: usize,
        size: usize,
        output_bit: u8,
    ) -> Result<usize, &'static str> {
        if size == 1 {
            return Ok(usize::from((expected[start] >> output_bit) & 1));
        }
        let half = size / 2;
        let low = self.visit(expected, start, half, output_bit)?;
        let high = self.visit(expected, start + half, half, output_bit)?;
        if low == high {
            return Ok(low);
        }
        let variable = (size.trailing_zeros() - 1) as u8;
        if let Some(&previous) = self.unique.get(&(variable, low, high)) {
            return Ok(previous);
        }
        if self.nodes.len() >= MAX_NODES {
            return Err("circuit exceeds prescribed node budget");
        }
        let id = self.nodes.len() + 2;
        self.nodes.push(Branch {
            variable,
            low,
            high,
        });
        self.unique.insert((variable, low, high), id);
        Ok(id)
    }
}

/// Interleave a and b bits so bit positions 2i/2i+1 represent ai/bi.
/// This fixed variable order is frozen before observing compressed size.
pub fn interleave(a: u8, b: u8) -> u16 {
    let mut packed = 0u16;
    for bit in 0..8 {
        packed |= u16::from((a >> bit) & 1) << (2 * bit);
        packed |= u16::from((b >> bit) & 1) << (2 * bit + 1);
    }
    packed
}
pub fn deinterleave(packed: u16) -> (u8, u8) {
    let mut a = 0u8;
    let mut b = 0u8;
    for bit in 0..8 {
        a |= (((packed >> (2 * bit)) & 1) as u8) << bit;
        b |= (((packed >> (2 * bit + 1)) & 1) as u8) << bit;
    }
    (a, b)
}
impl LearnedCircuit {
    /// Deterministic reduced ordered BDD synthesis, one 16-input Boolean
    /// network with eight output roots. All 65536 pairs are training facts;
    /// the subsequent exhaustive execution is an algebraic regression check,
    /// NOT a disjoint hold-out generalization or independent Posit oracle.
    pub fn synthesize(operation: Operation) -> Result<Self, &'static str> {
        let mut expected = vec![0u8; DOMAIN];
        for (encoded, output) in expected.iter_mut().enumerate() {
            let (a, b) = deinterleave(encoded as u16);
            *output = operation.oracle(a, b);
        }
        let mut builder = Builder {
            nodes: Vec::new(),
            unique: HashMap::new(),
        };
        let mut output_roots = [0usize; BITS];
        for (bit, root) in output_roots.iter_mut().enumerate() {
            *root = builder.visit(&expected, 0, DOMAIN, bit as u8)?;
        }
        Ok(Self {
            operation,
            nodes: builder.nodes,
            output_roots,
        })
    }
    pub fn nodes(&self) -> usize {
        self.nodes.len()
    }
    pub fn depth(&self) -> usize {
        16
    }
    pub fn evaluate(&self, a: u8, b: u8) -> u8 {
        let packed = interleave(a, b);
        let mut result = 0u8;
        for (bit, &root) in self.output_roots.iter().enumerate() {
            let mut current = root;
            while current >= 2 {
                let node = self.nodes[current - 2];
                current = if (packed >> node.variable) & 1 == 0 {
                    node.low
                } else {
                    node.high
                };
            }
            result |= (current as u8) << bit;
        }
        result
    }
    pub fn verify_exhaustive(&self) -> Result<(), &'static str> {
        for packed in 0..=65535u16 {
            let (a, b) = deinterleave(packed);
            if self.evaluate(a, b) != self.operation.oracle(a, b) {
                return Err("synthesized circuit != posit oracle");
            }
        }
        Ok(())
    }
    /// Emit bitwise MUX in safe Rust. Compiler lowers it to its actual native
    /// ISA. This is a circuit of Boolean operations, not a runtime LUT lookup,
    /// a Python/PT model or a claim of better performance.
    pub fn emit_native_rust(&self) -> String {
        let function = format!("learned_posit8_{}", self.operation.name());
        let mut code = format!(
            "#![forbid(unsafe_code)]\n#[inline(never)]\npub fn {function}(a: u8, b: u8) -> u8 {{\n"
        );
        for bit in 0..8 {
            writeln!(&mut code, "    let p{}: u8 = (a >> {}) & 1;", 2 * bit, bit).unwrap();
            writeln!(
                &mut code,
                "    let p{}: u8 = (b >> {}) & 1;",
                2 * bit + 1,
                bit
            )
            .unwrap();
        }
        code.push_str("    let w0: u8 = 0;\n    let w1: u8 = 1;\n");
        for (offset, node) in self.nodes.iter().enumerate() {
            let v = node.variable;
            let id = offset + 2;
            // A Boolean multiplexer: low XOR ((low XOR high) AND select).
            writeln!(
                &mut code,
                "    let w{id}: u8 = w{} ^ ((w{} ^ w{}) & p{v});",
                node.low, node.low, node.high
            )
            .unwrap();
        }
        let expr = self
            .output_roots
            .iter()
            .enumerate()
            .map(|(bit, root)| format!("(w{root} << {bit})"))
            .collect::<Vec<_>>()
            .join(" | ");
        writeln!(&mut code, "    {expr}\n}}").unwrap();
        code
    }
}
fn main() {
    let arg = std::env::args().nth(1);
    if arg.as_deref() == Some("--bench-bitslice64") {
        println!("POSIT8_BITPLANE64_BENCH_PROTOCOL input_count=65536 warmups=2 rounds=7");
        for operation in [Operation::Add, Operation::Mul] {
            let graph = complement_edges::ComplementCircuit::synthesize(operation)
                .expect("bounded graph synthesis");
            graph
                .verify_batch64_exhaustive()
                .expect("exactness prerequisite to benchmark");
            graph.benchmark_bitplanes64();
        }
        return;
    }
    if arg.as_deref() == Some("--bench-bitslice256") {
        println!("POSIT8_BITPLANE256_BENCH_PROTOCOL pairs=65536 warmups=2 rounds=7");
        for operation in [Operation::Add, Operation::Mul] {
            let graph = complement_edges::ComplementCircuit::synthesize(operation)
                .expect("bounded Boolean graph synthesis");
            graph
                .verify_batch256_exhaustive()
                .expect("full 256-lane exactness before benchmark");
            graph.benchmark_bitplanes256_against64();
        }
        return;
    }
    if arg.as_deref() == Some("--verify-bitslice512") {
        for operation in [Operation::Add, Operation::Mul] {
            let graph = complement_edges::ComplementCircuit::synthesize(operation)
                .expect("bounded graph synthesis");
            graph.verify_packed_exhaustive::<8>()
                .expect("512-lane exhaustive ADD/MUL validation");
            println!("POSIT8_BITPLANE512_{} PASS batches=128 pairs=65536 mismatches=0", operation.name());
        }
        return;
    }
    if arg.as_deref() == Some("--verify-bitslice256") {
        for operation in [Operation::Add, Operation::Mul] {
            let graph = complement_edges::ComplementCircuit::synthesize(operation)
                .expect("bounded graph synthesis");
            graph
                .verify_batch256_exhaustive()
                .expect("256-lane exhaustive arithmetic");
            println!(
                "POSIT8_BITPLANE256_{} PASS batches=256 pairs=65536 mismatches=0",
                operation.name()
            );
        }
        return;
    }
    if arg.as_deref() == Some("--verify-bitslice64") {
        for operation in [Operation::Add, Operation::Mul] {
            let graph = complement_edges::ComplementCircuit::synthesize(operation)
                .expect("bounded graph synthesis");
            graph
                .verify_batch64_exhaustive()
                .expect("64-lane exhaustive Boolean arithmetic");
            println!(
                "POSIT8_BITSLICE64_{} PASS batches=1024 pairs=65536 mismatches=0",
                operation.name()
            );
        }
        return;
    }
    let complemented_operation = match arg.as_deref() {
        Some("--emit-x86-ce-add") => Some(Operation::Add),
        Some("--emit-x86-ce-mul") => Some(Operation::Mul),
        _ => None,
    };
    if let Some(operation) = complemented_operation {
        let learned = complement_edges::ComplementCircuit::synthesize(operation)
            .expect("bounded complemented synthesis");
        learned
            .verify_exhaustive()
            .expect("all 65536 complemented cases");
        eprintln!(
            "POSIT8_CE_{} PASS nodes={} pairs=65536",
            operation.name(),
            learned.nodes()
        );
        print!(
            "{}",
            learned
                .emit_sysv_x86_64()
                .expect("bounded complemented assembly")
        );
        return;
    }
    let cached_assembly_operation = match arg.as_deref() {
        Some("--emit-x86-cached-add") => Some(Operation::Add),
        Some("--emit-x86-cached-mul") => Some(Operation::Mul),
        _ => None,
    };
    if let Some(operation) = cached_assembly_operation {
        let learned = LearnedCircuit::synthesize(operation).expect("bounded synthesis");
        print!(
            "{}",
            learned
                .emit_sysv_x86_64_cached()
                .expect("bounded cached assembly output")
        );
        return;
    }
    let assembly_operation = match arg.as_deref() {
        Some("--emit-x86-add") => Some(Operation::Add),
        Some("--emit-x86-mul") => Some(Operation::Mul),
        _ => None,
    };
    if let Some(operation) = assembly_operation {
        let learned = LearnedCircuit::synthesize(operation).expect("bounded synthesis");
        print!(
            "{}",
            learned.emit_sysv_x86_64().expect("bounded assembly output")
        );
        return;
    }
    let operation = match arg.as_deref() {
        Some("--emit-add") => Some(Operation::Add),
        Some("--emit-mul") => Some(Operation::Mul),
        _ => None,
    };
    if let Some(operation) = operation {
        let learned = LearnedCircuit::synthesize(operation).expect("bounded synthesis");
        print!("{}", learned.emit_native_rust());
        return;
    }
    for operation in [Operation::Add, Operation::Mul] {
        let learned = LearnedCircuit::synthesize(operation).expect("bounded synthesis");
        learned.verify_exhaustive().expect("all 65536 input pairs");
        println!(
            "POSIT8_BDD_{} PASS nodes={} depth<=16 pairs=65536 mismatches=0",
            operation.name(),
            learned.nodes()
        );
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn interleave_round_trip_is_total() {
        for packed in 0..=65535u16 {
            let (a, b) = deinterleave(packed);
            assert_eq!(interleave(a, b), packed);
        }
    }
    #[test]
    fn add_bdd_exact_65536_inputs() {
        let circuit = LearnedCircuit::synthesize(Operation::Add).unwrap();
        assert!(circuit.nodes() <= MAX_NODES);
        circuit.verify_exhaustive().unwrap();
    }
    #[test]
    fn mul_bdd_exact_65536_inputs() {
        let circuit = LearnedCircuit::synthesize(Operation::Mul).unwrap();
        assert!(circuit.nodes() <= MAX_NODES);
        circuit.verify_exhaustive().unwrap();
    }
}
