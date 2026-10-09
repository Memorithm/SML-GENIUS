//! Recurrent large ANF bank regression for depth-four.
use sml_core::anf_large::LargeAnfGate;
use sml_core::anf_large_bank::LargeAnfBank;
use sml_core::anf_large_recurrent::{LargeBankRecurrent, LargeRecurrentError};
use sml_core::semantic::{SemanticRewriter, SemanticState};
const WIDTH: usize = 8;
const BITS: u32 = 20;
const DEPTH: u32 = 4;
fn build() -> LargeBankRecurrent<WIDTH> {
    let bank = LargeAnfBank::new(
        (0..WIDTH)
            .map(|i| LargeAnfGate::new(BITS, vec![1u32 << i]).unwrap())
            .collect(),
    )
    .unwrap();
    LargeBankRecurrent::new(bank, DEPTH as usize).unwrap()
}
#[test]
fn identity_preserves_four_field_semantic_state() {
    let mut engine = build();
    let mut state = SemanticState::<WIDTH>::zeroed().unwrap();
    for i in 0..WIDTH {
        state.value.set(i, i % 3 == 0).unwrap();
        state.meaning_mask.set(i, i % 2 == 0).unwrap();
        state.ambiguity.set(i, i % 5 == 0).unwrap();
        state.routing.set(i, i % 7 == 0).unwrap();
    }
    let initial = state.clone();
    for d in 1..=DEPTH {
        state = engine.rewrite(&state, d).unwrap().state;
        assert_eq!(state, initial);
        assert_eq!(engine.history().len(), d as usize);
    }
    assert_eq!(
        engine.rewrite(&state, DEPTH + 1),
        Err(LargeRecurrentError::Depth)
    );
    assert_eq!(engine.history().len(), DEPTH as usize);
}
#[test]
fn rejected_depth_does_not_mutate_fork() {
    let mut a = build();
    let state = SemanticState::<WIDTH>::zeroed().unwrap();
    a.rewrite(&state, 1).unwrap();
    let before = a.clone();
    let mut fork = a.clone();
    assert_eq!(fork.rewrite(&state, 1), Err(LargeRecurrentError::Depth));
    assert_eq!(fork, before);
    assert_eq!(a, before);
}
