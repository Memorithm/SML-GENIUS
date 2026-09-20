//! Canonical algebraic-normal-form (Zhegalkin) reference semantics.

use core::fmt;

use crate::ExecutionCounters;

/// One non-empty square-free ANF monomial.
///
/// Variables must be strictly increasing, which gives each monomial one
/// deterministic representation.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct AnfMonomial {
    variables: Vec<u16>,
}

impl AnfMonomial {
    /// Constructs a canonical monomial.
    pub fn new(variables: Vec<u16>) -> Result<Self, AnfError> {
        if variables.is_empty() {
            return Err(AnfError::EmptyMonomial);
        }
        if variables.windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err(AnfError::NonCanonicalVariables);
        }
        Ok(Self { variables })
    }

    /// Returns the polynomial degree of this monomial.
    #[must_use]
    pub fn degree(&self) -> usize {
        self.variables.len()
    }
}

/// Canonical polynomial over GF(2) in algebraic normal form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnfPolynomial {
    input_count: usize,
    constant: bool,
    monomials: Vec<AnfMonomial>,
}

impl AnfPolynomial {
    /// Constructs an ANF polynomial.
    ///
    /// Monomials must be strictly increasing in lexicographic order. This
    /// rejects duplicates instead of silently cancelling them.
    pub fn new(
        input_count: usize,
        constant: bool,
        monomials: Vec<AnfMonomial>,
    ) -> Result<Self, AnfError> {
        if monomials.windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err(AnfError::NonCanonicalMonomialOrder);
        }

        for monomial in &monomials {
            for variable in &monomial.variables {
                if *variable as usize >= input_count {
                    return Err(AnfError::VariableOutOfRange {
                        variable: *variable,
                        input_count,
                    });
                }
            }
        }

        Ok(Self {
            input_count,
            constant,
            monomials,
        })
    }

    /// Evaluates the polynomial and records semantic primitive counts.
    pub fn evaluate(
        &self,
        inputs: &[bool],
        counters: &mut ExecutionCounters,
    ) -> Result<bool, AnfError> {
        if inputs.len() != self.input_count {
            return Err(AnfError::InputCount {
                expected: self.input_count,
                actual: inputs.len(),
            });
        }

        let mut result = self.constant;
        for monomial in &self.monomials {
            counters.anf_monomials = counters.anf_monomials.saturating_add(1);

            let mut variables = monomial.variables.iter();
            let first = *variables.next().ok_or(AnfError::EmptyMonomial)? as usize;
            let mut term = inputs[first];
            for variable in variables {
                term &= inputs[*variable as usize];
                counters.boolean_ops = counters.boolean_ops.saturating_add(1);
            }

            result ^= term;
            counters.gf2_ops = counters.gf2_ops.saturating_add(1);
        }
        Ok(result)
    }
}

/// Validation or evaluation errors for canonical ANF.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnfError {
    EmptyMonomial,
    NonCanonicalVariables,
    NonCanonicalMonomialOrder,
    VariableOutOfRange { variable: u16, input_count: usize },
    InputCount { expected: usize, actual: usize },
}

impl fmt::Display for AnfError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for AnfError {}
