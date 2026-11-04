//! CONTEXT-3 G1 search-key encoder v2 (`simhash64-sha256-v2-w<W>s<S>`).
//! Properties of the pre-registered rule only; no retrieval-quality claim.

use sml_store::g1_realtext_key::{
    simhash_v1, simhash_windows_v2, KeyError, KEY_ENCODER_V2, STRIDES_V2, WINDOW_V2,
};

fn toks(n: usize) -> Vec<String> {
    (0..n).map(|i| format!("t{i}")).collect()
}

#[test]
fn frozen_parameters() {
    assert_eq!(KEY_ENCODER_V2, "simhash64-sha256-v2");
    assert_eq!(WINDOW_V2, 8);
    assert_eq!(STRIDES_V2, [1, 8]);
}

#[test]
fn refusals() {
    assert_eq!(simhash_windows_v2::<&str>(&[], 8, 1), Err(KeyError::Empty));
    assert_eq!(simhash_windows_v2(&toks(9), 0, 1), Err(KeyError::Empty));
    assert_eq!(simhash_windows_v2(&toks(9), 8, 0), Err(KeyError::Empty));
}

#[test]
fn window_counts() {
    assert_eq!(simhash_windows_v2(&toks(64), 8, 1).unwrap().len(), 57);
    assert_eq!(simhash_windows_v2(&toks(64), 8, 8).unwrap().len(), 8);
    assert_eq!(simhash_windows_v2(&toks(20), 8, 8).unwrap().len(), 2);
    assert_eq!(simhash_windows_v2(&toks(5), 8, 1).unwrap().len(), 1);
    assert_eq!(simhash_windows_v2(&toks(8), 8, 1).unwrap().len(), 1);
}

#[test]
fn each_window_is_the_v1_key_of_that_window() {
    let t = toks(30);
    let keys = simhash_windows_v2(&t, 8, 1).unwrap();
    for (o, k) in keys.iter().enumerate() {
        assert_eq!(*k, simhash_v1(&t[o..o + 8]).unwrap());
    }
    let short = simhash_windows_v2(&t[..5], 8, 1).unwrap();
    assert_eq!(short, vec![simhash_v1(&t[..5]).unwrap()]);
}

#[test]
fn stride_one_contains_every_query_window() {
    // Any 8-token window inside the list is indexed exactly (distance 0).
    let t = toks(64);
    let keys = simhash_windows_v2(&t, 8, 1).unwrap();
    for o in 0..=56 {
        assert!(keys.contains(&simhash_v1(&t[o..o + 8]).unwrap()));
    }
}
