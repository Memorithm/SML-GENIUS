//! SYNTH-LEARN1B: bitwise active Boolean decision circuit induction.
//! Unlike whole-byte classes, eight binary target functions are trained from
//! the same limited membership queries, sharing canonical Boolean subgraphs.
use super::{
    is_holdout, permuted, ActiveReport, Gate, Operation, TreeBuilder, ACTIVE_ROUNDS, BATCH_QUERIES,
    DOMAIN, INITIAL_QUERIES, MAX_QUERIES,
};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Write;

pub struct BitwiseCircuit {
    nodes: Vec<Gate>,
    roots: [usize; 8],
}
impl BitwiseCircuit {
    pub fn learn(samples: &BTreeMap<u16, u8>, prefer_high: bool) -> Self {
        assert!(!samples.is_empty() && samples.len() <= MAX_QUERIES);
        let mut builder = TreeBuilder {
            nodes: Vec::new(),
            constants: HashMap::new(),
            shared: HashMap::new(),
            prefer_high,
        };
        let mut roots = [0; 8];
        for (bit, root) in roots.iter_mut().enumerate() {
            let rows: Vec<_> = samples
                .iter()
                .map(|(&input, &label)| (input, (label >> bit) & 1))
                .collect();
            *root = builder.build(&rows, u16::MAX);
        }
        Self {
            nodes: builder.nodes,
            roots,
        }
    }
    fn bit_value(&self, packed: u16, mut root: usize) -> u8 {
        loop {
            match self.nodes[root] {
                Gate::Constant(value) => return value,
                Gate::Select {
                    variable,
                    low,
                    high,
                } => {
                    root = if (packed >> variable) & 1 == 0 {
                        low
                    } else {
                        high
                    };
                }
            }
        }
    }
    pub fn evaluate(&self, packed: u16) -> u8 {
        self.roots
            .iter()
            .enumerate()
            .fold(0u8, |out, (bit, &root)| {
                out | (self.bit_value(packed, root) << bit)
            })
    }
    pub fn gates(&self) -> usize {
        self.nodes
            .iter()
            .filter(|node| matches!(node, Gate::Select { .. }))
            .count()
    }
    pub fn depth(&self) -> usize {
        fn find(model: &BitwiseCircuit, id: usize, memo: &mut [Option<usize>]) -> usize {
            if let Some(depth) = memo[id] {
                return depth;
            }
            let depth = match model.nodes[id] {
                Gate::Constant(_) => 0,
                Gate::Select { low, high, .. } => {
                    1 + find(model, low, memo).max(find(model, high, memo))
                }
            };
            memo[id] = Some(depth);
            depth
        }
        let mut memo = vec![None; self.nodes.len()];
        self.roots
            .iter()
            .map(|&id| find(self, id, &mut memo))
            .max()
            .unwrap_or(0)
    }
    /// Emit real GAS x86-64 code with eight sequential bitwise Boolean
    /// selection networks, sharing the learned Rust DAG before lowering.
    /// The output byte is assembled with OR immediates in eax.
    pub fn emit_x86_64_gas(&self, operation: Operation) -> String {
        let symbol = format!("mem_posit8_bitwise_{}", operation.symbol());
        let mut asm = format!(
            ".intel_syntax noprefix\n.text\n.globl {symbol}\n.type {symbol}, @function\n{symbol}:\n    xor eax, eax\n"
        );
        writeln!(&mut asm, "    jmp .L{symbol}_b0_n{}", self.roots[0]).unwrap();
        for bit in 0..8 {
            let root = self.roots[bit];
            let mut reachable = HashSet::new();
            let mut stack = vec![root];
            while let Some(id) = stack.pop() {
                if !reachable.insert(id) {
                    continue;
                }
                if let Gate::Select { low, high, .. } = self.nodes[id] {
                    stack.push(low);
                    stack.push(high);
                }
            }
            let mut reachable: Vec<_> = reachable.into_iter().collect();
            reachable.sort_unstable();
            for id in reachable {
                writeln!(&mut asm, ".L{symbol}_b{bit}_n{id}:").unwrap();
                match self.nodes[id] {
                    Gate::Constant(value) => {
                        if value != 0 {
                            assert_eq!(value, 1, "binary signal");
                            writeln!(&mut asm, "    or eax, {}", 1u16 << bit).unwrap();
                        }
                        if bit == 7 {
                            asm.push_str("    ret\n");
                        } else {
                            writeln!(
                                &mut asm,
                                "    jmp .L{symbol}_b{}_n{}",
                                bit + 1,
                                self.roots[bit + 1]
                            )
                            .unwrap();
                        }
                    }
                    Gate::Select {
                        variable,
                        low,
                        high,
                    } => {
                        let reg = if variable & 1 == 0 { "dil" } else { "sil" };
                        writeln!(&mut asm, "    test {reg}, {}", 1u8 << (variable / 2)).unwrap();
                        writeln!(&mut asm, "    jnz .L{symbol}_b{bit}_n{high}").unwrap();
                        writeln!(&mut asm, "    jmp .L{symbol}_b{bit}_n{low}").unwrap();
                    }
                }
            }
        }
        writeln!(&mut asm, ".size {symbol}, .-{symbol}").unwrap();
        asm.push_str(".section .note.GNU-stack,\"\",@progbits\n");
        asm
    }
}
fn next_batch(
    known: &BTreeMap<u16, u8>,
    first: Option<&BitwiseCircuit>,
    second: Option<&BitwiseCircuit>,
    count: usize,
    salt: u16,
) -> Vec<u16> {
    let mut disagreements = Vec::new();
    let mut exploration = Vec::new();
    for index in 0..DOMAIN {
        let packed = permuted(index as u16, salt);
        if is_holdout(packed) || known.contains_key(&packed) {
            continue;
        }
        if let (Some(a), Some(b)) = (first, second) {
            if a.evaluate(packed) != b.evaluate(packed) {
                disagreements.push(packed);
                continue;
            }
        }
        exploration.push(packed);
    }
    let quota = count * 3 / 4;
    let mut batch = Vec::with_capacity(count);
    batch.extend(disagreements.iter().take(quota).copied());
    batch.extend(exploration.iter().take(count - batch.len()).copied());
    if batch.len() < count {
        batch.extend(disagreements.into_iter().skip(quota).take(count - batch.len()));
    }
    batch
}
pub struct BitwiseOutcome {
    pub circuit: BitwiseCircuit,
    pub samples: BTreeMap<u16, u8>,
    pub report: ActiveReport,
}
pub fn learn_active_bitwise(operation: Operation) -> BitwiseOutcome {
    let mut samples = BTreeMap::new();
    for packed in next_batch(&samples, None, None, INITIAL_QUERIES, 12345) {
        samples.insert(packed, operation.teacher(packed));
    }
    let mut counterexamples = 0usize;
    let mut committee_disagreements = 0usize;
    for round in 0..ACTIVE_ROUNDS {
        let main = BitwiseCircuit::learn(&samples, false);
        let shadow = BitwiseCircuit::learn(&samples, true);
        let batch = next_batch(
            &samples,
            Some(&main),
            Some(&shadow),
            BATCH_QUERIES,
            5431u16.wrapping_add((round * 4099) as u16),
        );
        assert_eq!(batch.len(), BATCH_QUERIES);
        for packed in batch {
            committee_disagreements += usize::from(main.evaluate(packed) != shadow.evaluate(packed));
            let expected = operation.teacher(packed);
            counterexamples += usize::from(main.evaluate(packed) != expected);
            assert!(samples.insert(packed, expected).is_none());
        }
    }
    assert_eq!(samples.len(), MAX_QUERIES);
    let circuit = BitwiseCircuit::learn(&samples, false);
    let train_errors = samples
        .iter()
        .filter(|&(input, output)| circuit.evaluate(*input) != *output)
        .count();
    let mut holdout_size = 0usize;
    let mut holdout_errors = 0usize;
    let mut full_errors = 0usize;
    for index in 0..DOMAIN {
        let input = index as u16;
        let incorrect = circuit.evaluate(input) != operation.teacher(input);
        full_errors += usize::from(incorrect);
        if is_holdout(input) {
            holdout_size += 1;
            holdout_errors += usize::from(incorrect);
        }
    }
    let report = ActiveReport {
        operation,
        teacher_queries: samples.len(),
        committee_disagreements,
        counterexamples,
        train_errors,
        holdout_errors,
        holdout_size,
        full_errors,
        nodes: circuit.nodes.len(),
        gates: circuit.gates(),
        depth: circuit.depth(),
    };
    BitwiseOutcome {
        circuit,
        samples,
        report,
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bitwise_add_queries_are_unique_and_holdout_isolation_is_exact() {
        let trained = learn_active_bitwise(Operation::Add);
        assert_eq!(trained.samples.len(), MAX_QUERIES);
        assert!(trained.samples.keys().all(|&x| !is_holdout(x)));
        assert_eq!(trained.report.train_errors, 0);
        assert!(trained.report.depth <= 16);
        assert!(trained.report.gates > 0);
        assert!(trained.report.holdout_size > 0);
    }

    #[test]
    fn bitwise_mul_fits_all_queries_and_exports_direct_native_gates() {
        let trained = learn_active_bitwise(Operation::Mul);
        assert_eq!(trained.report.train_errors, 0);
        for (&packed, &expected) in &trained.samples {
            assert_eq!(trained.circuit.evaluate(packed), expected);
        }
        let asm = trained.circuit.emit_x86_64_gas(Operation::Mul);
        assert!(asm.contains("mem_posit8_bitwise_mul:"));
        assert!(asm.contains("    test sil, "));
        assert!(asm.contains("    or eax, "));
        assert!(asm.contains(".note.GNU-stack"));
    }
}
