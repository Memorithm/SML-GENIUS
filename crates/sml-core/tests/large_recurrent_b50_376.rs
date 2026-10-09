//! Exact recurrent sparse ANF neighbor-conjunction regression at 4 semantic bits.
use sml_core::anf_large::LargeAnfGate;
use sml_core::anf_large_bank::LargeAnfBank;
use sml_core::anf_large_recurrent::{LargeBankRecurrent, LargeRecurrentError};
use sml_core::semantic::{SemanticRewriter, SemanticState};
const WIDTH: usize = 4;
fn masks(i: usize) -> Vec<u32> {
    let terms = vec![(1_u32 << i) | (1_u32 << ((i + 1) % WIDTH))];
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
