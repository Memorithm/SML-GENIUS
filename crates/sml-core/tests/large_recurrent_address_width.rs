//! Construction must reject recurrent states wider than the ANF address domain.
use sml_core::anf_large::LargeAnfGate;
use sml_core::anf_large_bank::LargeAnfBank;
use sml_core::anf_large_recurrent::{LargeBankRecurrent, LargeRecurrentError};
use sml_core::semantic::{SemanticRewriter, SemanticState};

fn bank(bits: u32, gates: usize) -> LargeAnfBank {
    LargeAnfBank::new(
        (0..gates)
            .map(|i| LargeAnfGate::new(bits, vec![1_u32 << (i % bits as usize)]).unwrap())
            .collect(),
    )
    .unwrap()
}

#[test]
fn reject_unaddressable_recurrent_width_at_construction() {
    assert_eq!(
        LargeBankRecurrent::<8>::new(bank(7, 8), 3),
        Err(LargeRecurrentError::Width)
    );
    assert_eq!(
        LargeBankRecurrent::<32>::new(bank(31, 32), 3),
        Err(LargeRecurrentError::Width)
    );
    assert_eq!(
        LargeBankRecurrent::<0>::new(bank(1, 1), 3),
        Err(LargeRecurrentError::Width)
    );
}

#[test]
fn valid_address_width_executes_highest_semantic_bit() {
    let mut engine = LargeBankRecurrent::<8>::new(bank(8, 8), 2).unwrap();
    let mut state = SemanticState::<8>::zeroed().unwrap();
    state.value.set(7, true).unwrap();
    let next = engine.rewrite(&state, 1).unwrap().state;
    assert!(next.value.get(7).unwrap());
    assert_eq!(next, state);
    assert_eq!(engine.history().len(), 1);
}
