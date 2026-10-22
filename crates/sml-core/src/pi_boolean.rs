//! Fixed-prefix experiments for π-driven Boolean systems.
//!
//! Kauffman tables and connections consume successive *decimal* digits of π.
//! Binary neural network weights consume bits from the *hexadecimal* expansion.
//! These are public, bounded reference streams, not randomness or cryptographic entropy.

/// First 1,600 decimal digits after the decimal point of π.
pub const PI_DECIMAL_FRACTION: &str = concat!(
    "1415926535897932384626433832795028841971693993751058209749445923",
    "0781640628620899862803482534211706798214808651328230664709384460",
    "9550582231725359408128481117450284102701938521105559644622948954",
    "9303819644288109756659334461284756482337867831652712019091456485",
    "6692346034861045432664821339360726024914127372458700660631558817",
    "4881520920962829254091715364367892590360011330530548820466521384",
    "1469519415116094330572703657595919530921861173819326117931051185",
    "4807446237996274956735188575272489122793818301194912983367336244",
    "0656643086021394946395224737190702179860943702770539217176293176",
    "7523846748184676694051320005681271452635608277857713427577896091",
    "7363717872146844090122495343014654958537105079227968925892354201",
    "9956112129021960864034418159813629774771309960518707211349999998",
    "3729780499510597317328160963185950244594553469083026425223082533",
    "4468503526193118817101000313783875288658753320838142061717766914",
    "7303598253490428755468731159562863882353787593751957781857780532",
    "1712268066130019278766111959092164201989380952572010654858632788",
    "6593615338182796823030195203530185296899577362259941389124972177",
    "5283479131515574857242454150695950829533116861727855889075098381",
    "7546374649393192550604009277016711390098488240128583616035637076",
    "6010471018194295559619894676783744944825537977472684710404753464",
    "6208046684259069491293313677028989152104752162056966024058038150",
    "1935112533824300355876402474964732639141992726042699227967823547",
    "8163600934172164121992458631503028618297455570674983850549458858",
    "6926995690927210797509302955321165344987202755960236480665499119",
    "8818347977535663698074265425278625518184175746728909777727938000",
);
/// First 1,024 hexadecimal digits after the hexadecimal point of π.
pub const PI_HEX_FRACTION: &str = concat!(
    "243f6a8885a308d313198a2e03707344a4093822299f31d0082efa98ec4e6c89",
    "452821e638d01377be5466cf34e90c6cc0ac29b7c97c50dd3f84d5b5b5470917",
    "9216d5d98979fb1bd1310ba698dfb5ac2ffd72dbd01adfb7b8e1afed6a267e96",
    "ba7c9045f12c7f9924a19947b3916cf70801f2e2858efc16636920d871574e69",
    "a458fea3f4933d7e0d95748f728eb658718bcd5882154aee7b54a41dc25a59b5",
    "9c30d5392af26013c5d1b023286085f0ca417918b8db38ef8e79dcb0603a180e",
    "6c9e0e8bb01e8a3ed71577c1bd314b2778af2fda55605c60e65525f3aa55ab94",
    "5748986263e8144055ca396a2aab10b6b4cc5c341141e8cea15486af7c72e993",
    "b3ee1411636fbc2a2ba9c55d741831f6ce5c3e169b87931eafd6ba336c24cf5c",
    "7a325381289586773b8f48986b4bb9afc4bfe81b6628219361d809ccfb21a991",
    "487cac605dec8032ef845d5de98575b1dc262302eb651b8823893e81d396acc5",
    "0f6d6ff383f442392e0b4482a484200469c8f04a9e1f9b5e21c66842f6e96c9a",
    "670c9c61abd388f06a51a0d2d8542f68960fa728ab5133a36eef0b6c137a3be4",
    "ba3bf0507efb2a98a1f1651d39af017666ca593e82430e888cee8619456f9fb4",
    "7d84a5c33b8b5ebee06f75d885c12073401a449f56c16aa64ed3aa62363f7706",
    "1bfedf72429b023d37d0d724d00a1248db0fead349f1c09b075372c980991b7b",
);

