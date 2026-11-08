//! CONTEXT-2 G7: integrity envelope (`SWE1`) around SWC2 v1 snapshots.
//!
//! Pinned vectors were computed outside this codebase (Python `hashlib`
//! SHA-256 over `SWE1 | u16 1 | u8 1 | u8 0 | u32 len | payload`) for the four
//! frozen G5 fixtures. The test-side oracle parses envelopes from the spec
//! without calling the production `open`.
//!
//! The digest is **unkeyed**: these tests show corruption detection, not
//! authentication. A recomputed digest over a tampered payload passes the
//! envelope and must then be stopped (or not) by the wire decoder and
//! `WorkingContext::restore`. `NOT_MEASURED`.

use std::path::PathBuf;

use sml_core::context_ledger::{ContextContentHash, ContextSegmentId, ContextStreamId};
use sml_core::elastic_ngram_paged_sha::sha256;
use sml_store::working_context::{WorkingContext, WorkingPolicy, WorkingSegment};
use sml_store::working_context_envelope::{
    open, open_and_restore, open_with_max, restore_into_sealed, seal, seal_context, EnvelopeError,
    MAX_ENVELOPE_BYTES,
};
use sml_store::working_context_wire::{encode_context, SnapshotWireError, MAX_SNAPSHOT_BYTES};

struct Vector {
    name: &'static str,
    envelope_len: usize,
    digest_hex: &'static str,
    envelope_sha256_hex: &'static str,
}

const VECTORS: &[Vector] = &[
    Vector {
        name: "fixed_cap3_empty",
        envelope_len: 137,
        digest_hex: "06316162a375fd7ca819082261269762c7cf22da22ac90ee94089bcdab62c3c2",
        envelope_sha256_hex: "4377f3f279d37d9f15066e6a85d47173011e71e9d2c14b705008450995096d84",
    },
    Vector {
        name: "fixed_cap2_lost",
        envelope_len: 465,
        digest_hex: "334e7a3c5c6aaeb00272cf010ee366e8dae897b30fa6b993275ad6236d3d672a",
        envelope_sha256_hex: "c35714a5be00503ded1eec9b9d487c02e8bb8e2cb48b84923d205b68a7b379ae",
    },
    Vector {
        name: "expanding_hm4_partial",
        envelope_len: 383,
        digest_hex: "21de68dcc2b03d5ca22290cb0bc3f224449bfc64831b96879b5f48f1f745a317",
        envelope_sha256_hex: "bda5d38c9b5cf7fa64f81a87e422811c402d7186634c72dbf43805b9c5211414",
    },
    Vector {
        name: "capacity_bounded_cap3_evicting",
        envelope_len: 547,
        digest_hex: "4ed6cff76b0c238c2a642e70235cdac6f0bbfac54691b7ccbffb73f5fe4d52d2",
        envelope_sha256_hex: "876137e7f2e4276c5bf59a5c07856d80d96f83a7737927cbee2e7105ff8a2281",
    },
];

fn fixture(name: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/working_context_wire")
        .join(format!("{name}.hex"));
    let text = std::fs::read_to_string(path).expect("fixture");
    let mut out = Vec::new();
    for line in text.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        for k in (0..line.len()).step_by(2) {
            out.push(u8::from_str_radix(&line[k..k + 2], 16).unwrap());
        }
    }
    out
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Spec-level oracle: returns the payload range when the envelope is valid.
fn oracle_open(b: &[u8]) -> Option<(usize, usize)> {
    if b.len() > MAX_ENVELOPE_BYTES || b.len() < 12 || &b[..4] != b"SWE1" {
        return None;
    }
    if u16::from_le_bytes([b[4], b[5]]) != 1 || b[6] != 1 || b[7] != 0 {
        return None;
    }
    let len = u32::from_le_bytes(b[8..12].try_into().unwrap()) as usize;
    if len > MAX_SNAPSHOT_BYTES || b.len() != 12 + len + 32 {
        return None;
    }
    if sha256(&b[..12 + len])[..] != b[12 + len..] {
        return None;
    }
    Some((12, 12 + len))
}

