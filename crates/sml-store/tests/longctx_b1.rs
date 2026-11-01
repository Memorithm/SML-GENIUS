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

#[test]
fn b1_rng_is_deterministic_and_namespaces_are_disjoint() {
    let mut a = B1Rng::new(0xB100_0001);
    let mut b = B1Rng::new(0xB100_0001);
    assert_eq!((a.next_u64(), a.next_u64()), (b.next_u64(), b.next_u64()));
    let mut rng = B1Rng::new(7);
    for _ in 0..256 {
        let dev = hex_in_namespace(&mut rng, 16, B1Split::Dev);
        let test = hex_in_namespace(&mut rng, 16, B1Split::Test);
        assert!(b"01234567".contains(&dev[0]), "{dev:?}");
        assert!(b"89abcdef".contains(&test[0]), "{test:?}");
        assert!(dev.iter().chain(&test).all(|c| c.is_ascii_hexdigit()));
    }
}

#[test]
fn b1_near_miss_has_exact_hamming_distance_and_keeps_namespace() {
    let key = b"3a5f00112233c0de".to_vec();
    let mut rng = B1Rng::new(9);
    for distance in [1usize, 2] {
        for _ in 0..64 {
            let near = near_miss_key(&mut rng, &key, distance);
            let diff = key.iter().zip(&near).filter(|(x, y)| x != y).count();
            assert_eq!(diff, distance);
            assert_eq!(near[0], key[0]);
        }
    }
}
