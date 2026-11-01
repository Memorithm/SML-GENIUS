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

/// Hand-written fixture filler: lowercase words and spaces only, so no 8-hex
/// run can occur.
const FILLER: &[u8] = b"the slow river runs past quiet hills and old stone walls ";

fn filler() -> FixtureFiller {
    FixtureFiller { text: FILLER }
}

#[test]
fn b1_abort_when_needle_appears_in_filler() {
    let poisoned = b"prefix KEY=3a5f00112233c0de suffix";
    assert_eq!(
        ensure_absent_from_filler(poisoned, &[b"3a5f00112233c0de"]),
        Err(B1Error::NeedleInFiller { offset: 11 })
    );
    assert_eq!(
        ensure_absent_from_filler(FILLER, &[b"3a5f00112233c0de"]),
        Ok(())
    );
}

#[test]
fn b1_splice_hits_exact_span_and_reports_offsets() {
    let line = b"KEY=0000000000000000 VALUE=11111111111111111111111111111111\n".to_vec();
    let (hay, ins) = splice_haystack(
        &filler(),
        0xB100_0001,
        B1Split::Dev,
        1000,
        vec![(500, line.clone())],
    )
    .unwrap();
    assert_eq!(hay.len(), 1000);
    assert_eq!(ins.len(), 1);
    let off = ins[0].offset as usize;
    assert_eq!(off, 500 * (1000 - line.len()) / 1000);
    assert_eq!(&hay[off..off + line.len()], line.as_slice());
    assert_eq!(
        splice_haystack(&filler(), 0xB100_0001, B1Split::Dev, 10, vec![(0, line)]),
        Err(B1Error::SpanTooSmall {
            span: 10,
            inserted: 60
        })
    );
}

#[test]
fn b1_fixture_filler_is_never_campaign_pinned() {
    assert_eq!(filler().pinned_sha256(), None);
    assert_eq!(
        verify_filler_pin(&filler(), FILLER),
        Err(B1Error::FillerNotPinned)
    );
}

#[test]
fn b1_task1_generator_is_deterministic_and_well_formed() {
    let f = filler();
    let a = generate_task1(&f, 0xB100_0003, 4096, 250, 0).unwrap();
    let b = generate_task1(&f, 0xB100_0003, 4096, 250, 0).unwrap();
    assert_eq!(a, b);
    assert_eq!(a.split, B1Split::Test);
    assert_eq!(a.haystack.len(), 4096);
    assert_eq!(a.insertions.len(), 1 + TASK1_DISTRACTORS);
    assert_eq!((a.query_key.len(), a.answer.len()), (16, 32));
    let needle = needle_line(&a.query_key, &a.answer);
    let hits = a
        .haystack
        .windows(needle.len())
        .filter(|w| *w == needle.as_slice())
        .count();
    assert_eq!(hits, 1);
    let key_line = [b"KEY=".as_slice(), &a.query_key, b" "].concat();
    let key_hits = a
        .haystack
        .windows(key_line.len())
        .filter(|w| *w == key_line.as_slice())
        .count();
    assert_eq!(
        key_hits, 1,
        "near-miss distractors must not equal the query key"
    );
    assert_ne!(a, generate_task1(&f, 0xB100_0003, 4096, 250, 1).unwrap());
    assert_eq!(
        generate_task1(&f, 0xB100_0003, 4096, 300, 0).unwrap_err(),
        B1Error::InvalidDepth(300)
    );
}
