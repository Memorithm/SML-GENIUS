//! CONTEXT-2 G11: adversarial differential tests for G10 anchor admission.
//!
//! Independent pure oracle: the decision procedures in this file
//! (`oracle_admit`, `oracle_advance`, `oracle_atomic_crash`,
//! `oracle_inplace_prefix`) implement the published G10 contract with their
//! own types and priority-ordered rules. They do **not** call
//! `sml_store::working_context_anchor::{admit, advance}` or any other
//! production admission helper. Production results are classified and
//! compared row-by-row against the oracle on a frozen vector table.
//!
//! Coverage (frozen):
//! - stream IDs `0`, `1`, pinned `STREAM`, `u64::MAX`, and cross-stream pairs;
//! - generation boundaries `0` and `u64::MAX` (plus interior values);
//! - equal-generation digest conflicts;
//! - snapshot truncation at every prefix and one trailing-byte extension;
//! - every crash boundary of the (durable snapshot write, then anchor advance)
//!   protocol, plus in-place torn writes and a stale-disk substitution;
//! - `Unanchored` accepts an intact rollback;
//! - caller-supplied forged `Verified` anchors are **not** authenticated
//!   (documented limitation; attestation / authentication remain NOT QUALIFIED).
//!
//! Every rejected restore leaves the target encoding and `stats.admitted`
//! unchanged (no replay, no partial mutation). Anchors in this suite are
//! mocks constructed in-process. No hardware, E2E, transport or authenticity
//! claim is made. `NOT_MEASURED`.

use sml_core::context_ledger::{ContextContentHash, ContextSegmentId, ContextStreamId};
use sml_store::working_context::{WorkingContext, WorkingPolicy, WorkingSegment};
use sml_store::working_context_anchor::{
    admit, advance, restore_into_anchored, Admission, AnchorError, AnchorPolicy, AnchorRecord,
    AnchorState, Candidate,
};
use sml_store::working_context_envelope::seal_context;
use sml_store::working_context_wire::encode_context;

/// Pinned stream used for live `WorkingContext` fixtures (must be nonzero).
const STREAM: u64 = 0x4741_0000_0000_0011;
const OTHER: u64 = 0x4741_0000_0000_0022;

/// Frozen cartesian domains for the independent admit / advance oracles.
const STREAMS: [u64; 4] = [0, 1, STREAM, u64::MAX];
const GENS: [u64; 5] = [0, 1, 42, u64::MAX - 1, u64::MAX];
const DIGESTS: [u8; 2] = [0xA1, 0x5E];

const ENVELOPE_DIGEST_LEN: usize = 32;