/// Input validation and finite-stream errors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PiError {
    InvalidOffset { offset: usize, capacity: usize },
    Exhausted { requested: usize, remaining: usize },
    InvalidNetwork { nodes: usize, indegree: usize },
    InvalidStateLength { expected: usize, actual: usize },
}

/// Cursor over consecutive fractional decimal digits of π.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PiDecimalStream {
    offset: usize,
}

impl PiDecimalStream {
    /// Start at a zero-based decimal-digit position.
    pub fn new(offset: usize) -> Result<Self, PiError> {
        if offset > PI_DECIMAL_FRACTION.len() {
            return Err(PiError::InvalidOffset {
                offset,
                capacity: PI_DECIMAL_FRACTION.len(),
            });
        }
        Ok(Self { offset })
    }

    /// Number of decimal digits still available.
    #[must_use]
    pub fn remaining(&self) -> usize {
        PI_DECIMAL_FRACTION.len() - self.offset
    }

    fn ensure(&self, requested: usize) -> Result<(), PiError> {
        if requested > self.remaining() {
            return Err(PiError::Exhausted {
                requested,
                remaining: self.remaining(),
            });
        }
        Ok(())
    }

    /// Consume one digit, without recycling the stored prefix.
    pub fn read_digit(&mut self) -> Result<u8, PiError> {
        self.ensure(1)?;
        let digit = PI_DECIMAL_FRACTION.as_bytes()[self.offset] - b'0';
        self.offset += 1;
        Ok(digit)
    }

    fn read_bool(&mut self) -> Result<bool, PiError> {
        Ok(self.read_digit()? % 2 == 1)
    }

    /// Select an unbiased index for an ideal uniform 4-digit sample;
    /// modulo rejection removes arithmetic bias, not unknown bias in π.
    pub fn read_index(&mut self, upper_exclusive: usize) -> Result<usize, PiError> {
        if upper_exclusive == 0 || upper_exclusive > 256 {
            return Err(PiError::InvalidNetwork {
                nodes: upper_exclusive,
                indegree: 0,
            });
        }
        let upper_limit = 10_000 - (10_000 % upper_exclusive);
        loop {
            self.ensure(4)?;
            let number = PI_DECIMAL_FRACTION.as_bytes()[self.offset..self.offset + 4]
                .iter()
                .fold(0_usize, |acc, digit| acc * 10 + usize::from(*digit - b'0'));
            self.offset += 4;
            if number < upper_limit {
                return Ok(number % upper_exclusive);
            }
        }
    }
}

/// Big-endian-per-nibble bit cursor over the fractional hexadecimal expansion of π.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PiBitStream {
    bit_offset: usize,
}

impl PiBitStream {
    /// Start at a zero-based bit position.
    pub fn new(bit_offset: usize) -> Result<Self, PiError> {
        let capacity = PI_HEX_FRACTION.len() * 4;
        if bit_offset > capacity {
            return Err(PiError::InvalidOffset {
                offset: bit_offset,
                capacity,
            });
        }
        Ok(Self { bit_offset })
    }

    /// Number of fractional binary digits still available.
    #[must_use]
    pub fn remaining(&self) -> usize {
        PI_HEX_FRACTION.len() * 4 - self.bit_offset
    }

    fn ensure(&self, requested: usize) -> Result<(), PiError> {
        if requested > self.remaining() {
            return Err(PiError::Exhausted {
                requested,
                remaining: self.remaining(),
            });
        }
        Ok(())
    }

    /// Read one exact bit, high bit first within each hex digit.
    pub fn read_bit(&mut self) -> Result<bool, PiError> {
        self.ensure(1)?;
        let hex_byte = PI_HEX_FRACTION.as_bytes()[self.bit_offset / 4];
        let nibble = match hex_byte {
            b'0'..=b'9' => hex_byte - b'0',
            b'a'..=b'f' => hex_byte - b'a' + 10,
            _ => unreachable!("versioned hexadecimal π prefix contains only hex digits"),
        };
        let bit = (nibble >> (3 - self.bit_offset % 4)) & 1 != 0;
        self.bit_offset += 1;
        Ok(bit)
    }
}

