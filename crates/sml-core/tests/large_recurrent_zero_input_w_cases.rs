//! Table-driven merge of the 7 former `large_recurrent_zero_input_w_*.rs` test files.
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
            const WIDTH: usize = $p0;
            #[test]
            fn zero_input_keeps_value_clear_and_context_intact() {
                let bank = LargeAnfBank::new(
                    (0..WIDTH)
                        .map(|i| LargeAnfGate::new(20, vec![1_u32 << i]).unwrap())
                        .collect(),
                )
                .unwrap();
                let mut engine = LargeBankRecurrent::<WIDTH>::new(bank, 3).unwrap();
                let mut state = SemanticState::<WIDTH>::zeroed().unwrap();
                for i in 0..WIDTH {
                    state.meaning_mask.set(i, i % 2 == 0).unwrap();
                    state.ambiguity.set(i, i % 3 == 0).unwrap();
                    state.routing.set(i, i % 5 == 0).unwrap();
                }
                let initial = state.clone();
                for depth in 1..=3 {
                    state = engine.rewrite(&state, depth).unwrap().state;
                    assert_eq!(state, initial);
                    assert_eq!(engine.history().len(), depth as usize);
                }
                let before = engine.clone();
                assert_eq!(engine.rewrite(&state, 4), Err(LargeRecurrentError::Depth));
                assert_eq!(engine, before);
            }
        }
    };
}

// Independent recurrent sparse-ANF regression: zero-input, 1 semantic bits.
case!(large_recurrent_zero_input_w1, 1);
// Independent recurrent sparse-ANF regression: zero-input, 2 semantic bits.
case!(large_recurrent_zero_input_w2, 2);
// Independent recurrent sparse-ANF regression: zero-input, 4 semantic bits.
case!(large_recurrent_zero_input_w4, 4);
// Independent recurrent sparse-ANF regression: zero-input, 8 semantic bits.
case!(large_recurrent_zero_input_w8, 8);
// Independent recurrent sparse-ANF regression: zero-input, 12 semantic bits.
case!(large_recurrent_zero_input_w12, 12);
// Independent recurrent sparse-ANF regression: zero-input, 16 semantic bits.
case!(large_recurrent_zero_input_w16, 16);
// Independent recurrent sparse-ANF regression: zero-input, 20 semantic bits.
case!(large_recurrent_zero_input_w20, 20);
