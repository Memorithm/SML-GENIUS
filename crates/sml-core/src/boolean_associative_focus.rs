//! Experimental rule-gated Boolean memory focus (BAF-0).
//!
//! This is **not** Transformer attention or binarized Q/K attention. It does not
//! construct embeddings, similarities, token-pair matrices, weights or softmax.
//! Each ANF predicate reads the *current event and recurrent state* and either
//! opens a memory page or does not. Selected pages rewrite explicit state bits.
//!
//! Research-only scalar oracle: no learned parameters, model-quality, O(1)
//! lookup or speed claims. Training/policy and physical kernels are separate.

use core::fmt;
use std::collections::BTreeSet;

use crate::PageId;

/// One Boolean monomial in independent event/state bit fields.
/// Both zero masks form the constant-one monomial.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnfMonomial {
    pub event_mask: u64,
    pub state_mask: u64,
}

/// A native GF(2) predicate: bias XOR the conjunction of each monomial.
/// Coefficients are the presence/absence of each monomial in `terms`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnfActivation {
    pub bias: bool,
    pub terms: Vec<AnfMonomial>,
}

impl AnfActivation {
    #[must_use]
    pub fn is_active(&self, event: u64, state: u64) -> bool {
        self.terms.iter().fold(self.bias, |parity, term| {
            let satisfied = event & term.event_mask == term.event_mask
                && state & term.state_mask == term.state_mask;
            parity ^ satisfied
        })
    }
}

/// A memory page with an activation rule and a masked Boolean rewrite.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssociativeFocusPage {
    pub id: PageId,
    /// Higher priority wins writes to overlapping state bits.
    pub priority: u32,
    pub activation: AnfActivation,
    /// State bits this page may overwrite (not an attention probability).
    pub write_mask: u64,
    /// Values to write in the permitted mask.
    pub write_value: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AssociativeFocusConfig {
    pub event_bits: usize,
    pub state_bits: usize,
    /// Maximum number of pages allowed to fire in a single recurrent step.
    pub max_selected_pages: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssociativeFocusStep {
    /// Ordered by descending priority then ascending page ID.
    pub selected: Vec<PageId>,
    /// State after deterministic priority-controlled writes.
    pub next_state: u64,
    /// Union of bits claimed by selected pages.
    pub claimed_mask: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssociativeFocusError {
    InvalidConfig(&'static str),
    DuplicatePage(PageId),
    PredicateOutsideWidth(PageId),
    WriteOutsideWidth(PageId),
    WriteValueOutsideMask(PageId),
    EventOutsideWidth,
    StateOutsideWidth,
}

impl fmt::Display for AssociativeFocusError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfig(message) => write!(f, "invalid Boolean focus config: {message}"),
            Self::DuplicatePage(id) => write!(f, "duplicate Boolean focus page {}", id.0),
            Self::PredicateOutsideWidth(id) => {
                write!(f, "Boolean focus page {} has out-of-range ANF bits", id.0)
            }
            Self::WriteOutsideWidth(id) => {
                write!(f, "Boolean focus page {} writes outside state width", id.0)
            }
            Self::WriteValueOutsideMask(id) => {
                write!(f, "Boolean focus page {} writes values outside mask", id.0)
            }
            Self::EventOutsideWidth => write!(f, "event bits outside configured width"),
            Self::StateOutsideWidth => write!(f, "state bits outside configured width"),
        }
    }
}

impl std::error::Error for AssociativeFocusError {}

const fn low_mask(bits: usize) -> u64 {
    if bits == 64 {
        u64::MAX
    } else {
        (1u64 << bits) - 1
    }
}

/// Immutable, deterministic reference execution of Boolean memory focus.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BooleanAssociativeFocus {
    config: AssociativeFocusConfig,
    pages: Vec<AssociativeFocusPage>,
}

