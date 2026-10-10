//! Table-driven merge of the 12 former `large_anf_boundary_*.rs` test files.
//!
//! Each `case_*!` invocation below reproduces one former file verbatim (same
//! helpers, constants, assertions and test function names) inside a module named
//! after that file; only the per-file constants are passed as arguments.

macro_rules! case {
    ($name:ident, $p0:expr, $p1:expr, $p2:expr, $p3:expr) => {
        mod $name {
            use sml_core::anf_large::{LargeAnfError, LargeAnfGate};
            const BITS: u32 = $p0;
            const TERMS: &[u32] = &[0, $p1, $p2];
            const QUERIES: &[u32] = &[0, $p3, $p1, $p2];
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
        }
    };
}

// Independent literal-AND/XOR control for boundary-13.
case!(large_anf_boundary_13, 13, 4096, 8191, 4095);
// Independent literal-AND/XOR control for boundary-16.
case!(large_anf_boundary_16, 16, 32768, 65535, 32767);
// Independent literal-AND/XOR control for boundary-17.
case!(large_anf_boundary_17, 17, 65536, 131071, 65535);
// Independent literal-AND/XOR control for boundary-18.
case!(large_anf_boundary_18, 18, 131072, 262143, 131071);
// Independent literal-AND/XOR control for boundary-19.
case!(large_anf_boundary_19, 19, 262144, 524287, 262143);
// Independent literal-AND/XOR control for boundary-20.
case!(large_anf_boundary_20, 20, 524288, 1048575, 524287);
// Independent literal-AND/XOR control for boundary-21.
case!(large_anf_boundary_21, 21, 1048576, 2097151, 1048575);
// Independent literal-AND/XOR control for boundary-22.
case!(large_anf_boundary_22, 22, 2097152, 4194303, 2097151);
// Independent literal-AND/XOR control for boundary-24.
case!(large_anf_boundary_24, 24, 8388608, 16777215, 8388607);
// Independent literal-AND/XOR control for boundary-28.
case!(large_anf_boundary_28, 28, 134217728, 268435455, 134217727);
// Independent literal-AND/XOR control for boundary-30.
case!(large_anf_boundary_30, 30, 536870912, 1073741823, 536870911);
// Independent literal-AND/XOR control for boundary-31.
case!(
    large_anf_boundary_31,
    31,
    1073741824,
    2147483647,
    1073741823
);