/// Generate binary weights without floating-point operations or a conventional PRNG.
/// 0 → -1, 1 → +1; an exhausted prefix is an error, never a repeated stream.
pub fn bnn_weights_from_pi(count: usize, start_bit: usize) -> Result<Vec<i8>, PiError> {
    let mut bits = PiBitStream::new(start_bit)?;
    bits.ensure(count)?;
    let mut result = Vec::with_capacity(count);
    for _ in 0..count {
        result.push(if bits.read_bit()? { 1 } else { -1 });
    }
    Ok(result)
}

/// A synchronous, quenched Boolean network with up to 8 distinct inputs per node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KauffmanPiNetwork {
    inputs: Vec<Vec<usize>>,
    tables: Vec<Vec<bool>>,
}

impl KauffmanPiNetwork {
    /// Assign distinct input nodes and truth-table entries from consecutive decimal digits.
    pub fn from_pi(nodes: usize, indegree: usize, start_digit: usize) -> Result<Self, PiError> {
        if nodes == 0 || nodes > 256 || indegree > nodes || indegree > 8 {
            return Err(PiError::InvalidNetwork { nodes, indegree });
        }
        let mut stream = PiDecimalStream::new(start_digit)?;
        let mut inputs = Vec::with_capacity(nodes);
        let mut tables = Vec::with_capacity(nodes);
        for _ in 0..nodes {
            let mut node_inputs = Vec::with_capacity(indegree);
            while node_inputs.len() < indegree {
                let candidate = stream.read_index(nodes)?;
                if !node_inputs.contains(&candidate) {
                    node_inputs.push(candidate);
                }
            }
            let mut table = Vec::with_capacity(1_usize << indegree);
            for _ in 0..(1_usize << indegree) {
                table.push(stream.read_bool()?);
            }
            inputs.push(node_inputs);
            tables.push(table);
        }
        Ok(Self { inputs, tables })
    }

    /// Read the fixed incoming connections for one node.
    #[must_use]
    pub fn inputs(&self, node: usize) -> Option<&[usize]> {
        self.inputs.get(node).map(Vec::as_slice)
    }

    /// Read the complete fixed truth table for one node.
    #[must_use]
    pub fn table(&self, node: usize) -> Option<&[bool]> {
        self.tables.get(node).map(Vec::as_slice)
    }

    /// Update all nodes from the same previous state (no in-place propagation).
    pub fn step(&self, state: &[bool]) -> Result<Vec<bool>, PiError> {
        if state.len() != self.inputs.len() {
            return Err(PiError::InvalidStateLength {
                expected: self.inputs.len(),
                actual: state.len(),
            });
        }
        let mut next = Vec::with_capacity(state.len());
        for (sources, table) in self.inputs.iter().zip(&self.tables) {
            let mut address = 0_usize;
            for (bit, &source) in sources.iter().enumerate() {
                if state[source] {
                    address |= 1_usize << bit;
                }
            }
            next.push(table[address]);
        }
        Ok(next)
    }