impl BooleanAssociativeFocus {
    pub fn new(
        config: AssociativeFocusConfig,
        mut pages: Vec<AssociativeFocusPage>,
    ) -> Result<Self, AssociativeFocusError> {
        if !(1..=64).contains(&config.event_bits) || !(1..=64).contains(&config.state_bits) {
            return Err(AssociativeFocusError::InvalidConfig(
                "event_bits and state_bits must be 1..=64",
            ));
        }
        if config.max_selected_pages == 0 || config.max_selected_pages > pages.len() {
            return Err(AssociativeFocusError::InvalidConfig(
                "max_selected_pages must be 1..=page count",
            ));
        }
        let event_mask = low_mask(config.event_bits);
        let state_mask = low_mask(config.state_bits);
        let mut ids = BTreeSet::new();
        for page in &pages {
            if !ids.insert(page.id.0) {
                return Err(AssociativeFocusError::DuplicatePage(page.id));
            }
            if page.activation.terms.iter().any(|term| {
                term.event_mask & !event_mask != 0 || term.state_mask & !state_mask != 0
            }) {
                return Err(AssociativeFocusError::PredicateOutsideWidth(page.id));
            }
            if page.write_mask & !state_mask != 0 {
                return Err(AssociativeFocusError::WriteOutsideWidth(page.id));
            }
            if page.write_value & !page.write_mask != 0 {
                return Err(AssociativeFocusError::WriteValueOutsideMask(page.id));
            }
        }
        pages.sort_by(|a, b| {
            b.priority
                .cmp(&a.priority)
                .then_with(|| a.id.0.cmp(&b.id.0))
        });
        Ok(Self { config, pages })
    }

    #[must_use]
    pub fn config(&self) -> AssociativeFocusConfig {
        self.config
    }