// ---------------------------------------------------------------------------
// Independent oracle types and decision procedures.
// These types are deliberately *not* the production enums.
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OPolicy {
    Unanchored,
    Required,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct OIdent {
    stream: u64,
    generation: u64,
    digest: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OAnchor {
    Absent,
    Unverifiable,
    Verified(OIdent),
}

/// Independent verdict, including payload fields used by the production errors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OOut {
    Unanchored,
    Anchored,
    Absent,
    Unverifiable,
    StreamMismatch { anchor: u64, candidate: u64 },
    Rollback { anchor: u64, candidate: u64 },
    Stale { anchor: u64, candidate: u64 },
    Conflict { generation: u64 },
    NonIncreasing { current: u64, next: u64 },
    Overflow,
    Envelope,
}

/// Pure admission oracle. Priority-ordered rules from the G10 module contract:
///
/// 1. `Unanchored` policy → `Unanchored` (the anchor is ignored).
/// 2. `Required` + absent → `Absent`.
/// 3. `Required` + unverifiable → `Unverifiable`.
/// 4. stream mismatch → `StreamMismatch` (before generation is consulted).
/// 5. candidate generation `<` anchor → `Rollback`.
/// 6. candidate generation `>` anchor → `Stale`.
/// 7. equal generation, different digest → `Conflict`.
/// 8. equal generation, same digest → `Anchored`.
fn oracle_admit(policy: OPolicy, anchor: OAnchor, cand: OIdent) -> OOut {
    if matches!(policy, OPolicy::Unanchored) {
        return OOut::Unanchored;
    }
    let rec = match anchor {
        OAnchor::Absent => return OOut::Absent,
        OAnchor::Unverifiable => return OOut::Unverifiable,
        OAnchor::Verified(r) => r,
    };
    if rec.stream != cand.stream {
        return OOut::StreamMismatch {
            anchor: rec.stream,
            candidate: cand.stream,
        };
    }
    if cand.generation < rec.generation {
        return OOut::Rollback {
            anchor: rec.generation,
            candidate: cand.generation,
        };
    }
    if cand.generation > rec.generation {
        return OOut::Stale {
            anchor: rec.generation,
            candidate: cand.generation,
        };
    }
    if rec.digest != cand.digest {
        return OOut::Conflict {
            generation: rec.generation,
        };
    }
    OOut::Anchored
}

/// Pure `advance` oracle.
///
/// 1. no current record → accept `next` as the new record;
/// 2. stream mismatch → `StreamMismatch`;
/// 3. `current.generation == u64::MAX` → `Overflow` (cannot form `current + 1`);
/// 4. `next.generation < current + 1` → `NonIncreasing`;
/// 5. otherwise accept `next` (generation jumps forward are allowed).
fn oracle_advance(current: Option<OIdent>, next: OIdent) -> Result<OIdent, OOut> {
    if let Some(cur) = current {
        if cur.stream != next.stream {
            return Err(OOut::StreamMismatch {
                anchor: cur.stream,
                candidate: next.stream,
            });
        }
        let floor = match cur.generation.checked_add(1) {
            Some(f) => f,
            None => return Err(OOut::Overflow),
        };
        if next.generation < floor {
            return Err(OOut::NonIncreasing {
                current: cur.generation,
                next: next.generation,
            });
        }
    }
    Ok(next)
}

fn classify(r: Result<Admission, AnchorError>) -> OOut {
    match r {
        Ok(Admission::Unanchored) => OOut::Unanchored,
        Ok(Admission::Anchored) => OOut::Anchored,
        Err(AnchorError::AnchorAbsent) => OOut::Absent,
        Err(AnchorError::AnchorUnverifiable) => OOut::Unverifiable,
        Err(AnchorError::StreamMismatch { anchor, candidate }) => {
            OOut::StreamMismatch { anchor, candidate }
        }
        Err(AnchorError::Rollback { anchor, candidate }) => OOut::Rollback { anchor, candidate },
        Err(AnchorError::AnchorStale { anchor, candidate }) => OOut::Stale { anchor, candidate },
        Err(AnchorError::ConflictingHistory { generation }) => OOut::Conflict { generation },
        Err(AnchorError::NonIncreasing { current, next }) => OOut::NonIncreasing { current, next },
        Err(AnchorError::GenerationOverflow) => OOut::Overflow,
        Err(AnchorError::Envelope(_)) => OOut::Envelope,
    }
}

fn digest32(tag: u8) -> [u8; 32] {
    [tag; 32]
}

fn prod_cand(id: OIdent) -> Candidate {
    Candidate {
        stream_id: id.stream,
        generation: id.generation,
        digest: digest32(id.digest),
    }
}

fn prod_rec(id: OIdent) -> AnchorRecord {
    AnchorRecord {
        stream_id: id.stream,
        generation: id.generation,
        digest: digest32(id.digest),
    }
}

fn prod_state(anchor: OAnchor) -> AnchorState {
    match anchor {
        OAnchor::Absent => AnchorState::Absent,
        OAnchor::Unverifiable => AnchorState::Unverifiable,
        OAnchor::Verified(id) => AnchorState::Verified(prod_rec(id)),
    }
}

fn prod_policy(p: OPolicy) -> AnchorPolicy {
    match p {
        OPolicy::Unanchored => AnchorPolicy::Unanchored,
        OPolicy::Required => AnchorPolicy::Required,
    }
}

fn ident(stream: u64, generation: u64, digest: u8) -> OIdent {
    OIdent {
        stream,
        generation,
        digest,
    }
}

// ---------------------------------------------------------------------------
// Frozen named golden vectors (spec exemplars, not derived from production).
// ---------------------------------------------------------------------------

struct NamedAdmit {
    name: &'static str,
    policy: OPolicy,
    anchor: OAnchor,
    cand: OIdent,
    expect: OOut,
}

fn named_admit_vectors() -> Vec<NamedAdmit> {
    // Built from the published contract, not by probing production `admit`.
    vec![
        NamedAdmit {
            name: "unanchored_ignores_absent_at_gen0",
            policy: OPolicy::Unanchored,
            anchor: OAnchor::Absent,
            cand: ident(0, 0, 0xA1),
            expect: OOut::Unanchored,
        },
        NamedAdmit {
            name: "unanchored_ignores_mismatch_at_max",
            policy: OPolicy::Unanchored,
            anchor: OAnchor::Verified(ident(STREAM, u64::MAX, 0xA1)),
            cand: ident(0, 0, 0x5E),
            expect: OOut::Unanchored,
        },
        NamedAdmit {
            name: "required_absent_at_gen0",
            policy: OPolicy::Required,
            anchor: OAnchor::Absent,
            cand: ident(STREAM, 0, 0xA1),
            expect: OOut::Absent,
        },
        NamedAdmit {
            name: "required_unverifiable_at_max",
            policy: OPolicy::Required,
            anchor: OAnchor::Unverifiable,
            cand: ident(STREAM, u64::MAX, 0xA1),
            expect: OOut::Unverifiable,
        },
        NamedAdmit {
            name: "required_exact_match_gen0",
            policy: OPolicy::Required,
            anchor: OAnchor::Verified(ident(STREAM, 0, 0xA1)),
            cand: ident(STREAM, 0, 0xA1),
            expect: OOut::Anchored,
        },
        NamedAdmit {
            name: "required_exact_match_gen_max",
            policy: OPolicy::Required,
            anchor: OAnchor::Verified(ident(STREAM, u64::MAX, 0x5E)),
            cand: ident(STREAM, u64::MAX, 0x5E),
            expect: OOut::Anchored,
        },
        NamedAdmit {
            name: "required_gen0_vs_1_is_stale",
            policy: OPolicy::Required,
            anchor: OAnchor::Verified(ident(STREAM, 0, 0xA1)),
            cand: ident(STREAM, 1, 0xA1),
            expect: OOut::Stale {
                anchor: 0,
                candidate: 1,
            },
        },
        NamedAdmit {
            name: "required_cand0_vs_anchor1_is_rollback",
            policy: OPolicy::Required,
            anchor: OAnchor::Verified(ident(STREAM, 1, 0xA1)),
            cand: ident(STREAM, 0, 0xA1),
            expect: OOut::Rollback {
                anchor: 1,
                candidate: 0,
            },
        },
        NamedAdmit {
            name: "required_max_vs_max_minus_1_is_rollback",
            policy: OPolicy::Required,
            anchor: OAnchor::Verified(ident(STREAM, u64::MAX, 0xA1)),
            cand: ident(STREAM, u64::MAX - 1, 0xA1),
            expect: OOut::Rollback {
                anchor: u64::MAX,
                candidate: u64::MAX - 1,
            },
        },
        NamedAdmit {
            name: "required_max_minus_1_vs_max_is_stale",
            policy: OPolicy::Required,
            anchor: OAnchor::Verified(ident(STREAM, u64::MAX - 1, 0xA1)),
            cand: ident(STREAM, u64::MAX, 0xA1),
            expect: OOut::Stale {
                anchor: u64::MAX - 1,
                candidate: u64::MAX,
            },
        },
        NamedAdmit {
            name: "required_equal_gen0_digest_conflict",
            policy: OPolicy::Required,
            anchor: OAnchor::Verified(ident(STREAM, 0, 0xA1)),
            cand: ident(STREAM, 0, 0x5E),
            expect: OOut::Conflict { generation: 0 },
        },
        NamedAdmit {
            name: "required_equal_gen_max_digest_conflict",
            policy: OPolicy::Required,
            anchor: OAnchor::Verified(ident(STREAM, u64::MAX, 0xA1)),
            cand: ident(STREAM, u64::MAX, 0x5E),
            expect: OOut::Conflict {
                generation: u64::MAX,
            },
        },
        NamedAdmit {
            name: "required_stream0_vs_1_before_generation",
            policy: OPolicy::Required,
            anchor: OAnchor::Verified(ident(0, u64::MAX, 0xA1)),
            cand: ident(1, 0, 0xA1),
            expect: OOut::StreamMismatch {
                anchor: 0,
                candidate: 1,
            },
        },
        NamedAdmit {
            name: "required_stream_max_vs_0",
            policy: OPolicy::Required,
            anchor: OAnchor::Verified(ident(u64::MAX, 0, 0xA1)),
            cand: ident(0, 0, 0xA1),
            expect: OOut::StreamMismatch {
                anchor: u64::MAX,
                candidate: 0,
            },
        },
        NamedAdmit {
            name: "required_digest_ignored_when_generations_differ",
            policy: OPolicy::Required,
            anchor: OAnchor::Verified(ident(STREAM, 42, 0xA1)),
            cand: ident(STREAM, 0, 0x5E),
            expect: OOut::Rollback {
                anchor: 42,
                candidate: 0,
            },
        },
    ]
}

// ---------------------------------------------------------------------------
// Live WorkingContext helpers (small generations only; u64::MAX cannot be
// materialized as `stats.admitted` in-process and is covered via `admit` /
// `advance` synthetic candidates instead).
// ---------------------------------------------------------------------------

fn sample_ctx(stream: u64, n: u64, salt: u8) -> WorkingContext {
    let s = ContextStreamId::new(stream).expect("nonzero stream id");
    let mut ctx = WorkingContext::new(s, WorkingPolicy::CapacityBounded { capacity: 4 }).unwrap();
    for i in 1..=n {
        let mut hash = [salt; 32];
        hash[0] = u8::try_from(i).expect("fixture generation fits u8");
        let seg = WorkingSegment::new(
            ContextSegmentId::new(s, 3_000 + i).unwrap(),
            5 * i,
            ContextContentHash::from_sha256(hash),
            (i << 16) | u64::from(salt) | 1,
        )
        .unwrap();
        ctx.admit(seg).unwrap();
    }
    ctx
}

fn fingerprint(ctx: &WorkingContext) -> (Vec<u8>, u64) {
    (encode_context(ctx).unwrap(), ctx.snapshot().stats.admitted)
}

fn assert_unchanged(ctx: &WorkingContext, before: &(Vec<u8>, u64), what: &str) {
    let now = fingerprint(ctx);
    assert_eq!(now.0, before.0, "{what}: encoding must be unchanged");
    assert_eq!(now.1, before.1, "{what}: admitted must be unchanged");
}

/// Independent digest extraction from the published SWE1 layout (last 32 bytes).
/// This is not a call into production admission logic.
fn trailer_digest(env: &[u8]) -> [u8; 32] {
    assert!(
        env.len() >= ENVELOPE_DIGEST_LEN,
        "sealed fixture too short for a digest trailer"
    );
    let mut out = [0_u8; 32];
    out.copy_from_slice(&env[env.len() - ENVELOPE_DIGEST_LEN..]);
    out
}

fn sealed_pair(old_n: u64, new_n: u64, salt: u8) -> (Vec<u8>, Vec<u8>, Candidate, Candidate) {
    let old_env = seal_context(&sample_ctx(STREAM, old_n, salt)).unwrap();
    let new_env = seal_context(&sample_ctx(STREAM, new_n, salt)).unwrap();
    let old_c = Candidate {
        stream_id: STREAM,
        generation: old_n,
        digest: trailer_digest(&old_env),
    };
    let new_c = Candidate {
        stream_id: STREAM,
        generation: new_n,
        digest: trailer_digest(&new_env),
    };
    (old_env, new_env, old_c, new_c)
}

fn rec_from(c: &Candidate) -> AnchorRecord {
    AnchorRecord {
        stream_id: c.stream_id,
        generation: c.generation,
        digest: c.digest,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn admit_matches_independent_oracle_on_frozen_cartesian_and_named_vectors() {
    let named = named_admit_vectors();
    assert_eq!(named.len(), 15, "frozen named admit table size");
    for row in &named {
        let got = classify(admit(
            prod_policy(row.policy),
            &prod_state(row.anchor),
            &prod_cand(row.cand),
        ));
        assert_eq!(got, row.expect, "named vector {}", row.name);
        assert_eq!(
            oracle_admit(row.policy, row.anchor, row.cand),
            row.expect,
            "oracle disagrees with frozen named vector {}",
            row.name
        );
    }

    let mut rows = 0_usize;
    let mut saw_gen0_equal = false;
    let mut saw_gen_max_equal = false;
    for policy in [OPolicy::Unanchored, OPolicy::Required] {
        for anchor_kind in 0_u8..3 {
            for &a_stream in &STREAMS {
                for &c_stream in &STREAMS {
                    for &a_gen in &GENS {
                        for &c_gen in &GENS {
                            for &a_dig in &DIGESTS {
                                for &c_dig in &DIGESTS {
                                    let cand = ident(c_stream, c_gen, c_dig);
                                    let anchor = match anchor_kind {
                                        0 => OAnchor::Absent,
                                        1 => OAnchor::Unverifiable,
                                        _ => OAnchor::Verified(ident(a_stream, a_gen, a_dig)),
                                    };
                                    let expect = oracle_admit(policy, anchor, cand);
                                    let got = classify(admit(
                                        prod_policy(policy),
                                        &prod_state(anchor),
                                        &prod_cand(cand),
                                    ));
                                    assert_eq!(got, expect, "{policy:?} {anchor:?} {cand:?}");
                                    if matches!(anchor, OAnchor::Verified(_))
                                        && a_gen == 0
                                        && c_gen == 0
                                        && a_stream == c_stream
                                    {
                                        saw_gen0_equal = true;
                                    }
                                    if matches!(anchor, OAnchor::Verified(_))
                                        && a_gen == u64::MAX
                                        && c_gen == u64::MAX
                                        && a_stream == c_stream
                                    {
                                        saw_gen_max_equal = true;
                                    }
                                    rows += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    assert_eq!(
        rows,
        2 * 3
            * STREAMS.len()
            * STREAMS.len()
            * GENS.len()
            * GENS.len()
            * DIGESTS.len()
            * DIGESTS.len()
    );
    assert!(saw_gen0_equal && saw_gen_max_equal);
}

#[test]
fn advance_matches_independent_oracle_on_frozen_vectors() {
    // Named boundary exemplars.
    let none_zero = ident(STREAM, 0, 0xA1);
    assert_eq!(oracle_advance(None, none_zero), Ok(none_zero));
    assert_eq!(
        advance(None, &prod_cand(none_zero)).map(rec_from_prod),
        Ok(none_zero)
    );

    let none_max = ident(STREAM, u64::MAX, 0x5E);
    assert_eq!(oracle_advance(None, none_max), Ok(none_max));
    assert_eq!(
        advance(None, &prod_cand(none_max)).map(rec_from_prod),
        Ok(none_max)
    );

    let at_zero = ident(STREAM, 0, 0xA1);
    let at_one = ident(STREAM, 1, 0x5E);
    assert_eq!(oracle_advance(Some(at_zero), at_one), Ok(at_one));
    assert_eq!(
        classify_advance(advance(Some(&prod_rec(at_zero)), &prod_cand(at_one))),
        Ok(at_one)
    );

    let at_max = ident(STREAM, u64::MAX, 0xA1);
    assert_eq!(
        oracle_advance(Some(at_max), ident(STREAM, u64::MAX, 0x5E)),
        Err(OOut::Overflow)
    );
    assert_eq!(
        classify_advance(advance(
            Some(&prod_rec(at_max)),
            &prod_cand(ident(STREAM, u64::MAX, 0x5E))
        )),
        Err(OOut::Overflow)
    );
    assert_eq!(
        oracle_advance(Some(at_max), ident(STREAM, 0, 0xA1)),
        Err(OOut::Overflow)
    );

    let mut rows = 0_usize;
    let currents: Vec<Option<OIdent>> = std::iter::once(None)
        .chain(
            STREAMS
                .iter()
                .copied()
                .flat_map(|s| GENS.iter().copied().map(move |g| (s, g)))
                .flat_map(|(s, g)| DIGESTS.iter().copied().map(move |d| ident(s, g, d)))
                .map(Some),
        )
        .collect();
    for current in &currents {
        for &s in &STREAMS {
            for &g in &GENS {
                for &d in &DIGESTS {
                    let next = ident(s, g, d);
                    let expect = oracle_advance(*current, next);
                    let got = match current {
                        None => classify_advance(advance(None, &prod_cand(next))),
                        Some(cur) => {
                            classify_advance(advance(Some(&prod_rec(*cur)), &prod_cand(next)))
                        }
                    };
                    assert_eq!(got, expect, "current={current:?} next={next:?}");
                    rows += 1;
                }
            }
        }
    }
    assert_eq!(
        rows,
        currents.len() * STREAMS.len() * GENS.len() * DIGESTS.len()
    );
}

fn rec_from_prod(r: AnchorRecord) -> OIdent {
    // Inverse of prod_rec for `[tag; 32]` digests used by synthetic candidates.
    assert!(r.digest.iter().all(|&b| b == r.digest[0]));
    ident(r.stream_id, r.generation, r.digest[0])
}

fn classify_advance(r: Result<AnchorRecord, AnchorError>) -> Result<OIdent, OOut> {
    match r {
        Ok(rec) => Ok(rec_from_prod(rec)),
        Err(e) => Err(classify(Err(e))),
    }
}

#[test]
fn generation_and_stream_boundaries_include_zero_and_u64_max() {
    assert!(GENS.contains(&0) && GENS.contains(&u64::MAX));
    assert!(STREAMS.contains(&0) && STREAMS.contains(&u64::MAX));

    // Equal-generation exact match at both extremes.
    for &(stream, gen) in &[
        (0_u64, 0_u64),
        (0, u64::MAX),
        (u64::MAX, 0),
        (u64::MAX, u64::MAX),
        (STREAM, 0),
        (STREAM, u64::MAX),
    ] {
        let id = ident(stream, gen, 0xA1);
        assert_eq!(
            oracle_admit(OPolicy::Required, OAnchor::Verified(id), id),
            OOut::Anchored
        );
        assert_eq!(
            classify(admit(
                AnchorPolicy::Required,
                &AnchorState::Verified(prod_rec(id)),
                &prod_cand(id)
            )),
            OOut::Anchored,
            "stream={stream:#x} gen={gen}"
        );
    }
}

#[test]
fn equal_generation_digest_conflicts_rejected_without_mutation() {
    // Live snapshots at the same generation, different content (and digest).
    let a_env = seal_context(&sample_ctx(STREAM, 3, 0xC1)).unwrap();
    let b_env = seal_context(&sample_ctx(STREAM, 3, 0xC2)).unwrap();
    assert_ne!(a_env, b_env);
    let a_c = Candidate {
        stream_id: STREAM,
        generation: 3,
        digest: trailer_digest(&a_env),
    };
    let b_c = Candidate {
        stream_id: STREAM,
        generation: 3,
        digest: trailer_digest(&b_env),
    };
    assert_ne!(a_c.digest, b_c.digest);

    // Oracle (synthetic tags would hide the real digest); construct OOut
    // directly from the published equal-generation rule.
    let expect = OOut::Conflict { generation: 3 };
    let st = AnchorState::Verified(rec_from(&a_c));
    assert_eq!(classify(admit(AnchorPolicy::Required, &st, &b_c)), expect);

    let mut target = sample_ctx(STREAM, 1, 0x00);
    let before = fingerprint(&target);
    assert_eq!(
        classify(restore_into_anchored(
            &mut target,
            &b_env,
            AnchorPolicy::Required,
            &st
        )),
        expect
    );
    assert_unchanged(&target, &before, "conflict restore");

    // Cross-stream snapshot at the same generation is a stream mismatch,
    // not a digest conflict (stream is checked first).
    let other_env = seal_context(&sample_ctx(OTHER, 3, 0xC1)).unwrap();
    assert_eq!(
        classify(restore_into_anchored(
            &mut target,
            &other_env,
            AnchorPolicy::Required,
            &st
        )),
        OOut::StreamMismatch {
            anchor: STREAM,
            candidate: OTHER
        }
    );
    assert_unchanged(&target, &before, "cross-stream restore");
}

#[test]
fn snapshot_truncation_and_trailing_bytes_fail_closed_without_mutation() {
    let (old_env, new_env, old_c, new_c) = sealed_pair(0, 4, 0xD3);
    assert_eq!(old_c.generation, 0);
    assert_eq!(new_c.generation, 4);
    let honest = AnchorState::Verified(rec_from(&new_c));
    let n = new_env.len();
    assert!(n > ENVELOPE_DIGEST_LEN + 12);

    let mut prefixes = 0_usize;
    for k in 0..n {
        for policy in [AnchorPolicy::Required, AnchorPolicy::Unanchored] {
            let mut target = sample_ctx(STREAM, 2, 0x11);
            let before = fingerprint(&target);
            let got = restore_into_anchored(&mut target, &new_env[..k], policy, &honest);
            assert!(
                matches!(got, Err(AnchorError::Envelope(_))),
                "prefix {k}/{n} {policy:?} got {got:?}"
            );
            assert_unchanged(
                &target,
                &before,
                &format!("truncated prefix {k}/{n} {policy:?}"),
            );
            prefixes += 1;
        }
    }
    assert_eq!(prefixes, n * 2);

    // Trailing extra byte is also rejected (published exact-length rule).
    let mut extra = new_env.clone();
    extra.push(0xFF);
    for policy in [AnchorPolicy::Required, AnchorPolicy::Unanchored] {
        let mut target = sample_ctx(STREAM, 2, 0x11);
        let before = fingerprint(&target);
        assert!(matches!(
            restore_into_anchored(&mut target, &extra, policy, &honest),
            Err(AnchorError::Envelope(_))
        ));
        assert_unchanged(&target, &before, &format!("trailing byte {policy:?}"));
    }

    // Intact new snapshot with a matching anchor is accepted (control).
    let mut target = sample_ctx(STREAM, 2, 0x11);
    assert_eq!(
        restore_into_anchored(&mut target, &new_env, AnchorPolicy::Required, &honest),
        Ok(Admission::Anchored)
    );
    assert_eq!(seal_context(&target).unwrap(), new_env);

    // Intact empty (generation 0) snapshot exists and round-trips under itself.
    let empty_st = AnchorState::Verified(rec_from(&old_c));
    let mut empty_tgt = sample_ctx(STREAM, 2, 0x11);
    assert_eq!(
        restore_into_anchored(&mut empty_tgt, &old_env, AnchorPolicy::Required, &empty_st),
        Ok(Admission::Anchored)
    );
    assert_eq!(empty_tgt.snapshot().stats.admitted, 0);
}

/// Crash boundaries of the two-phase checkpoint: write a durable snapshot,
/// then advance the external anchor. The oracle is a pure function of the
/// crash point and the known identities of the old/new snapshots; it does
/// not call production `admit`.
#[derive(Clone, Copy, Debug)]
enum AtomicCrash {
    BeforeWrite,
    AfterTmpBytes(usize),
    AfterFsync,
    AfterRename,
    AfterAnchor,
}

struct Disk {
    live: Vec<u8>,
    tmp: Option<Vec<u8>>,
    anchor: AnchorRecord,
}

fn apply_atomic(
    old: &[u8],
    new: &[u8],
    old_a: AnchorRecord,
    new_a: AnchorRecord,
    crash: AtomicCrash,
) -> Disk {
    match crash {
        AtomicCrash::BeforeWrite => Disk {
            live: old.to_vec(),
            tmp: None,
            anchor: old_a,
        },
        AtomicCrash::AfterTmpBytes(k) => Disk {
            live: old.to_vec(),
            tmp: Some(new[..k].to_vec()),
            anchor: old_a,
        },
        AtomicCrash::AfterFsync => Disk {
            live: old.to_vec(),
            tmp: Some(new.to_vec()),
            anchor: old_a,
        },
        AtomicCrash::AfterRename => Disk {
            live: new.to_vec(),
            tmp: None,
            anchor: old_a,
        },
        AtomicCrash::AfterAnchor => Disk {
            live: new.to_vec(),
            tmp: None,
            anchor: new_a,
        },
    }
}

fn oracle_atomic_crash(crash: AtomicCrash, policy: OPolicy, old_n: u64, new_n: u64) -> OOut {
    // Durable observation: tmp is discarded on restart unless renamed.
    let (cand_n, anc_n) = match crash {
        AtomicCrash::BeforeWrite | AtomicCrash::AfterTmpBytes(_) | AtomicCrash::AfterFsync => {
            (old_n, old_n)
        }
        AtomicCrash::AfterRename => (new_n, old_n),
        AtomicCrash::AfterAnchor => (new_n, new_n),
    };
    oracle_admit(
        policy,
        OAnchor::Verified(ident(STREAM, anc_n, 0)),
        ident(STREAM, cand_n, 0),
    )
}

#[test]
fn all_crash_boundaries_match_independent_oracle_and_preserve_state_on_reject() {
    let (old_env, new_env, old_c, new_c) = sealed_pair(0, 5, 0xE4);
    assert_eq!((old_c.generation, new_c.generation), (0, 5));
    let old_a = rec_from(&old_c);
    let new_a = rec_from(&new_c);
    let n = new_env.len();

    let mut crashes: Vec<AtomicCrash> = vec![AtomicCrash::BeforeWrite];
    crashes.extend((0..=n).map(AtomicCrash::AfterTmpBytes));
    crashes.push(AtomicCrash::AfterFsync);
    crashes.push(AtomicCrash::AfterRename);
    crashes.push(AtomicCrash::AfterAnchor);
    // Frozen size: start + every tmp prefix including full + fsync + rename + anchor.
    assert_eq!(crashes.len(), 1 + (n + 1) + 3);

    let mut exercised = 0_usize;
    for crash in crashes {
        for policy in [OPolicy::Required, OPolicy::Unanchored] {
            let disk = apply_atomic(&old_env, &new_env, old_a, new_a, crash);
            if let AtomicCrash::AfterTmpBytes(k) = crash {
                assert_eq!(disk.tmp.as_ref().map(Vec::len), Some(k));
            }
            let expect = oracle_atomic_crash(crash, policy, 0, 5);
            let mut target = sample_ctx(STREAM, 2, 0x22);
            let before = fingerprint(&target);
            let classified = classify(restore_into_anchored(
                &mut target,
                &disk.live,
                prod_policy(policy),
                &AnchorState::Verified(disk.anchor),
            ));
            assert_eq!(classified, expect, "{crash:?} {policy:?}");
            match expect {
                OOut::Anchored | OOut::Unanchored => {
                    let want = match crash {
                        AtomicCrash::AfterRename | AtomicCrash::AfterAnchor => &new_env,
                        _ => &old_env,
                    };
                    assert_eq!(
                        seal_context(&target).unwrap(),
                        *want,
                        "{crash:?} {policy:?}"
                    );
                }
                _ => assert_unchanged(&target, &before, &format!("{crash:?} {policy:?}")),
            }
            exercised += 1;
        }
    }

    // In-place torn overwrite of `live` (non-atomic protocol): every proper
    // prefix is an envelope failure; the full new image with the old anchor
    // is `Stale` under Required and `Unanchored` under Unanchored.
    for k in 0..=n {
        for policy in [OPolicy::Required, OPolicy::Unanchored] {
            let live = new_env[..k].to_vec();
            let expect = if k < n {
                OOut::Envelope
            } else {
                oracle_admit(
                    policy,
                    OAnchor::Verified(ident(STREAM, 0, 0)),
                    ident(STREAM, 5, 0),
                )
            };
            let mut target = sample_ctx(STREAM, 2, 0x22);
            let before = fingerprint(&target);
            let classified = classify(restore_into_anchored(
                &mut target,
                &live,
                prod_policy(policy),
                &AnchorState::Verified(old_a),
            ));
            assert_eq!(classified, expect, "inplace k={k} {policy:?}");
            if matches!(expect, OOut::Unanchored) {
                assert_eq!(seal_context(&target).unwrap(), new_env);
            } else {
                assert_unchanged(&target, &before, &format!("inplace k={k} {policy:?}"));
            }
            exercised += 1;
        }
    }

    // Stale-disk substitution after a successful anchor advance: live is
    // replaced by the intact older snapshot.
    for policy in [OPolicy::Required, OPolicy::Unanchored] {
        let expect = oracle_admit(
            policy,
            OAnchor::Verified(ident(STREAM, 5, 0)),
            ident(STREAM, 0, 0),
        );
        let mut target = sample_ctx(STREAM, 2, 0x22);
        let before = fingerprint(&target);
        let classified = classify(restore_into_anchored(
            &mut target,
            &old_env,
            prod_policy(policy),
            &AnchorState::Verified(new_a),
        ));
        assert_eq!(classified, expect, "stale-disk {policy:?}");
        match expect {
            OOut::Unanchored => assert_eq!(seal_context(&target).unwrap(), old_env),
            _ => assert_unchanged(&target, &before, "stale-disk Required"),
        }
        exercised += 1;
    }

    assert!(exercised > 0);
}

#[test]
fn unanchored_mode_accepts_intact_rollback() {
    let (old_env, new_env, old_c, new_c) = sealed_pair(2, 6, 0xF1);
    assert!(old_c.generation < new_c.generation);
    let latest = AnchorState::Verified(rec_from(&new_c));

    // Required + honest latest anchor rejects the older intact snapshot.
    let mut required = sample_ctx(STREAM, 4, 0x33);
    let before = fingerprint(&required);
    assert_eq!(
        restore_into_anchored(&mut required, &old_env, AnchorPolicy::Required, &latest),
        Err(AnchorError::Rollback {
            anchor: 6,
            candidate: 2
        })
    );
    assert_unchanged(&required, &before, "required rollback");

    // Unanchored accepts the same intact older snapshot, even when the
    // caller also supplies the latest Verified anchor — the policy ignores it.
    let mut unanchored = sample_ctx(STREAM, 4, 0x33);
    assert_eq!(
        restore_into_anchored(&mut unanchored, &old_env, AnchorPolicy::Unanchored, &latest),
        Ok(Admission::Unanchored)
    );
    assert_eq!(seal_context(&unanchored).unwrap(), old_env);
    assert_ne!(old_env, new_env);
}

/// LIMITATION (documented, not a product defect): `AnchorState::Verified` is a
/// caller assertion. `working_context_anchor` never authenticates, attests,
/// provisions or binds the record to a trusted store. A caller who constructs
/// `Verified(forged)` can make any intact snapshot appear `Anchored`, including
/// an older snapshot that an honest latest anchor would reject as `Rollback`.
/// Authentication of anchors remains **NOT QUALIFIED**.
#[test]
fn caller_supplied_forged_verified_anchors_are_not_authenticated() {
    let (old_env, new_env, old_c, new_c) = sealed_pair(2, 6, 0xF2);
    let honest = AnchorState::Verified(rec_from(&new_c));
    let forged_as_old = AnchorState::Verified(rec_from(&old_c));

    // Honest latest anchor: rollback is refused and state is untouched.
    let mut honest_tgt = sample_ctx(STREAM, 3, 0x44);
    let honest_before = fingerprint(&honest_tgt);
    assert_eq!(
        restore_into_anchored(&mut honest_tgt, &old_env, AnchorPolicy::Required, &honest),
        Err(AnchorError::Rollback {
            anchor: 6,
            candidate: 2
        })
    );
    assert_unchanged(&honest_tgt, &honest_before, "honest reject of rollback");

    // Forged Verified matching the older snapshot: the same rollback is
    // accepted as Anchored. This is the limitation this test exists to show.
    let mut forged_tgt = sample_ctx(STREAM, 3, 0x44);
    assert_eq!(
        restore_into_anchored(
            &mut forged_tgt,
            &old_env,
            AnchorPolicy::Required,
            &forged_as_old
        ),
        Ok(Admission::Anchored)
    );
    assert_eq!(seal_context(&forged_tgt).unwrap(), old_env);

    // Forged Verified can also deny a legitimate latest snapshot by claiming
    // a conflicting digest at the same generation (still not authentication).
    let mut bogus = new_c;
    bogus.digest[0] ^= 0xFF;
    let forged_conflict = AnchorState::Verified(rec_from(&bogus));
    let mut deny_tgt = sample_ctx(STREAM, 3, 0x44);
    let deny_before = fingerprint(&deny_tgt);
    assert_eq!(
        restore_into_anchored(
            &mut deny_tgt,
            &new_env,
            AnchorPolicy::Required,
            &forged_conflict
        ),
        Err(AnchorError::ConflictingHistory { generation: 6 })
    );
    assert_unchanged(&deny_tgt, &deny_before, "forged conflict deny");

    // Absent / Unverifiable cannot be "forged into success": Required fails
    // closed, which is the contrast that shows only Verified is trusted-as-is.
    for st in [AnchorState::Absent, AnchorState::Unverifiable] {
        let mut t = sample_ctx(STREAM, 3, 0x44);
        let before = fingerprint(&t);
        assert!(restore_into_anchored(&mut t, &new_env, AnchorPolicy::Required, &st).is_err());
        assert_unchanged(&t, &before, "absent/unverifiable stay fail-closed");
    }
}
