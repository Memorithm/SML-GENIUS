//! Native role/value semantic composition for Semantic Boolean Geometry.
//!
//! Composition deliberately keeps two Boolean views:
//! - `content`: order-invariant concept content;
//! - `relational`: role-bound structure.
//!
//! This separation lets SBG preserve concept relatedness while retaining
//! subject/action/object distinctions. Negation is an explicit reversible
//! Boolean transform of the relational view; no floating-point score is used.

use core::fmt;

use super::{bind, bundle_majority, SemanticAlgebraError, SemanticBitField};

/// Named structural role used when validating a role set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticRoleKind {
    Subject,
    Action,
    Object,
    Negation,
}

/// Explicit Boolean role signatures used by relation composition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticRoleSet<const BITS: usize> {
    pub subject: SemanticBitField<BITS>,
    pub action: SemanticBitField<BITS>,
    pub object: SemanticBitField<BITS>,
    pub negation: SemanticBitField<BITS>,
}

impl<const BITS: usize> SemanticRoleSet<BITS> {
    /// Builds a fail-closed role set.
    ///
    /// Role signatures must be non-zero and pairwise distinct. This avoids
    /// silently aliasing structural positions in the reference semantics.
    pub fn new(
        subject: SemanticBitField<BITS>,
        action: SemanticBitField<BITS>,
        object: SemanticBitField<BITS>,
        negation: SemanticBitField<BITS>,
    ) -> Result<Self, SemanticCompositionError> {
        let roles = [
            (SemanticRoleKind::Subject, &subject),
            (SemanticRoleKind::Action, &action),
            (SemanticRoleKind::Object, &object),
            (SemanticRoleKind::Negation, &negation),
        ];

        for (kind, role) in roles {
            if role.count_ones() == 0 {
                return Err(SemanticCompositionError::EmptyRole(kind));
            }
        }

        let roles = [
            (SemanticRoleKind::Subject, &subject),
            (SemanticRoleKind::Action, &action),
            (SemanticRoleKind::Object, &object),
            (SemanticRoleKind::Negation, &negation),
        ];
        for left in 0..roles.len() {
            for right in (left + 1)..roles.len() {
                if roles[left].1 == roles[right].1 {
                    return Err(SemanticCompositionError::RoleCollision {
                        left: roles[left].0,
                        right: roles[right].0,
                    });
                }
            }
        }

        Ok(Self {
            subject,
            action,
            object,
            negation,
        })
    }
}

/// Explicit polarity of a composed relation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticPolarity {
    Affirmed,
    Negated,
}

/// Two-view Boolean composition.
///
/// `content` is invariant to permutations of the same three concept fields.
/// `relational` depends on which concept is bound to each role.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticComposition<const BITS: usize> {
    pub content: SemanticBitField<BITS>,
    pub relational: SemanticBitField<BITS>,
    pub polarity: SemanticPolarity,
}

impl<const BITS: usize> SemanticComposition<BITS> {
    /// Exact Hamming difference between the content views.
    #[must_use]
    pub fn content_distance(&self, other: &Self) -> u64 {
        self.content.xor_popcount(&other.content)
    }

    /// Exact Hamming difference between the relational views.
    #[must_use]
    pub fn relational_distance(&self, other: &Self) -> u64 {
        self.relational.xor_popcount(&other.relational)
    }

    /// Toggles negation using the declared role-set negation signature.
    ///
    /// Applying this operation twice restores the original relational field
    /// exactly. Concept content is intentionally untouched.
    pub fn toggle_negation(&mut self, roles: &SemanticRoleSet<BITS>) {
        self.relational.xor_assign(&roles.negation);
        self.polarity = match self.polarity {
            SemanticPolarity::Affirmed => SemanticPolarity::Negated,
            SemanticPolarity::Negated => SemanticPolarity::Affirmed,
        };
    }
}

/// Composes one subject/action/object relation.
///
/// The content view is a strict-majority bundle of the unbound concept fields.
/// The relational view is a strict-majority bundle of independently role-bound
/// fields. Role reversal therefore preserves the content view while normally
/// changing the relational view. This reference contract does not claim that
/// every possible Boolean fixture is collision-free.
pub fn compose_relation<const BITS: usize>(
    roles: &SemanticRoleSet<BITS>,
    subject: &SemanticBitField<BITS>,
    action: &SemanticBitField<BITS>,
    object: &SemanticBitField<BITS>,
    polarity: SemanticPolarity,
) -> Result<SemanticComposition<BITS>, SemanticCompositionError> {
    let content = bundle_majority(&[subject, action, object])?;

    let subject_bound = bind(&roles.subject, subject)?;
    let action_bound = bind(&roles.action, action)?;
    let object_bound = bind(&roles.object, object)?;
    let mut relational = bundle_majority(&[&subject_bound, &action_bound, &object_bound])?;

    if polarity == SemanticPolarity::Negated {
        relational.xor_assign(&roles.negation);
    }

    Ok(SemanticComposition {
        content,
        relational,
        polarity,
    })
}

