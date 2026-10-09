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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LargeAnfBatchError {
    QueryBudgetExceeded,
    QueryOutOfRange(u32),
    Allocation,
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

    /// Evaluate a bounded query batch into a caller-owned output vector.
    /// Reject every invalid query before allocating or evaluating any result.
    pub fn evaluate_batch(
        &self,
        queries: &[u32],
        max_queries: usize,
    ) -> Result<Vec<bool>, LargeAnfBatchError> {
        if queries.len() > max_queries {
            return Err(LargeAnfBatchError::QueryBudgetExceeded);
        }
        let domain = 1_u32 << self.address_bits;
        for &query in queries {
            if query >= domain {
                return Err(LargeAnfBatchError::QueryOutOfRange(query));
            }
        }
        let mut values = Vec::new();
        values
            .try_reserve_exact(queries.len())
            .map_err(|_| LargeAnfBatchError::Allocation)?;
        for &query in queries {
            let parity = self
                .monomials
                .iter()
                .fold(false, |value, &mask| value ^ (query & mask == mask));
            values.push(parity);
        }
        Ok(values)
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

#[cfg(test)]
mod batch_tests {
    use super::*;
    #[test]
    fn complete_batched_values_match_independent_literal_control() {
        let terms = [0u32, 1, 3, 65536, 262145, 1_048_575];
        let gate = LargeAnfGate::new(20, terms.to_vec()).unwrap();
        let queries = [0, 1, 3, 65536, 262145, 524287, 1048575];
        let expected = queries
            .iter()
            .map(|&query| {
                terms.iter().fold(false, |parity, &mask| {
                    let mut product = true;
                    for bit in 0..20 {
                        if mask & (1 << bit) != 0 {
                            product &= query & (1 << bit) != 0;
                        }
                    }
                    parity ^ product
                })
            })
            .collect::<Vec<_>>();
        assert_eq!(gate.evaluate_batch(&queries, queries.len()), Ok(expected));
    }
    #[test]
    fn invalid_queries_and_budget_fail_closed() {
        let gate = LargeAnfGate::new(20, vec![0, 1]).unwrap();
        assert_eq!(
            gate.evaluate_batch(&[0, 1], 1),
            Err(LargeAnfBatchError::QueryBudgetExceeded)
        );
        assert_eq!(
            gate.evaluate_batch(&[0, 1_048_576], 2),
            Err(LargeAnfBatchError::QueryOutOfRange(1_048_576))
        );
        assert_eq!(gate.evaluate_batch(&[], 0), Ok(vec![]));
    }
}
