//! CONTEXT-2 G12: optional opt-in anchor provenance / identity policy.
//!
//! Independent pure oracle: `oracle_admit_provenance` in this file uses
//! its own types and the published G12 decision order. It does **not**
//! call production `admit_provenance`, `admit_with_provenance`,
//! `sml_store::working_context_anchor::admit` or `advance`. Production
//! results are classified and compared row-by-row against the oracle on
//! a versioned frozen vector table
//! ([`sml_store::working_context_anchor_provenance::ANCHOR_PROVENANCE_POLICY_VERSION`] = 1).
//!
//! Coverage (frozen):
//! - positive exact match of stream, authority and digest domain;
//! - negative cross-stream (presented, derived, or both);
//! - authority-identifier mismatch;
//! - digest-domain confusion (`SWE1` vs a distinct `OTHR` tag);
//! - priority: stream, then authority, then domain;
//! - [`ProvenancePolicy::Unbound`] ignores every mismatch (G10 default);
//! - live SWE1 restore: reject leaves encoding and `stats.admitted`
//!   unchanged; accept replaces the target exactly and replays nothing;
//! - caller-constructed matching [`AuthorityId`] is **not** authenticated.
//!
//! Anchors and authority tags in this suite are mocks constructed
//! in-process. SWE1 is unkeyed SHA-256, not authentication. No
//! attestation, hardware, E2E or transport claim is made. `NOT_MEASURED`.

use sml_core::context_ledger::{ContextContentHash, ContextSegmentId, ContextStreamId};
use sml_store::working_context::{WorkingContext, WorkingPolicy, WorkingSegment};
use sml_store::working_context_anchor::{
    admit, advance, restore_into_anchored, Admission, AnchorError, AnchorPolicy, AnchorRecord,
    AnchorState, Candidate,
};
use sml_store::working_context_anchor_provenance::{
    admit_provenance, admit_with_provenance, derived_binding,
    restore_into_anchored_with_provenance, AnchorIdentity, AuthorityId, DerivedBinding,
    DigestDomain, ProvenanceAdmission, ProvenanceError, ProvenancePolicy, ProvenanceRestoreError,
    ANCHOR_PROVENANCE_POLICY_VERSION, DIGEST_DOMAIN_SWE1,
};
use sml_store::working_context_envelope::seal_context;
use sml_store::working_context_wire::encode_context;

/// Pinned stream used for live `WorkingContext` fixtures (must be nonzero).
const STREAM: u64 = 0x4742_0000_0000_0012;
const OTHER: u64 = 0x4742_0000_0000_0023;

const STREAMS: [u64; 4] = [0, 1, STREAM, u64::MAX];
const AUTH_TAGS: [u8; 2] = [0xA1, 0xB2];
const DOMAINS: [DigestDomain; 2] = [DIGEST_DOMAIN_SWE1, DigestDomain(*b"OTHR")];

