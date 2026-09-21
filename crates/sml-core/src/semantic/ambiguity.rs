//! Explicit ambiguity dynamics for Semantic Boolean Geometry.
//!
//! Ambiguity is represented as a Boolean field whose active coordinates are
//! supported by competing hypotheses. The reference policy is integer-only and
//! intentionally permits ambiguity to increase as new conflicting evidence
//! appears, then decrease when hypotheses converge.

use core::fmt;

use super::{SemanticBitField, SemanticBitFieldError};

/// Policy for deciding whether one coordinate remains semantically ambiguous.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AmbiguityPolicy {
    /// Minimum number of hypotheses required on both Boolean sides.
    ///
    /// A coordinate is ambiguous when at least this many hypotheses vote false
    /// and at least this many hypotheses vote true.
    pub min_minority_support: usize,
}

impl AmbiguityPolicy {
    /// Validates the ambiguity rule for a concrete hypothesis count.
    pub fn validate(self, hypothesis_count: usize) -> Result<Self, AmbiguityError> {
        if hypothesis_count < 2 {
            return Err(AmbiguityError::InsufficientHypotheses(hypothesis_count));
        }
        if self.min_minority_support == 0
            || self.min_minority_support.saturating_mul(2) > hypothesis_count
        {
            return Err(AmbiguityError::InvalidMinoritySupport {
                min_minority_support: self.min_minority_support,
                hypothesis_count,
            });
        }
        Ok(self)
    }
}

/// Exact transition summary between two ambiguity fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AmbiguityTransition<const BITS: usize> {
    /// Ambiguity after applying the current hypothesis set.
    pub next: SemanticBitField<BITS>,
    /// Coordinates that changed from resolved to ambiguous.
    pub introduced: SemanticBitField<BITS>,
    /// Coordinates that changed from ambiguous to resolved.
    pub resolved: SemanticBitField<BITS>,
    /// Coordinates that stayed ambiguous.
    pub persistent: SemanticBitField<BITS>,
}

impl<const BITS: usize> AmbiguityTransition<BITS> {
    /// Number of newly ambiguous coordinates.
    #[must_use]
    pub fn introduced_count(&self) -> u64 {
        self.introduced.count_ones()
    }

    /// Number of newly resolved coordinates.
    #[must_use]
    pub fn resolved_count(&self) -> u64 {
        self.resolved.count_ones()
    }

    /// Number of coordinates that remained ambiguous.
    #[must_use]
    pub fn persistent_count(&self) -> u64 {
        self.persistent.count_ones()
    }
}

/// Derives an ambiguity field from competing Boolean semantic hypotheses.
///
/// The operation is symmetric with respect to hypothesis order. A coordinate is
/// ambiguous exactly when both Boolean values receive at least the configured
/// minority support.
pub fn derive_ambiguity<const BITS: usize>(
    hypotheses: &[&SemanticBitField<BITS>],
    policy: AmbiguityPolicy,
) -> Result<SemanticBitField<BITS>, AmbiguityError> {
    let policy = policy.validate(hypotheses.len())?;
    let mut ambiguity = SemanticBitField::zeroed()?;

    for index in 0..BITS {
        let mut true_votes = 0_usize;
        let mut false_votes = 0_usize;

        for hypothesis in hypotheses {
            if hypothesis.get(index)? {
                true_votes += 1;
            } else {
                false_votes += 1;
            }

            if true_votes >= policy.min_minority_support
                && false_votes >= policy.min_minority_support
            {
                ambiguity.set(index, true)?;
                break;
            }
        }
    }

    Ok(ambiguity)
}

/// Recomputes ambiguity from the current hypotheses and reports exact changes.
pub fn update_ambiguity<const BITS: usize>(
    previous: &SemanticBitField<BITS>,
    hypotheses: &[&SemanticBitField<BITS>],
    policy: AmbiguityPolicy,
) -> Result<AmbiguityTransition<BITS>, AmbiguityError> {
    let next = derive_ambiguity(hypotheses, policy)?;

    let mut introduced = next.clone();
    let mut not_previous = previous.clone();
    not_previous.not_assign();
    introduced.and_assign(&not_previous);

    let mut resolved = previous.clone();
    let mut not_next = next.clone();
    not_next.not_assign();
    resolved.and_assign(&not_next);

    let mut persistent = previous.clone();
    persistent.and_assign(&next);

    Ok(AmbiguityTransition {
        next,
        introduced,
        resolved,
        persistent,
    })
}

/// Invalid ambiguity-dynamics request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AmbiguityError {
    /// At least two competing hypotheses are needed to define disagreement.
    InsufficientHypotheses(usize),
    /// Minority support must be non-zero and achievable on both Boolean sides.
    InvalidMinoritySupport {
        min_minority_support: usize,
        hypothesis_count: usize,
    },
    /// A packed semantic-field invariant failed.
    BitField(SemanticBitFieldError),
}

impl fmt::Display for AmbiguityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for AmbiguityError {}