    /// Total next-step Hamming distance across all single-bit perturbations
    /// of the same supplied state. Divide by node count to get a mean.
    pub fn one_step_damage_count(&self, state: &[bool]) -> Result<usize, PiError> {
        let reference = self.step(state)?;
        let mut total = 0_usize;
        let mut perturbed = state.to_vec();
        for index in 0..state.len() {
            perturbed[index] = !state[index];
            let changed = self.step(&perturbed)?;
            total += changed
                .iter()
                .zip(&reference)
                .filter(|(left, right)| left != right)
                .count();
            perturbed[index] = state[index];
        }
        Ok(total)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_pi_prefixes_are_correct() {
        assert_eq!(PI_DECIMAL_FRACTION.len(), 1_600);
        assert_eq!(PI_HEX_FRACTION.len(), 1_024);
        assert!(PI_DECIMAL_FRACTION.starts_with("141592653589793238462643383279"));
        assert!(PI_HEX_FRACTION.starts_with("243f6a8885a308d313198a2e03707344"));
    }

    #[test]
    fn decimal_digits_and_offsets_are_exact() {
        let mut digits = PiDecimalStream::new(0).unwrap();
        assert_eq!(digits.read_digit(), Ok(1));
        assert_eq!(digits.read_digit(), Ok(4));
        assert_eq!(digits.read_digit(), Ok(1));
        assert_eq!(PiDecimalStream::new(3).unwrap().read_digit(), Ok(5));
    }

    #[test]
    fn binary_weights_read_exact_pi_bits() {
        assert_eq!(bnn_weights_from_pi(8, 0), Ok(vec![-1, -1, 1, -1, -1, 1, -1, -1]));
        assert_eq!(bnn_weights_from_pi(4, 4), Ok(vec![-1, 1, -1, -1]));
        assert!(matches!(bnn_weights_from_pi(1, 4096), Err(PiError::Exhausted { .. })));
    }

    #[test]
    fn both_streams_reject_exhaustion_without_advancing() {
        let mut decimals = PiDecimalStream::new(1_599).unwrap();
        assert!(matches!(decimals.read_index(16), Err(PiError::Exhausted { .. })));
        assert_eq!(decimals.remaining(), 1);
        assert_eq!(decimals.read_digit().is_ok(), true);

        let mut bits = PiBitStream::new(4_095).unwrap();
        assert_eq!(bits.remaining(), 1);
        assert!(bits.read_bit().is_ok());
        assert!(matches!(bits.read_bit(), Err(PiError::Exhausted { .. })));
    }

    #[test]
    fn topology_is_unique_and_replayable() {
        let a = KauffmanPiNetwork::from_pi(16, 2, 0).unwrap();
        let b = KauffmanPiNetwork::from_pi(16, 2, 0).unwrap();
        assert_eq!(a, b);
        for node in 0..16 {
            let edges = a.inputs(node).unwrap();
            assert_eq!(edges.len(), 2);
            assert_ne!(edges[0], edges[1]);
            assert!(edges.iter().all(|&source| source < 16));
            assert_eq!(a.table(node).unwrap().len(), 4);
        }
        assert_ne!(a, KauffmanPiNetwork::from_pi(16, 2, 32).unwrap());
    }

    #[test]
    fn synchronous_oracle_and_causal_damage() {
        // Each node copies the OTHER node: simultaneous execution swaps state.
        let net = KauffmanPiNetwork {
            inputs: vec![vec![1], vec![0]],
            tables: vec![vec![false, true], vec![false, true]],
        };
        assert_eq!(net.step(&[true, false]), Ok(vec![false, true]));
        assert_eq!(net.one_step_damage_count(&[true, false]), Ok(2));
        assert!(matches!(
            net.step(&[true]),
            Err(PiError::InvalidStateLength { .. })
        ));
    }

    #[test]
    fn zero_input_network_is_state_independent() {
        let net = KauffmanPiNetwork::from_pi(8, 0, 0).unwrap();
        assert_eq!(net.step(&[false; 8]), net.step(&[true; 8]));
    }

    #[test]
    fn topology_and_prefix_capacity_are_enforced() {
        assert!(matches!(
            KauffmanPiNetwork::from_pi(257, 2, 0),
            Err(PiError::InvalidNetwork { .. })
        ));
        assert!(matches!(
            KauffmanPiNetwork::from_pi(32, 8, 0),
            Err(PiError::Exhausted { .. })
        ));
        assert!(matches!(
            bnn_weights_from_pi(4097, 0),
            Err(PiError::Exhausted { .. })
        ));
    }
}