// ---------------------------------------------------------------------------
// Independent oracle types and decision procedure.
// These types are deliberately *not* the production enums.
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OPolicy {
    Unbound,
    Bound,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct OIdent {
    stream: u64,
    authority: u8,
    domain: DigestDomain,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ODerived {
    stream: u64,
    domain: DigestDomain,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OOut {
    Unbound,
    Bound,
    Absent,
    StreamBindingMismatch {
        expected: u64,
        presented: u64,
        derived: Option<u64>,
    },
    AuthorityMismatch {
        expected: u8,
        presented: u8,
    },
    DigestDomainMismatch {
        expected: DigestDomain,
        presented: DigestDomain,
        derived: Option<DigestDomain>,
    },
}

/// Pure provenance oracle. Priority-ordered rules from the G12 module:
///
/// 1. `Unbound` → `Unbound` (presented and derived are ignored).
/// 2. `Bound` + no presented identity → `Absent`.
/// 3. stream mismatch (expected vs presented, or expected vs derived)
///    → `StreamBindingMismatch`.
/// 4. authority mismatch → `AuthorityMismatch`.
/// 5. digest-domain mismatch (expected vs presented, or expected vs
///    derived) → `DigestDomainMismatch`.
/// 6. otherwise `Bound`.
fn oracle_admit_provenance(
    policy: OPolicy,
    expected: OIdent,
    presented: Option<OIdent>,
    derived: Option<ODerived>,
) -> OOut {
    if matches!(policy, OPolicy::Unbound) {
        return OOut::Unbound;
    }
    let presented = match presented {
        Some(p) => p,
        None => return OOut::Absent,
    };
    let derived_stream = derived.map(|d| d.stream);
    let stream_ok = presented.stream == expected.stream
        && derived_stream
            .map(|stream| stream == expected.stream)
            .unwrap_or(true);
    if !stream_ok {
        return OOut::StreamBindingMismatch {
            expected: expected.stream,
            presented: presented.stream,
            derived: derived_stream,
        };
    }
    if presented.authority != expected.authority {
        return OOut::AuthorityMismatch {
            expected: expected.authority,
            presented: presented.authority,
        };
    }
    let derived_domain = derived.map(|d| d.domain);
    let domain_ok = presented.domain == expected.domain
        && derived_domain
            .map(|domain| domain == expected.domain)
            .unwrap_or(true);
    if !domain_ok {
        return OOut::DigestDomainMismatch {
            expected: expected.domain,
            presented: presented.domain,
            derived: derived_domain,
        };
    }
    OOut::Bound
}

fn prod_authority(tag: u8) -> AuthorityId {
    AuthorityId::repeating(tag)
}

fn prod_ident(id: OIdent) -> AnchorIdentity {
    AnchorIdentity {
        stream_id: id.stream,
        authority: prod_authority(id.authority),
        digest_domain: id.domain,
    }
}

fn prod_derived(d: ODerived) -> DerivedBinding {
    DerivedBinding {
        stream_id: d.stream,
        digest_domain: d.domain,
    }
}

fn prod_policy(policy: OPolicy, expected: OIdent) -> ProvenancePolicy {
    match policy {
        OPolicy::Unbound => ProvenancePolicy::Unbound,
        OPolicy::Bound => ProvenancePolicy::Bound(prod_ident(expected)),
    }
}

fn classify(r: Result<ProvenanceAdmission, ProvenanceError>) -> OOut {
    match r {
        Ok(ProvenanceAdmission::Unbound) => OOut::Unbound,
        Ok(ProvenanceAdmission::Bound) => OOut::Bound,
        Err(ProvenanceError::ProvenanceAbsent) => OOut::Absent,
        Err(ProvenanceError::StreamBindingMismatch {
            expected,
            presented,
            derived,
        }) => OOut::StreamBindingMismatch {
            expected,
            presented,
            derived,
        },
        Err(ProvenanceError::AuthorityMismatch {
            expected,
            presented,
        }) => OOut::AuthorityMismatch {
            expected: expected.0[0],
            presented: presented.0[0],
        },
        Err(ProvenanceError::DigestDomainMismatch {
            expected,
            presented,
            derived,
        }) => OOut::DigestDomainMismatch {
            expected,
            presented,
            derived,
        },
    }
}

fn run_prod(
    policy: OPolicy,
    expected: OIdent,
    presented: Option<OIdent>,
    derived: Option<ODerived>,
) -> OOut {
    let presented_id = presented.map(prod_ident);
    let derived_id = derived.map(prod_derived);
    classify(admit_provenance(
        prod_policy(policy, expected),
        presented_id.as_ref(),
        derived_id.as_ref(),
    ))
}

fn ident(stream: u64, authority: u8, domain: DigestDomain) -> OIdent {
    OIdent {
        stream,
        authority,
        domain,
    }
}

fn der(stream: u64, domain: DigestDomain) -> ODerived {
    ODerived { stream, domain }
}

// ---------------------------------------------------------------------------
// Frozen named golden vectors (spec exemplars, not derived from production).
// ---------------------------------------------------------------------------

struct NamedProv {
    version: u16,
    name: &'static str,
    policy: OPolicy,
    expected: OIdent,
    presented: Option<OIdent>,
    derived: Option<ODerived>,
    expect: OOut,
}

fn named_provenance_vectors() -> Vec<NamedProv> {
    let swe1 = DIGEST_DOMAIN_SWE1;
    let othr = DigestDomain(*b"OTHR");
    let exp = ident(STREAM, 0xA1, swe1);
    vec![
        NamedProv {
            version: ANCHOR_PROVENANCE_POLICY_VERSION,
            name: "unbound_ignores_absent_presented",
            policy: OPolicy::Unbound,
            expected: exp,
            presented: None,
            derived: None,
            expect: OOut::Unbound,
        },
        NamedProv {
            version: ANCHOR_PROVENANCE_POLICY_VERSION,
            name: "unbound_ignores_cross_stream_foreign_authority_and_domain",
            policy: OPolicy::Unbound,
            expected: exp,
            presented: Some(ident(0, 0xB2, othr)),
            derived: Some(der(u64::MAX, othr)),
            expect: OOut::Unbound,
        },
        NamedProv {
            version: ANCHOR_PROVENANCE_POLICY_VERSION,
            name: "bound_absent_presented_fails_closed",
            policy: OPolicy::Bound,
            expected: exp,
            presented: None,
            derived: Some(der(STREAM, swe1)),
            expect: OOut::Absent,
        },
        NamedProv {
            version: ANCHOR_PROVENANCE_POLICY_VERSION,
            name: "bound_exact_match_no_derived",
            policy: OPolicy::Bound,
            expected: exp,
            presented: Some(exp),
            derived: None,
            expect: OOut::Bound,
        },
        NamedProv {
            version: ANCHOR_PROVENANCE_POLICY_VERSION,
            name: "bound_exact_match_with_matching_derived",
            policy: OPolicy::Bound,
            expected: exp,
            presented: Some(exp),
            derived: Some(der(STREAM, swe1)),
            expect: OOut::Bound,
        },
        NamedProv {
            version: ANCHOR_PROVENANCE_POLICY_VERSION,
            name: "bound_cross_stream_presented",
            policy: OPolicy::Bound,
            expected: exp,
            presented: Some(ident(OTHER, 0xA1, swe1)),
            derived: Some(der(STREAM, swe1)),
            expect: OOut::StreamBindingMismatch {
                expected: STREAM,
                presented: OTHER,
                derived: Some(STREAM),
            },
        },
        NamedProv {
            version: ANCHOR_PROVENANCE_POLICY_VERSION,
            name: "bound_cross_stream_derived",
            policy: OPolicy::Bound,
            expected: exp,
            presented: Some(exp),
            derived: Some(der(OTHER, swe1)),
            expect: OOut::StreamBindingMismatch {
                expected: STREAM,
                presented: STREAM,
                derived: Some(OTHER),
            },
        },
        NamedProv {
            version: ANCHOR_PROVENANCE_POLICY_VERSION,
            name: "bound_cross_stream_zero_vs_max",
            policy: OPolicy::Bound,
            expected: ident(0, 0xA1, swe1),
            presented: Some(ident(u64::MAX, 0xA1, swe1)),
            derived: None,
            expect: OOut::StreamBindingMismatch {
                expected: 0,
                presented: u64::MAX,
                derived: None,
            },
        },
        NamedProv {
            version: ANCHOR_PROVENANCE_POLICY_VERSION,
            name: "bound_authority_mismatch_same_stream_same_domain",
            policy: OPolicy::Bound,
            expected: exp,
            presented: Some(ident(STREAM, 0xB2, swe1)),
            derived: Some(der(STREAM, swe1)),
            expect: OOut::AuthorityMismatch {
                expected: 0xA1,
                presented: 0xB2,
            },
        },
        NamedProv {
            version: ANCHOR_PROVENANCE_POLICY_VERSION,
            name: "bound_domain_confusion_presented_othr",
            policy: OPolicy::Bound,
            expected: exp,
            presented: Some(ident(STREAM, 0xA1, othr)),
            derived: Some(der(STREAM, swe1)),
            expect: OOut::DigestDomainMismatch {
                expected: swe1,
                presented: othr,
                derived: Some(swe1),
            },
        },
        NamedProv {
            version: ANCHOR_PROVENANCE_POLICY_VERSION,
            name: "bound_domain_confusion_derived_othr",
            policy: OPolicy::Bound,
            expected: exp,
            presented: Some(exp),
            derived: Some(der(STREAM, othr)),
            expect: OOut::DigestDomainMismatch {
                expected: swe1,
                presented: swe1,
                derived: Some(othr),
            },
        },
        NamedProv {
            version: ANCHOR_PROVENANCE_POLICY_VERSION,
            name: "bound_domain_confusion_expected_othr_derived_swe1",
            policy: OPolicy::Bound,
            expected: ident(STREAM, 0xA1, othr),
            presented: Some(ident(STREAM, 0xA1, othr)),
            derived: Some(der(STREAM, swe1)),
            expect: OOut::DigestDomainMismatch {
                expected: othr,
                presented: othr,
                derived: Some(swe1),
            },
        },
        NamedProv {
            version: ANCHOR_PROVENANCE_POLICY_VERSION,
            name: "bound_stream0_authority_match_swe1",
            policy: OPolicy::Bound,
            expected: ident(0, 0xA1, swe1),
            presented: Some(ident(0, 0xA1, swe1)),
            derived: Some(der(0, swe1)),
            expect: OOut::Bound,
        },
        NamedProv {
            version: ANCHOR_PROVENANCE_POLICY_VERSION,
            name: "bound_stream_max_authority_b_swe1",
            policy: OPolicy::Bound,
            expected: ident(u64::MAX, 0xB2, swe1),
            presented: Some(ident(u64::MAX, 0xB2, swe1)),
            derived: Some(der(u64::MAX, swe1)),
            expect: OOut::Bound,
        },
        NamedProv {
            version: ANCHOR_PROVENANCE_POLICY_VERSION,
            name: "bound_stream_mismatch_masks_authority_and_domain",
            policy: OPolicy::Bound,
            expected: exp,
            presented: Some(ident(OTHER, 0xB2, othr)),
            derived: Some(der(OTHER, othr)),
            expect: OOut::StreamBindingMismatch {
                expected: STREAM,
                presented: OTHER,
                derived: Some(OTHER),
            },
        },
        NamedProv {
            version: ANCHOR_PROVENANCE_POLICY_VERSION,
            name: "bound_authority_mismatch_masks_domain_confusion",
            policy: OPolicy::Bound,
            expected: exp,
            presented: Some(ident(STREAM, 0xB2, othr)),
            derived: Some(der(STREAM, othr)),
            expect: OOut::AuthorityMismatch {
                expected: 0xA1,
                presented: 0xB2,
            },
        },
    ]
}

#[test]
fn frozen_named_vectors_are_versioned_and_match_independent_oracle() {
    assert_eq!(ANCHOR_PROVENANCE_POLICY_VERSION, 1);
    let rows = named_provenance_vectors();
    assert_eq!(rows.len(), 16);
    for row in &rows {
        assert_eq!(
            row.version, ANCHOR_PROVENANCE_POLICY_VERSION,
            "{}",
            row.name
        );
        let oracle = oracle_admit_provenance(row.policy, row.expected, row.presented, row.derived);
        assert_eq!(oracle, row.expect, "oracle {}", row.name);
        assert_eq!(
            run_prod(row.policy, row.expected, row.presented, row.derived),
            row.expect,
            "production {}",
            row.name
        );
    }
}

#[test]
fn cartesian_cross_stream_authority_and_domain_matches_oracle() {
    let mut rows = 0_usize;
    for policy in [OPolicy::Unbound, OPolicy::Bound] {
        for expected_stream in STREAMS {
            for expected_auth in AUTH_TAGS {
                for expected_domain in DOMAINS {
                    let expected = ident(expected_stream, expected_auth, expected_domain);
                    // Absent presented: one derived=None and one matching derived.
                    for derived in [None, Some(der(expected_stream, expected_domain))] {
                        let oracle = oracle_admit_provenance(policy, expected, None, derived);
                        assert_eq!(
                            run_prod(policy, expected, None, derived),
                            oracle,
                            "{policy:?} absent {expected:?} {derived:?}"
                        );
                        rows += 1;
                    }
                    for presented_stream in STREAMS {
                        for presented_auth in AUTH_TAGS {
                            for presented_domain in DOMAINS {
                                let presented =
                                    ident(presented_stream, presented_auth, presented_domain);
                                let derived_cases = [
                                    None,
                                    Some(der(expected_stream, expected_domain)),
                                    Some(der(presented_stream, presented_domain)),
                                    Some(der(
                                        if presented_stream == expected_stream {
                                            OTHER
                                        } else {
                                            expected_stream
                                        },
                                        if presented_domain == expected_domain {
                                            DigestDomain(*b"OTHR")
                                        } else {
                                            expected_domain
                                        },
                                    )),
                                ];
                                for derived in derived_cases {
                                    let oracle = oracle_admit_provenance(
                                        policy,
                                        expected,
                                        Some(presented),
                                        derived,
                                    );
                                    assert_eq!(
                                        run_prod(
                                            policy,
                                            expected,
                                            Some(presented),
                                            derived
                                        ),
                                        oracle,
                                        "{policy:?} exp={expected:?} pres={presented:?} der={derived:?}"
                                    );
                                    rows += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    // 2 policies * 4 streams * 2 auths * 2 domains *
    // (2 absent-presented + 4*2*2 presented * 4 derived cases)
    assert_eq!(rows, 2 * 4 * 2 * 2 * (2 + 4 * 2 * 2 * 4));
}

// ---------------------------------------------------------------------------
// Live restore fixtures
// ---------------------------------------------------------------------------

fn sample_ctx(stream: u64, n: u64, salt: u8) -> WorkingContext {
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

fn fingerprint(ctx: &WorkingContext) -> (Vec<u8>, u64) {
    (encode_context(ctx).unwrap(), ctx.snapshot().stats.admitted)
}

fn assert_unchanged(ctx: &WorkingContext, before: &(Vec<u8>, u64), label: &str) {
    assert_eq!(encode_context(ctx).unwrap(), before.0, "{label} encoding");
    assert_eq!(ctx.snapshot().stats.admitted, before.1, "{label} admitted");
}

fn identity_for(stream: u64, auth: u8, domain: DigestDomain) -> AnchorIdentity {
    AnchorIdentity {
        stream_id: stream,
        authority: AuthorityId::repeating(auth),
        digest_domain: domain,
    }
}

fn rec_from(cand: &Candidate) -> AnchorRecord {
    AnchorRecord {
        stream_id: cand.stream_id,
        generation: cand.generation,
        digest: cand.digest,
    }
}

#[test]
fn unbound_is_a_noop_and_leaves_g10_default_unchanged() {
    let expected = identity_for(STREAM, 0xA1, DIGEST_DOMAIN_SWE1);
    let foreign = identity_for(OTHER, 0xB2, DigestDomain(*b"OTHR"));
    let derived = DerivedBinding {
        stream_id: OTHER,
        digest_domain: DigestDomain(*b"OTHR"),
    };
    assert_eq!(
        admit_provenance(ProvenancePolicy::Unbound, Some(&foreign), Some(&derived)),
        Ok(ProvenanceAdmission::Unbound)
    );

    let live = sample_ctx(STREAM, 4, 0x11);
    let env = seal_context(&live).unwrap();
    let (_, cand) = sml_store::working_context_anchor::candidate_of(&env).unwrap();
    let extracted = derived_binding(&cand, &env);
    assert_eq!(extracted.stream_id, STREAM);
    assert_eq!(extracted.digest_domain, DIGEST_DOMAIN_SWE1);
    assert_eq!(&env[..4], b"SWE1");

    let st = AnchorState::Verified(rec_from(&cand));
    let g10 = admit(AnchorPolicy::Required, &st, &cand);
    let combined = admit_with_provenance(
        AnchorPolicy::Required,
        &st,
        &cand,
        ProvenancePolicy::Unbound,
        Some(&foreign),
        Some(&derived),
    );
    assert_eq!(g10, Ok(Admission::Anchored));
    assert_eq!(
        combined,
        Ok((Admission::Anchored, ProvenanceAdmission::Unbound))
    );

    let rollback = seal_context(&sample_ctx(STREAM, 1, 0x11)).unwrap();
    let mut t_g10 = sample_ctx(STREAM, 2, 0x22);
    let mut t_g12 = sample_ctx(STREAM, 2, 0x22);
    let before = fingerprint(&t_g10);
    let r_g10 = restore_into_anchored(&mut t_g10, &rollback, AnchorPolicy::Required, &st);
    let r_g12 = restore_into_anchored_with_provenance(
        &mut t_g12,
        &rollback,
        AnchorPolicy::Required,
        &st,
        ProvenancePolicy::Unbound,
        Some(&expected),
    );
    assert!(matches!(r_g10, Err(AnchorError::Rollback { .. })));
    assert!(matches!(
        r_g12,
        Err(ProvenanceRestoreError::Anchor(AnchorError::Rollback { .. }))
    ));
    assert_unchanged(&t_g10, &before, "g10 rollback");
    assert_unchanged(&t_g12, &before, "g12 unbound rollback");
    assert_eq!(expected.stream_id, STREAM);
}

#[test]
fn restore_rejects_cross_stream_authority_and_domain_without_mutation() {
    let live = sample_ctx(STREAM, 3, 0x33);
    let env = seal_context(&live).unwrap();
    let (_, cand) = sml_store::working_context_anchor::candidate_of(&env).unwrap();
    let st = AnchorState::Verified(rec_from(&cand));
    let matching = identity_for(STREAM, 0xA1, DIGEST_DOMAIN_SWE1);
    let bound = ProvenancePolicy::Bound(matching);

    let reject = |label: &str, presented: Option<&AnchorIdentity>, expect: ProvenanceError| {
        let mut target = sample_ctx(STREAM, 1, 0x44);
        let before = fingerprint(&target);
        let err = restore_into_anchored_with_provenance(
            &mut target,
            &env,
            AnchorPolicy::Required,
            &st,
            bound,
            presented,
        )
        .expect_err(label);
        match err {
            ProvenanceRestoreError::Provenance(got) => {
                assert_eq!(got, expect, "{label}")
            }
            other => panic!("{label}: {other:?}"),
        }
        assert_unchanged(&target, &before, label);
    };

    reject("absent", None, ProvenanceError::ProvenanceAbsent);
    reject(
        "cross-stream",
        Some(&identity_for(OTHER, 0xA1, DIGEST_DOMAIN_SWE1)),
        ProvenanceError::StreamBindingMismatch {
            expected: STREAM,
            presented: OTHER,
            derived: Some(STREAM),
        },
    );
    reject(
        "authority",
        Some(&identity_for(STREAM, 0xB2, DIGEST_DOMAIN_SWE1)),
        ProvenanceError::AuthorityMismatch {
            expected: AuthorityId::repeating(0xA1),
            presented: AuthorityId::repeating(0xB2),
        },
    );
    reject(
        "domain",
        Some(&identity_for(STREAM, 0xA1, DigestDomain(*b"OTHR"))),
        ProvenanceError::DigestDomainMismatch {
            expected: DIGEST_DOMAIN_SWE1,
            presented: DigestDomain(*b"OTHR"),
            derived: Some(DIGEST_DOMAIN_SWE1),
        },
    );

    // Snapshot from another stream, presented as if it belonged here.
    let other_env = seal_context(&sample_ctx(OTHER, 3, 0x33)).unwrap();
    let mut target = sample_ctx(STREAM, 1, 0x44);
    let before = fingerprint(&target);
    let err = restore_into_anchored_with_provenance(
        &mut target,
        &other_env,
        AnchorPolicy::Unanchored,
        &AnchorState::Absent,
        bound,
        Some(&matching),
    )
    .expect_err("live cross-stream snapshot");
    assert!(matches!(
        err,
        ProvenanceRestoreError::Provenance(ProvenanceError::StreamBindingMismatch {
            expected: STREAM,
            presented: STREAM,
            derived: Some(OTHER)
        })
    ));
    assert_unchanged(&target, &before, "live cross-stream snapshot");
}

#[test]
fn restore_accepts_exact_binding_and_replays_nothing() {
    let live = sample_ctx(STREAM, 4, 0x55);
    let env = seal_context(&live).unwrap();
    let (_, cand) = sml_store::working_context_anchor::candidate_of(&env).unwrap();
    let st = AnchorState::Verified(advance(None, &cand).unwrap());
    let matching = identity_for(STREAM, 0xA1, DIGEST_DOMAIN_SWE1);
    let mut target = sample_ctx(STREAM, 9, 0x66);
    for _ in 0..3 {
        assert_eq!(
            restore_into_anchored_with_provenance(
                &mut target,
                &env,
                AnchorPolicy::Required,
                &st,
                ProvenancePolicy::Bound(matching),
                Some(&matching),
            ),
            Ok((Admission::Anchored, ProvenanceAdmission::Bound))
        );
        assert_eq!(target.snapshot().stats.admitted, 4);
        assert_eq!(seal_context(&target).unwrap(), env);
    }
}

#[test]
fn absent_presented_and_corrupt_inputs_fail_closed_without_mutation() {
    let good = seal_context(&sample_ctx(STREAM, 3, 0x77)).unwrap();
    let (_, cand) = sml_store::working_context_anchor::candidate_of(&good).unwrap();
    let st = AnchorState::Verified(rec_from(&cand));
    let matching = identity_for(STREAM, 0xA1, DIGEST_DOMAIN_SWE1);
    let mut corrupt = good.clone();
    corrupt[20] ^= 0x08;
    let bound = ProvenancePolicy::Bound(matching);

    let cases: Vec<(&str, &[u8], Option<&AnchorIdentity>)> = vec![
        ("absent", good.as_slice(), None),
        ("truncated", &good[..good.len() - 1], Some(&matching)),
        ("corrupt", corrupt.as_slice(), Some(&matching)),
    ];
    for (label, bytes, presented) in cases {
        let mut target = sample_ctx(STREAM, 2, 0x88);
        let before = fingerprint(&target);
        assert!(
            restore_into_anchored_with_provenance(
                &mut target,
                bytes,
                AnchorPolicy::Required,
                &st,
                bound,
                presented,
            )
            .is_err(),
            "{label}"
        );
        assert_unchanged(&target, &before, label);
    }
}

/// LIMITATION (documented, not a product defect): `AuthorityId` is a
/// caller assertion compared for equality. This module never
/// authenticates, signs, attests or binds the identifier to a key or
/// hardware root. A caller who constructs the expected bytes is
/// accepted as `Bound`. Authentication of provenance remains
/// **NOT QUALIFIED**. SWE1 stays unkeyed SHA-256.
#[test]
fn caller_supplied_authority_is_not_authenticated() {
    let live = sample_ctx(STREAM, 3, 0x99);
    let env = seal_context(&live).unwrap();
    let (_, cand) = sml_store::working_context_anchor::candidate_of(&env).unwrap();
    let st = AnchorState::Verified(rec_from(&cand));
    let expected = identity_for(STREAM, 0xA1, DIGEST_DOMAIN_SWE1);
    let foreign = identity_for(STREAM, 0xB2, DIGEST_DOMAIN_SWE1);

    let mut denied = sample_ctx(STREAM, 1, 0x10);
    let before = fingerprint(&denied);
    assert!(matches!(
        restore_into_anchored_with_provenance(
            &mut denied,
            &env,
            AnchorPolicy::Required,
            &st,
            ProvenancePolicy::Bound(expected),
            Some(&foreign),
        ),
        Err(ProvenanceRestoreError::Provenance(
            ProvenanceError::AuthorityMismatch { .. }
        ))
    ));
    assert_unchanged(&denied, &before, "foreign authority");

    // Anyone who *constructs* the expected AuthorityId bytes is accepted.
    // There is no signature check that would distinguish an honest
    // presenter from a forger of the same 32 bytes.
    let forged_matching = identity_for(STREAM, 0xA1, DIGEST_DOMAIN_SWE1);
    assert_eq!(forged_matching, expected);
    let mut accepted = sample_ctx(STREAM, 1, 0x10);
    assert_eq!(
        restore_into_anchored_with_provenance(
            &mut accepted,
            &env,
            AnchorPolicy::Required,
            &st,
            ProvenancePolicy::Bound(expected),
            Some(&forged_matching),
        ),
        Ok((Admission::Anchored, ProvenanceAdmission::Bound))
    );
    assert_eq!(seal_context(&accepted).unwrap(), env);

    // Claiming the SWE1 domain tag does not authenticate the digest:
    // the tag is compared for equality only. The envelope digest remains
    // unkeyed SHA-256.
    assert_eq!(DIGEST_DOMAIN_SWE1, DigestDomain(*b"SWE1"));
}
