//! Checkpoint rewind and copy-on-write fork isolation for recurrent ANF.
use sml_core::anf_large::LargeAnfGate;
use sml_core::anf_large_bank::LargeAnfBank;
use sml_core::anf_large_recurrent::{LargeBankRecurrent, LargeRecurrentError};
use sml_core::semantic::{SemanticRewriter, SemanticState};

fn engine() -> LargeBankRecurrent<2> {
    let gates = vec![
        LargeAnfGate::new(20, vec![1]).unwrap(),
        LargeAnfGate::new(20, vec![2]).unwrap(),
    ];
    LargeBankRecurrent::new(LargeAnfBank::new(gates).unwrap(), 4).unwrap()
}

#[test]
fn rewind_replays_a_checkpoint_without_changing_another_fork() {
    let mut original = engine();
    let mut state = SemanticState::<2>::zeroed().unwrap();
    state.value.set(0, true).unwrap();
    state.meaning_mask.set(1, true).unwrap();
    let checkpoint_state = original.rewrite(&state, 1).unwrap().state;
    let second_state = original.rewrite(&checkpoint_state, 2).unwrap().state;
    let fork = original.clone();
    assert_eq!(original.history().len(), 2);
    original.rewind_to(1).unwrap();
    assert_eq!(original.history().len(), 1);
    assert_eq!(fork.history().len(), 2);
    let replayed = original.rewrite(&checkpoint_state, 2).unwrap().state;
    assert_eq!(replayed, second_state);
    assert_eq!(original.history(), fork.history());
}

#[test]
fn invalid_checkpoint_and_noop_leave_history_unchanged() {
    let mut original = engine();
    let state = SemanticState::<2>::zeroed().unwrap();
    original.rewrite(&state, 1).unwrap();
    let before = original.clone();
    assert_eq!(original.rewind_to(2), Err(LargeRecurrentError::Depth));
    assert_eq!(original, before);
    original.rewind_to(1).unwrap();
    assert_eq!(original, before);
    original.rewind_to(0).unwrap();
    assert!(original.history().is_empty());
    original.rewrite(&state, 1).unwrap();
    assert_eq!(original.history().len(), 1);
}
