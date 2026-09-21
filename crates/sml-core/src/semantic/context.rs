//! Context Meaning Mask derivation for Semantic Boolean Geometry.
//!
//! A context mask selects which semantic coordinates participate in contextual
//! comparison. This reference slice derives relevance by exact Boolean support
//! counting over explicit context cues; it uses no floating-point values and
//! makes the threshold/update policy visible to the caller.

use core::fmt;

use super::{SemanticBitField, SemanticBitFieldError};

/// How a newly derived context mask changes the previous mask.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextMaskUpdate {
    /// Ignore the previous mask and use the newly derived mask.
    Replace,
    /// Keep coordinates active when either old or new context marks them relevant.
    Expand,
    /// Keep only coordinates supported by both old and new context.
    Refine,
}

/// Explicit policy for deriving and updating a Context Meaning Mask.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContextMaskPolicy {
    /// Minimum number of cue fields that must activate a coordinate.
    pub min_support: usize,
    /// How the derived mask combines with an existing mask.
    pub update: ContextMaskUpdate,
}

impl ContextMaskPolicy {
    /// Validates the support threshold against a concrete cue count.
    pub fn validate(self, cue_count: usize) -> Result<Self, ContextMaskError> {
        if cue_count == 0 {
            return Err(ContextMaskError::EmptyContext);
        }
        if self.min_support == 0 || self.min_support > cue_count {
            return Err(ContextMaskError::InvalidSupportThreshold {
                min_support: self.min_support,
                cue_count,
            });
        }
        Ok(self)
    }
}

/// Derives a fresh context mask from explicit Boolean cue fields.
///
/// A coordinate is active exactly when at least min_support cue fields set it.
/// The result is deterministic and integer-only.
pub fn derive_context_mask<const BITS: usize>(
    cues: &[&SemanticBitField<BITS>],
    min_support: usize,
) -> Result<SemanticBitField<BITS>, ContextMaskError> {
    let policy = ContextMaskPolicy {
        min_support,
        update: ContextMaskUpdate::Replace,
    }
    .validate(cues.len())?;

    let mut mask = SemanticBitField::zeroed()?;
    for index in 0..BITS {
        let mut support = 0_usize;
        for cue in cues {
            if cue.get(index)? {
                support += 1;
                if support >= policy.min_support {
                    mask.set(index, true)?;
                    break;
                }
            }
        }
    }
    Ok(mask)
}

/// Derives context relevance and combines it with the previous Meaning Mask.
pub fn update_context_mask<const BITS: usize>(
    previous: &SemanticBitField<BITS>,
    cues: &[&SemanticBitField<BITS>],
    policy: ContextMaskPolicy,
) -> Result<SemanticBitField<BITS>, ContextMaskError> {
    let policy = policy.validate(cues.len())?;
    let mut next = derive_context_mask(cues, policy.min_support)?;

    match policy.update {
        ContextMaskUpdate::Replace => {}
        ContextMaskUpdate::Expand => next.or_assign(previous),
        ContextMaskUpdate::Refine => next.and_assign(previous),
    }

    Ok(next)
}

/// Invalid Context Meaning Mask request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextMaskError {
    /// At least one context cue is required.
    EmptyContext,
    /// The support threshold must be in 1..=cue_count.
    InvalidSupportThreshold {
        min_support: usize,
        cue_count: usize,
    },
    /// A packed-field invariant failed.
    BitField(SemanticBitFieldError),
}

impl fmt::Display for ContextMaskError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for ContextMaskError {}

impl From<SemanticBitFieldError> for ContextMaskError {
    fn from(value: SemanticBitFieldError) -> Self {
        Self::BitField(value)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        derive_context_mask, update_context_mask, ContextMaskError, ContextMaskPolicy,
        ContextMaskUpdate,
    };
    use crate::semantic::{masked_hamming_distance, SemanticBitField};

    #[test]
    fn support_threshold_derives_exact_relevance() {
        let a = SemanticBitField::<8>::from_set_indices(&[0, 1, 4]).expect("a");
        let b = SemanticBitField::<8>::from_set_indices(&[0, 2, 4]).expect("b");
        let c = SemanticBitField::<8>::from_set_indices(&[0, 3, 5]).expect("c");

        let mask = derive_context_mask(&[&a, &b, &c], 2).expect("mask");

        assert_eq!(mask.count_ones(), 2);
        assert_eq!(mask.get(0), Ok(true));
        assert_eq!(mask.get(4), Ok(true));
    }

