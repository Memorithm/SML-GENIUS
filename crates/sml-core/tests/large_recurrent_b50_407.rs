//! Exact sparse ANF recurrent mixed-monomials control.
use sml_core::anf_large::LargeAnfGate;
use sml_core::anf_large_bank::LargeAnfBank;
use sml_core::anf_large_recurrent::{LargeBankRecurrent, LargeRecurrentError};
use sml_core::semantic::{SemanticRewriter, SemanticState};
const WIDTH: usize = 16;
fn masks(i: usize) -> Vec<u32> {
    vec![0, 1_u32 << i, (1_u32 << i) | (1_u32 << ((i + 1) % WIDTH))]
        .into_iter()
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
