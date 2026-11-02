//! Table-driven rejection matrix for `PackedBooleanPage` (tests-only).
//!
//! Every rejected call must return the exact `BooleanPageError` variant and
//! must leave page bits and generation untouched.

use sml_core::{
    boolean_hierarchical::{
        BooleanCommitEvidence, BooleanCommitPolicy, BooleanPageGroup, HierarchicalBooleanError,
    },
    boolean_page::{BooleanPageError, PackedBooleanPage},
    PageId,
};

const PAGE: PageId = PageId(3);
const COUNT: u64 = 70;

fn improving() -> BooleanCommitEvidence {
    BooleanCommitEvidence {
        train_baseline_loss_ppm: 500_000,
        train_candidate_loss_ppm: 400_000,
        holdout_baseline_loss_ppm: 500_000,
        holdout_candidate_loss_ppm: 400_000,
    }
}

fn snapshot(page: &PackedBooleanPage) -> (u64, Vec<bool>) {
    (
        page.generation(),
        (0..page.parameter_count())
            .map(|i| page.bit(i).unwrap())
            .collect(),
    )
}

#[test]
fn constructors_reject_zero_parameters() {
    assert_eq!(
        PackedBooleanPage::zeros(PAGE, 0).unwrap_err(),
        BooleanPageError::ZeroParameters(PAGE)
    );
    assert_eq!(
        PackedBooleanPage::from_bits(PAGE, &[]).unwrap_err(),
        BooleanPageError::ZeroParameters(PAGE)
    );
}

#[test]
fn bit_rejects_out_of_range_at_boundary() {
    let page = PackedBooleanPage::zeros(PAGE, COUNT).unwrap();
    assert!(page.bit(COUNT - 1).is_ok());
    for index in [COUNT, COUNT + 1, u64::MAX] {
        assert_eq!(
            page.bit(index).unwrap_err(),
            BooleanPageError::ParameterOutOfRange {
                parameter_index: index,
                parameter_count: COUNT,
            }
        );
    }
}

#[test]
fn group_views_reject_mismatch_and_overflow() {
    let page = PackedBooleanPage::zeros(PAGE, COUNT).unwrap();
    let cases: &[(BooleanPageGroup, BooleanPageError)] = &[
        (
            BooleanPageGroup {
                page_id: PageId(4),
                start: 0,
                len: 2,
                depth: 0,
            },
            BooleanPageError::PageMismatch {
                expected: PAGE,
                actual: PageId(4),
            },
        ),
        (
            BooleanPageGroup {
                page_id: PAGE,
                start: COUNT - 1,
                len: 2,
                depth: 0,
            },
            BooleanPageError::GroupOutsidePage {
                group: BooleanPageGroup {
                    page_id: PAGE,
                    start: COUNT - 1,
                    len: 2,
                    depth: 0,
                },
                parameter_count: COUNT,
            },
        ),
        (
            BooleanPageGroup {
                page_id: PAGE,
                start: 0,
                len: COUNT + 1,
                depth: 0,
            },
            BooleanPageError::GroupOutsidePage {
                group: BooleanPageGroup {
                    page_id: PAGE,
                    start: 0,
                    len: COUNT + 1,
                    depth: 0,
                },
                parameter_count: COUNT,
            },
        ),
    ];
    for (group, expected) in cases {
        assert_eq!(page.group_view(*group).unwrap_err(), *expected, "{group:?}");
    }
    let edge = BooleanPageGroup {
        page_id: PAGE,
        start: COUNT - 2,
        len: 2,
        depth: 0,
    };
    assert!(page.group_view(edge).is_ok());
}

#[test]
fn index_flip_view_rejection_matrix() {
    let page = PackedBooleanPage::zeros(PAGE, COUNT).unwrap();
    let cases: &[(&[u64], BooleanPageError)] = &[
        (&[], BooleanPageError::EmptyCommit),
        (
            &[1, COUNT],
            BooleanPageError::ParameterOutOfRange {
                parameter_index: COUNT,
                parameter_count: COUNT,
            },
        ),
        (&[5, 2, 5], BooleanPageError::DuplicateCommitIndex(5)),
        // Out-of-range is checked before duplicates.
        (
            &[COUNT, COUNT],
            BooleanPageError::ParameterOutOfRange {
                parameter_index: COUNT,
                parameter_count: COUNT,
            },
        ),
    ];
    for (indexes, expected) in cases {
        assert_eq!(
            page.index_flip_view(indexes).unwrap_err(),
            *expected,
            "{indexes:?}"
        );
    }
}

#[test]
fn commit_rejection_matrix_leaves_page_untouched() {
    let bad_ppm = BooleanCommitEvidence {
        train_baseline_loss_ppm: 1_000_001,
        ..improving()
    };
    let bad_policy = BooleanCommitPolicy {
        min_train_improvement_ppm: 2_000_000,
        min_holdout_improvement_ppm: 0,
    };
    let cases: Vec<(
        &[u64],
        u64,
        BooleanCommitEvidence,
        BooleanCommitPolicy,
        BooleanPageError,
    )> = vec![
        (
            &[0],
            1,
            improving(),
            BooleanCommitPolicy::default(),
            BooleanPageError::StaleGeneration {
                expected: 0,
                actual: 1,
            },
        ),
        // Stale generation is checked before emptiness.
        (
            &[],
            9,
            improving(),
            BooleanCommitPolicy::default(),
            BooleanPageError::StaleGeneration {
                expected: 0,
                actual: 9,
            },
        ),
        (
            &[],
            0,
            improving(),
            BooleanCommitPolicy::default(),
            BooleanPageError::EmptyCommit,
        ),
        (
            &[0, COUNT],
            0,
            improving(),
            BooleanCommitPolicy::default(),
            BooleanPageError::ParameterOutOfRange {
                parameter_index: COUNT,
                parameter_count: COUNT,
            },
        ),
        (
            &[3, 3],
            0,
            improving(),
            BooleanCommitPolicy::default(),
            BooleanPageError::DuplicateCommitIndex(3),
        ),
        // Index validation precedes evidence validation.
        (
            &[3, 3],
            0,
            bad_ppm,
            BooleanCommitPolicy::default(),
            BooleanPageError::DuplicateCommitIndex(3),
        ),
        (
            &[1],
            0,
            bad_ppm,
            BooleanCommitPolicy::default(),
            BooleanPageError::CommitEvidence(HierarchicalBooleanError::PpmOutOfRange(1_000_001)),
        ),
        (
            &[1],
            0,
            improving(),
            bad_policy,
            BooleanPageError::CommitEvidence(HierarchicalBooleanError::PpmOutOfRange(2_000_000)),
        ),
    ];
    let mut bits = vec![false; COUNT as usize];
    bits[1] = true;
    bits[64] = true;
    let mut page = PackedBooleanPage::from_bits(PAGE, &bits).unwrap();
    let before = snapshot(&page);
    for (indexes, generation, evidence, policy, expected) in cases {
        let err = page
            .commit_verified_flips(indexes, generation, evidence, policy)
            .unwrap_err();
        assert_eq!(err, expected, "{indexes:?} gen={generation}");
        assert_eq!(
            snapshot(&page),
            before,
            "page mutated by rejected commit {indexes:?}"
        );
    }
}