    /// One recurrence step; no mutation of stored pages or caller state.
    ///
    /// O(sum of examined ANF terms + selected writes), with up to all pages
    /// examined. This is not a constant-time content-addressed index.
    pub fn step(
        &self,
        event: u64,
        state: u64,
    ) -> Result<AssociativeFocusStep, AssociativeFocusError> {
        if event & !low_mask(self.config.event_bits) != 0 {
            return Err(AssociativeFocusError::EventOutsideWidth);
        }
        if state & !low_mask(self.config.state_bits) != 0 {
            return Err(AssociativeFocusError::StateOutsideWidth);
        }

        let mut next_state = state;
        let mut claimed_mask = 0u64;
        let mut selected = Vec::new();
        for page in &self.pages {
            if !page.activation.is_active(event, state) {
                continue;
            }
            // Higher-priority pages own conflicting bits; other bits still
            // pass through lower-priority pages.
            let effective = page.write_mask & !claimed_mask;
            next_state = (next_state & !effective) | (page.write_value & effective);
            claimed_mask |= effective;
            selected.push(page.id);
            if selected.len() == self.config.max_selected_pages {
                break;
            }
        }
        Ok(AssociativeFocusStep {
            selected,
            next_state,
            claimed_mask,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(id: u32, priority: u32, terms: &[(u64, u64)], write_mask: u64, write_value: u64) -> AssociativeFocusPage {
        AssociativeFocusPage {
            id: PageId(id),
            priority,
            activation: AnfActivation {
                bias: false,
                terms: terms
                    .iter()
                    .map(|&(event_mask, state_mask)| AnfMonomial { event_mask, state_mask })
                    .collect(),
            },
            write_mask,
            write_value,
        }
    }

    fn config(max_selected_pages: usize) -> AssociativeFocusConfig {
        AssociativeFocusConfig { event_bits: 4, state_bits: 4, max_selected_pages }
    }

    #[test]
    fn anf_predicate_is_non_linear_and_uses_parity_not_or() {
        let predicate = AnfActivation {
            bias: true,
            terms: vec![
                AnfMonomial { event_mask: 0b0011, state_mask: 0b0001 },
                AnfMonomial { event_mask: 0b0010, state_mask: 0 },
            ],
        };
        // bias=true, both monomials true => true XOR true XOR true
        assert!(predicate.is_active(0b0011, 0b0001));
        // One monomial true => true XOR true => false
        assert!(!predicate.is_active(0b0010, 0b0001));
        // Neither monomial true => bias only
        assert!(predicate.is_active(0b0000, 0b0001));
    }

    #[test]
    fn independent_truth_table_exhaustive_for_four_bit_inputs() {
        let rules = vec![
            AnfActivation { bias: false, terms: vec![AnfMonomial { event_mask: 0, state_mask: 0 }] },
            AnfActivation { bias: true, terms: vec![
                AnfMonomial { event_mask: 0b1010, state_mask: 0b0101 },
                AnfMonomial { event_mask: 0b0010, state_mask: 0 },
                AnfMonomial { event_mask: 0, state_mask: 0b1000 },
            ] },
        ];
        for event in 0u64..16 {
            for state in 0u64..16 {
                for gate in &rules {
                    let mut expected = gate.bias;
                    for term in &gate.terms {
                        let mut satisfies = true;
                        for i in 0..4 {
                            if ((term.event_mask >> i) & 1 == 1 && (event >> i) & 1 == 0)
                                || ((term.state_mask >> i) & 1 == 1 && (state >> i) & 1 == 0)
                            {
                                satisfies = false;
                            }
                        }
                        expected ^= satisfies;
                    }
                    assert_eq!(gate.is_active(event, state), expected);
                }
            }
        }
    }

    #[test]
    fn priority_merges_disjoint_bits_and_resolves_conflicts() {
        let focus = BooleanAssociativeFocus::new(config(2), vec![
            page(9, 1, &[(0, 0)], 0b1100, 0b1000),
            page(2, 2, &[(0b0010, 0b0001)], 0b0100, 0b0100),
        ]).unwrap();
        let out = focus.step(0b0011, 0b0001).unwrap();
        assert_eq!(out.selected, vec![PageId(2), PageId(9)]);
        assert_eq!(out.next_state, 0b1101);
        assert_eq!(out.claimed_mask, 0b1100);
    }

    #[test]
    fn deterministic_priority_and_capacity_are_explicit() {
        let focus = BooleanAssociativeFocus::new(config(1), vec![
            page(3, 4, &[(0, 0)], 0b0010, 0b0010),
            page(1, 4, &[(0, 0)], 0b0100, 0b0100),
            page(5, 5, &[(0b1000, 0)], 0b1000, 0b1000),
        ]).unwrap();
        let out = focus.step(0b0000, 0).unwrap();
        assert_eq!(out.selected, vec![PageId(1)]);
        assert_eq!(out.next_state, 0b0100);
        assert_eq!(focus.step(0b1000, 0).unwrap().selected, vec![PageId(5)]);
    }

    #[test]
    fn no_activation_leaves_state_unchanged() {
        let focus = BooleanAssociativeFocus::new(
            config(1),
            vec![page(1, 1, &[(0b0100, 0b0010)], 0b1111, 0)],
        ).unwrap();
        let out = focus.step(0, 0b0011).unwrap();
        assert!(out.selected.is_empty());
        assert_eq!(out.next_state, 0b0011);
        assert_eq!(out.claimed_mask, 0);
    }

    #[test]
    fn malformed_config_pages_and_inputs_fail_closed() {
        assert!(matches!(
            BooleanAssociativeFocus::new(config(0), vec![page(1, 0, &[], 1, 1)]),
            Err(AssociativeFocusError::InvalidConfig(_))
        ));
        assert!(matches!(
            BooleanAssociativeFocus::new(config(2), vec![page(1, 0, &[], 1, 1), page(1, 1, &[], 1, 1)]),
            Err(AssociativeFocusError::DuplicatePage(PageId(1)))
        ));
        assert!(matches!(
            BooleanAssociativeFocus::new(config(1), vec![page(1, 0, &[(0b1_0000, 0)], 1, 1)]),
            Err(AssociativeFocusError::PredicateOutsideWidth(PageId(1)))
        ));
        assert!(matches!(
            BooleanAssociativeFocus::new(config(1), vec![page(1, 0, &[], 0b1_0000, 0)]),
            Err(AssociativeFocusError::WriteOutsideWidth(PageId(1)))
        ));
        assert!(matches!(
            BooleanAssociativeFocus::new(config(1), vec![page(1, 0, &[], 0b0001, 0b0010)]),
            Err(AssociativeFocusError::WriteValueOutsideMask(PageId(1)))
        ));
        let focus = BooleanAssociativeFocus::new(config(1), vec![page(1, 0, &[(0, 0)], 1, 1)]).unwrap();
        assert_eq!(focus.step(16, 0), Err(AssociativeFocusError::EventOutsideWidth));
        assert_eq!(focus.step(0, 16), Err(AssociativeFocusError::StateOutsideWidth));
    }
}
