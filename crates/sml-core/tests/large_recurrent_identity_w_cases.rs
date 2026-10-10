//! Table-driven merge of the 4 former `large_recurrent_identity_w_*.rs` test files.
//!
//! Each `case_*!` invocation below reproduces one former file verbatim (same
//! helpers, constants, assertions and test function names) inside a module named
//! after that file; only the per-file constants are passed as arguments.

macro_rules! case {
    ($name:ident, $p0:expr) => {
        mod $name {
            use sml_core::anf_large::LargeAnfGate;
            use sml_core::anf_large_bank::LargeAnfBank;
            use sml_core::anf_large_recurrent::{LargeBankRecurrent, LargeRecurrentError};
            use sml_core::semantic::{SemanticRewriter, SemanticState};
            #[test]
            fn recurrent_value_and_context_remain_consistent() {
                const WIDTH: usize = $p0;
                let bank = LargeAnfBank::new(
                    (0..WIDTH)
                        .map(|i| LargeAnfGate::new(20, vec![1_u32 << i]).unwrap())
                        .collect(),
                )
                .unwrap();
                let mut engine = LargeBankRecurrent::<WIDTH>::new(bank, 3).unwrap();
                let mut state = SemanticState::<WIDTH>::zeroed().unwrap();
                for i in 0..WIDTH {
                    state.value.set(i, i % 2 == 0).unwrap();
                    state.meaning_mask.set(i, i % 3 == 0).unwrap();
                    state.ambiguity.set(i, i % 4 == 0).unwrap();
                    state.routing.set(i, i % 5 == 0).unwrap();
                }
                let original = state.clone();
                for depth in 1..=3 {
                    state = engine.rewrite(&state, depth).unwrap().state;
                    for i in 0..WIDTH {
                        assert_eq!(state.value.get(i).unwrap(), i % 2 == 0);
                    }
                    assert_eq!(state.meaning_mask, original.meaning_mask);
                    assert_eq!(state.ambiguity, original.ambiguity);
                    assert_eq!(state.routing, original.routing);
                    assert_eq!(engine.history().len(), depth as usize);
                }
                let snapshot = engine.clone();
                assert_eq!(engine.rewrite(&state, 4), Err(LargeRecurrentError::Depth));
                assert_eq!(engine, snapshot);
            }
        }
    };
}

// ANF recurrent regression for identity, 1 bits.
case!(large_recurrent_identity_w1, 1);
// ANF recurrent regression for identity, 2 bits.
case!(large_recurrent_identity_w2, 2);
// ANF recurrent regression for identity, 4 bits.
case!(large_recurrent_identity_w4, 4);
// ANF recurrent regression for identity, 8 bits.
case!(large_recurrent_identity_w8, 8);
