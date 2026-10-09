//! Model-owned large sparse ANF bank with exact query-major packed outputs.
use crate::anf_large::{LargeAnfBatchError, LargeAnfGate};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LargeAnfBank {
    address_bits: u32,
    gates: Vec<LargeAnfGate>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LargeBankError {
    EmptyBank,
    WidthMismatch,
    BudgetExceeded,
    QueryOutOfRange(u32),
    Overflow,
    Allocation,
}

impl LargeAnfBank {
    pub fn new(gates: Vec<LargeAnfGate>) -> Result<Self, LargeBankError> {
        let first = gates.first().ok_or(LargeBankError::EmptyBank)?;
        let address_bits = first.address_bits();
        if gates.iter().any(|gate| gate.address_bits() != address_bits) {
            return Err(LargeBankError::WidthMismatch);
        }
        Ok(Self {
            address_bits,
            gates,
        })
    }

    #[must_use]
    pub fn gate_count(&self) -> usize {
        self.gates.len()
    }

    #[must_use]
    pub fn address_bits(&self) -> u32 {
        self.address_bits
    }

    /// Query-major output: ceil(gates / 64) words per query.
    /// Validates all bounds and output limits before allocating.
    pub fn evaluate_packed(
        &self,
        queries: &[u32],
        max_queries: usize,
        max_output_words: usize,
    ) -> Result<Vec<u64>, LargeBankError> {
        if queries.len() > max_queries {
            return Err(LargeBankError::BudgetExceeded);
        }
        for &q in queries {
            if q >= (1_u32 << self.address_bits) {
                return Err(LargeBankError::QueryOutOfRange(q));
            }
        }
        let words_per_query = self.gates.len().div_ceil(64);
        let count = queries
            .len()
            .checked_mul(words_per_query)
            .ok_or(LargeBankError::Overflow)?;
        if count > max_output_words {
            return Err(LargeBankError::BudgetExceeded);
        }
        let mut output = Vec::new();
        output
            .try_reserve_exact(count)
            .map_err(|_| LargeBankError::Allocation)?;
        output.resize(count, 0);
        for (index, &query) in queries.iter().enumerate() {
            for (gate_index, gate) in self.gates.iter().enumerate() {
                let value = gate
                    .evaluate(query)
                    .map_err(|_| LargeBankError::QueryOutOfRange(query))?;
                if value {
                    output[index * words_per_query + gate_index / 64] |= 1_u64 << (gate_index % 64);
                }
            }
        }
        Ok(output)
    }

    pub fn evaluate_bool(
        &self,
        queries: &[u32],
        max_queries: usize,
    ) -> Result<Vec<Vec<bool>>, LargeBankError> {
        if queries.len() > max_queries {
            return Err(LargeBankError::BudgetExceeded);
        }
        let mut output = Vec::new();
        output
            .try_reserve_exact(queries.len())
            .map_err(|_| LargeBankError::Allocation)?;
        for &q in queries {
            let mut row = Vec::new();
            row.try_reserve_exact(self.gates.len())
                .map_err(|_| LargeBankError::Allocation)?;
            for gate in &self.gates {
                row.push(
                    gate.evaluate(q)
                        .map_err(|_| LargeBankError::QueryOutOfRange(q))?,
                );
            }
            output.push(row);
        }
        Ok(output)
    }
}

impl From<LargeAnfBatchError> for LargeBankError {
    fn from(error: LargeAnfBatchError) -> Self {
        match error {
            LargeAnfBatchError::QueryBudgetExceeded => Self::BudgetExceeded,
            LargeAnfBatchError::QueryOutOfRange(q) => Self::QueryOutOfRange(q),
            LargeAnfBatchError::Allocation => Self::Allocation,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn packed_65_gate_boundary_matches_direct_literal_oracle() {
        let terms = [0u32, 1, 3, 65536, 1048575];
        let gates = (0..65)
            .map(|i| LargeAnfGate::new(20, vec![terms[i % terms.len()]]).unwrap())
            .collect();
        let bank = LargeAnfBank::new(gates).unwrap();
        let queries = [0, 1, 3, 65536, 1048575];
        let packed = bank.evaluate_packed(&queries, 5, 10).unwrap();
        assert_eq!(packed.len(), 10);
        for (qi, &q) in queries.iter().enumerate() {
            for gate in 0..65 {
                let mask = terms[gate % terms.len()];
                let expected = (q & mask) == mask;
                assert_eq!(
                    (packed[qi * 2 + gate / 64] >> (gate % 64)) & 1 != 0,
                    expected
                );
            }
            assert_eq!(packed[qi * 2 + 1] & !1, 0);
        }
    }
    #[test]
    fn invalid_width_queries_and_output_budgets_rejected() {
        let a = LargeAnfGate::new(20, vec![0]).unwrap();
        let b = LargeAnfGate::new(19, vec![0]).unwrap();
        assert_eq!(LargeAnfBank::new(vec![]), Err(LargeBankError::EmptyBank));
        assert_eq!(
            LargeAnfBank::new(vec![a.clone(), b]),
            Err(LargeBankError::WidthMismatch)
        );
        let bank = LargeAnfBank::new(vec![a]).unwrap();
        assert_eq!(
            bank.evaluate_packed(&[1048576], 1, 1),
            Err(LargeBankError::QueryOutOfRange(1048576))
        );
        assert_eq!(
            bank.evaluate_packed(&[0], 1, 0),
            Err(LargeBankError::BudgetExceeded)
        );
        assert_eq!(
            bank.evaluate_packed(&[0], 0, 1),
            Err(LargeBankError::BudgetExceeded)
        );
    }
}
