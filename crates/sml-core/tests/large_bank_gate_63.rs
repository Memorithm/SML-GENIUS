//! Multi-gate packed ANF control for 63 gates and a 20-bit domain.
use sml_core::anf_large::LargeAnfGate;
use sml_core::anf_large_bank::{LargeAnfBank, LargeBankError};
const BITS: u32 = 20;
const GATES: usize = 63;
fn masks(index: usize) -> Vec<u32> {
    let domain = 1u32 << BITS;
    vec![
        0,
        (index as u32 * 131 + 1) % domain,
        (index as u32 * 8191 + 3) % domain,
    ]
    .into_iter()
    .collect::<std::collections::BTreeSet<_>>()
    .into_iter()
    .collect()
}
fn oracle(mask: u32, q: u32) -> bool {
    let mut product = true;
    for bit in 0..BITS {
        if mask & (1 << bit) != 0 {
            product &= q & (1 << bit) != 0;
        }
    }
    product
}
#[test]
fn complete_packed_rows_match_independent_literal_oracle() {
    let bank = LargeAnfBank::new(
        (0..GATES)
            .map(|i| LargeAnfGate::new(BITS, masks(i)).unwrap())
            .collect(),
    )
    .unwrap();
    let limit = 1u32 << BITS;
    let queries = [0, 1, 3, limit / 2, limit - 1];
    let words = GATES.div_ceil(64);
    let packed = bank.evaluate_packed(&queries, 5, 5 * words).unwrap();
    assert_eq!(packed.len(), queries.len() * words);
    for (row, &q) in queries.iter().enumerate() {
        for gate in 0..GATES {
            let expected = masks(gate)
                .iter()
                .fold(false, |parity, &mask| parity ^ oracle(mask, q));
            assert_eq!(
                (packed[row * words + gate / 64] >> (gate % 64)) & 1 != 0,
                expected,
                "row={row} gate={gate}"
            );
        }
        for bit in GATES..words * 64 {
            assert_eq!((packed[row * words + bit / 64] >> (bit % 64)) & 1, 0);
        }
    }
}
#[test]
fn query_and_output_budgets_fail_closed() {
    let bank = LargeAnfBank::new(
        (0..GATES)
            .map(|i| LargeAnfGate::new(BITS, masks(i)).unwrap())
            .collect(),
    )
    .unwrap();
    assert_eq!(bank.gate_count(), GATES);
    assert_eq!(bank.address_bits(), BITS);
    assert_eq!(
        bank.evaluate_packed(&[0], 0, 100),
        Err(LargeBankError::BudgetExceeded)
    );
    assert_eq!(
        bank.evaluate_packed(&[1u32 << BITS], 1, 100),
        Err(LargeBankError::QueryOutOfRange(1u32 << BITS))
    );
    assert_eq!(
        bank.evaluate_packed(&[0], 1, 0),
        Err(LargeBankError::BudgetExceeded)
    );
}
