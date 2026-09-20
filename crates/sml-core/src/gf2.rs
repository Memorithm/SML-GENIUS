//! Deterministic bit-packed GF(2) reference operators.

use core::fmt;

use crate::ExecutionCounters;

/// Bit-packed vector over GF(2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Gf2Vector {
    words: Vec<u64>,
    len_bits: usize,
}

impl Gf2Vector {
    /// Builds a vector from explicit coefficients in little-index order.
    #[must_use]
    pub fn from_bits(bits: &[bool]) -> Self {
        let mut words = vec![0_u64; bits.len().div_ceil(64)];
        for (index, bit) in bits.iter().copied().enumerate() {
            if bit {
                words[index / 64] |= 1_u64 << (index % 64);
            }
        }
        Self {
            words,
            len_bits: bits.len(),
        }
    }

    /// Returns the logical vector width.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.len_bits
    }

    /// Returns true when the vector has zero coordinates.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len_bits == 0
    }

    /// Reads one coefficient.
    #[must_use]
    pub fn get(&self, index: usize) -> Option<bool> {
        if index >= self.len_bits {
            return None;
        }
        Some((self.words[index / 64] & (1_u64 << (index % 64))) != 0)
    }

    /// Coordinate-wise addition over GF(2).
    pub fn xor(&self, other: &Self, counters: &mut ExecutionCounters) -> Result<Self, Gf2Error> {
        if self.len_bits != other.len_bits {
            return Err(Gf2Error::LengthMismatch {
                left: self.len_bits,
                right: other.len_bits,
            });
        }

        counters.gf2_ops = counters.gf2_ops.saturating_add(self.len_bits as u64);
        Ok(Self {
            words: self
                .words
                .iter()
                .zip(&other.words)
                .map(|(left, right)| left ^ right)
                .collect(),
            len_bits: self.len_bits,
        })
    }

    /// GF(2) dot product, evaluated bit-parallel but accounted semantically.
    pub fn dot_parity(
        &self,
        other: &Self,
        counters: &mut ExecutionCounters,
    ) -> Result<bool, Gf2Error> {
        if self.len_bits != other.len_bits {
            return Err(Gf2Error::LengthMismatch {
                left: self.len_bits,
                right: other.len_bits,
            });
        }

        if self.len_bits != 0 {
            let semantic_ops = (self.len_bits as u64).saturating_mul(2).saturating_sub(1);
            counters.gf2_ops = counters.gf2_ops.saturating_add(semantic_ops);
        }

        let parity = self
            .words
            .iter()
            .zip(&other.words)
            .fold(0_u32, |acc, (left, right)| {
                acc ^ (left & right).count_ones()
            })
            & 1;
        Ok(parity != 0)
    }

    /// Interprets up to 64 coordinates as a little-endian integer.
    pub fn to_u64_le(&self) -> Result<u64, Gf2Error> {
        if self.len_bits > 64 {
            return Err(Gf2Error::TooWideForU64(self.len_bits));
        }
        Ok(self.words.first().copied().unwrap_or(0))
    }
}

/// Deterministic linear map over GF(2), represented by output rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Gf2LinearMap {
    input_bits: usize,
    rows: Vec<Gf2Vector>,
}

impl Gf2LinearMap {
    /// Constructs a linear map and validates every row width.
    pub fn new(input_bits: usize, rows: Vec<Gf2Vector>) -> Result<Self, Gf2Error> {
        for row in &rows {
            if row.len() != input_bits {
                return Err(Gf2Error::MapRowWidth {
                    expected: input_bits,
                    actual: row.len(),
                });
            }
        }
        Ok(Self { input_bits, rows })
    }

    /// Applies the map using exact GF(2) dot products.
    pub fn apply(
        &self,
        input: &Gf2Vector,
        counters: &mut ExecutionCounters,
    ) -> Result<Gf2Vector, Gf2Error> {
        if input.len() != self.input_bits {
            return Err(Gf2Error::LengthMismatch {
                left: self.input_bits,
                right: input.len(),
            });
        }

        let mut output = Vec::with_capacity(self.rows.len());
        for row in &self.rows {
            output.push(row.dot_parity(input, counters)?);
        }
        Ok(Gf2Vector::from_bits(&output))
    }
}

/// Validation errors for GF(2) primitives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gf2Error {
    LengthMismatch { left: usize, right: usize },
    MapRowWidth { expected: usize, actual: usize },
    TooWideForU64(usize),
}

impl fmt::Display for Gf2Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for Gf2Error {}
