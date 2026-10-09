//! Regression for constant-one sparse ANF gates in the recurrent semantic bridge.
use sml_core::anf_large::LargeAnfGate;
use sml_core::anf_large_bank::LargeAnfBank;
use sml_core::anf_large_recurrent::{LargeBankRecurrent, LargeRecurrentError};
use sml_core::semantic::{SemanticRewriter, SemanticState};

#[test]
fn constant_one_gate_updates_value_without_touching_context() {
    let bank = LargeAnfBank::new(vec![
        LargeAnfGate::new(20, vec![0]).unwrap(),
        LargeAnfGate::new(20, vec![0]).unwrap(),
        LargeAnfGate::new(20, vec![0]).unwrap(),
        LargeAnfGate::new(20, vec![0]).unwrap(),
    ]).unwrap();
    let mut recurrent = LargeBankRecurrent::<4>::new(bank, 3).unwrap();
    let mut state = SemanticState::<4>::zeroed().unwrap();
    state.meaning_mask.set(1, true).unwrap();
    state.ambiguity.set(2, true).unwrap();
    state.routing.set(3, true).unwrap();
    let context = state.clone();
    for depth in 1..=3 {
        let step = recurrent.rewrite(&state, depth).unwrap();
        state = step.state;
        for bit in 0..4 {
            assert!(state.value.get(bit).unwrap());
        }
        assert_eq!(state.meaning_mask, context.meaning_mask);
        assert_eq!(state.ambiguity, context.ambiguity);
        assert_eq!(state.routing, context.routing);
        assert_eq!(recurrent.history().len(), depth as usize);
    }
    let before = recurrent.clone();
    assert_eq!(recurrent.rewrite(&state, 4), Err(LargeRecurrentError::Depth));
    assert_eq!(recurrent, before);
}

#[test]
fn invalid_depth_does_not_mutate_cloned_recurrent_history() {
    let bank = LargeAnfBank::new(vec![LargeAnfGate::new(20, vec![0]).unwrap()]).unwrap();
    let mut original = LargeBankRecurrent::<1>::new(bank, 2).unwrap();
    let state = SemanticState::<1>::zeroed().unwrap();
    original.rewrite(&state, 1).unwrap();
    let mut fork = original.clone();
    assert_eq!(fork.rewrite(&state, 1), Err(LargeRecurrentError::Depth));
    assert_eq!(fork, original);
    fork.rewrite(&state, 2).unwrap();
    assert_eq!(original.history().len(), 1);
    assert_eq!(fork.history().len(), 2);
}