/// Invalid semantic-composition request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticCompositionError {
    /// Structural role signature contains no active coordinate.
    EmptyRole(SemanticRoleKind),
    /// Two structural roles use the same Boolean signature.
    RoleCollision {
        left: SemanticRoleKind,
        right: SemanticRoleKind,
    },
    /// Lower-level Boolean algebra contract failed.
    Algebra(SemanticAlgebraError),
}

impl fmt::Display for SemanticCompositionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for SemanticCompositionError {}

impl From<SemanticAlgebraError> for SemanticCompositionError {
    fn from(value: SemanticAlgebraError) -> Self {
        Self::Algebra(value)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        compose_relation, SemanticCompositionError, SemanticPolarity, SemanticRoleKind,
        SemanticRoleSet,
    };
    use crate::semantic::SemanticBitField;

    fn roles() -> SemanticRoleSet<128> {
        SemanticRoleSet::new(
            SemanticBitField::from_set_indices(&[1, 7, 19, 67]).expect("subject role"),
            SemanticBitField::from_set_indices(&[2, 11, 41, 83]).expect("action role"),
            SemanticBitField::from_set_indices(&[3, 13, 53, 101]).expect("object role"),
            SemanticBitField::from_set_indices(&[5, 17, 61, 109]).expect("negation role"),
        )
        .expect("valid roles")
    }

    #[test]
    fn role_reversal_preserves_content_but_changes_structure() {
        let roles = roles();
        let cat = SemanticBitField::<128>::from_set_indices(&[
            0, 4, 8, 12, 16, 20, 64, 68, 72, 76, 80, 84,
        ])
        .expect("cat");
        let chase = SemanticBitField::<128>::from_set_indices(&[
            0, 4, 9, 13, 17, 21, 64, 68, 73, 77, 81, 85,
        ])
        .expect("chase");
        let mouse = SemanticBitField::<128>::from_set_indices(&[
            0, 5, 8, 12, 18, 22, 64, 69, 72, 76, 82, 86,
        ])
        .expect("mouse");

        let cat_chases_mouse =
            compose_relation(&roles, &cat, &chase, &mouse, SemanticPolarity::Affirmed)
                .expect("composition");
        let mouse_chases_cat =
            compose_relation(&roles, &mouse, &chase, &cat, SemanticPolarity::Affirmed)
                .expect("composition");

        assert_eq!(cat_chases_mouse.content_distance(&mouse_chases_cat), 0);
        assert_eq!(cat_chases_mouse.content, mouse_chases_cat.content);
        assert_ne!(cat_chases_mouse.relational, mouse_chases_cat.relational);
        assert!(cat_chases_mouse.relational_distance(&mouse_chases_cat) > 0);
    }

    #[test]
    fn negation_changes_relation_without_changing_content_and_is_reversible() {
        let roles = roles();
        let subject = SemanticBitField::<128>::from_set_indices(&[0, 1, 32, 64]).expect("subject");
        let action = SemanticBitField::<128>::from_set_indices(&[0, 2, 33, 65]).expect("action");
        let object = SemanticBitField::<128>::from_set_indices(&[0, 3, 34, 66]).expect("object");

        let affirmed =
            compose_relation(&roles, &subject, &action, &object, SemanticPolarity::Affirmed)
                .expect("affirmed");
        let negated =
            compose_relation(&roles, &subject, &action, &object, SemanticPolarity::Negated)
                .expect("negated");

        assert_eq!(affirmed.content, negated.content);
        assert_ne!(affirmed.relational, negated.relational);
        assert_eq!(
            affirmed.relational_distance(&negated),
            roles.negation.count_ones()
        );

        let mut toggled = affirmed.clone();
        toggled.toggle_negation(&roles);
        assert_eq!(toggled, negated);
        toggled.toggle_negation(&roles);
        assert_eq!(toggled, affirmed);
    }

    #[test]
    fn invalid_role_sets_fail_closed() {
        let empty = SemanticBitField::<64>::zeroed().expect("empty");
        let role_a = SemanticBitField::<64>::from_set_indices(&[1, 5]).expect("role a");
        let role_b = SemanticBitField::<64>::from_set_indices(&[2, 6]).expect("role b");
        let role_c = SemanticBitField::<64>::from_set_indices(&[3, 7]).expect("role c");

        assert_eq!(
            SemanticRoleSet::new(
                empty,
                role_a.clone(),
                role_b.clone(),
                role_c.clone()
            ),
            Err(SemanticCompositionError::EmptyRole(
                SemanticRoleKind::Subject
            ))
        );

        assert_eq!(
            SemanticRoleSet::new(role_a.clone(), role_a, role_b, role_c),
            Err(SemanticCompositionError::RoleCollision {
                left: SemanticRoleKind::Subject,
                right: SemanticRoleKind::Action,
            })
        );
    }
}
