//! Exact 64-way bit-sliced execution of synthesized posit8 Boolean DAGs.
//! One u64 bit lane corresponds to one independent operand pair.
//! No floating point, pointer-chasing during per-lane gate execution, or GPU.
use super::ComplementCircuit;

const VARIABLES: usize = 16;
const LANES: usize = 64;

/// Caller-reusable workspace: O(number of DAG gates) 64-bit words.
/// An incorrect workspace size is detected before touching the output.
pub struct Scratch64 {
    input_masks: [u64; VARIABLES],
    node_masks: Vec<u64>,
}

impl ComplementCircuit {
    pub fn new_scratch64(&self) -> Scratch64 {
        Scratch64 {
            input_masks: [0; VARIABLES],
            node_masks: vec![0; self.nodes.len() + 1],
        }
    }

    /// Execute one Boolean DAG over exactly 64 independent posit8 pairs.
    /// Each gate is evaluated once per 64 lanes using native u64 AND/XOR.
    /// This portable reference makes no AVX2-acceleration claim.
    pub fn evaluate_batch64_into(
        &self,
        inputs: &[(u8, u8); LANES],
        scratch: &mut Scratch64,
        outputs: &mut [u8; LANES],
    ) -> Result<(), &'static str> {
        if scratch.node_masks.len() != self.nodes.len() + 1 {
            return Err("scratch belongs to a different graph");
        }
        scratch.input_masks.fill(0);
        for (lane, &(a, b)) in inputs.iter().enumerate() {
            for bit in 0..8 {
                let selected = 1u64 << lane;
                if (a >> bit) & 1 != 0 {
                    scratch.input_masks[bit * 2] |= selected;
                }
                if (b >> bit) & 1 != 0 {
                    scratch.input_masks[bit * 2 + 1] |= selected;
                }
            }
        }

        scratch.node_masks[0] = 0;
        for (index, node) in self.nodes.iter().enumerate() {
            let low_id = node.low >> 1;
            let high_id = node.high >> 1;
            if low_id > index || high_id > index {
                return Err("non-topological Boolean DAG");
            }
            if usize::from(node.variable) >= VARIABLES {
                return Err("Boolean variable out of range");
            }
            let low = scratch.node_masks[low_id] ^ if node.low & 1 != 0 { u64::MAX } else { 0 };
            let high = scratch.node_masks[high_id] ^ if node.high & 1 != 0 { u64::MAX } else { 0 };
            let selector = scratch.input_masks[usize::from(node.variable)];
            scratch.node_masks[index + 1] = low ^ ((low ^ high) & selector);
        }

        let mut result_planes = [0u64; 8];
        for (bit, &root) in self.roots.iter().enumerate() {
            let id = root >> 1;
            if id >= scratch.node_masks.len() {
                return Err("invalid result root");
            }
            result_planes[bit] = scratch.node_masks[id] ^ if root & 1 != 0 { u64::MAX } else { 0 };
        }
        for (lane, output) in outputs.iter_mut().enumerate() {
            let mut value = 0u8;
            for (bit, &plane) in result_planes.iter().enumerate() {
                value |= (((plane >> lane) & 1) as u8) << bit;
            }
            *output = value;
        }
        Ok(())
    }

    /// Exhaustive reference validation against the scalar posit8 oracle.
    /// Covers 65536 pairs as 1024 consecutive, exact 64-way batches.
    pub fn verify_batch64_exhaustive(&self) -> Result<(), &'static str> {
        let mut scratch = self.new_scratch64();
        let mut output = [0u8; LANES];
        for block in 0..(65536 / LANES) {
            let inputs = std::array::from_fn(|lane| {
                let packed = (block * LANES + lane) as u16;
                crate::deinterleave(packed)
            });
            self.evaluate_batch64_into(&inputs, &mut scratch, &mut output)?;
            for (lane, &(a, b)) in inputs.iter().enumerate() {
                if output[lane] != self.operation.oracle(a, b) {
                    return Err("bit-sliced kernel disagrees with posit8 reference");
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Operation;

    #[test]
    fn exact_all_65536_additions() {
        ComplementCircuit::synthesize(Operation::Add)
            .unwrap()
            .verify_batch64_exhaustive()
            .unwrap();
    }

    #[test]
    fn exact_all_65536_multiplications() {
        ComplementCircuit::synthesize(Operation::Mul)
            .unwrap()
            .verify_batch64_exhaustive()
            .unwrap();
    }

    #[test]
    fn erroneous_scratch_fails_before_modifying_output() {
        let graph = ComplementCircuit::synthesize(Operation::Add).unwrap();
        let mut scratch = graph.new_scratch64();
        scratch.node_masks.pop();
        let pairs = [(0u8, 0u8); LANES];
        let mut outputs = [0x5au8; LANES];
        assert!(graph
            .evaluate_batch64_into(&pairs, &mut scratch, &mut outputs)
            .is_err());
        assert_eq!(outputs, [0x5a; LANES]);
    }

    #[test]
    fn all_nar_and_extreme_lanes() {
        for operation in [Operation::Add, Operation::Mul] {
            let graph = ComplementCircuit::synthesize(operation).unwrap();
            let mut scratch = graph.new_scratch64();
            let pairs = std::array::from_fn(|lane| {
                let values = [0u8, 1, 0x40, 0x7f, 0x80, 0x81, 0xc0, 0xff];
                (values[lane & 7], values[(lane * 3 + 5) & 7])
            });
            let mut outputs = [0u8; LANES];
            graph
                .evaluate_batch64_into(&pairs, &mut scratch, &mut outputs)
                .unwrap();
            for (i, &(a, b)) in pairs.iter().enumerate() {
                assert_eq!(outputs[i], operation.oracle(a, b));
            }
        }
    }
}
