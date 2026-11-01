//! LONGCTX-B1 harness tests. Hand-written fixtures only; no campaign, no metric.

use sml_store::longctx_b1::*;

#[test]
fn b1_tokenizer_is_one_byte_one_token() {
    assert_eq!(SML_BYTE_V0, "sml-byte-v0");
    assert_eq!(sml_byte_v0_token_count(b""), 0);
    assert_eq!(sml_byte_v0_token_count("é".as_bytes()), 2);
    assert_eq!(sml_byte_v0_token_count(b"KEY=0"), 5);
}

#[test]
fn b1_seeds_splits_and_ladder_are_frozen() {
    assert_eq!(
        B1_SEEDS,
        [
            0xB100_0001,
            0xB100_0002,
            0xB100_0003,
            0xB100_0004,
            0xB100_0005
        ]
    );
    assert_eq!(B1Split::for_seed(0xB100_0001), Ok(B1Split::Dev));
    assert_eq!(B1Split::for_seed(0xB100_0002), Ok(B1Split::Dev));
    for s in [0xB100_0003, 0xB100_0004, 0xB100_0005] {
        assert_eq!(B1Split::for_seed(s), Ok(B1Split::Test));
    }
    assert_eq!(B1Split::for_seed(42), Err(B1Error::UnknownSeed(42)));
    assert_eq!(validate_span(1_048_576), Ok(1_048_576));
    assert_eq!(
        validate_span(1_000_000),
        Err(B1Error::SpanNotOnLadder(1_000_000))
    );
}