impl From<SemanticBitFieldError> for AmbiguityError {
    fn from(value: SemanticBitFieldError) -> Self {
        Self::BitField(value)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        derive_ambiguity, update_ambiguity, AmbiguityError, AmbiguityPolicy,
    };
    use crate::semantic::SemanticBitField;

    const POLICY: AmbiguityPolicy = AmbiguityPolicy {
        min_minority_support: 1,
    };

    #[test]
    fn ambiguity_is_exactly_coordinate_disagreement_for_minority_one() {
        let a = SemanticBitField::<8>::from_set_indices(&[0, 1, 4]).expect("a");
        let b = SemanticBitField::<8>::from_set_indices(&[0, 2, 4]).expect("b");
        let c = SemanticBitField::<8>::from_set_indices(&[0, 3, 4]).expect("c");

        let ambiguity = derive_ambiguity(&[&a, &b, &c], POLICY).expect("ambiguity");

        assert_eq!(ambiguity.count_ones(), 3);
        assert_eq!(ambiguity.get(1), Ok(true));
        assert_eq!(ambiguity.get(2), Ok(true));
        assert_eq!(ambiguity.get(3), Ok(true));
        assert_eq!(ambiguity.get(0), Ok(false));
        assert_eq!(ambiguity.get(4), Ok(false));
    }

    #[test]
    fn stronger_minority_support_filters_single_outlier() {
        let a = SemanticBitField::<8>::from_set_indices(&[0, 1]).expect("a");
        let b = SemanticBitField::<8>::from_set_indices(&[0, 1]).expect("b");
        let c = SemanticBitField::<8>::from_set_indices(&[0]).expect("c");
        let d = SemanticBitField::<8>::from_set_indices(&[0]).expect("d");

        let ambiguity = derive_ambiguity(
            &[&a, &b, &c, &d],
            AmbiguityPolicy {
                min_minority_support: 2,
            },
        )
        .expect("ambiguity");

        assert_eq!(ambiguity.count_ones(), 1);
        assert_eq!(ambiguity.get(1), Ok(true));
    }

    #[test]
    fn invalid_hypothesis_sets_fail_closed() {
        let a = SemanticBitField::<8>::zeroed().expect("a");

        assert_eq!(
            derive_ambiguity(&[&a], POLICY),
            Err(AmbiguityError::InsufficientHypotheses(1))
        );
        assert_eq!(
            derive_ambiguity(
                &[&a, &a],
                AmbiguityPolicy {
                    min_minority_support: 0,
                },
            ),
            Err(AmbiguityError::InvalidMinoritySupport {
                min_minority_support: 0,
                hypothesis_count: 2,
            })
        );
        assert_eq!(
            derive_ambiguity(
                &[&a, &a, &a],
                AmbiguityPolicy {
                    min_minority_support: 2,
                },
            ),
            Err(AmbiguityError::InvalidMinoritySupport {
                min_minority_support: 2,
                hypothesis_count: 3,
            })
        );
    }

    #[test]
    fn ambiguity_can_increase_and_then_resolve() {
        let clear = SemanticBitField::<8>::from_set_indices(&[0, 2, 4]).expect("clear");
        let conflict = SemanticBitField::<8>::from_set_indices(&[0, 3, 4]).expect("conflict");

        let previous = derive_ambiguity(&[&clear, &clear], POLICY).expect("previous");
        assert_eq!(previous.count_ones(), 0);

        let introduced =
            update_ambiguity(&previous, &[&clear, &conflict], POLICY).expect("introduced");
        assert_eq!(introduced.next.count_ones(), 2);
        assert_eq!(introduced.introduced_count(), 2);
        assert_eq!(introduced.resolved_count(), 0);
        assert_eq!(introduced.persistent_count(), 0);

        let resolved =
            update_ambiguity(&introduced.next, &[&clear, &clear], POLICY).expect("resolved");
        assert_eq!(resolved.next.count_ones(), 0);
        assert_eq!(resolved.introduced_count(), 0);
        assert_eq!(resolved.resolved_count(), 2);
        assert_eq!(resolved.persistent_count(), 0);
    }

    #[test]
    fn ambiguity_transition_tracks_persistent_coordinates() {
        let a = SemanticBitField::<8>::from_set_indices(&[0, 1]).expect("a");
        let b = SemanticBitField::<8>::from_set_indices(&[0, 2]).expect("b");
        let c = SemanticBitField::<8>::from_set_indices(&[0, 3]).expect("c");

        let previous = derive_ambiguity(&[&a, &b], POLICY).expect("previous");
        let transition =
            update_ambiguity(&previous, &[&a, &c], POLICY).expect("transition");

        assert_eq!(previous.count_ones(), 2);
        assert_eq!(transition.next.count_ones(), 2);
        assert_eq!(transition.introduced_count(), 1);
        assert_eq!(transition.resolved_count(), 1);
        assert_eq!(transition.persistent_count(), 1);
    }
}
