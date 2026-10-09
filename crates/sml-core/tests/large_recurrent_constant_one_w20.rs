//! ANF recurrent regression for constant-one, 20 bits.
use sml_core::anf_large::LargeAnfGate;
use sml_core::anf_large_bank::LargeAnfBank;
use sml_core::anf_large_recurrent::{LargeBankRecurrent,LargeRecurrentError};
use sml_core::semantic::{SemanticRewriter,SemanticState};
#[test]
fn recurrent_value_and_context_remain_consistent() {
    const WIDTH:usize=20;
    let bank=LargeAnfBank::new((0..WIDTH).map(|i|
        LargeAnfGate::new(20,vec![{ let _ = i; 0_u32 }]).unwrap()
    ).collect()).unwrap();
    let mut engine=LargeBankRecurrent::<WIDTH>::new(bank,3).unwrap();
    let mut state=SemanticState::<WIDTH>::zeroed().unwrap();
    for i in 0..WIDTH {
        state.value.set(i,i%2==0).unwrap();
        state.meaning_mask.set(i,i%3==0).unwrap();
        state.ambiguity.set(i,i%4==0).unwrap();
        state.routing.set(i,i%5==0).unwrap();
    }
    let original=state.clone();
    for depth in 1..=3 {
        state=engine.rewrite(&state,depth).unwrap().state;
        for i in 0..WIDTH {assert_eq!(state.value.get(i).unwrap(),true);}
        assert_eq!(state.meaning_mask,original.meaning_mask);
        assert_eq!(state.ambiguity,original.ambiguity);
        assert_eq!(state.routing,original.routing);
        assert_eq!(engine.history().len(),depth as usize);
    }
    let snapshot=engine.clone();
    assert_eq!(engine.rewrite(&state,4),Err(LargeRecurrentError::Depth));
    assert_eq!(engine,snapshot);
}