fn sealed_corpus() -> Vec<(&'static str, Vec<u8>, Vec<u8>)> {
    VECTORS
        .iter()
        .map(|v| {
            let p = fixture(v.name);
            let e = seal(&p).unwrap();
            (v.name, p, e)
        })
        .collect()
}

fn live(admissions: u64) -> WorkingContext {
    let stream = ContextStreamId::new(0x4737_0000_0000_0001).unwrap();
    let mut ctx =
        WorkingContext::new(stream, WorkingPolicy::CapacityBounded { capacity: 3 }).unwrap();
    for i in 1..=admissions {
        let mut h = [0_u8; 32];
        h[0] = i as u8;
        let seg = WorkingSegment::new(
            ContextSegmentId::new(stream, 500 + i).unwrap(),
            3 * i,
            ContextContentHash::from_sha256(h),
            i ^ 0x55,
        )
        .unwrap();
        ctx.admit(seg).unwrap();
    }
    ctx
}

#[test]
fn seal_matches_independent_pinned_vectors() {
    for v in VECTORS {
        let e = seal(&fixture(v.name)).unwrap();
        assert_eq!(e.len(), v.envelope_len, "{}", v.name);
        assert_eq!(hex(&e[e.len() - 32..]), v.digest_hex, "{}", v.name);
        assert_eq!(hex(&sha256(&e)), v.envelope_sha256_hex, "{}", v.name);
        assert!(oracle_open(&e).is_some(), "{}", v.name);
    }
}

#[test]
fn open_returns_exact_payload_and_restores_without_replay() {
    for (name, payload, env) in sealed_corpus() {
        assert_eq!(open(&env).unwrap(), &payload[..], "{name}");
        let ctx = open_and_restore(&env).unwrap();
        assert_eq!(encode_context(&ctx).unwrap(), payload, "{name}");
        assert_eq!(seal_context(&ctx).unwrap(), env, "{name}: reseal drift");
    }
    let old = seal_context(&live(2)).unwrap();
    let ctx = open_and_restore(&old).unwrap();
    assert_eq!(ctx.snapshot().stats.admitted, 2, "admission replayed");
}

#[test]
fn every_prefix_and_trailing_extension_is_rejected() {
    for (name, _, env) in sealed_corpus() {
        for k in 0..env.len() {
            assert!(oracle_open(&env[..k]).is_none());
            assert!(
                matches!(open(&env[..k]), Err(EnvelopeError::Truncated { .. })),
                "{name} prefix {k}: {:?}",
                open(&env[..k])
            );
        }
        for extra in [1_usize, 32] {
            let mut m = env.clone();
            m.extend(std::iter::repeat_n(0_u8, extra));
            assert_eq!(
                open(&m),
                Err(EnvelopeError::TrailingBytes { extra }),
                "{name}"
            );
        }
    }
}

#[test]
fn every_single_bit_flip_is_detected_and_agrees_with_oracle() {
    for (name, _, env) in sealed_corpus() {
        for pos in 0..env.len() {
            for bit in 0..8 {
                let mut m = env.clone();
                m[pos] ^= 1 << bit;
                assert!(oracle_open(&m).is_none(), "{name} {pos}.{bit}");
                assert!(open(&m).is_err(), "{name} {pos}.{bit}: accepted");
            }
        }
    }
}

#[test]
fn unknown_version_algorithm_and_reserved_are_rejected_before_digest() {
    for (name, _, env) in sealed_corpus() {
        for v in [0_u16, 2, 0xFFFF] {
            let mut m = env.clone();
            m[4..6].copy_from_slice(&v.to_le_bytes());
            assert_eq!(
                open(&m),
                Err(EnvelopeError::UnknownEnvelopeVersion { got: v }),
                "{name}"
            );
        }
        for a in [0_u8, 2, 0xFF] {
            let mut m = env.clone();
            m[6] = a;
            assert_eq!(
                open(&m),
                Err(EnvelopeError::UnknownDigestAlgorithm { got: a }),
                "{name}"
            );
        }
        let mut m = env.clone();
        m[7] = 1;
        assert_eq!(
            open(&m),
            Err(EnvelopeError::NonzeroReserved { got: 1 }),
            "{name}"
        );
        let mut m = env.clone();
        m[..4].copy_from_slice(b"SWE2");
        assert_eq!(open(&m), Err(EnvelopeError::BadMagic), "{name}");
    }
}

