//! Table-driven merge of the 34 former `large_recurrent_b50_*.rs` test files.
//!
//! Each `case_*!` invocation below reproduces one former file verbatim (same
//! helpers, constants, assertions and test function names) inside a module named
//! after that file; only the per-file constants are passed as arguments.

macro_rules! case_a {
    ($name:ident, $x0:ident, $x1:expr, $p0:expr) => {
        mod $name {
            use sml_core::anf_large::LargeAnfGate;
            use sml_core::anf_large_bank::LargeAnfBank;
            use sml_core::anf_large_recurrent::{LargeBankRecurrent, LargeRecurrentError};
            use sml_core::semantic::{SemanticRewriter, SemanticState};
            const WIDTH: usize = $p0;
            fn masks($x0: usize) -> Vec<u32> {
                let terms = $x1;
                terms
                    .into_iter()
                    .collect::<std::collections::BTreeSet<_>>()
                    .into_iter()
                    .collect()
            }
            fn oracle(query: u32, i: usize) -> bool {
                masks(i).iter().fold(false, |parity, &mask| {
                    let mut product = true;
                    for bit in 0..20 {
                        if mask & (1_u32 << bit) != 0 {
                            product &= query & (1_u32 << bit) != 0;
                        }
                    }
                    parity ^ product
                })
            }
            #[test]
            fn recurrent_transition_matches_literal_oracle_and_preserves_context() {
                let bank = LargeAnfBank::new(
                    (0..WIDTH)
                        .map(|i| LargeAnfGate::new(20, masks(i)).unwrap())
                        .collect(),
                )
                .unwrap();
                let mut engine = LargeBankRecurrent::<WIDTH>::new(bank, 3).unwrap();
                let mut state = SemanticState::<WIDTH>::zeroed().unwrap();
                for i in 0..WIDTH {
                    state.value.set(i, i % 3 == 0).unwrap();
                    state.meaning_mask.set(i, i % 2 == 0).unwrap();
                    state.ambiguity.set(i, i % 4 == 0).unwrap();
                    state.routing.set(i, i % 5 == 0).unwrap();
                }
                let initial = state.clone();
                for depth in 1..=3 {
                    let mut query = 0_u32;
                    for i in 0..WIDTH {
                        if state.value.get(i).unwrap() {
                            query |= 1_u32 << i;
                        }
                    }
                    state = engine.rewrite(&state, depth).unwrap().state;
                    for i in 0..WIDTH {
                        assert_eq!(state.value.get(i).unwrap(), oracle(query, i));
                    }
                    assert_eq!(state.meaning_mask, initial.meaning_mask);
                    assert_eq!(state.ambiguity, initial.ambiguity);
                    assert_eq!(state.routing, initial.routing);
                    assert_eq!(engine.history().len(), depth as usize);
                }
                let before = engine.clone();
                assert_eq!(engine.rewrite(&state, 4), Err(LargeRecurrentError::Depth));
                assert_eq!(engine, before);
            }
        }
    };
}

// Exact recurrent sparse ANF inversion regression at 1 semantic bits.
case_a!(large_recurrent_b50_374, i, vec![0, 1_u32 << i], 1);
// Exact recurrent sparse ANF neighbor-parity regression at 2 semantic bits.
case_a!(
    large_recurrent_b50_375,
    i,
    vec![1_u32 << i, 1_u32 << ((i + 1) % WIDTH)],
    2
);
// Exact recurrent sparse ANF neighbor-conjunction regression at 4 semantic bits.
case_a!(
    large_recurrent_b50_376,
    i,
    vec![(1_u32 << i) | (1_u32 << ((i + 1) % WIDTH))],
    4
);
// Exact recurrent sparse ANF mixed-monomials regression at 8 semantic bits.
case_a!(
    large_recurrent_b50_377,
    i,
    vec![0, 1_u32 << i, (1_u32 << i) | (1_u32 << ((i + 1) % WIDTH))],
    8
);
// Exact recurrent sparse ANF high-bit regression at 12 semantic bits.
case_a!(large_recurrent_b50_378, _i, vec![1_u32 << (WIDTH - 1)], 12);
// Exact recurrent sparse ANF fork-isolation regression at 16 semantic bits.
case_a!(large_recurrent_b50_379, i, vec![1_u32 << i], 16);

