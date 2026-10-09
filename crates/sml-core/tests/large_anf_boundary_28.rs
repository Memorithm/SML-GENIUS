//! Independent literal-AND/XOR control for boundary-28.
use sml_core::anf_large::{LargeAnfError, LargeAnfGate};
const BITS: u32 = 28;
const TERMS: &[u32] = &[0, 134217728, 268435455];
const QUERIES: &[u32] = &[0, 134217727, 134217728, 268435455];
fn literal(query: u32) -> bool {
    TERMS.iter().fold(false, |parity, &mask| {
        let mut product = true;
        for bit in 0..BITS {
            if mask & (1u32 << bit) != 0 {
                product &= query & (1u32 << bit) != 0;
            }
        }
        parity ^ product
    })
}
#[test]
fn exact_sparse_domain_oracle() {
    let gate = LargeAnfGate::new(BITS, TERMS.to_vec()).unwrap();
    assert_eq!(gate.address_bits(), BITS);
    for &query in QUERIES {
        assert_eq!(gate.evaluate(query), Ok(literal(query)));
    }
    assert_eq!(
        gate.evaluate(1u32 << BITS),
        Err(LargeAnfError::QueryOutOfRange(1u32 << BITS))
    );
}
#[test]
fn canonical_masks_and_invalid_replacements() {
    let gate = LargeAnfGate::new(BITS, TERMS.to_vec()).unwrap();
    assert!(gate.monomials().windows(2).all(|w| w[0] < w[1]));
    assert_eq!(
        LargeAnfGate::new(BITS, vec![0, 0]),
        Err(LargeAnfError::DuplicateMonomial(0))
    );
    assert_eq!(
        LargeAnfGate::new(BITS, vec![1u32 << BITS]),
        Err(LargeAnfError::MonomialOutOfRange(1u32 << BITS))
    );
}