#[test]
fn inflated_payload_length_is_rejected_before_hashing() {
    for (name, _, env) in sealed_corpus() {
        for len in [u32::MAX, (MAX_SNAPSHOT_BYTES + 1) as u32] {
            let mut m = env.clone();
            m[8..12].copy_from_slice(&len.to_le_bytes());
            assert!(
                matches!(open(&m), Err(EnvelopeError::PayloadTooLarge { .. })),
                "{name} {len}"
            );
        }
        let mut m = env.clone();
        let l = u32::from_le_bytes(m[8..12].try_into().unwrap()) + 1;
        m[8..12].copy_from_slice(&l.to_le_bytes());
        assert!(
            matches!(open(&m), Err(EnvelopeError::Truncated { .. })),
            "{name}"
        );
    }
    assert!(matches!(
        seal(&vec![0_u8; MAX_SNAPSHOT_BYTES + 1]),
        Err(EnvelopeError::PayloadTooLarge { .. })
    ));
}

#[test]
fn caller_byte_cap_is_enforced_before_reading() {
    for (name, _, env) in sealed_corpus() {
        assert!(
            matches!(
                open_with_max(&env, env.len() - 1),
                Err(EnvelopeError::InputTooLarge { .. })
            ),
            "{name}"
        );
        assert!(open_with_max(&env, env.len()).is_ok(), "{name}");
    }
    let huge = vec![0_u8; MAX_ENVELOPE_BYTES + 1];
    assert!(matches!(
        open(&huge),
        Err(EnvelopeError::InputTooLarge { .. })
    ));
}

#[test]
fn resealed_invalid_payload_passes_digest_but_fails_wire_atomically() {
    // Unkeyed digest: a tampered payload with a recomputed digest is NOT
    // rejected by the envelope. It must be stopped downstream.
    for (name, payload, _) in sealed_corpus() {
        let mut bad = payload.clone();
        bad[4] = 2; // unknown wire version inside
        let env = seal(&bad).unwrap();
        assert!(
            open(&env).is_ok(),
            "{name}: unkeyed digest is not authentication"
        );
        let mut target = live(3);
        let before = encode_context(&target).unwrap();
        assert_eq!(
            restore_into_sealed(&mut target, &env),
            Err(EnvelopeError::Payload(
                SnapshotWireError::UnknownWireVersion { got: 2 }
            )),
            "{name}"
        );
        assert_eq!(
            encode_context(&target).unwrap(),
            before,
            "{name}: live mutated"
        );
    }
}

#[test]
fn failed_open_never_mutates_live_state() {
    for (name, _, env) in sealed_corpus() {
        let mut target = live(3);
        let before = encode_context(&target).unwrap();
        let mut flipped = env.clone();
        let mid = env.len() / 2;
        flipped[mid] ^= 0x10;
        for img in [&env[..env.len() - 1], &flipped[..], &env[..12]] {
            assert!(restore_into_sealed(&mut target, img).is_err(), "{name}");
            assert_eq!(encode_context(&target).unwrap(), before, "{name}");
        }
        restore_into_sealed(&mut target, &env).unwrap();
        assert_eq!(seal_context(&target).unwrap(), env, "{name}");
    }
}

#[test]
fn torn_writes_between_two_sealed_checkpoints_are_rejected() {
    let old = seal_context(&live(2)).unwrap();
    let new = seal_context(&live(5)).unwrap();
    assert_ne!(old, new);
    let mut target = live(1);
    let before = encode_context(&target).unwrap();
    for k in 0..new.len() {
        let mut torn = old[..k.min(old.len())].to_vec();
        torn.extend_from_slice(&new[k.min(new.len())..]);
        let images = [new[..k].to_vec(), torn];
        for img in &images {
            if img == &old || img == &new {
                continue;
            }
            assert!(oracle_open(img).is_none(), "image at {k}");
            assert!(
                restore_into_sealed(&mut target, img).is_err(),
                "image at {k}"
            );
            assert_eq!(encode_context(&target).unwrap(), before, "image at {k}");
        }
    }
}
