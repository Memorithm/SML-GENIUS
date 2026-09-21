//! Packed Boolean field used by Semantic Boolean Geometry.

use core::fmt;

/// A fixed logical-width Boolean field stored in machine words.
///
/// Constructors allocate the backing storage once.  The `*_assign` Boolean
/// operations mutate that storage in place and therefore allocate no memory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticBitField<const BITS: usize> {
    words: Vec<u64>,
}

impl<const BITS: usize> SemanticBitField<BITS> {
    /// Builds an all-zero field.
    pub fn zeroed() -> Result<Self, SemanticBitFieldError> {
        validate_width::<BITS>()?;
        Ok(Self {
            words: vec![0; required_words::<BITS>()],
        })
    }

    /// Builds an all-one logical field. Padding bits remain zero.
    pub fn ones() -> Result<Self, SemanticBitFieldError> {
        validate_width::<BITS>()?;
        let mut words = vec![u64::MAX; required_words::<BITS>()];
        mask_padding::<BITS>(&mut words);
        Ok(Self { words })
    }

    /// Builds a field from already packed words, rejecting malformed padding.
    pub fn from_words(words: Vec<u64>) -> Result<Self, SemanticBitFieldError> {
        validate_width::<BITS>()?;
        let expected = required_words::<BITS>();
        if words.len() != expected {
            return Err(SemanticBitFieldError::WordCount {
                expected,
                actual: words.len(),
            });
        }
        if has_nonzero_padding::<BITS>(&words) {
            return Err(SemanticBitFieldError::NonZeroPadding);
        }
        Ok(Self { words })
    }

    /// Builds a field with exactly the supplied logical indices set.
    pub fn from_set_indices(indices: &[usize]) -> Result<Self, SemanticBitFieldError> {
        let mut field = Self::zeroed()?;
        for &index in indices {
            field.set(index, true)?;
        }
        Ok(field)
    }

    /// Returns the fixed logical width.
    #[must_use]
    pub const fn len(&self) -> usize {
        BITS
    }

    /// A valid SBG field is never logically empty.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        BITS == 0
    }

    /// Returns the packed machine words.
    #[must_use]
    pub fn words(&self) -> &[u64] {
        &self.words
    }

    /// Reads one logical bit.
    pub fn get(&self, index: usize) -> Result<bool, SemanticBitFieldError> {
        validate_index::<BITS>(index)?;
        Ok((self.words[index / 64] & (1_u64 << (index % 64))) != 0)
    }

    /// Writes one logical bit.
    pub fn set(&mut self, index: usize, value: bool) -> Result<(), SemanticBitFieldError> {
        validate_index::<BITS>(index)?;
        let mask = 1_u64 << (index % 64);
        let word = &mut self.words[index / 64];
        if value {
            *word |= mask;
        } else {
            *word &= !mask;
        }
        Ok(())
    }

    /// Number of set logical bits.
    #[must_use]
    pub fn count_ones(&self) -> u64 {
        self.words
            .iter()
            .map(|word| u64::from(word.count_ones()))
            .sum()
    }

    /// Number of clear logical bits.
    #[must_use]
    pub fn count_zeros(&self) -> u64 {
        BITS as u64 - self.count_ones()
    }

    /// Clears every logical bit without reallocating.
    pub fn clear(&mut self) {
        self.words.fill(0);
    }

    /// In-place coordinate-wise XOR.
    pub fn xor_assign(&mut self, other: &Self) {
        for (left, right) in self.words.iter_mut().zip(&other.words) {
            *left ^= *right;
        }
    }

    /// In-place coordinate-wise XNOR.
    pub fn xnor_assign(&mut self, other: &Self) {
        for (left, right) in self.words.iter_mut().zip(&other.words) {
            *left = !(*left ^ *right);
        }
        mask_padding::<BITS>(&mut self.words);
    }

    /// In-place coordinate-wise AND.
    pub fn and_assign(&mut self, other: &Self) {
        for (left, right) in self.words.iter_mut().zip(&other.words) {
            *left &= *right;
        }
    }

    /// In-place coordinate-wise OR.
    pub fn or_assign(&mut self, other: &Self) {
        for (left, right) in self.words.iter_mut().zip(&other.words) {
            *left |= *right;
        }
    }

    /// In-place coordinate-wise NOT.
    pub fn not_assign(&mut self) {
        for word in &mut self.words {
            *word = !*word;
        }
        mask_padding::<BITS>(&mut self.words);
    }

    /// Counts differing logical coordinates without constructing a temporary field.
    #[must_use]
    pub fn xor_popcount(&self, other: &Self) -> u64 {
        self.words
            .iter()
            .zip(&other.words)
            .map(|(left, right)| u64::from((left ^ right).count_ones()))
            .sum()
    }

    /// Counts equal logical coordinates without constructing a temporary field.
    #[must_use]
    pub fn xnor_popcount(&self, other: &Self) -> u64 {
        BITS as u64 - self.xor_popcount(other)
    }
}

