//! CONTEXT-2 G8: independent negative / compatibility oracle for the `SWE1`
//! v1 integrity envelope (G7), over all four frozen G5 SWC2 fixtures.
//!
//! Independence:
//! - This file carries its own FIPS 180-4 SHA-256 (`ref_sha256`) and its own
//!   spec-level sealer/parser (`ref_seal`, `ref_open`). It calls neither
//!   `sml_core::elastic_ngram_paged_sha::sha256` nor the production
//!   `seal`/`open` to build expectations; production is only the subject.
//! - Pinned vectors (payload SHA-256, envelope SHA-256, resealed-tamper
//!   envelope SHA-256, empty-payload envelope bytes) were computed outside
//!   this codebase with Python `hashlib` + `struct` from the `.hex` fixtures
//!   and the documented layout `SWE1 | u16 1 | u8 1 | u8 0 | u32 len | payload
//!   | sha256(header || payload)`.
//!
//! The digest is **unkeyed**: everything here is corruption detection and
//! fail-closed parsing, **not authentication**. Tests explicitly show that an
//! attacker-controlled payload with a recomputed digest passes the envelope.
//! No key lifecycle exists; no authenticity claim. `NOT_MEASURED`.

use std::path::PathBuf;

use sml_core::context_ledger::{ContextContentHash, ContextSegmentId, ContextStreamId};
use sml_store::working_context::{WorkingContext, WorkingPolicy, WorkingSegment};
use sml_store::working_context_envelope::{
    open, open_and_restore, open_with_max, restore_into_sealed, seal, seal_context, EnvelopeError,
    ENVELOPE_DIGEST_BYTES, ENVELOPE_HEADER_BYTES, MAX_ENVELOPE_BYTES,
};
use sml_store::working_context_wire::{encode_context, MAX_SNAPSHOT_BYTES};

// ---------------------------------------------------------------------------
// Independent SHA-256 (FIPS 180-4), test-local.
// ---------------------------------------------------------------------------

const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