macro_rules! case_b {
    ($name:ident, $x0:ident, $x1:expr, $p0:expr) => {
        mod $name {
            use sml_core::anf_large::LargeAnfGate;
            use sml_core::anf_large_bank::LargeAnfBank;
            use sml_core::anf_large_recurrent::{LargeBankRecurrent, LargeRecurrentError};
            use sml_core::semantic::{SemanticRewriter, SemanticState};
            const WIDTH: usize = $p0;
            fn masks($x0: usize) -> Vec<u32> {
                $x1.into_iter()
                    .collect::<std::collections::BTreeSet<_>>()
                    .into_iter()
                    .collect()
            }
            fn oracle(q: u32, i: usize) -> bool {
                masks(i).iter().fold(false, |p, &m| p ^ (q & m == m))
            }
            #[test]
            fn independent_oracle_and_rollback() {
                let bank = LargeAnfBank::new(
                    (0..WIDTH)
                        .map(|i| LargeAnfGate::new(20, masks(i)).unwrap())
                        .collect(),
                )
                .unwrap();
                let mut engine = LargeBankRecurrent::<WIDTH>::new(bank, 3).unwrap();
                let mut state = SemanticState::<WIDTH>::zeroed().unwrap();
                for i in 0..WIDTH {
                    state.value.set(i, i % 3 == 0).unwrap();
                    state.meaning_mask.set(i, i % 2 == 0).unwrap();
                    state.ambiguity.set(i, i % 4 == 0).unwrap();
                    state.routing.set(i, i % 5 == 0).unwrap();
                }
                let original = state.clone();
                for depth in 1..=3 {
                    let mut q = 0u32;
                    for i in 0..WIDTH {
                        if state.value.get(i).unwrap() {
                            q |= 1u32 << i;
                        }
                    }
                    state = engine.rewrite(&state, depth).unwrap().state;
                    for i in 0..WIDTH {
                        assert_eq!(state.value.get(i).unwrap(), oracle(q, i));
                    }
                    assert_eq!(state.meaning_mask, original.meaning_mask);
                    assert_eq!(state.ambiguity, original.ambiguity);
                    assert_eq!(state.routing, original.routing);
                    assert_eq!(engine.history().len(), depth as usize);
                }
                let before = engine.clone();
                assert_eq!(engine.rewrite(&state, 4), Err(LargeRecurrentError::Depth));
                assert_eq!(engine, before);
            }
        }
    };
}

