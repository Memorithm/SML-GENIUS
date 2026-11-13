//! Four-word SIMD-friendly Boolean planes for 256 simultaneous posit8 pairs.
//! No unsafe blocks, no GPU, no external runtime; a gate operates on four u64.
//! Any AVX2 acceleration must be established from assembly and host timings.
use super::ComplementCircuit;

const PLANES: usize = 16;
const WORDS: usize = 4;
const LANES: usize = 256;

/// Reusable graph-scoped workspace, about (nodes + 1) * 32 + 512 bytes.
pub struct Scratch256 {
    input: [[u64; WORDS]; PLANES],
    gates: Vec<[u64; WORDS]>,
}

impl ComplementCircuit {
    pub fn new_scratch256(&self) -> Scratch256 {
        Scratch256 {
            input: [[0; WORDS]; PLANES],
            gates: vec![[0; WORDS]; self.nodes.len() + 1],
        }
    }

    /// One node traversal computes the same gate on 256 input pairs.
    /// All reads and writes are statically limited by the graph and lanes.
    pub fn evaluate_batch256_into(
        &self,
        inputs: &[(u8, u8); LANES],
        scratch: &mut Scratch256,
        outputs: &mut [u8; LANES],
    ) -> Result<(), &'static str> {
        if scratch.gates.len() != self.nodes.len() + 1 {
            return Err("scratch belongs to another Boolean circuit");
        }
        scratch.input.fill([0; WORDS]);
        for (lane, &(a, b)) in inputs.iter().enumerate() {
            let word = lane / 64;
            let mask = 1u64 << (lane % 64);
            for bit in 0..8 {
                if (a >> bit) & 1 != 0 {
                    scratch.input[2 * bit][word] |= mask;
                }
                if (b >> bit) & 1 != 0 {
                    scratch.input[2 * bit + 1][word] |= mask;
                }
            }
        }
        scratch.gates[0] = [0; WORDS];
        for (index, node) in self.nodes.iter().enumerate() {
            let low_id = node.low >> 1;
            let high_id = node.high >> 1;
            if low_id > index || high_id > index {
                return Err("non-topological Boolean graph");
            }
            if usize::from(node.variable) >= PLANES {
                return Err("input variable exceeds sixteen Boolean planes");
            }
            let low_inverted = node.low & 1 != 0;
            let high_inverted = node.high & 1 != 0;
            let variable = usize::from(node.variable);
            for word in 0..WORDS {
                let mut low = scratch.gates[low_id][word];
                let mut high = scratch.gates[high_id][word];
                if low_inverted {
                    low = !low;
                }
                if high_inverted {
                    high = !high;
                }
                let selected = scratch.input[variable][word];
                scratch.gates[index + 1][word] = low ^ ((low ^ high) & selected);
            }
        }

        let mut output_planes = [[0u64; WORDS]; 8];
        for (bit, &root) in self.roots.iter().enumerate() {
            let id = root >> 1;
            if id >= scratch.gates.len() {
                return Err("output references missing Boolean node");
            }
            for word in 0..WORDS {
                output_planes[bit][word] = if root & 1 != 0 {
                    !scratch.gates[id][word]
                } else {
                    scratch.gates[id][word]
                };
            }
        }
        for (lane, output) in outputs.iter_mut().enumerate() {
            let word = lane / 64;
            let offset = lane % 64;
            let mut raw = 0u8;
            for (bit, planes) in output_planes.iter().enumerate() {
                raw |= (((planes[word] >> offset) & 1) as u8) << bit;
            }
            *output = raw;
        }
        Ok(())
    }

    /// Arbitrary-sized stream, including empty and incomplete 256-lane tails.
    pub fn evaluate_slice256_into(
        &self,
        inputs: &[(u8, u8)],
        scratch: &mut Scratch256,
        outputs: &mut [u8],
    ) -> Result<(), &'static str> {
        if inputs.len() != outputs.len() {
            return Err("input and output shapes differ");
        }
        if scratch.gates.len() != self.nodes.len() + 1 {
            return Err("scratch belongs to another Boolean circuit");
        }
        for (slice, result) in inputs.chunks(LANES).zip(outputs.chunks_mut(LANES)) {
            let mut padded = [(0u8, 0u8); LANES];
            padded[..slice.len()].copy_from_slice(slice);
            let mut computed = [0u8; LANES];
            self.evaluate_batch256_into(&padded, scratch, &mut computed)?;
            result.copy_from_slice(&computed[..slice.len()]);
        }
        Ok(())
    }

    /// 65536 input pairs, exactly 256 groups, with no excluded cases.
    pub fn verify_batch256_exhaustive(&self) -> Result<(), &'static str> {
        let mut scratch = self.new_scratch256();
        let mut outputs = [0u8; LANES];
        for batch in 0..(65536 / LANES) {
            let inputs: [(u8, u8); LANES] = std::array::from_fn(|lane| {
                crate::deinterleave((batch * LANES + lane) as u16)
            });
            self.evaluate_batch256_into(&inputs, &mut scratch, &mut outputs)?;
            for (index, &(a, b)) in inputs.iter().enumerate() {
                if outputs[index] != self.operation.oracle(a, b) {
                    return Err("256-lane graph result differs from integer oracle");
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
    fn exhaustive_addition_256_lanes() {
        ComplementCircuit::synthesize(Operation::Add)
            .unwrap()
            .verify_batch256_exhaustive()
            .unwrap();
    }

    #[test]
    fn exhaustive_multiplication_256_lanes() {
        ComplementCircuit::synthesize(Operation::Mul)
            .unwrap()
            .verify_batch256_exhaustive()
            .unwrap();
    }

    #[test]
    fn tails_and_empty_inputs_preserve_reference_semantics() {
        for operation in [Operation::Add, Operation::Mul] {
            let circuit = ComplementCircuit::synthesize(operation).unwrap();
            let mut scratch = circuit.new_scratch256();
            for len in [0usize, 1, 64, 127, 255, 256, 257, 513, 1024] {
                let inputs: Vec<_> = (0..len)
                    .map(|i| crate::deinterleave((i * 163 + 0x8080) as u16))
                    .collect();
                let mut output = vec![0xa5u8; len];
                circuit
                    .evaluate_slice256_into(&inputs, &mut scratch, &mut output)
                    .unwrap();
                for (i, &(a, b)) in inputs.iter().enumerate() {
                    assert_eq!(output[i], operation.oracle(a, b), "len={len} i={i}");
                }
            }
        }
    }

    #[test]
    fn incorrect_shapes_leave_existing_outputs_unchanged() {
        let graph = ComplementCircuit::synthesize(Operation::Add).unwrap();
        let mut scratch = graph.new_scratch256();
        let input = [(0x40u8, 0x40u8); 257];
        let mut output = [0xb3u8; 256];
        assert!(graph
            .evaluate_slice256_into(&input, &mut scratch, &mut output)
            .is_err());
        assert_eq!(output, [0xb3u8; 256]);

        scratch.gates.pop();
        let mut output = [0x66u8; LANES];
        let input = [(0u8, 0u8); LANES];
        assert!(graph
            .evaluate_batch256_into(&input, &mut scratch, &mut output)
            .is_err());
        assert_eq!(output, [0x66u8; LANES]);
    }
}
