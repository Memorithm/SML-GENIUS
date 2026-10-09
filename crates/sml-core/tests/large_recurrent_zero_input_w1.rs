//! Independent recurrent sparse-ANF regression: zero-input, 1 semantic bits.
use sml_core::anf_large::LargeAnfGate;
use sml_core::anf_large_bank::LargeAnfBank;
use sml_core::anf_large_recurrent::{LargeBankRecurrent, LargeRecurrentError};
use sml_core::semantic::{SemanticRewriter, SemanticState};
const WIDTH: usize = 1;
#[test]
fn zero_input_keeps_value_clear_and_context_intact() {
    let bank = LargeAnfBank::new((0..WIDTH).map(|i| {
        LargeAnfGate::new(20, vec![1_u32 << i]).unwrap()
    }).collect()).unwrap();
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
    assert_eq!(
        engine.rewrite(&state, 4),
        Err(LargeRecurrentError::Depth)
    );
    assert_eq!(engine, before);
}
