//! Regression tests for externally constructed BooleanPageGroup ranges.
//!
//! BooleanPageGroup fields are public, so callers may bypass the checked
//! constructor. Page views must reject malformed ranges without panicking.

use sml_core::{
    boolean_hierarchical::BooleanPageGroup,
    boolean_page::{BooleanPageError, BooleanPageRead, PackedBooleanPage},
    PageId,
};

const PAGE: PageId = PageId(7);
const COUNT: u64 = 65;

#[test]
fn forged_zero_width_and_overflow_groups_fail_closed() {
    let page = PackedBooleanPage::zeros(PAGE, COUNT).unwrap();
    let original = page.clone();
    let malformed = [
        BooleanPageGroup {
            page_id: PAGE,
            start: u64::MAX,
            len: 2,
            depth: 0,
        },
        BooleanPageGroup {
            page_id: PAGE,
            start: COUNT - 1,
            len: u64::MAX,
            depth: 0,
        },
        BooleanPageGroup {
            page_id: PAGE,
            start: 0,
            len: 0,
            depth: 0,
        },
        BooleanPageGroup {
            page_id: PAGE,
            start: COUNT,
            len: 0,
            depth: 0,
        },
        BooleanPageGroup {
            page_id: PAGE,
            start: COUNT + 1,
            len: 1,
            depth: 0,
        },
    ];
    for group in malformed {
        assert_eq!(
            page.group_view(group).unwrap_err(),
            BooleanPageError::GroupOutsidePage {
                group,
                parameter_count: COUNT,
            },
            "{group:?}"
        );
        assert_eq!(page, original, "page changed after {group:?}");
    }
}

#[test]
fn wrong_page_is_rejected_before_range_arithmetic() {
    let page = PackedBooleanPage::zeros(PAGE, COUNT).unwrap();
    let group = BooleanPageGroup {
        page_id: PageId(8),
        start: u64::MAX,
        len: 2,
        depth: 0,
    };
    assert_eq!(
        page.group_view(group).unwrap_err(),
        BooleanPageError::PageMismatch {
            expected: PAGE,
            actual: PageId(8),
        }
    );
}

#[test]
fn valid_last_bit_group_still_toggles_only_its_own_bit() {
    let mut bits = vec![false; COUNT as usize];
    bits[COUNT as usize - 1] = true;
    let page = PackedBooleanPage::from_bits(PAGE, &bits).unwrap();
    let group = BooleanPageGroup {
        page_id: PAGE,
        start: COUNT - 1,
        len: 1,
        depth: 0,
    };
    let view = page.group_view(group).unwrap();
    assert_eq!(view.bit(COUNT - 1), Ok(false));
    assert_eq!(view.bit(COUNT - 2), Ok(false));
    assert_eq!(page.bit(COUNT - 1), Ok(true));
    assert_eq!(page.generation(), 0);
}