fn ref_sha256(msg: &[u8]) -> [u8; 32] {
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let mut data = msg.to_vec();
    let bit_len = (msg.len() as u64).wrapping_mul(8);
    data.push(0x80);
    while data.len() % 64 != 56 {
        data.push(0);
    }
    data.extend_from_slice(&bit_len.to_be_bytes());
    for block in data.chunks_exact(64) {
        let mut w = [0_u32; 64];
        for (i, c) in block.chunks_exact(4).enumerate() {
            w[i] = u32::from_be_bytes([c[0], c[1], c[2], c[3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        for (x, y) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) {
            *x = x.wrapping_add(y);
        }
    }
    let mut out = [0_u8; 32];
    for (i, x) in h.iter().enumerate() {
        out[4 * i..4 * i + 4].copy_from_slice(&x.to_be_bytes());
    }
    out
}

// ---------------------------------------------------------------------------
// Independent spec-level sealer / parser.
// ---------------------------------------------------------------------------

/// Oracle classification, mirrors the documented admission order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Verdict {
    TooLarge,
    Truncated,
    BadMagic,
    Version,
    Algorithm,
    Reserved,
    PayloadTooLarge,
    Trailing,
    Digest,
    Ok,
}

fn ref_seal(payload: &[u8]) -> Vec<u8> {
    let mut out = b"SWE1".to_vec();
    out.extend_from_slice(&[1, 0, 1, 0]);
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(payload);
    let d = ref_sha256(&out);
    out.extend_from_slice(&d);
    out
}

fn ref_open(b: &[u8], cap: usize) -> Verdict {
    let cap = cap.min(12 + MAX_SNAPSHOT_BYTES + 32);
    if b.len() > cap {
        return Verdict::TooLarge;
    }
    if b.len() < 12 {
        return Verdict::Truncated;
    }
    if &b[..4] != b"SWE1" {
        return Verdict::BadMagic;
    }
    if b[4] != 1 || b[5] != 0 {
        return Verdict::Version;
    }
    if b[6] != 1 {
        return Verdict::Algorithm;
    }
    if b[7] != 0 {
        return Verdict::Reserved;
    }
    let len = u32::from_le_bytes([b[8], b[9], b[10], b[11]]) as u64;
    if len > MAX_SNAPSHOT_BYTES as u64 {
        return Verdict::PayloadTooLarge;
    }
    let need = 12 + len as usize + 32;
    if b.len() < need {
        return Verdict::Truncated;
    }
    if b.len() > need {
        return Verdict::Trailing;
    }
    if ref_sha256(&b[..need - 32])[..] != b[need - 32..] {
        return Verdict::Digest;
    }
    Verdict::Ok
}

fn classify(r: &Result<&[u8], EnvelopeError>) -> Verdict {
    match r {
        Ok(_) => Verdict::Ok,
        Err(EnvelopeError::InputTooLarge { .. }) => Verdict::TooLarge,
        Err(EnvelopeError::Truncated { .. }) => Verdict::Truncated,
        Err(EnvelopeError::BadMagic) => Verdict::BadMagic,
        Err(EnvelopeError::UnknownEnvelopeVersion { .. }) => Verdict::Version,
        Err(EnvelopeError::UnknownDigestAlgorithm { .. }) => Verdict::Algorithm,
        Err(EnvelopeError::NonzeroReserved { .. }) => Verdict::Reserved,
        Err(EnvelopeError::PayloadTooLarge { .. }) => Verdict::PayloadTooLarge,
        Err(EnvelopeError::TrailingBytes { .. }) => Verdict::Trailing,
        Err(EnvelopeError::DigestMismatch) => Verdict::Digest,
        Err(EnvelopeError::Payload(e)) => panic!("open must not decode payload: {e:?}"),
    }
}

fn agree(img: &[u8], cap: usize, ctx: &str) -> Verdict {
    let want = ref_open(img, cap);
    let got = classify(&open_with_max(img, cap));
    assert_eq!(got, want, "{ctx}: production/oracle disagree");
    want
}

// ---------------------------------------------------------------------------
// Pinned, independently derived vectors (Python hashlib).
// ---------------------------------------------------------------------------

struct Pinned {
    name: &'static str,
    payload_len: usize,
    payload_sha256: &'static str,
    envelope_sha256: &'static str,
    /// Envelope over the payload with byte 4 (wire version) set to 2, resealed.
    resealed_tamper_sha256: &'static str,
}

const PINNED: &[Pinned] = &[
    Pinned {
        name: "fixed_cap3_empty",
        payload_len: 93,
        payload_sha256: "dac52fd87fd6bc7086bfea7a3057186baf8ebbd973bb783d80d5fc689945317c",
        envelope_sha256: "4377f3f279d37d9f15066e6a85d47173011e71e9d2c14b705008450995096d84",
        resealed_tamper_sha256: "75a8414c4c44ce5620773efccf4a5fd5d849dea16f7714de34ace6e4555a38ea",
    },
    Pinned {
        name: "fixed_cap2_lost",
        payload_len: 421,
        payload_sha256: "67cdd8ef664064f4d1ca754c60ec043597a70ad3ef91e57a2a36e236519b3fef",
        envelope_sha256: "c35714a5be00503ded1eec9b9d487c02e8bb8e2cb48b84923d205b68a7b379ae",
        resealed_tamper_sha256: "43cea043021fe4600351fbd8d90438b8b3d6b8b078b6ad44b7150d2d42e70c86",
    },
    Pinned {
        name: "expanding_hm4_partial",
        payload_len: 339,
        payload_sha256: "2c76f1fbbb863d234deab1b2ee69a9df8ce422a6728bdac615232175b858dd3f",
        envelope_sha256: "bda5d38c9b5cf7fa64f81a87e422811c402d7186634c72dbf43805b9c5211414",
        resealed_tamper_sha256: "6839841288a73e297307231d26a20f17a58b005244f0a4d3ca486521dbdbcce9",
    },
    Pinned {
        name: "capacity_bounded_cap3_evicting",
        payload_len: 503,
        payload_sha256: "80b61c57c34e2f5eba1f6501de5f3defcbe8dbb969854476c347967789191f50",
        envelope_sha256: "876137e7f2e4276c5bf59a5c07856d80d96f83a7737927cbee2e7105ff8a2281",
        resealed_tamper_sha256: "77151fa49b9b5c97d48c38693bae3ddadd02a6148cb1d3c226aa7ceaf5189f60",
    },
];

/// `seal(b"")`, computed with Python.
const EMPTY_ENVELOPE_HEX: &str =
    "535745310100010000000000b254b0bf7c56d43f2218a541f5d8a5307414e3a5cfa77e0846c6b5b3b7c4ef13";

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

fn corpus() -> Vec<(&'static str, Vec<u8>, Vec<u8>)> {
    PINNED
        .iter()
        .map(|p| {
            let payload = fixture(p.name);
            let env = ref_seal(&payload);
            (p.name, payload, env)
        })
        .collect()
}

fn live(admissions: u64) -> WorkingContext {
    let stream = ContextStreamId::new(0x4738_0000_0000_0008).unwrap();
    let mut ctx =
        WorkingContext::new(stream, WorkingPolicy::CapacityBounded { capacity: 3 }).unwrap();
    for i in 1..=admissions {
        let mut h = [0_u8; 32];
        h[0] = i as u8;
        h[31] = 0xA8;
        let seg = WorkingSegment::new(
            ContextSegmentId::new(stream, 800 + i).unwrap(),
            5 * i,
            ContextContentHash::from_sha256(h),
            i ^ 0x33,
        )
        .unwrap();
        ctx.admit(seg).unwrap();
    }
    ctx
}

// ---------------------------------------------------------------------------
// Tests.
// ---------------------------------------------------------------------------

#[test]
fn reference_sha256_matches_known_answers() {
    assert_eq!(
        hex(&ref_sha256(b"")),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(
        hex(&ref_sha256(b"abc")),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(
        hex(&ref_sha256(
            b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"
        )),
        "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
    );
}

#[test]
fn oracle_and_production_seal_agree_with_pinned_vectors() {
    for p in PINNED {
        let payload = fixture(p.name);
        assert_eq!(payload.len(), p.payload_len, "{}", p.name);
        assert_eq!(hex(&ref_sha256(&payload)), p.payload_sha256, "{}", p.name);
        let env = ref_seal(&payload);
        assert_eq!(
            env.len(),
            ENVELOPE_HEADER_BYTES + p.payload_len + ENVELOPE_DIGEST_BYTES
        );
        assert_eq!(hex(&ref_sha256(&env)), p.envelope_sha256, "{}", p.name);
        assert_eq!(
            seal(&payload).unwrap(),
            env,
            "{}: production seal drift",
            p.name
        );
        assert_eq!(agree(&env, usize::MAX, p.name), Verdict::Ok);
    }
    assert_eq!(hex(&seal(b"").unwrap()), EMPTY_ENVELOPE_HEX);
    assert_eq!(hex(&ref_seal(b"")), EMPTY_ENVELOPE_HEX);
}

#[test]
fn canonical_reencoding_is_exact_and_restart_is_deterministic() {
    for (name, payload, env) in corpus() {
        assert_eq!(open(&env).unwrap(), &payload[..], "{name}");
        let first = open_and_restore(&env).unwrap();
        let admitted = first.snapshot().stats.admitted;
        let env1 = seal_context(&first).unwrap();
        assert_eq!(env1, env, "{name}: re-seal not canonical");
        // Restart twice from the re-sealed bytes: identical, no replay.
        let second = open_and_restore(&env1).unwrap();
        let env2 = seal_context(&second).unwrap();
        assert_eq!(env2, env, "{name}: restart drift");
        assert_eq!(second.snapshot().stats.admitted, admitted, "{name}: replay");
        assert_eq!(encode_context(&second).unwrap(), payload, "{name}");
    }
}

#[test]
fn every_header_byte_value_agrees_with_oracle() {
    for (name, _, env) in corpus() {
        for pos in 0..ENVELOPE_HEADER_BYTES {
            for v in 0..=255_u8 {
                let mut m = env.clone();
                m[pos] = v;
                let verdict = agree(&m, usize::MAX, &format!("{name} hdr[{pos}]={v:#04x}"));
                assert_eq!(verdict == Verdict::Ok, m == env, "{name} hdr[{pos}]={v}");
            }
        }
    }
}

#[test]
fn version_algorithm_and_reserved_rejection_precede_digest() {
    for (name, _, env) in corpus() {
        for v in [0_u16, 2, 3, 0x0100, 0x0101, 0x7FFF, 0xFFFF] {
            let mut m = env.clone();
            m[4..6].copy_from_slice(&v.to_le_bytes());
            assert_eq!(agree(&m, usize::MAX, name), Verdict::Version);
            // Even with a correctly recomputed digest the version is refused.
            let n = m.len();
            let d = ref_sha256(&m[..n - 32]);
            m[n - 32..].copy_from_slice(&d);
            assert_eq!(
                open(&m),
                Err(EnvelopeError::UnknownEnvelopeVersion { got: v }),
                "{name}"
            );
        }
        for a in [0_u8, 2, 0x7F, 0xFF] {
            let mut m = env.clone();
            m[6] = a;
            let n = m.len();
            let d = ref_sha256(&m[..n - 32]);
            m[n - 32..].copy_from_slice(&d);
            assert_eq!(agree(&m, usize::MAX, name), Verdict::Algorithm);
        }
        for r in [1_u8, 0x80, 0xFF] {
            let mut m = env.clone();
            m[7] = r;
            let n = m.len();
            let d = ref_sha256(&m[..n - 32]);
            m[n - 32..].copy_from_slice(&d);
            assert_eq!(
                open(&m),
                Err(EnvelopeError::NonzeroReserved { got: r }),
                "{name}"
            );
        }
    }
}

#[test]
fn u32_length_overflow_truncation_and_trailing_bytes() {
    for (name, payload, env) in corpus() {
        let l = payload.len() as u32;
        let lens = [
            0_u32,
            1,
            l - 1,
            l + 1,
            l + 32,
            MAX_SNAPSHOT_BYTES as u32,
            MAX_SNAPSHOT_BYTES as u32 + 1,
            0x8000_0000,
            u32::MAX - 43,
            u32::MAX,
        ];
        for len in lens {
            let mut m = env.clone();
            m[8..12].copy_from_slice(&len.to_le_bytes());
            let v = agree(&m, usize::MAX, &format!("{name} len={len}"));
            assert_ne!(v, Verdict::Ok, "{name} len={len}");
            if len as u64 > MAX_SNAPSHOT_BYTES as u64 {
                assert_eq!(v, Verdict::PayloadTooLarge, "{name} len={len}");
            }
        }
        for k in 0..env.len() {
            assert_eq!(agree(&env[..k], usize::MAX, name), Verdict::Truncated);
        }
        for extra in [1_usize, 2, 31, 32, 33, 64] {
            let mut m = env.clone();
            m.extend(std::iter::repeat_n(0xA5_u8, extra));
            assert_eq!(open(&m), Err(EnvelopeError::TrailingBytes { extra }));
            assert_eq!(ref_open(&m, usize::MAX), Verdict::Trailing);
        }
    }
}

#[test]
fn digest_mismatch_on_every_payload_and_digest_byte() {
    for (name, _, env) in corpus() {
        for pos in ENVELOPE_HEADER_BYTES..env.len() {
            for mask in [0x01_u8, 0x80, 0xFF] {
                let mut m = env.clone();
                m[pos] ^= mask;
                assert_eq!(
                    agree(&m, usize::MAX, &format!("{name} {pos}^{mask:#04x}")),
                    Verdict::Digest
                );
            }
        }
        // Digest trailer zeroed / swapped from another fixture.
        let mut m = env.clone();
        let n = m.len();
        m[n - 32..].fill(0);
        assert_eq!(open(&m), Err(EnvelopeError::DigestMismatch), "{name}");
    }
}

#[test]
fn resealed_attacker_payload_is_not_authenticated() {
    let c = corpus();
    for (i, (name, payload, _)) in c.iter().enumerate() {
        // (a) Wire-invalid payload with recomputed digest: envelope accepts,
        // wire decoder rejects, live state untouched.
        let mut bad = payload.clone();
        bad[4] = 2;
        let env = ref_seal(&bad);
        assert_eq!(
            hex(&ref_sha256(&env)),
            PINNED[i].resealed_tamper_sha256,
            "{name}"
        );
        assert_eq!(seal(&bad).unwrap(), env, "{name}");
        assert_eq!(open(&env).unwrap(), &bad[..], "{name}: unkeyed digest");
        let mut target = live(4);
        let before = encode_context(&target).unwrap();
        assert!(matches!(
            restore_into_sealed(&mut target, &env),
            Err(EnvelopeError::Payload(_))
        ));
        assert_eq!(encode_context(&target).unwrap(), before, "{name}: mutated");

        // (b) Substitution: a different, fully valid snapshot resealed is
        // accepted end to end. This is the documented non-authenticity.
        let (other_name, other_payload, other_env) = &c[(i + 1) % c.len()];
        let mut target = live(4);
        restore_into_sealed(&mut target, other_env).unwrap();
        assert_eq!(
            encode_context(&target).unwrap(),
            *other_payload,
            "{name}->{other_name}: substitution is not detected (expected, unkeyed)"
        );
    }
}

#[test]
fn caller_byte_caps_are_enforced_before_parsing() {
    for (name, _, env) in corpus() {
        for cap in [0_usize, 11, 12, env.len() - 1] {
            assert_eq!(
                agree(&env, cap, name),
                Verdict::TooLarge,
                "{name} cap={cap}"
            );
        }
        for cap in [env.len(), env.len() + 1, usize::MAX] {
            assert_eq!(agree(&env, cap, name), Verdict::Ok, "{name} cap={cap}");
        }
        // A cap smaller than a garbage image still wins over its content.
        let garbage = vec![0xFF_u8; env.len()];
        assert_eq!(agree(&garbage, env.len() - 1, name), Verdict::TooLarge);
        assert_eq!(agree(&garbage, env.len(), name), Verdict::BadMagic);
    }
    assert_eq!(
        classify(&open(&vec![0_u8; MAX_ENVELOPE_BYTES + 1])),
        Verdict::TooLarge
    );
}

#[test]
fn failed_restore_never_mutates_live_state() {
    for (name, _, env) in corpus() {
        let mut target = live(5);
        let before = encode_context(&target).unwrap();
        let mut images: Vec<Vec<u8>> = vec![env[..env.len() - 1].to_vec(), env[..12].to_vec()];
        for pos in [0_usize, 4, 6, 7, 8, 12, env.len() / 2, env.len() - 1] {
            let mut m = env.clone();
            m[pos] ^= 0x40;
            images.push(m);
        }
        let mut trailing = env.clone();
        trailing.push(0);
        images.push(trailing);
        for img in &images {
            assert!(restore_into_sealed(&mut target, img).is_err(), "{name}");
            assert_eq!(encode_context(&target).unwrap(), before, "{name}: mutated");
        }
    }
}

#[test]
fn old_checkpoint_restore_does_not_replay_later_admissions() {
    let old_ctx = live(2);
    let old = ref_seal(&encode_context(&old_ctx).unwrap());
    assert_eq!(seal_context(&old_ctx).unwrap(), old);
    let mut target = live(6);
    restore_into_sealed(&mut target, &old).unwrap();
    assert_eq!(target.snapshot().stats.admitted, 2, "replayed admissions");
    assert_eq!(seal_context(&target).unwrap(), old);
    // Restoring the same bytes again is idempotent.
    restore_into_sealed(&mut target, &old).unwrap();
    assert_eq!(seal_context(&target).unwrap(), old);
}
