//! Table-driven rejection matrix for hierarchical Boolean-credit scheduling.

use sml_core::{
    boolean_hierarchical::{
        BooleanGroupObservation, BooleanPageGroup, HierarchicalBooleanError,
        HierarchicalBooleanPolicy, HierarchicalBooleanSearch,
    },
    PageId,
};

fn policy() -> HierarchicalBooleanPolicy {
    HierarchicalBooleanPolicy {
        root_group_width: 1_024,
        leaf_width: 128,
        min_improvement_ppm: 1,
        max_next_groups: 8,
        max_depth: 16,
    }
}

#[test]
fn group_construction_rejections() {
    assert_eq!(
        BooleanPageGroup::new(PageId(1), 0, 0, 0),
        Err(HierarchicalBooleanError::ZeroGroupWidth)
    );
    assert_eq!(
        BooleanPageGroup::new(PageId(1), u64::MAX, 1, 0),
        Err(HierarchicalBooleanError::ArithmeticOverflow)
    );
    assert!(BooleanPageGroup::new(PageId(1), u64::MAX - 1, 1, 0).is_ok());
}

#[test]
fn split_rejections() {
    let single = BooleanPageGroup::new(PageId(2), 5, 1, 0).unwrap();
    assert_eq!(
        single.split(),
        Err(HierarchicalBooleanError::UnsplittableGroup(single))
    );
    let deep = BooleanPageGroup::new(PageId(2), 0, 4, u16::MAX).unwrap();
    assert_eq!(deep.split(), Err(HierarchicalBooleanError::DepthOverflow));
    let odd = BooleanPageGroup::new(PageId(2), 10, 3, 0).unwrap();
    let (l, r) = odd.split().unwrap();
    assert_eq!((l.start, l.len, r.start, r.len, r.depth), (10, 1, 11, 2, 1));
}

#[test]
fn policy_validation_matrix() {
    let base = policy();
    let cases: [(HierarchicalBooleanPolicy, HierarchicalBooleanError); 6] = [
        (
            HierarchicalBooleanPolicy {
                root_group_width: 0,
                ..base
            },
            HierarchicalBooleanError::ZeroRootGroupWidth,
        ),
        (
            HierarchicalBooleanPolicy {
                leaf_width: 0,
                ..base
            },
            HierarchicalBooleanError::ZeroLeafWidth,
        ),
        (
            HierarchicalBooleanPolicy {
                leaf_width: 2_048,
                ..base
            },
            HierarchicalBooleanError::LeafWiderThanRoot,
        ),
        (
            HierarchicalBooleanPolicy {
                min_improvement_ppm: 1_000_001,
                ..base
            },
            HierarchicalBooleanError::PpmOutOfRange(1_000_001),
        ),
        (
            HierarchicalBooleanPolicy {
                max_next_groups: 0,
                ..base
            },
            HierarchicalBooleanError::ZeroNextGroupBudget,
        ),
        (
            HierarchicalBooleanPolicy {
                max_depth: 0,
                ..base
            },
            HierarchicalBooleanError::ZeroMaximumDepth,
        ),
    ];
    for (bad, expected) in cases {
        assert_eq!(bad.validate(), Err(expected), "{bad:?}");
        assert_eq!(
            HierarchicalBooleanSearch::initial_groups(PageId(3), 64, bad),
            Err(expected)
        );
    }
    assert_eq!(base.validate(), Ok(base));
}

#[test]
fn initial_groups_rejections() {
    assert_eq!(
        HierarchicalBooleanSearch::initial_groups(PageId(4), 0, policy()),
        Err(HierarchicalBooleanError::ZeroParameters(PageId(4)))
    );
    // 8 groups fit exactly; one more parameter needs a ninth group.
    assert_eq!(
        HierarchicalBooleanSearch::initial_groups(PageId(4), 8 * 1_024, policy())
            .unwrap()
            .len(),
        8
    );
    assert_eq!(
        HierarchicalBooleanSearch::initial_groups(PageId(4), 8 * 1_024 + 1, policy()),
        Err(HierarchicalBooleanError::GroupBudgetExceeded {
            required: 9,
            maximum: 8
        })
    );
}

#[test]
fn refine_rejections() {
    let group = BooleanPageGroup::new(PageId(5), 0, 1_024, 0).unwrap();
    let ok = BooleanGroupObservation {
        group,
        baseline_loss_ppm: 500_000,
        perturbed_loss_ppm: 400_000,
    };
    assert_eq!(
        HierarchicalBooleanSearch::refine(&[], policy()).err(),
        Some(HierarchicalBooleanError::EmptyObservations)
    );
    assert_eq!(
        HierarchicalBooleanSearch::refine(&[ok, ok], policy()).err(),
        Some(HierarchicalBooleanError::DuplicateGroup(group))
    );
    for (baseline, perturbed, bad) in [(1_000_001, 0, 1_000_001), (0, 2_000_000, 2_000_000)] {
        let obs = BooleanGroupObservation {
            group,
            baseline_loss_ppm: baseline,
            perturbed_loss_ppm: perturbed,
        };
        assert_eq!(
            obs.validate(),
            Err(HierarchicalBooleanError::PpmOutOfRange(bad))
        );
        assert_eq!(
            HierarchicalBooleanSearch::refine(&[obs], policy()).err(),
            Some(HierarchicalBooleanError::PpmOutOfRange(bad))
        );
    }
    let bad_policy = HierarchicalBooleanPolicy {
        max_depth: 0,
        ..policy()
    };
    assert_eq!(
        HierarchicalBooleanSearch::refine(&[ok], bad_policy).err(),
        Some(HierarchicalBooleanError::ZeroMaximumDepth)
    );
}