const fn required_words<const BITS: usize>() -> usize {
    if BITS == 0 {
        0
    } else {
        1 + (BITS - 1) / 64
    }
}

const fn tail_mask<const BITS: usize>() -> u64 {
    let tail = BITS % 64;
    if tail == 0 {
        u64::MAX
    } else {
        (1_u64 << tail) - 1
    }
}

fn mask_padding<const BITS: usize>(words: &mut [u64]) {
    if BITS % 64 != 0 {
        if let Some(last) = words.last_mut() {
            *last &= tail_mask::<BITS>();
        }
    }
}

fn has_nonzero_padding<const BITS: usize>(words: &[u64]) -> bool {
    if BITS % 64 == 0 {
        return false;
    }
    words
        .last()
        .is_some_and(|last| (*last & !tail_mask::<BITS>()) != 0)
}

const fn validate_width<const BITS: usize>() -> Result<(), SemanticBitFieldError> {
    if BITS == 0 {
        Err(SemanticBitFieldError::ZeroWidth)
    } else {
        Ok(())
    }
}

const fn validate_index<const BITS: usize>(index: usize) -> Result<(), SemanticBitFieldError> {
    if index >= BITS {
        Err(SemanticBitFieldError::IndexOutOfRange { index, width: BITS })
    } else {
        Ok(())
    }
}

/// Structural errors for packed semantic fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticBitFieldError {
    /// Semantic fields must have at least one logical coordinate.
    ZeroWidth,
    /// Packed word count differs from the const-generic logical width.
    WordCount { expected: usize, actual: usize },
    /// Bits outside the declared logical width were set.
    NonZeroPadding,
    /// A logical coordinate was addressed outside the declared width.
    IndexOutOfRange { index: usize, width: usize },
}

impl fmt::Display for SemanticBitFieldError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for SemanticBitFieldError {}

#[cfg(test)]
mod tests {
    use super::{SemanticBitField, SemanticBitFieldError};

    #[test]
    fn zero_width_fails_closed() {
        assert_eq!(
            SemanticBitField::<0>::zeroed(),
            Err(SemanticBitFieldError::ZeroWidth)
        );
    }

    #[test]
    fn rejects_nonzero_padding() {
        assert_eq!(
            SemanticBitField::<65>::from_words(vec![0, 2]),
            Err(SemanticBitFieldError::NonZeroPadding)
        );
    }

    #[test]
    fn set_get_and_popcount_are_exact() {
        let mut field = SemanticBitField::<130>::zeroed().expect("valid field");
        field.set(0, true).expect("index");
        field.set(64, true).expect("index");
        field.set(129, true).expect("index");

        assert_eq!(field.count_ones(), 3);
        assert_eq!(field.count_zeros(), 127);
        assert_eq!(field.get(64), Ok(true));
        assert_eq!(
            field.get(130),
            Err(SemanticBitFieldError::IndexOutOfRange {
                index: 130,
                width: 130,
            })
        );
    }

    #[test]
    fn boolean_assign_operations_preserve_logical_width() {
        let a = SemanticBitField::<70>::from_set_indices(&[0, 69]).expect("a");
        let b = SemanticBitField::<70>::from_set_indices(&[0, 1]).expect("b");

        let mut xor = a.clone();
        xor.xor_assign(&b);
        assert_eq!(xor.count_ones(), 2);
        assert_eq!(xor.get(1), Ok(true));
        assert_eq!(xor.get(69), Ok(true));

        let mut xnor = a.clone();
        xnor.xnor_assign(&b);
        assert_eq!(xnor.count_ones(), 68);

        let mut and = a.clone();
        and.and_assign(&b);
        assert_eq!(and.count_ones(), 1);
        assert_eq!(and.get(0), Ok(true));

        let mut or = a.clone();
        or.or_assign(&b);
        assert_eq!(or.count_ones(), 3);

        let mut inverted = a;
        inverted.not_assign();
        assert_eq!(inverted.count_ones(), 68);
        assert_eq!(inverted.words().len(), 2);
        assert_eq!(inverted.words()[1] & !0x3f, 0);
    }

    #[test]
    fn direct_similarity_counts_need_no_temporary_allocation() {
        let a = SemanticBitField::<128>::from_set_indices(&[0, 1, 64]).expect("a");
        let b = SemanticBitField::<128>::from_set_indices(&[0, 2, 64]).expect("b");
        assert_eq!(a.xor_popcount(&b), 2);
        assert_eq!(a.xnor_popcount(&b), 126);
    }
}
