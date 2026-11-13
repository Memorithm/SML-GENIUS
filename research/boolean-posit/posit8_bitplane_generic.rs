//! Const-generic Boolean bitplanes over 1..=16 u64 words per signal.
//! Exact CPU execution for any nonzero number of input pairs without padded
//! temporary arrays; no unsafe, external runtime, JIT, floating point or GPU.
use super::ComplementCircuit;

const INPUT_PLANES: usize = 16;
const MAX_WORDS: usize = 16;

/// Reusable workspace per graph and width. Width 1 is 64 independent pairs;
/// widths 4 and 8 represent 256 and 512 pairs, respectively.
pub struct PackedScratch<const W: usize> {
    inputs: [[u64; W]; INPUT_PLANES],
    gates: Vec<[u64; W]>,
}

impl ComplementCircuit {
    pub fn new_packed_scratch<const W: usize>(&self) -> PackedScratch<W> {
        PackedScratch {
            inputs: [[0; W]; INPUT_PLANES],
            gates: vec![[0; W]; self.nodes.len() + 1],
        }
    }

    /// Compute up to W * 64 operand pairs through one Boolean DAG walk.
    /// Partial chunks are handled without zero-padded input/output copies.
    pub fn eval_packed_chunk<const W: usize>(
        &self,
        inputs: &[(u8, u8)],
        scratch: &mut PackedScratch<W>,
        outputs: &mut [u8],
    ) -> Result<(), &'static str> {
        if W == 0 || W > MAX_WORDS {
            return Err("unsupported packed width");
        }
        if inputs.len() > W * 64 || outputs.len() != inputs.len() {
            return Err("invalid packed chunk dimensions");
        }
        if scratch.gates.len() != self.nodes.len() + 1 {
            return Err("packed scratch does not belong to this graph");
        }
        scratch.inputs.fill([0; W]);
        for (lane, &(a, b)) in inputs.iter().enumerate() {
            let word = lane / 64;
            let bit_mask = 1u64 << (lane % 64);
            for bit in 0..8 {
                if (a >> bit) & 1 != 0 {
                    scratch.inputs[2 * bit][word] |= bit_mask;
                }
                if (b >> bit) & 1 != 0 {
                    scratch.inputs[2 * bit + 1][word] |= bit_mask;
                }
            }
        }
        scratch.gates[0] = [0; W];
        for (index, node) in self.nodes.iter().enumerate() {
            let low_id = node.low >> 1;
            let high_id = node.high >> 1;
            if low_id > index || high_id > index {
                return Err("invalid forward gate dependency");
            }
            let variable = usize::from(node.variable);
            if variable >= INPUT_PLANES {
                return Err("invalid input bit");
            }
            let invert_low = node.low & 1 != 0;
            let invert_high = node.high & 1 != 0;
            for word in 0..W {
                let low = scratch.gates[low_id][word] ^ if invert_low { u64::MAX } else { 0 };
                let high = scratch.gates[high_id][word] ^ if invert_high { u64::MAX } else { 0 };
                let selected = scratch.inputs[variable][word];
                scratch.gates[index + 1][word] = low ^ ((low ^ high) & selected);
            }
        }
        if self
            .roots
            .iter()
            .any(|&root| root >> 1 >= scratch.gates.len())
        {
            return Err("invalid packed output reference");
        }
        for (lane, result) in outputs.iter_mut().enumerate() {
            let word = lane / 64;
            let bit = lane % 64;
            let mut encoded = 0u8;
            for (output_bit, &root) in self.roots.iter().enumerate() {
                let mut mask = scratch.gates[root >> 1][word];
                if root & 1 != 0 {
                    mask = !mask;
                }
                encoded |= (((mask >> bit) & 1) as u8) << output_bit;
            }
            *result = encoded;
        }
        Ok(())
    }

    /// Any number of operand pairs, including zero, with reusable memory.
    pub fn eval_packed_stream<const W: usize>(
        &self,
        inputs: &[(u8, u8)],
        scratch: &mut PackedScratch<W>,
        outputs: &mut [u8],
    ) -> Result<(), &'static str> {
        if W == 0 || W > MAX_WORDS {
            return Err("unsupported packed width");
        }
        if outputs.len() != inputs.len() || scratch.gates.len() != self.nodes.len() + 1 {
            return Err("invalid stream shape or scratch");
        }
        for (pairs, result) in inputs.chunks(W * 64).zip(outputs.chunks_mut(W * 64)) {
            self.eval_packed_chunk(pairs, scratch, result)?;
        }
        Ok(())
    }

    /// Exact oracle check on all pairs for any selected monomorphized width.
    pub fn verify_packed_exhaustive<const W: usize>(&self) -> Result<(), &'static str> {
        if W == 0 || W > MAX_WORDS {
            return Err("unsupported packed width");
        }
        let mut scratch = self.new_packed_scratch::<W>();
        let mut actual = vec![0u8; 65536];
        let inputs: Vec<_> = (0..65536u32)
            .map(|x| crate::deinterleave(x as u16))
            .collect();
        self.eval_packed_stream(&inputs, &mut scratch, &mut actual)?;
        for (index, &(a, b)) in inputs.iter().enumerate() {
            if actual[index] != self.operation.oracle(a, b) {
                return Err("const-generic Boolean output disagrees with oracle");
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
    fn exhaustive_add_mul_256_and_512_lanes() {
        for operation in [Operation::Add, Operation::Mul] {
            let graph = ComplementCircuit::synthesize(operation).unwrap();
            graph.verify_packed_exhaustive::<4>().unwrap();
            graph.verify_packed_exhaustive::<8>().unwrap();
        }
    }

    #[test]
    fn tails_and_empty_input_match_original_widths() {
        for operation in [Operation::Add, Operation::Mul] {
            let graph = ComplementCircuit::synthesize(operation).unwrap();
            let mut scratch1 = graph.new_packed_scratch::<1>();
            let mut scratch4 = graph.new_packed_scratch::<4>();
            let mut scratch8 = graph.new_packed_scratch::<8>();
            for len in [0usize, 1, 7, 63, 64, 65, 255, 256, 257, 511, 512, 513, 1025] {
                let pairs: Vec<_> = (0..len)
                    .map(|i| crate::deinterleave((i * 349 + 0x8000) as u16))
                    .collect();
                let mut a = vec![0xa5; len];
                let mut b = vec![0x5a; len];
                let mut c = vec![0x33; len];
                graph
                    .eval_packed_stream(&pairs, &mut scratch1, &mut a)
                    .unwrap();
                graph
                    .eval_packed_stream(&pairs, &mut scratch4, &mut b)
                    .unwrap();
                graph
                    .eval_packed_stream(&pairs, &mut scratch8, &mut c)
                    .unwrap();
                assert_eq!(a, b, "width1/4 len={len}");
                assert_eq!(b, c, "width4/8 len={len}");
            }
        }
    }

    #[test]
    fn invalid_widths_and_shapes_reject_before_writing_output() {
        let graph = ComplementCircuit::synthesize(Operation::Add).unwrap();
        let mut zero = graph.new_packed_scratch::<0>();
        let mut too_wide = graph.new_packed_scratch::<17>();
        let mut normal = graph.new_packed_scratch::<8>();
        let inputs = [(0u8, 0u8); 9];
        let mut output = [0x5au8; 9];
        assert!(graph
            .eval_packed_stream(&inputs, &mut zero, &mut output)
            .is_err());
        assert!(graph
            .eval_packed_stream(&inputs, &mut too_wide, &mut output)
            .is_err());
        assert!(graph
            .eval_packed_chunk(&inputs, &mut normal, &mut output[..8])
            .is_err());
        normal.gates.pop();
        assert!(graph
            .eval_packed_stream(&inputs, &mut normal, &mut output)
            .is_err());
        assert_eq!(output, [0x5a; 9]);
    }
}
