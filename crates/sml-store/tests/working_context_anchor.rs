//! CONTEXT-2 G10: optional external monotonic-anchor admission contract.
//!
//! All anchors here are **mock** trusted anchors built in the test. Nothing
//! here exercises real attestation, provisioning, authentication or hardware
//! monotonic storage; those are **NOT QUALIFIED**. The SWE1 digest stays an
//! unkeyed integrity check, never authenticity.
//!
//! Independent oracle: `EXPECTED` is a frozen, hand-written truth table over
//! (policy, anchor state, stream relation, generation relation, digest
//! relation). It is checked row by row against the production `admit`, and
//! the table is checked to cover every combination. Frozen fixtures: the four
//! G5 SWC2 fixtures, whose SWE1 trailer digests were computed independently
//! with Python `hashlib` (same values pinned in the G7 suite).

use std::path::PathBuf;

use sml_core::context_ledger::{ContextContentHash, ContextSegmentId, ContextStreamId};
use sml_store::working_context::{WorkingContext, WorkingPolicy, WorkingSegment};
use sml_store::working_context_anchor::{
    admit, advance, candidate_of, restore_into_anchored, Admission, AnchorError, AnchorPolicy,
    AnchorRecord, AnchorState, Candidate,
};
use sml_store::working_context_envelope::{seal, seal_context};
use sml_store::working_context_wire::encode_context;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pol {
    Unanchored,
    Required,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum St {
    Absent,
    Unverifiable,
    Verified,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Rel {
    Same,
    Differs,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Gen {
    Less,
    Equal,
    Greater,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum V {
    Unanchored,
    Anchored,
    Absent,
    Unverifiable,
    Stream,
    Rollback,
    Stale,
    Conflict,
}

use Gen::*;
use Pol::*;
use Rel::*;
use St::*;

/// Frozen oracle rows: (policy, state, stream, generation, digest) -> verdict.
/// Rows not depending on later fields are expanded explicitly.
fn expected(p: Pol, s: St, stream: Rel, g: Gen, d: Rel) -> V {
    const T: &[(Pol, St, Rel, Gen, Rel, V)] = &[
        (Required, Verified, Same, Equal, Same, V::Anchored),
        (Required, Verified, Same, Equal, Differs, V::Conflict),
        (Required, Verified, Same, Less, Same, V::Rollback),
        (Required, Verified, Same, Less, Differs, V::Rollback),
        (Required, Verified, Same, Greater, Same, V::Stale),
        (Required, Verified, Same, Greater, Differs, V::Stale),
    ];
    if p == Unanchored {
        return V::Unanchored;
    }
    match s {
        Absent => return V::Absent,
        Unverifiable => return V::Unverifiable,
        Verified => {}
    }
    if stream == Differs {
        return V::Stream;
    }
    T.iter()
        .find(|r| (r.0, r.1, r.2, r.3, r.4) == (p, s, stream, g, d))
        .map(|r| r.5)
        .expect("oracle row")
}

fn classify(r: Result<Admission, AnchorError>) -> V {
    match r {
        Ok(Admission::Unanchored) => V::Unanchored,
        Ok(Admission::Anchored) => V::Anchored,
        Err(AnchorError::AnchorAbsent) => V::Absent,
        Err(AnchorError::AnchorUnverifiable) => V::Unverifiable,
        Err(AnchorError::StreamMismatch { .. }) => V::Stream,
        Err(AnchorError::Rollback { .. }) => V::Rollback,
        Err(AnchorError::AnchorStale { .. }) => V::Stale,
        Err(AnchorError::ConflictingHistory { .. }) => V::Conflict,
        Err(e) => panic!("unexpected {e:?}"),
    }
}

const PINNED_FIXTURE_DIGESTS: &[(&str, &str)] = &[
    (
        "fixed_cap3_empty",
        "06316162a375fd7ca819082261269762c7cf22da22ac90ee94089bcdab62c3c2",
    ),
    (
        "fixed_cap2_lost",
        "334e7a3c5c6aaeb00272cf010ee366e8dae897b30fa6b993275ad6236d3d672a",
    ),
    (
        "expanding_hm4_partial",
        "21de68dcc2b03d5ca22290cb0bc3f224449bfc64831b96879b5f48f1f745a317",
    ),
    (
        "capacity_bounded_cap3_evicting",
        "4ed6cff76b0c238c2a642e70235cdac6f0bbfac54691b7ccbffb73f5fe4d52d2",
    ),
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

const STREAM: u64 = 0x4740_0000_0000_0010;

/// Deterministic context; `salt` changes content without changing count.
fn ctx(stream: u64, n: u64, salt: u8) -> WorkingContext {
    let s = ContextStreamId::new(stream).unwrap();
    let mut c = WorkingContext::new(s, WorkingPolicy::CapacityBounded { capacity: 3 }).unwrap();
    for i in 1..=n {
        let mut h = [salt; 32];
        h[0] = i as u8;
        let seg = WorkingSegment::new(
            ContextSegmentId::new(s, 2000 + i).unwrap(),
            4 * i,
            ContextContentHash::from_sha256(h),
            (i << 8) | u64::from(salt) | 1,
        )
        .unwrap();
        c.admit(seg).unwrap();
    }
    c
}

fn cand(stream: u64, generation: u64, d: u8) -> Candidate {
    Candidate {
        stream_id: stream,
        generation,
        digest: [d; 32],
    }
}

fn rec(c: &Candidate) -> AnchorRecord {
    AnchorRecord {
        stream_id: c.stream_id,
        generation: c.generation,
        digest: c.digest,
    }
}

#[test]
fn admit_matches_frozen_truth_table_on_every_combination() {
    let mut rows = 0;
    for p in [Unanchored, Required] {
        for s in [Absent, Unverifiable, Verified] {
            for stream in [Same, Differs] {
                for g in [Less, Equal, Greater] {
                    for d in [Same, Differs] {
                        for base in [1_u64, 7, u64::MAX - 1] {
                            let a = cand(STREAM, base, 0x11);
                            let c = Candidate {
                                stream_id: if stream == Same { STREAM } else { STREAM + 1 },
                                generation: match g {
                                    Less => base - 1,
                                    Equal => base,
                                    Greater => base + 1,
                                },
                                digest: if d == Same { a.digest } else { [0x22; 32] },
                            };
                            let state = match s {
                                Absent => AnchorState::Absent,
                                Unverifiable => AnchorState::Unverifiable,
                                Verified => AnchorState::Verified(rec(&a)),
                            };
                            let policy = match p {
                                Unanchored => AnchorPolicy::Unanchored,
                                Required => AnchorPolicy::Required,
                            };
                            assert_eq!(
                                classify(admit(policy, &state, &c)),
                                expected(p, s, stream, g, d),
                                "{p:?} {s:?} {stream:?} {g:?} {d:?} base={base}"
                            );
                            rows += 1;
                        }
                    }
                }
            }
        }
    }
    assert_eq!(rows, 2 * 3 * 2 * 3 * 2 * 3);
}

#[test]
fn advance_requires_strictly_increasing_generation_and_same_stream() {
    let cur = rec(&cand(STREAM, 5, 1));
    assert_eq!(advance(None, &cand(STREAM, 0, 9)).unwrap().generation, 0);
    assert_eq!(
        advance(Some(&cur), &cand(STREAM, 6, 2)).unwrap().generation,
        6
    );
    assert_eq!(
        advance(Some(&cur), &cand(STREAM, 40, 2))
            .unwrap()
            .generation,
        40
    );
    for g in [0_u64, 4, 5] {
        assert_eq!(
            advance(Some(&cur), &cand(STREAM, g, 2)),
            Err(AnchorError::NonIncreasing {
                current: 5,
                next: g
            })
        );
    }
    assert!(matches!(
        advance(Some(&cur), &cand(STREAM + 1, 6, 2)),
        Err(AnchorError::StreamMismatch { .. })
    ));
    let top = rec(&cand(STREAM, u64::MAX, 1));
    assert_eq!(
        advance(Some(&top), &cand(STREAM, u64::MAX, 3)),
        Err(AnchorError::GenerationOverflow)
    );
    let near = rec(&cand(STREAM, u64::MAX - 1, 1));
    assert_eq!(
        advance(Some(&near), &cand(STREAM, u64::MAX, 3))
            .unwrap()
            .generation,
        u64::MAX
    );
}

#[test]
fn frozen_fixture_candidates_carry_independent_digests() {
    for (name, digest_hex) in PINNED_FIXTURE_DIGESTS {
        let env = seal(&fixture(name)).unwrap();
        let (restored, c) = candidate_of(&env).unwrap();
        assert_eq!(hex(&c.digest), *digest_hex, "{name}");
        assert_eq!(c.generation, restored.snapshot().stats.admitted, "{name}");
        // Anchored to itself it is admitted; any other digest conflicts.
        let st = AnchorState::Verified(rec(&c));
        assert_eq!(
            admit(AnchorPolicy::Required, &st, &c),
            Ok(Admission::Anchored)
        );
        let mut other = c;
        other.digest[0] ^= 1;
        assert!(matches!(
            admit(AnchorPolicy::Required, &st, &other),
            Err(AnchorError::ConflictingHistory { .. })
        ));
    }
    let (_, empty) = candidate_of(&seal(&fixture("fixed_cap3_empty")).unwrap()).unwrap();
    assert_eq!(empty.generation, 0);
}

/// Checkpoint protocol: (1) durable snapshot write, (2) anchor advance.
#[derive(Clone, Copy, Debug)]
enum Crash {
    BeforeSnapshot,
    AfterSnapshotBeforeAnchor,
    AfterAnchor,
}

#[test]
fn crash_before_and_after_durable_anchor_update() {
    let old_c = ctx(STREAM, 2, 0xA1);
    let new_c = ctx(STREAM, 5, 0xA1);
    let old = seal_context(&old_c).unwrap();
    let new = seal_context(&new_c).unwrap();
    let (_, oc) = candidate_of(&old).unwrap();
    let (_, nc) = candidate_of(&new).unwrap();
    assert_eq!((oc.generation, nc.generation), (2, 5));
    let anchor_old = advance(None, &oc).unwrap();
    for crash in [
        Crash::BeforeSnapshot,
        Crash::AfterSnapshotBeforeAnchor,
        Crash::AfterAnchor,
    ] {
        let (live, anchor) = match crash {
            Crash::BeforeSnapshot => (&old, anchor_old),
            Crash::AfterSnapshotBeforeAnchor => (&new, anchor_old),
            Crash::AfterAnchor => (&new, advance(Some(&anchor_old), &nc).unwrap()),
        };
        let st = AnchorState::Verified(anchor);
        let mut target = ctx(STREAM, 1, 0x00);
        let before = encode_context(&target).unwrap();
        let r = restore_into_anchored(&mut target, live, AnchorPolicy::Required, &st);
        match crash {
            Crash::BeforeSnapshot => {
                assert_eq!(r, Ok(Admission::Anchored));
                assert_eq!(seal_context(&target).unwrap(), old);
            }
            Crash::AfterSnapshotBeforeAnchor => {
                // Fail closed: anchor lags; recovery is an explicit re-anchor.
                assert_eq!(
                    r,
                    Err(AnchorError::AnchorStale {
                        anchor: 2,
                        candidate: 5
                    })
                );
                assert_eq!(encode_context(&target).unwrap(), before, "{crash:?}");
                // Unanchored policy still restores, labelled as such.
                let mut t2 = ctx(STREAM, 1, 0x00);
                assert_eq!(
                    restore_into_anchored(&mut t2, live, AnchorPolicy::Unanchored, &st),
                    Ok(Admission::Unanchored)
                );
                assert_eq!(seal_context(&t2).unwrap(), new);
            }
            Crash::AfterAnchor => {
                assert_eq!(r, Ok(Admission::Anchored));
                assert_eq!(seal_context(&target).unwrap(), new);
            }
        }
    }
}

#[test]
fn stale_disk_rollback_is_rejected_only_when_anchor_is_required() {
    let old = seal_context(&ctx(STREAM, 2, 0xB2)).unwrap();
    let new = seal_context(&ctx(STREAM, 5, 0xB2)).unwrap();
    let (_, nc) = candidate_of(&new).unwrap();
    let st = AnchorState::Verified(advance(None, &nc).unwrap());
    let mut target = ctx(STREAM, 4, 0x01);
    let before = encode_context(&target).unwrap();
    assert_eq!(
        restore_into_anchored(&mut target, &old, AnchorPolicy::Required, &st),
        Err(AnchorError::Rollback {
            anchor: 5,
            candidate: 2
        })
    );
    assert_eq!(encode_context(&target).unwrap(), before);
    // Without an anchor the same rollback is accepted (G9 behaviour).
    assert_eq!(
        restore_into_anchored(&mut target, &old, AnchorPolicy::Unanchored, &st),
        Ok(Admission::Unanchored)
    );
    assert_eq!(seal_context(&target).unwrap(), old);
}

#[test]
fn conflicting_histories_at_equal_generation_are_rejected() {
    let a = seal_context(&ctx(STREAM, 3, 0xC3)).unwrap();
    let b = seal_context(&ctx(STREAM, 3, 0xD4)).unwrap();
    assert_ne!(a, b);
    let (_, ac) = candidate_of(&a).unwrap();
    let (_, bc) = candidate_of(&b).unwrap();
    assert_eq!(ac.generation, bc.generation);
    let st = AnchorState::Verified(advance(None, &ac).unwrap());
    let mut target = ctx(STREAM, 1, 0x02);
    let before = encode_context(&target).unwrap();
    assert_eq!(
        restore_into_anchored(&mut target, &b, AnchorPolicy::Required, &st),
        Err(AnchorError::ConflictingHistory { generation: 3 })
    );
    assert_eq!(encode_context(&target).unwrap(), before);
    // Cross-stream snapshot with a matching generation is refused too.
    let other = seal_context(&ctx(STREAM + 7, 3, 0xC3)).unwrap();
    assert!(matches!(
        restore_into_anchored(&mut target, &other, AnchorPolicy::Required, &st),
        Err(AnchorError::StreamMismatch { .. })
    ));
    assert_eq!(encode_context(&target).unwrap(), before);
}

#[test]
fn absent_unverifiable_and_corrupt_inputs_fail_closed_without_mutation() {
    let good = seal_context(&ctx(STREAM, 3, 0xE5)).unwrap();
    let (_, gc) = candidate_of(&good).unwrap();
    let mut corrupt = good.clone();
    corrupt[20] ^= 0x08;
    let cases: Vec<(&[u8], AnchorState)> = vec![
        (&good, AnchorState::Absent),
        (&good, AnchorState::Unverifiable),
        (&corrupt, AnchorState::Verified(rec(&gc))),
        (&good[..good.len() - 1], AnchorState::Verified(rec(&gc))),
    ];
    for (bytes, st) in cases {
        let mut target = ctx(STREAM, 2, 0x03);
        let before = encode_context(&target).unwrap();
        let admitted_before = target.snapshot().stats.admitted;
        assert!(restore_into_anchored(&mut target, bytes, AnchorPolicy::Required, &st).is_err());
        assert_eq!(encode_context(&target).unwrap(), before);
        assert_eq!(target.snapshot().stats.admitted, admitted_before, "replay");
    }
}

#[test]
fn accepted_anchored_restore_is_exact_and_replays_nothing() {
    let c = ctx(STREAM, 4, 0xF6);
    let env = seal_context(&c).unwrap();
    let (_, cc) = candidate_of(&env).unwrap();
    let st = AnchorState::Verified(advance(None, &cc).unwrap());
    let mut target = ctx(STREAM, 9, 0x04);
    for _ in 0..3 {
        assert_eq!(
            restore_into_anchored(&mut target, &env, AnchorPolicy::Required, &st),
            Ok(Admission::Anchored)
        );
        assert_eq!(target.snapshot().stats.admitted, 4);
        assert_eq!(seal_context(&target).unwrap(), env);
    }
}
