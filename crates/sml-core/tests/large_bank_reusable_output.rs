//! Reusable packed ANF output: exactness and fail-closed buffer contract.
use sml_core::anf_large::LargeAnfGate;
use sml_core::anf_large_bank::{LargeAnfBank, LargeBankError};

fn bank() -> LargeAnfBank {
    LargeAnfBank::new(
        (0..65)
            .map(|i| LargeAnfGate::new(20, vec![0, 1_u32 << (i % 20)]).unwrap())
            .collect(),
    )
    .unwrap()
}

#[test]
fn reusable_output_matches_allocating_path_and_clears_stale_bits() {
    let bank = bank();
    let queries = [0_u32, 1, 3, 65536, 1048575];
    let expected = bank.evaluate_packed(&queries, 5, 10).unwrap();
    let mut output = vec![u64::MAX; 12];
    assert_eq!(bank.evaluate_packed_into(&queries, 5, &mut output), Ok(10));
    assert_eq!(&output[..10], expected.as_slice());
    assert_eq!(&output[10..], &[u64::MAX, u64::MAX]);
    let second = [1048575_u32, 0];
    let expected_second = bank.evaluate_packed(&second, 2, 4).unwrap();
    assert_eq!(bank.evaluate_packed_into(&second, 2, &mut output), Ok(4));
    assert_eq!(&output[..4], expected_second.as_slice());
    assert_eq!(&output[4..], &expected[4..10].iter().copied().chain([u64::MAX, u64::MAX]).collect::<Vec<_>>()[..]);
}

#[test]
fn validation_errors_never_mutate_destination() {
    let bank = bank();
    let mut output = [0xDEADBEEF_u64; 4];
    let before = output;
    assert_eq!(bank.evaluate_packed_into(&[0, 1048576], 2, &mut output), Err(LargeBankError::QueryOutOfRange(1048576)));
    assert_eq!(output, before);
    assert_eq!(bank.evaluate_packed_into(&[0, 1, 3], 3, &mut output), Err(LargeBankError::BudgetExceeded));
    assert_eq!(output, before);
    assert_eq!(bank.evaluate_packed_into(&[0], 0, &mut output), Err(LargeBankError::BudgetExceeded));
    assert_eq!(output, before);
}