// Exact sparse ANF recurrent inversion control.
case_b!(large_recurrent_b50_380, i, vec![0, 1_u32 << i], 20);
// Exact sparse ANF recurrent neighbor-parity control.
case_b!(large_recurrent_b50_381, i, vec![1_u32 << i, 1_u32 << 0], 1);
// Exact sparse ANF recurrent neighbor-conjunction control.
case_b!(
    large_recurrent_b50_382,
    i,
    vec![(1_u32 << i) | (1_u32 << ((i + 1) % WIDTH))],
    2
);
// Exact sparse ANF recurrent mixed-monomials control.
case_b!(
    large_recurrent_b50_383,
    i,
    vec![0, 1_u32 << i, (1_u32 << i) | (1_u32 << ((i + 1) % WIDTH))],
    4
);
// Exact sparse ANF recurrent high-bit control.
case_b!(large_recurrent_b50_384, _i, vec![1_u32 << (WIDTH - 1)], 8);
// Exact sparse ANF recurrent fork-isolation control.
case_b!(large_recurrent_b50_385, i, vec![1_u32 << i], 12);
// Exact sparse ANF recurrent inversion control.
case_b!(large_recurrent_b50_386, i, vec![0, 1_u32 << i], 16);
// Exact sparse ANF recurrent neighbor-parity control.
case_b!(
    large_recurrent_b50_387,
    i,
    vec![1_u32 << i, 1_u32 << ((i + 1) % WIDTH)],
    20
);
// Exact sparse ANF recurrent neighbor-conjunction control.
case_b!(
    large_recurrent_b50_388,
    i,
    vec![(1_u32 << i) | (1_u32 << 0)],
    1
);
// Exact sparse ANF recurrent mixed-monomials control.
case_b!(
    large_recurrent_b50_389,
    i,
    vec![0, 1_u32 << i, (1_u32 << i) | (1_u32 << ((i + 1) % WIDTH))],
    2
);
// Exact sparse ANF recurrent high-bit control.
case_b!(large_recurrent_b50_390, _i, vec![1_u32 << (WIDTH - 1)], 4);
// Exact sparse ANF recurrent fork-isolation control.
case_b!(large_recurrent_b50_391, i, vec![1_u32 << i], 8);
// Exact sparse ANF recurrent inversion control.
case_b!(large_recurrent_b50_392, i, vec![0, 1_u32 << i], 12);
// Exact sparse ANF recurrent neighbor-parity control.
case_b!(
    large_recurrent_b50_393,
    i,
    vec![1_u32 << i, 1_u32 << ((i + 1) % WIDTH)],
    16
);
// Exact sparse ANF recurrent neighbor-conjunction control.
case_b!(
    large_recurrent_b50_394,
    i,
    vec![(1_u32 << i) | (1_u32 << ((i + 1) % WIDTH))],
    20
);
// Exact sparse ANF recurrent mixed-monomials control.
case_b!(
    large_recurrent_b50_395,
    i,
    vec![0, 1_u32 << i, (1_u32 << i) | (1_u32 << 0)],
    1
);
// Exact sparse ANF recurrent high-bit control.
case_b!(large_recurrent_b50_396, _i, vec![1_u32 << (WIDTH - 1)], 2);
// Exact sparse ANF recurrent fork-isolation control.
case_b!(large_recurrent_b50_397, i, vec![1_u32 << i], 4);
// Exact sparse ANF recurrent inversion control.
case_b!(large_recurrent_b50_398, i, vec![0, 1_u32 << i], 8);
// Exact sparse ANF recurrent neighbor-parity control.
case_b!(
    large_recurrent_b50_399,
    i,
    vec![1_u32 << i, 1_u32 << ((i + 1) % WIDTH)],
    12
);
// Exact sparse ANF recurrent neighbor-conjunction control.
case_b!(
    large_recurrent_b50_400,
    i,
    vec![(1_u32 << i) | (1_u32 << ((i + 1) % WIDTH))],
    16
);
// Exact sparse ANF recurrent mixed-monomials control.
case_b!(
    large_recurrent_b50_401,
    i,
    vec![0, 1_u32 << i, (1_u32 << i) | (1_u32 << ((i + 1) % WIDTH))],
    20
);
// Exact sparse ANF recurrent high-bit control.
case_b!(large_recurrent_b50_402, _i, vec![1_u32 << (WIDTH - 1)], 1);
// Exact sparse ANF recurrent fork-isolation control.
case_b!(large_recurrent_b50_403, i, vec![1_u32 << i], 2);
// Exact sparse ANF recurrent inversion control.
case_b!(large_recurrent_b50_404, i, vec![0, 1_u32 << i], 4);
// Exact sparse ANF recurrent neighbor-parity control.
case_b!(
    large_recurrent_b50_405,
    i,
    vec![1_u32 << i, 1_u32 << ((i + 1) % WIDTH)],
    8
);
// Exact sparse ANF recurrent neighbor-conjunction control.
case_b!(
    large_recurrent_b50_406,
    i,
    vec![(1_u32 << i) | (1_u32 << ((i + 1) % WIDTH))],
    12
);
// Exact sparse ANF recurrent mixed-monomials control.
case_b!(
    large_recurrent_b50_407,
    i,
    vec![0, 1_u32 << i, (1_u32 << i) | (1_u32 << ((i + 1) % WIDTH))],
    16
);