    #[test]
    fn invalid_context_thresholds_fail_closed() {
        let cue = SemanticBitField::<8>::zeroed().expect("cue");

        assert_eq!(
            derive_context_mask::<8>(&[], 1),
            Err(ContextMaskError::EmptyContext)
        );
        assert_eq!(
            derive_context_mask(&[&cue], 0),
            Err(ContextMaskError::InvalidSupportThreshold {
                min_support: 0,
                cue_count: 1,
            })
        );
        assert_eq!(
            derive_context_mask(&[&cue], 2),
            Err(ContextMaskError::InvalidSupportThreshold {
                min_support: 2,
                cue_count: 1,
            })
        );
    }

    #[test]
    fn update_policy_can_replace_expand_or_refine() {
        let previous = SemanticBitField::<8>::from_set_indices(&[0, 1, 2]).expect("previous");
        let a = SemanticBitField::<8>::from_set_indices(&[2, 3, 4]).expect("a");
        let b = SemanticBitField::<8>::from_set_indices(&[2, 3, 5]).expect("b");
        let cues = [&a, &b];

        let replaced = update_context_mask(
            &previous,
            &cues,
            ContextMaskPolicy {
                min_support: 2,
                update: ContextMaskUpdate::Replace,
            },
        )
        .expect("replace");
        assert_eq!(replaced.count_ones(), 2);
        assert_eq!(replaced.get(2), Ok(true));
        assert_eq!(replaced.get(3), Ok(true));

        let expanded = update_context_mask(
            &previous,
            &cues,
            ContextMaskPolicy {
                min_support: 2,
                update: ContextMaskUpdate::Expand,
            },
        )
        .expect("expand");
        assert_eq!(expanded.count_ones(), 4);

        let refined = update_context_mask(
            &previous,
            &cues,
            ContextMaskPolicy {
                min_support: 2,
                update: ContextMaskUpdate::Refine,
            },
        )
        .expect("refine");
        assert_eq!(refined.count_ones(), 1);
        assert_eq!(refined.get(2), Ok(true));
    }

    #[test]
    fn independently_derived_contexts_reverse_the_nearest_candidate() {
        // Controlled polysemy fixture: the names are mnemonic only. The test
        // proves contextual geometry behavior, not language understanding.
        let anchor = SemanticBitField::<8>::from_set_indices(&[0, 2, 4, 6]).expect("anchor");
        let finance = SemanticBitField::<8>::from_set_indices(&[0, 2, 5, 7]).expect("finance");
        let river = SemanticBitField::<8>::from_set_indices(&[1, 3, 4, 6]).expect("river");

        let f1 = SemanticBitField::<8>::from_set_indices(&[0, 1, 2, 3]).expect("f1");
        let f2 = SemanticBitField::<8>::from_set_indices(&[0, 1, 2, 3]).expect("f2");
        let f3 = SemanticBitField::<8>::from_set_indices(&[0, 1, 2, 3]).expect("f3");
        let finance_mask = derive_context_mask(&[&f1, &f2, &f3], 2).expect("finance mask");

        let r1 = SemanticBitField::<8>::from_set_indices(&[4, 5, 6, 7]).expect("r1");
        let r2 = SemanticBitField::<8>::from_set_indices(&[4, 5, 6, 7]).expect("r2");
        let r3 = SemanticBitField::<8>::from_set_indices(&[4, 5, 6, 7]).expect("r3");
        let river_mask = derive_context_mask(&[&r1, &r2, &r3], 2).expect("river mask");

        assert_eq!(
            masked_hamming_distance(&anchor, &finance, &finance_mask),
            Ok(0)
        );
        assert_eq!(
            masked_hamming_distance(&anchor, &river, &finance_mask),
            Ok(4)
        );
        assert_eq!(
            masked_hamming_distance(&anchor, &finance, &river_mask),
            Ok(4)
        );
        assert_eq!(
            masked_hamming_distance(&anchor, &river, &river_mask),
            Ok(0)
        );
    }
}
