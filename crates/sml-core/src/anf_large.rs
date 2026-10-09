//! Exact sparse ANF evaluation for domains beyond the u16 PVP6 address limit.
//! The address width is explicit and the representation remains sparse in monomials.
//! No dense K-word coefficient table is allocated by this evaluator.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LargeAnfGate {
    address_bits: u32,
    monomials: Vec<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LargeAnfError {
    InvalidAddressBits,
    EmptyMonomials,
    DuplicateMonomial(u32),
    MonomialOutOfRange(u32),
    QueryOutOfRange(u32),
}

impl LargeAnfGate {
    pub fn new(address_bits: u32, mut monomials: Vec<u32>) -> Result<Self, LargeAnfError> {
        if !(1..=31).contains(&address_bits) {
            return Err(LargeAnfError::InvalidAddressBits);
        }
        if monomials.is_empty() {
            return Err(LargeAnfError::EmptyMonomials);
        }
        monomials.sort_unstable();
        let domain = 1_u32 << address_bits;
        for &mask in &monomials {
            if mask >= domain {
                return Err(LargeAnfError::MonomialOutOfRange(mask));
            }
        }
        for pair in monomials.windows(2) {
            if pair[0] == pair[1] {
                return Err(LargeAnfError::DuplicateMonomial(pair[0]));
            }
        }
        Ok(Self {
            address_bits,
            monomials,
        })
    }

    pub fn evaluate(&self, query: u32) -> Result<bool, LargeAnfError> {
        if query >= (1_u32 << self.address_bits) {
            return Err(LargeAnfError::QueryOutOfRange(query));
        }
        Ok(self
            .monomials
            .iter()
            .fold(false, |parity, &mask| parity ^ (query & mask == mask)))
    }

    #[must_use]
    pub fn monomials(&self) -> &[u32] {
        &self.monomials
    }
    #[must_use]
    pub fn address_bits(&self) -> u32 {
        self.address_bits
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn million_address_domain_matches_independent_literal_oracle() {
        let terms = [0, 1, 3, 65536, 262145, 1_048_575];
        let gate = LargeAnfGate::new(20, terms.to_vec()).unwrap();
        for q in [0, 1, 3, 65_536, 262_145, 524_287, 1_048_575] {
            let expected = terms.iter().fold(false, |parity, &mask| {
                let mut product = true;
                for bit in 0..20 {
                    if mask & (1 << bit) != 0 {
                        product &= q & (1 << bit) != 0;
                    }
                }
                parity ^ product
            });
            assert_eq!(gate.evaluate(q), Ok(expected));
        }
        assert_eq!(
            gate.evaluate(1_048_576),
            Err(LargeAnfError::QueryOutOfRange(1_048_576))
        );
    }
    #[test]
    fn invalid_inputs_fail_closed() {
        assert_eq!(
            LargeAnfGate::new(0, vec![0]),
            Err(LargeAnfError::InvalidAddressBits)
        );
        assert_eq!(
            LargeAnfGate::new(32, vec![0]),
            Err(LargeAnfError::InvalidAddressBits)
        );
        assert_eq!(
            LargeAnfGate::new(20, vec![]),
            Err(LargeAnfError::EmptyMonomials)
        );
        assert_eq!(
            LargeAnfGate::new(20, vec![1, 1]),
            Err(LargeAnfError::DuplicateMonomial(1))
        );
        assert_eq!(
            LargeAnfGate::new(20, vec![1_048_576]),
            Err(LargeAnfError::MonomialOutOfRange(1_048_576))
        );
    }
}
