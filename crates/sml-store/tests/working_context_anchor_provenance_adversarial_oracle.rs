//! CONTEXT-2 G13: adversarial differential tests for the G12 provenance policy.
//!
//! Independent pure oracle: the decision procedures in this file
//! (`oracle_admit_provenance`, `oracle_admit`, `oracle_advance`,
//! `oracle_admit_with_provenance`, `oracle_derived_binding`) implement the
//! published G12 + G10 contracts with their own types and priority-ordered
//! rules. They do **not** call production `admit_provenance`,
//! `admit_with_provenance`, `derived_binding`, or
//! `sml_store::working_context_anchor::{admit, advance}`. Production
//! results are classified and compared row-by-row against the oracle on a
//! versioned frozen vector table
//! ([`sml_store::working_context_anchor_provenance::ANCHOR_PROVENANCE_POLICY_VERSION`] = 1).
//!
//! Coverage (frozen):
//! - stream-id boundaries `0`, `1`, pinned `STREAM`, `u64::MAX`, and
//!   cross-stream pairs (presented, derived, or both);
//! - authority-id near-collisions: identifiers that share a long prefix
//!   but differ in a later octet are a mismatch, not a binding;
//! - digest-domain confusion, including prefix-sharing tags (`SWE1` vs
//!   `SWE2`) and the all-zero domain;
//! - absent presented identity, and derived bindings taken from empty,
//!   short or prefix-only envelopes (malformed domain separators);
//! - generation overflow (`advance` when the current generation is
//!   `u64::MAX`) and the G12-before-G10 decision order on synthetic
//!   candidates at generations `0` and `u64::MAX`;
//! - cross-stream snapshot substitution on live SWE1 envelopes;
//! - every rejected restore leaves the target encoding and
//!   `stats.admitted` unchanged (no replay, no partial mutation);
//! - a caller-constructed matching [`AuthorityId`] is an identity
//!   binding (equality tag), **not** an authenticated origin.
//!
//! Anchors and authority tags in this suite are mocks constructed
//! in-process. SWE1 remains unkeyed SHA-256 (corruption detection, not
//! authentication). No signatures, key provisioning, attestation,
//! hardware, E2E or transport claim is made. `NOT_MEASURED`.
//! Authentication of provenance remains **NOT QUALIFIED**.

use sml_core::context_ledger::{ContextContentHash, ContextSegmentId, ContextStreamId};
use sml_store::working_context::{WorkingContext, WorkingPolicy, WorkingSegment};
use sml_store::working_context_anchor::{
    advance, Admission, AnchorError, AnchorPolicy, AnchorRecord, AnchorState, Candidate,
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
const STREAM: u64 = 0x4743_0000_0000_0013;
const OTHER: u64 = 0x4743_0000_0000_0024;

const STREAMS: [u64; 4] = [0, 1, STREAM, u64::MAX];
const GENS: [u64; 4] = [0, 1, u64::MAX - 1, u64::MAX];

const DOMAIN_SWE1: [u8; 4] = *b"SWE1";
const DOMAIN_SWE2: [u8; 4] = *b"SWE2";
const DOMAIN_ZERO: [u8; 4] = [0, 0, 0, 0];
const DOMAINS: [[u8; 4]; 3] = [DOMAIN_SWE1, DOMAIN_SWE2, DOMAIN_ZERO];

// ---------------------------------------------------------------------------
// Independent oracle types and decision procedures.
// These types are deliberately *not* the production enums.
// Authority identifiers are compared as full 32-byte values so a shared
// prefix cannot be mistaken for a binding.
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct OAuth([u8; 32]);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OProvPolicy {
    Unbound,
    Bound,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct OIdent {
    stream: u64,
    authority: OAuth,
    domain: [u8; 4],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ODerived {
    stream: u64,
    domain: [u8; 4],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OProvOut {
    Unbound,
    Bound,
    Absent,
    StreamBindingMismatch {
        expected: u64,
        presented: u64,
        derived: Option<u64>,
    },
    AuthorityMismatch {
        expected: OAuth,
        presented: OAuth,
    },
    DigestDomainMismatch {
        expected: [u8; 4],
        presented: [u8; 4],
        derived: Option<[u8; 4]>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OG10Policy {
    Unanchored,
    Required,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct OG10Ident {
    stream: u64,
    generation: u64,
    digest: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OG10Anchor {
    Absent,
    Unverifiable,
    Verified(OG10Ident),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OG10Out {
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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OCombined {
    Ok {
        admission: OG10Out,
        provenance: OProvOut,
    },
    Provenance(OProvOut),
    Anchor(OG10Out),
}

/// Repeat `byte` across 32 octets.
fn auth_repeat(byte: u8) -> OAuth {
    OAuth([byte; 32])
}

/// Near-collision: first 31 octets equal `byte`, last octet `byte + 1`.
fn auth_near(byte: u8) -> OAuth {
    let mut bytes = [byte; 32];
    bytes[31] = byte.wrapping_add(1);
    OAuth(bytes)
}

fn auths() -> [OAuth; 3] {
    [auth_repeat(0xC1), auth_near(0xC1), auth_repeat(0x00)]
}

/// Pure provenance oracle. Priority-ordered rules from the G12 module:
///
/// 1. `Unbound` → `Unbound` (presented and derived are ignored).
/// 2. `Bound` + no presented identity → `Absent`.
/// 3. stream mismatch (expected vs presented, or expected vs derived)
///    → `StreamBindingMismatch`.
/// 4. authority mismatch (full 32-byte compare) → `AuthorityMismatch`.
/// 5. digest-domain mismatch (expected vs presented, or expected vs
///    derived) → `DigestDomainMismatch`.
/// 6. otherwise `Bound`.
fn oracle_admit_provenance(
    policy: OProvPolicy,
    expected: OIdent,
    presented: Option<OIdent>,
    derived: Option<ODerived>,
) -> OProvOut {
    if matches!(policy, OProvPolicy::Unbound) {
        return OProvOut::Unbound;
    }
    let presented = match presented {
        Some(p) => p,
        None => return OProvOut::Absent,
    };
    let derived_stream = derived.map(|d| d.stream);
    let stream_ok = presented.stream == expected.stream
        && derived_stream
            .map(|stream| stream == expected.stream)
            .unwrap_or(true);
    if !stream_ok {
        return OProvOut::StreamBindingMismatch {
            expected: expected.stream,
            presented: presented.stream,
            derived: derived_stream,
        };
    }
    if presented.authority != expected.authority {
        return OProvOut::AuthorityMismatch {
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
        return OProvOut::DigestDomainMismatch {
            expected: expected.domain,
            presented: presented.domain,
            derived: derived_domain,
        };
    }
    OProvOut::Bound
}

/// Pure G10 admission oracle (published generation/digest contract).
///
/// 1. `Unanchored` → `Unanchored`.
/// 2. `Required` + absent → `Absent`.
/// 3. `Required` + unverifiable → `Unverifiable`.
/// 4. stream mismatch → `StreamMismatch` (before generation is consulted).
/// 5. candidate generation `<` anchor → `Rollback`.
/// 6. candidate generation `>` anchor → `Stale`.
/// 7. equal generation, different digest → `Conflict`.
/// 8. equal generation, same digest → `Anchored`.
fn oracle_admit(policy: OG10Policy, anchor: OG10Anchor, cand: OG10Ident) -> OG10Out {
    if matches!(policy, OG10Policy::Unanchored) {
        return OG10Out::Unanchored;
    }
    let rec = match anchor {
        OG10Anchor::Absent => return OG10Out::Absent,
        OG10Anchor::Unverifiable => return OG10Out::Unverifiable,
        OG10Anchor::Verified(r) => r,
    };
    if rec.stream != cand.stream {
        return OG10Out::StreamMismatch {
            anchor: rec.stream,
            candidate: cand.stream,
        };
    }
    if cand.generation < rec.generation {
        return OG10Out::Rollback {
            anchor: rec.generation,
            candidate: cand.generation,
        };
    }
    if cand.generation > rec.generation {
        return OG10Out::Stale {
            anchor: rec.generation,
            candidate: cand.generation,
        };
    }
    if rec.digest != cand.digest {
        return OG10Out::Conflict {
            generation: rec.generation,
        };
    }
    OG10Out::Anchored
}

/// Pure `advance` oracle.
///
/// 1. no current record → accept `next`;
/// 2. stream mismatch → `StreamMismatch`;
/// 3. `current.generation == u64::MAX` → `Overflow` (cannot form `current + 1`);
/// 4. `next.generation < current + 1` → `NonIncreasing`;
/// 5. otherwise accept `next` (generation jumps forward are allowed).
fn oracle_advance(current: Option<OG10Ident>, next: OG10Ident) -> Result<OG10Ident, OG10Out> {
    if let Some(cur) = current {
        if cur.stream != next.stream {
            return Err(OG10Out::StreamMismatch {
                anchor: cur.stream,
                candidate: next.stream,
            });
        }
        let floor = match cur.generation.checked_add(1) {
            Some(f) => f,
            None => return Err(OG10Out::Overflow),
        };
        if next.generation < floor {
            return Err(OG10Out::NonIncreasing {
                current: cur.generation,
                next: next.generation,
            });
        }
    }
    Ok(next)
}

/// Pure combined oracle: provenance first, then G10 admission.
/// A provenance failure is returned even when G10 would also fail
/// (rollback, stale, conflict, stream mismatch).
fn oracle_admit_with_provenance(
    prov_policy: OProvPolicy,
    expected: OIdent,
    presented: Option<OIdent>,
    derived: Option<ODerived>,
    g10_policy: OG10Policy,
    g10_anchor: OG10Anchor,
    g10_cand: OG10Ident,
) -> OCombined {
    let provenance = oracle_admit_provenance(prov_policy, expected, presented, derived);
    match provenance {
        OProvOut::Unbound | OProvOut::Bound => match oracle_admit(g10_policy, g10_anchor, g10_cand)
        {
            admission @ (OG10Out::Unanchored | OG10Out::Anchored) => OCombined::Ok {
                admission,
                provenance,
            },
            other => OCombined::Anchor(other),
        },
        other => OCombined::Provenance(other),
    }
}

/// Independent extraction of the four-byte domain tag from an envelope
/// prefix (zero-padded when shorter than four bytes). Domain separation
/// only; this is not a call into production `derived_binding`.
fn oracle_derived_binding(stream: u64, envelope_bytes: &[u8]) -> ODerived {
    let mut tag = [0_u8; 4];
    let n = core::cmp::min(4, envelope_bytes.len());
    tag[..n].copy_from_slice(&envelope_bytes[..n]);
    ODerived {
        stream,
        domain: tag,
    }
}

fn ident(stream: u64, authority: OAuth, domain: [u8; 4]) -> OIdent {
    OIdent {
        stream,
        authority,
        domain,
    }
}

fn der(stream: u64, domain: [u8; 4]) -> ODerived {
    ODerived { stream, domain }
}

fn g10_ident(stream: u64, generation: u64, digest: u8) -> OG10Ident {
    OG10Ident {
        stream,
        generation,
        digest,
    }
}

fn digest32(tag: u8) -> [u8; 32] {
    [tag; 32]
}

fn prod_authority(auth: OAuth) -> AuthorityId {
    AuthorityId(auth.0)
}

fn prod_ident(id: OIdent) -> AnchorIdentity {
    AnchorIdentity {
        stream_id: id.stream,
        authority: prod_authority(id.authority),
        digest_domain: DigestDomain(id.domain),
    }
}

fn prod_derived(d: ODerived) -> DerivedBinding {
    DerivedBinding {
        stream_id: d.stream,
        digest_domain: DigestDomain(d.domain),
    }
}

fn prod_prov_policy(policy: OProvPolicy, expected: OIdent) -> ProvenancePolicy {
    match policy {
        OProvPolicy::Unbound => ProvenancePolicy::Unbound,
        OProvPolicy::Bound => ProvenancePolicy::Bound(prod_ident(expected)),
    }
}

fn prod_g10_policy(p: OG10Policy) -> AnchorPolicy {
    match p {
        OG10Policy::Unanchored => AnchorPolicy::Unanchored,
        OG10Policy::Required => AnchorPolicy::Required,
    }
}

fn prod_cand(id: OG10Ident) -> Candidate {
    Candidate {
        stream_id: id.stream,
        generation: id.generation,
        digest: digest32(id.digest),
    }
}

fn prod_rec(id: OG10Ident) -> AnchorRecord {
    AnchorRecord {
        stream_id: id.stream,
        generation: id.generation,
        digest: digest32(id.digest),
    }
}

fn prod_state(anchor: OG10Anchor) -> AnchorState {
    match anchor {
        OG10Anchor::Absent => AnchorState::Absent,
        OG10Anchor::Unverifiable => AnchorState::Unverifiable,
        OG10Anchor::Verified(id) => AnchorState::Verified(prod_rec(id)),
    }
}

fn classify_prov(r: Result<ProvenanceAdmission, ProvenanceError>) -> OProvOut {
    match r {
        Ok(ProvenanceAdmission::Unbound) => OProvOut::Unbound,
        Ok(ProvenanceAdmission::Bound) => OProvOut::Bound,
        Err(ProvenanceError::ProvenanceAbsent) => OProvOut::Absent,
        Err(ProvenanceError::StreamBindingMismatch {
            expected,
            presented,
            derived,
        }) => OProvOut::StreamBindingMismatch {
            expected,
            presented,
            derived,
        },
        Err(ProvenanceError::AuthorityMismatch {
            expected,
            presented,
        }) => OProvOut::AuthorityMismatch {
            expected: OAuth(expected.0),
            presented: OAuth(presented.0),
        },
        Err(ProvenanceError::DigestDomainMismatch {
            expected,
            presented,
            derived,
        }) => OProvOut::DigestDomainMismatch {
            expected: expected.0,
            presented: presented.0,
            derived: derived.map(|d| d.0),
        },
    }
}

fn classify_g10(r: Result<Admission, AnchorError>) -> OG10Out {
    match r {
        Ok(Admission::Unanchored) => OG10Out::Unanchored,
        Ok(Admission::Anchored) => OG10Out::Anchored,
        Err(AnchorError::AnchorAbsent) => OG10Out::Absent,
        Err(AnchorError::AnchorUnverifiable) => OG10Out::Unverifiable,
        Err(AnchorError::StreamMismatch { anchor, candidate }) => {
            OG10Out::StreamMismatch { anchor, candidate }
        }
        Err(AnchorError::Rollback { anchor, candidate }) => OG10Out::Rollback { anchor, candidate },
        Err(AnchorError::AnchorStale { anchor, candidate }) => OG10Out::Stale { anchor, candidate },
        Err(AnchorError::ConflictingHistory { generation }) => OG10Out::Conflict { generation },
        Err(AnchorError::NonIncreasing { current, next }) => {
            OG10Out::NonIncreasing { current, next }
        }
        Err(AnchorError::GenerationOverflow) => OG10Out::Overflow,
        Err(AnchorError::Envelope(_)) => panic!("synthetic candidates do not produce Envelope"),
    }
}

fn classify_advance(r: Result<AnchorRecord, AnchorError>) -> Result<OG10Ident, OG10Out> {
    match r {
        Ok(rec) => {
            assert!(
                rec.digest.iter().all(|&b| b == rec.digest[0]),
                "synthetic digests are repeating tags"
            );
            Ok(g10_ident(rec.stream_id, rec.generation, rec.digest[0]))
        }
        Err(e) => Err(classify_g10(Err(e))),
    }
}

fn classify_combined(
    r: Result<(Admission, ProvenanceAdmission), ProvenanceRestoreError>,
) -> OCombined {
    match r {
        Ok((adm, prov)) => OCombined::Ok {
            admission: classify_g10(Ok(adm)),
            provenance: classify_prov(Ok(prov)),
        },
        Err(ProvenanceRestoreError::Provenance(e)) => OCombined::Provenance(classify_prov(Err(e))),
        Err(ProvenanceRestoreError::Anchor(e)) => OCombined::Anchor(classify_g10(Err(e))),
    }
}

fn run_prod_prov(
    policy: OProvPolicy,
    expected: OIdent,
    presented: Option<OIdent>,
    derived: Option<ODerived>,
) -> OProvOut {
    let presented_id = presented.map(prod_ident);
    let derived_id = derived.map(prod_derived);
    classify_prov(admit_provenance(
        prod_prov_policy(policy, expected),
        presented_id.as_ref(),
        derived_id.as_ref(),
    ))
}

fn run_prod_combined(
    prov_policy: OProvPolicy,
    expected: OIdent,
    presented: Option<OIdent>,
    derived: Option<ODerived>,
    g10_policy: OG10Policy,
    g10_anchor: OG10Anchor,
    g10_cand: OG10Ident,
) -> OCombined {
    let presented_id = presented.map(prod_ident);
    let derived_id = derived.map(prod_derived);
    classify_combined(admit_with_provenance(
        prod_g10_policy(g10_policy),
        &prod_state(g10_anchor),
        &prod_cand(g10_cand),
        prod_prov_policy(prov_policy, expected),
        presented_id.as_ref(),
        derived_id.as_ref(),
    ))
}

fn other_stream(stream: u64) -> u64 {
    if stream == STREAM {
        OTHER
    } else {
        STREAM
    }
}

fn other_domain(domain: [u8; 4]) -> [u8; 4] {
    if domain == DOMAIN_SWE1 {
        DOMAIN_SWE2
    } else {
        DOMAIN_SWE1
    }
}

// ---------------------------------------------------------------------------
// Frozen named golden vectors (spec exemplars, not derived from production).
// ---------------------------------------------------------------------------

struct NamedProv {
    version: u16,
    name: &'static str,
    policy: OProvPolicy,
    expected: OIdent,
    presented: Option<OIdent>,
    derived: Option<ODerived>,
    expect: OProvOut,
}

fn named_provenance_vectors() -> Vec<NamedProv> {
    let a = auth_repeat(0xC1);
    let near = auth_near(0xC1);
    let zero = auth_repeat(0x00);
    let exp = ident(STREAM, a, DOMAIN_SWE1);
    vec![
        NamedProv {
            version: ANCHOR_PROVENANCE_POLICY_VERSION,
            name: "unbound_ignores_absent_and_malformed_derived",
            policy: OProvPolicy::Unbound,
            expected: exp,
            presented: None,
            derived: Some(der(0, DOMAIN_ZERO)),
            expect: OProvOut::Unbound,
        },
        NamedProv {
            version: ANCHOR_PROVENANCE_POLICY_VERSION,
            name: "unbound_ignores_cross_stream_near_authority_and_swe2",
            policy: OProvPolicy::Unbound,
            expected: exp,
            presented: Some(ident(u64::MAX, near, DOMAIN_SWE2)),
            derived: Some(der(0, DOMAIN_ZERO)),
            expect: OProvOut::Unbound,
        },
        NamedProv {
            version: ANCHOR_PROVENANCE_POLICY_VERSION,
            name: "bound_absent_presented_fails_closed_even_with_matching_derived",
            policy: OProvPolicy::Bound,
            expected: exp,
            presented: None,
            derived: Some(der(STREAM, DOMAIN_SWE1)),
            expect: OProvOut::Absent,
        },
        NamedProv {
            version: ANCHOR_PROVENANCE_POLICY_VERSION,
            name: "bound_absent_presented_no_derived",
            policy: OProvPolicy::Bound,
            expected: exp,
            presented: None,
            derived: None,
            expect: OProvOut::Absent,
        },
        NamedProv {
            version: ANCHOR_PROVENANCE_POLICY_VERSION,
            name: "bound_exact_match_no_derived",
            policy: OProvPolicy::Bound,
            expected: exp,
            presented: Some(exp),
            derived: None,
            expect: OProvOut::Bound,
        },
        NamedProv {
            version: ANCHOR_PROVENANCE_POLICY_VERSION,
            name: "bound_exact_match_with_matching_derived",
            policy: OProvPolicy::Bound,
            expected: exp,
            presented: Some(exp),
            derived: Some(der(STREAM, DOMAIN_SWE1)),
            expect: OProvOut::Bound,
        },
        NamedProv {
            version: ANCHOR_PROVENANCE_POLICY_VERSION,
            name: "bound_stream0_vs_max_presented",
            policy: OProvPolicy::Bound,
            expected: ident(0, a, DOMAIN_SWE1),
            presented: Some(ident(u64::MAX, a, DOMAIN_SWE1)),
            derived: None,
            expect: OProvOut::StreamBindingMismatch {
                expected: 0,
                presented: u64::MAX,
                derived: None,
            },
        },
        NamedProv {
            version: ANCHOR_PROVENANCE_POLICY_VERSION,
            name: "bound_stream_max_vs_0_derived",
            policy: OProvPolicy::Bound,
            expected: ident(u64::MAX, a, DOMAIN_SWE1),
            presented: Some(ident(u64::MAX, a, DOMAIN_SWE1)),
            derived: Some(der(0, DOMAIN_SWE1)),
            expect: OProvOut::StreamBindingMismatch {
                expected: u64::MAX,
                presented: u64::MAX,
                derived: Some(0),
            },
        },
        NamedProv {
            version: ANCHOR_PROVENANCE_POLICY_VERSION,
            name: "bound_cross_stream_presented_and_derived",
            policy: OProvPolicy::Bound,
            expected: exp,
            presented: Some(ident(OTHER, a, DOMAIN_SWE1)),
            derived: Some(der(OTHER, DOMAIN_SWE1)),
            expect: OProvOut::StreamBindingMismatch {
                expected: STREAM,
                presented: OTHER,
                derived: Some(OTHER),
            },
        },
        NamedProv {
            version: ANCHOR_PROVENANCE_POLICY_VERSION,
            name: "bound_authority_near_collision_last_octet",
            policy: OProvPolicy::Bound,
            expected: exp,
            presented: Some(ident(STREAM, near, DOMAIN_SWE1)),
            derived: Some(der(STREAM, DOMAIN_SWE1)),
            expect: OProvOut::AuthorityMismatch {
                expected: a,
                presented: near,
            },
        },
        NamedProv {
            version: ANCHOR_PROVENANCE_POLICY_VERSION,
            name: "bound_authority_zero_vs_c1",
            policy: OProvPolicy::Bound,
            expected: exp,
            presented: Some(ident(STREAM, zero, DOMAIN_SWE1)),
            derived: Some(der(STREAM, DOMAIN_SWE1)),
            expect: OProvOut::AuthorityMismatch {
                expected: a,
                presented: zero,
            },
        },
        NamedProv {
            version: ANCHOR_PROVENANCE_POLICY_VERSION,
            name: "bound_domain_confusion_presented_swe2_prefix",
            policy: OProvPolicy::Bound,
            expected: exp,
            presented: Some(ident(STREAM, a, DOMAIN_SWE2)),
            derived: Some(der(STREAM, DOMAIN_SWE1)),
            expect: OProvOut::DigestDomainMismatch {
                expected: DOMAIN_SWE1,
                presented: DOMAIN_SWE2,
                derived: Some(DOMAIN_SWE1),
            },
        },
        NamedProv {
            version: ANCHOR_PROVENANCE_POLICY_VERSION,
            name: "bound_domain_confusion_derived_zero_from_short_envelope",
            policy: OProvPolicy::Bound,
            expected: exp,
            presented: Some(exp),
            derived: Some(der(STREAM, DOMAIN_ZERO)),
            expect: OProvOut::DigestDomainMismatch {
                expected: DOMAIN_SWE1,
                presented: DOMAIN_SWE1,
                derived: Some(DOMAIN_ZERO),
            },
        },
        NamedProv {
            version: ANCHOR_PROVENANCE_POLICY_VERSION,
            name: "bound_domain_confusion_expected_swe2_derived_swe1",
            policy: OProvPolicy::Bound,
            expected: ident(STREAM, a, DOMAIN_SWE2),
            presented: Some(ident(STREAM, a, DOMAIN_SWE2)),
            derived: Some(der(STREAM, DOMAIN_SWE1)),
            expect: OProvOut::DigestDomainMismatch {
                expected: DOMAIN_SWE2,
                presented: DOMAIN_SWE2,
                derived: Some(DOMAIN_SWE1),
            },
        },
        NamedProv {
            version: ANCHOR_PROVENANCE_POLICY_VERSION,
            name: "bound_stream_mismatch_masks_near_authority_and_swe2",
            policy: OProvPolicy::Bound,
            expected: exp,
            presented: Some(ident(OTHER, near, DOMAIN_SWE2)),
            derived: Some(der(OTHER, DOMAIN_SWE2)),
            expect: OProvOut::StreamBindingMismatch {
                expected: STREAM,
                presented: OTHER,
                derived: Some(OTHER),
            },
        },
        NamedProv {
            version: ANCHOR_PROVENANCE_POLICY_VERSION,
            name: "bound_authority_near_collision_masks_domain_confusion",
            policy: OProvPolicy::Bound,
            expected: exp,
            presented: Some(ident(STREAM, near, DOMAIN_SWE2)),
            derived: Some(der(STREAM, DOMAIN_SWE2)),
            expect: OProvOut::AuthorityMismatch {
                expected: a,
                presented: near,
            },
        },
        NamedProv {
            version: ANCHOR_PROVENANCE_POLICY_VERSION,
            name: "bound_stream1_zero_authority_zero_domain",
            policy: OProvPolicy::Bound,
            expected: ident(1, zero, DOMAIN_ZERO),
            presented: Some(ident(1, zero, DOMAIN_ZERO)),
            derived: Some(der(1, DOMAIN_ZERO)),
            expect: OProvOut::Bound,
        },
        NamedProv {
            version: ANCHOR_PROVENANCE_POLICY_VERSION,
            name: "bound_stream_max_near_authority_swe2_exact",
            policy: OProvPolicy::Bound,
            expected: ident(u64::MAX, near, DOMAIN_SWE2),
            presented: Some(ident(u64::MAX, near, DOMAIN_SWE2)),
            derived: Some(der(u64::MAX, DOMAIN_SWE2)),
            expect: OProvOut::Bound,
        },
    ]
}

// ---------------------------------------------------------------------------
// Live WorkingContext helpers (small generations only; u64::MAX cannot be
// materialized as `stats.admitted` in-process and is covered via synthetic
// `admit` / `advance` candidates instead).
// ---------------------------------------------------------------------------

fn sample_ctx(stream: u64, n: u64, salt: u8) -> WorkingContext {
    let s = ContextStreamId::new(stream).expect("nonzero stream id");
    let mut ctx = WorkingContext::new(s, WorkingPolicy::CapacityBounded { capacity: 4 }).unwrap();
    for i in 1..=n {
        let mut hash = [salt; 32];
        hash[0] = u8::try_from(i).expect("fixture generation fits u8");
        let seg = WorkingSegment::new(
            ContextSegmentId::new(s, 4_000 + i).unwrap(),
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
fn frozen_named_vectors_are_versioned_and_match_independent_oracle() {
    assert_eq!(ANCHOR_PROVENANCE_POLICY_VERSION, 1);
    let rows = named_provenance_vectors();
    assert_eq!(rows.len(), 18, "frozen named provenance table size");
    for row in &rows {
        assert_eq!(
            row.version, ANCHOR_PROVENANCE_POLICY_VERSION,
            "{}",
            row.name
        );
        let oracle = oracle_admit_provenance(row.policy, row.expected, row.presented, row.derived);
        assert_eq!(oracle, row.expect, "oracle {}", row.name);
        assert_eq!(
            run_prod_prov(row.policy, row.expected, row.presented, row.derived),
            row.expect,
            "production {}",
            row.name
        );
    }
}

#[test]
fn cartesian_stream_authority_collision_and_domain_matches_oracle() {
    let auths = auths();
    let mut rows = 0_usize;
    let mut saw_near_collision = false;
    let mut saw_swe1_vs_swe2 = false;
    let mut saw_stream0_vs_max = false;
    for policy in [OProvPolicy::Unbound, OProvPolicy::Bound] {
        for expected_stream in STREAMS {
            for &expected_auth in &auths {
                for expected_domain in DOMAINS {
                    let expected = ident(expected_stream, expected_auth, expected_domain);
                    for derived in [None, Some(der(expected_stream, expected_domain))] {
                        let oracle = oracle_admit_provenance(policy, expected, None, derived);
                        assert_eq!(
                            run_prod_prov(policy, expected, None, derived),
                            oracle,
                            "{policy:?} absent {expected:?} {derived:?}"
                        );
                        rows += 1;
                    }
                    for presented_stream in STREAMS {
                        for &presented_auth in &auths {
                            for presented_domain in DOMAINS {
                                let presented =
                                    ident(presented_stream, presented_auth, presented_domain);
                                let derived_cases = [
                                    None,
                                    Some(der(expected_stream, expected_domain)),
                                    Some(der(presented_stream, presented_domain)),
                                    Some(der(
                                        other_stream(expected_stream),
                                        other_domain(expected_domain),
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
                                        run_prod_prov(
                                            policy,
                                            expected,
                                            Some(presented),
                                            derived
                                        ),
                                        oracle,
                                        "{policy:?} exp={expected:?} pres={presented:?} der={derived:?}"
                                    );
                                    let derived_stream_ok = derived
                                        .map(|d| d.stream == expected_stream)
                                        .unwrap_or(true);
                                    if expected_auth == auth_repeat(0xC1)
                                        && presented_auth == auth_near(0xC1)
                                        && expected_stream == presented_stream
                                        && expected_domain == presented_domain
                                        && matches!(policy, OProvPolicy::Bound)
                                    {
                                        saw_near_collision = true;
                                        // Stream is checked before authority: a
                                        // foreign derived stream is a binding
                                        // mismatch, not an authority mismatch.
                                        if derived_stream_ok {
                                            assert!(matches!(
                                                oracle,
                                                OProvOut::AuthorityMismatch { .. }
                                            ));
                                        }
                                    }
                                    if expected_domain == DOMAIN_SWE1
                                        && presented_domain == DOMAIN_SWE2
                                        && expected_stream == presented_stream
                                        && expected_auth == presented_auth
                                        && matches!(policy, OProvPolicy::Bound)
                                    {
                                        saw_swe1_vs_swe2 = true;
                                        if derived_stream_ok {
                                            assert!(matches!(
                                                oracle,
                                                OProvOut::DigestDomainMismatch { .. }
                                            ));
                                        }
                                    }
                                    if expected_stream == 0
                                        && presented_stream == u64::MAX
                                        && matches!(policy, OProvPolicy::Bound)
                                    {
                                        saw_stream0_vs_max = true;
                                        assert!(matches!(
                                            oracle,
                                            OProvOut::StreamBindingMismatch { .. }
                                        ));
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
    // 2 policies * 4 streams * 3 auths * 3 domains *
    // (2 absent-presented + 4*3*3 presented * 4 derived cases)
    assert_eq!(
        rows,
        2 * STREAMS.len()
            * auths.len()
            * DOMAINS.len()
            * (2 + STREAMS.len() * auths.len() * DOMAINS.len() * 4)
    );
    assert!(saw_near_collision && saw_swe1_vs_swe2 && saw_stream0_vs_max);
}

#[test]
fn authority_id_prefix_collision_is_mismatch_not_binding() {
    let expected = ident(STREAM, auth_repeat(0xC1), DOMAIN_SWE1);
    let near = ident(STREAM, auth_near(0xC1), DOMAIN_SWE1);
    assert_ne!(expected.authority.0, near.authority.0);
    assert_eq!(&expected.authority.0[..31], &near.authority.0[..31]);
    assert_eq!(
        oracle_admit_provenance(
            OProvPolicy::Bound,
            expected,
            Some(near),
            Some(der(STREAM, DOMAIN_SWE1))
        ),
        OProvOut::AuthorityMismatch {
            expected: expected.authority,
            presented: near.authority,
        }
    );
    assert_eq!(
        run_prod_prov(
            OProvPolicy::Bound,
            expected,
            Some(near),
            Some(der(STREAM, DOMAIN_SWE1))
        ),
        OProvOut::AuthorityMismatch {
            expected: expected.authority,
            presented: near.authority,
        }
    );

    // The same 32 bytes constructed independently still bind: equality
    // is the whole identifier, not a truncated prefix or a signature.
    let rebuilt = ident(STREAM, auth_repeat(0xC1), DOMAIN_SWE1);
    assert_eq!(rebuilt.authority, expected.authority);
    assert_eq!(
        run_prod_prov(
            OProvPolicy::Bound,
            expected,
            Some(rebuilt),
            Some(der(STREAM, DOMAIN_SWE1))
        ),
        OProvOut::Bound
    );
}

#[test]
fn combined_path_respects_provenance_first_and_generation_overflow() {
    let expected = ident(STREAM, auth_repeat(0xC1), DOMAIN_SWE1);
    let matching = Some(expected);
    let derived_ok = Some(der(STREAM, DOMAIN_SWE1));
    let at_max = g10_ident(STREAM, u64::MAX, 0xA1);
    let at_zero = g10_ident(STREAM, 0, 0xA1);
    let at_max_conflict = g10_ident(STREAM, u64::MAX, 0x5E);

    // `advance` overflows at u64::MAX regardless of the proposed next
    // generation. G12 does not wrap `advance`; overflow stays a G10 error.
    for next in [at_max, at_zero, g10_ident(STREAM, u64::MAX - 1, 0xA1)] {
        assert_eq!(oracle_advance(Some(at_max), next), Err(OG10Out::Overflow));
        assert_eq!(
            classify_advance(advance(Some(&prod_rec(at_max)), &prod_cand(next))),
            Err(OG10Out::Overflow),
            "advance overflow next={next:?}"
        );
    }
    assert_eq!(oracle_advance(None, at_max), Ok(at_max));
    assert_eq!(
        classify_advance(advance(None, &prod_cand(at_max))),
        Ok(at_max)
    );

    // Equal-generation exact match at 0 and u64::MAX is G10 Anchored once
    // provenance binds (or is Unbound). Overflow is not an `admit` error.
    for gen in [0_u64, u64::MAX] {
        let cand = g10_ident(STREAM, gen, 0xA1);
        let anchor = OG10Anchor::Verified(cand);
        let expect = oracle_admit_with_provenance(
            OProvPolicy::Bound,
            expected,
            matching,
            derived_ok,
            OG10Policy::Required,
            anchor,
            cand,
        );
        assert_eq!(
            expect,
            OCombined::Ok {
                admission: OG10Out::Anchored,
                provenance: OProvOut::Bound,
            },
            "gen={gen}"
        );
        assert_eq!(
            run_prod_combined(
                OProvPolicy::Bound,
                expected,
                matching,
                derived_ok,
                OG10Policy::Required,
                anchor,
                cand,
            ),
            expect,
            "production gen={gen}"
        );
    }

    // Provenance is decided first: an absent identity fails closed even
    // when G10 would have accepted the u64::MAX exact match.
    let absent = oracle_admit_with_provenance(
        OProvPolicy::Bound,
        expected,
        None,
        derived_ok,
        OG10Policy::Required,
        OG10Anchor::Verified(at_max),
        at_max,
    );
    assert_eq!(absent, OCombined::Provenance(OProvOut::Absent));
    assert_eq!(
        run_prod_combined(
            OProvPolicy::Bound,
            expected,
            None,
            derived_ok,
            OG10Policy::Required,
            OG10Anchor::Verified(at_max),
            at_max,
        ),
        absent
    );

    // Near-collision authority also wins over a G10 rollback-from-MAX.
    let near = Some(ident(STREAM, auth_near(0xC1), DOMAIN_SWE1));
    let masked = oracle_admit_with_provenance(
        OProvPolicy::Bound,
        expected,
        near,
        derived_ok,
        OG10Policy::Required,
        OG10Anchor::Verified(at_max),
        at_zero,
    );
    assert_eq!(
        masked,
        OCombined::Provenance(OProvOut::AuthorityMismatch {
            expected: expected.authority,
            presented: auth_near(0xC1),
        })
    );
    assert_eq!(
        run_prod_combined(
            OProvPolicy::Bound,
            expected,
            near,
            derived_ok,
            OG10Policy::Required,
            OG10Anchor::Verified(at_max),
            at_zero,
        ),
        masked
    );

    // Matching provenance, then G10 rollback / conflict at MAX.
    let rollback = oracle_admit_with_provenance(
        OProvPolicy::Bound,
        expected,
        matching,
        derived_ok,
        OG10Policy::Required,
        OG10Anchor::Verified(at_max),
        at_zero,
    );
    assert_eq!(
        rollback,
        OCombined::Anchor(OG10Out::Rollback {
            anchor: u64::MAX,
            candidate: 0,
        })
    );
    assert_eq!(
        run_prod_combined(
            OProvPolicy::Bound,
            expected,
            matching,
            derived_ok,
            OG10Policy::Required,
            OG10Anchor::Verified(at_max),
            at_zero,
        ),
        rollback
    );

    let conflict = oracle_admit_with_provenance(
        OProvPolicy::Bound,
        expected,
        matching,
        derived_ok,
        OG10Policy::Required,
        OG10Anchor::Verified(at_max),
        at_max_conflict,
    );
    assert_eq!(
        conflict,
        OCombined::Anchor(OG10Out::Conflict {
            generation: u64::MAX,
        })
    );
    assert_eq!(
        run_prod_combined(
            OProvPolicy::Bound,
            expected,
            matching,
            derived_ok,
            OG10Policy::Required,
            OG10Anchor::Verified(at_max),
            at_max_conflict,
        ),
        conflict
    );

    // Frozen combined cartesian over generation boundaries and the two
    // provenance policies, with matching vs absent vs near-collision
    // presented identities.
    let mut rows = 0_usize;
    let presented_cases = [None, matching, near];
    for prov_policy in [OProvPolicy::Unbound, OProvPolicy::Bound] {
        for g10_policy in [OG10Policy::Unanchored, OG10Policy::Required] {
            for &gen in &GENS {
                let cand = g10_ident(STREAM, gen, 0xA1);
                let anchors = [
                    OG10Anchor::Absent,
                    OG10Anchor::Unverifiable,
                    OG10Anchor::Verified(cand),
                    OG10Anchor::Verified(g10_ident(STREAM, u64::MAX, 0xA1)),
                    OG10Anchor::Verified(g10_ident(STREAM, 0, 0x5E)),
                ];
                for anchor in anchors {
                    for presented in presented_cases {
                        let expect = oracle_admit_with_provenance(
                            prov_policy,
                            expected,
                            presented,
                            derived_ok,
                            g10_policy,
                            anchor,
                            cand,
                        );
                        assert_eq!(
                            run_prod_combined(
                                prov_policy,
                                expected,
                                presented,
                                derived_ok,
                                g10_policy,
                                anchor,
                                cand,
                            ),
                            expect,
                            "{prov_policy:?} {g10_policy:?} {anchor:?} {cand:?} {presented:?}"
                        );
                        rows += 1;
                    }
                }
            }
        }
    }
    assert_eq!(rows, 2 * 2 * GENS.len() * 5 * presented_cases.len());
}

#[test]
fn derived_binding_and_malformed_absent_identity_match_oracle() {
    let prefixes: &[&[u8]] = &[
        b"",
        b"S",
        b"SW",
        b"SWE",
        b"SWE1",
        b"SWE2",
        b"\0\0\0\0",
        b"SWE1xxxx",
    ];
    for &prefix in prefixes {
        let oracle = oracle_derived_binding(STREAM, prefix);
        let prod = derived_binding(
            &Candidate {
                stream_id: STREAM,
                generation: 0,
                digest: [0; 32],
            },
            prefix,
        );
        assert_eq!(prod.stream_id, oracle.stream, "{prefix:?}");
        assert_eq!(prod.digest_domain.0, oracle.domain, "{prefix:?}");
    }
    assert_eq!(oracle_derived_binding(STREAM, b"").domain, DOMAIN_ZERO);
    assert_eq!(oracle_derived_binding(STREAM, b"SWE").domain, *b"SWE\0");
    assert_eq!(
        oracle_derived_binding(STREAM, b"SWE1xxxx").domain,
        DOMAIN_SWE1
    );
    assert_eq!(DIGEST_DOMAIN_SWE1, DigestDomain(DOMAIN_SWE1));

    // Live restore: truncated / empty envelopes fail at `candidate_of`
    // (envelope) before provenance runs. Absent identity on an intact
    // envelope is `ProvenanceAbsent`. Both leave the target untouched.
    let live = sample_ctx(STREAM, 3, 0x31);
    let env = seal_context(&live).unwrap();
    let extracted = derived_binding(
        &sml_store::working_context_anchor::candidate_of(&env)
            .unwrap()
            .1,
        &env,
    );
    assert_eq!(
        oracle_derived_binding(STREAM, &env),
        der(STREAM, DOMAIN_SWE1)
    );
    assert_eq!(extracted.digest_domain, DIGEST_DOMAIN_SWE1);
    assert_eq!(&env[..4], b"SWE1");

    let matching = prod_ident(ident(STREAM, auth_repeat(0xC1), DOMAIN_SWE1));
    let bound = ProvenancePolicy::Bound(matching);
    let st = AnchorState::Absent;
    let cases: Vec<(&str, &[u8], Option<&AnchorIdentity>)> = vec![
        ("empty", b"".as_slice(), Some(&matching)),
        ("short-magic", &env[..3], Some(&matching)),
        ("prefix-only", &env[..4], Some(&matching)),
        ("truncated-trailer", &env[..env.len() - 1], Some(&matching)),
        ("absent-identity", env.as_slice(), None),
    ];
    for (label, bytes, presented) in cases {
        let mut target = sample_ctx(STREAM, 2, 0x32);
        let before = fingerprint(&target);
        let err = restore_into_anchored_with_provenance(
            &mut target,
            bytes,
            AnchorPolicy::Unanchored,
            &st,
            bound,
            presented,
        )
        .expect_err(label);
        if label == "absent-identity" {
            assert_eq!(
                err,
                ProvenanceRestoreError::Provenance(ProvenanceError::ProvenanceAbsent),
                "{label}"
            );
        } else {
            assert!(
                matches!(
                    err,
                    ProvenanceRestoreError::Anchor(AnchorError::Envelope(_))
                ),
                "{label}: {err:?}"
            );
        }
        assert_unchanged(&target, &before, label);
    }
}

#[test]
fn cross_stream_snapshot_substitution_rejected_without_mutation() {
    let home = sample_ctx(STREAM, 3, 0x41);
    let foreign = sample_ctx(OTHER, 3, 0x41);
    let home_env = seal_context(&home).unwrap();
    let foreign_env = seal_context(&foreign).unwrap();
    assert_ne!(home_env, foreign_env);
    let (_, home_c) = sml_store::working_context_anchor::candidate_of(&home_env).unwrap();
    let matching = prod_ident(ident(STREAM, auth_repeat(0xC1), DOMAIN_SWE1));
    let foreign_id = prod_ident(ident(OTHER, auth_repeat(0xC1), DOMAIN_SWE1));
    let bound = ProvenancePolicy::Bound(matching);
    let st = AnchorState::Verified(rec_from(&home_c));

    // Snapshot from OTHER, presented as the STREAM identity: derived
    // stream comes from the candidate, so the substitution is a stream
    // binding mismatch (not an authority or domain error).
    let mut target = sample_ctx(STREAM, 1, 0x42);
    let before = fingerprint(&target);
    let err = restore_into_anchored_with_provenance(
        &mut target,
        &foreign_env,
        AnchorPolicy::Unanchored,
        &AnchorState::Absent,
        bound,
        Some(&matching),
    )
    .expect_err("substituted foreign snapshot");
    assert_eq!(
        err,
        ProvenanceRestoreError::Provenance(ProvenanceError::StreamBindingMismatch {
            expected: STREAM,
            presented: STREAM,
            derived: Some(OTHER),
        })
    );
    assert_unchanged(&target, &before, "substituted foreign snapshot");

    // Same foreign snapshot, but now presented with a matching OTHER
    // identity against a STREAM policy: presented stream fails first.
    let err = restore_into_anchored_with_provenance(
        &mut target,
        &foreign_env,
        AnchorPolicy::Unanchored,
        &AnchorState::Absent,
        bound,
        Some(&foreign_id),
    )
    .expect_err("presented other identity");
    assert_eq!(
        err,
        ProvenanceRestoreError::Provenance(ProvenanceError::StreamBindingMismatch {
            expected: STREAM,
            presented: OTHER,
            derived: Some(OTHER),
        })
    );
    assert_unchanged(&target, &before, "presented other identity");

    // Unbound provenance does not bind identity: G10 Required + a
    // Verified STREAM anchor then rejects the foreign snapshot as a
    // G10 stream mismatch (still no mutation).
    let err = restore_into_anchored_with_provenance(
        &mut target,
        &foreign_env,
        AnchorPolicy::Required,
        &st,
        ProvenancePolicy::Unbound,
        Some(&matching),
    )
    .expect_err("unbound then g10 stream mismatch");
    assert!(matches!(
        err,
        ProvenanceRestoreError::Anchor(AnchorError::StreamMismatch {
            anchor: STREAM,
            candidate: OTHER,
        })
    ));
    assert_unchanged(&target, &before, "unbound then g10 stream mismatch");

    // Control: the honest STREAM snapshot binds and replaces the target.
    assert_eq!(
        restore_into_anchored_with_provenance(
            &mut target,
            &home_env,
            AnchorPolicy::Required,
            &st,
            bound,
            Some(&matching),
        ),
        Ok((Admission::Anchored, ProvenanceAdmission::Bound))
    );
    assert_eq!(seal_context(&target).unwrap(), home_env);
}

#[test]
fn rejected_restore_never_mutates_target_across_adversarial_cases() {
    let live = sample_ctx(STREAM, 4, 0x51);
    let env = seal_context(&live).unwrap();
    let (_, cand) = sml_store::working_context_anchor::candidate_of(&env).unwrap();
    let st = AnchorState::Verified(rec_from(&cand));
    let matching = prod_ident(ident(STREAM, auth_repeat(0xC1), DOMAIN_SWE1));
    let bound = ProvenancePolicy::Bound(matching);
    let near = prod_ident(ident(STREAM, auth_near(0xC1), DOMAIN_SWE1));
    let swe2 = prod_ident(ident(STREAM, auth_repeat(0xC1), DOMAIN_SWE2));
    let other = prod_ident(ident(OTHER, auth_repeat(0xC1), DOMAIN_SWE1));
    let mut corrupt = env.clone();
    corrupt[20] ^= 0x08;
    let foreign_env = seal_context(&sample_ctx(OTHER, 4, 0x51)).unwrap();
    let older = seal_context(&sample_ctx(STREAM, 1, 0x51)).unwrap();
    let absent_anchor = AnchorState::Absent;

    struct RestoreReject<'a> {
        label: &'a str,
        bytes: &'a [u8],
        policy: ProvenancePolicy,
        presented: Option<&'a AnchorIdentity>,
        g10: AnchorPolicy,
        anchor: &'a AnchorState,
    }
    let cases = [
        RestoreReject {
            label: "absent",
            bytes: env.as_slice(),
            policy: bound,
            presented: None,
            g10: AnchorPolicy::Required,
            anchor: &st,
        },
        RestoreReject {
            label: "near-authority",
            bytes: env.as_slice(),
            policy: bound,
            presented: Some(&near),
            g10: AnchorPolicy::Required,
            anchor: &st,
        },
        RestoreReject {
            label: "domain-swe2",
            bytes: env.as_slice(),
            policy: bound,
            presented: Some(&swe2),
            g10: AnchorPolicy::Required,
            anchor: &st,
        },
        RestoreReject {
            label: "presented-other-stream",
            bytes: env.as_slice(),
            policy: bound,
            presented: Some(&other),
            g10: AnchorPolicy::Required,
            anchor: &st,
        },
        RestoreReject {
            label: "foreign-snapshot",
            bytes: foreign_env.as_slice(),
            policy: bound,
            presented: Some(&matching),
            g10: AnchorPolicy::Unanchored,
            anchor: &absent_anchor,
        },
        RestoreReject {
            label: "corrupt",
            bytes: corrupt.as_slice(),
            policy: bound,
            presented: Some(&matching),
            g10: AnchorPolicy::Required,
            anchor: &st,
        },
        RestoreReject {
            label: "truncated",
            bytes: &env[..env.len() - 1],
            policy: bound,
            presented: Some(&matching),
            g10: AnchorPolicy::Required,
            anchor: &st,
        },
        RestoreReject {
            label: "rollback",
            bytes: older.as_slice(),
            policy: bound,
            presented: Some(&matching),
            g10: AnchorPolicy::Required,
            anchor: &st,
        },
    ];
    for case in cases {
        let mut target = sample_ctx(STREAM, 2, 0x52);
        let before = fingerprint(&target);
        assert!(
            restore_into_anchored_with_provenance(
                &mut target,
                case.bytes,
                case.g10,
                case.anchor,
                case.policy,
                case.presented,
            )
            .is_err(),
            "{}",
            case.label
        );
        assert_unchanged(&target, &before, case.label);
    }
}

/// LIMITATION (documented, not a product defect): `AuthorityId` is a
/// caller assertion compared for equality. This module never
/// authenticates, signs, attests or binds the identifier to a key or
/// hardware root. A caller who constructs the expected 32 bytes is
/// accepted as `Bound`. Identity binding is not authenticated origin.
/// Authentication of provenance remains **NOT QUALIFIED**. SWE1 stays
/// unkeyed SHA-256. No signatures or key provisioning are involved.
#[test]
fn forged_matching_authority_id_is_identity_binding_not_authenticated_origin() {
    let live = sample_ctx(STREAM, 3, 0x61);
    let env = seal_context(&live).unwrap();
    let (_, cand) = sml_store::working_context_anchor::candidate_of(&env).unwrap();
    let st = AnchorState::Verified(rec_from(&cand));
    let expected = prod_ident(ident(STREAM, auth_repeat(0xC1), DOMAIN_SWE1));
    let foreign = prod_ident(ident(STREAM, auth_repeat(0xD2), DOMAIN_SWE1));
    let near = prod_ident(ident(STREAM, auth_near(0xC1), DOMAIN_SWE1));

    let mut denied = sample_ctx(STREAM, 1, 0x62);
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
    assert!(matches!(
        restore_into_anchored_with_provenance(
            &mut denied,
            &env,
            AnchorPolicy::Required,
            &st,
            ProvenancePolicy::Bound(expected),
            Some(&near),
        ),
        Err(ProvenanceRestoreError::Provenance(
            ProvenanceError::AuthorityMismatch { .. }
        ))
    ));
    assert_unchanged(&denied, &before, "near-collision authority");

    // Anyone who *constructs* the expected AuthorityId bytes is accepted.
    // There is no signature, MAC or hardware quote that would distinguish
    // an honest presenter from a forger of the same 32 bytes.
    let forged_matching = AnchorIdentity {
        stream_id: STREAM,
        authority: AuthorityId([0xC1; 32]),
        digest_domain: DigestDomain(DOMAIN_SWE1),
    };
    assert_eq!(forged_matching, expected);
    let mut accepted = sample_ctx(STREAM, 1, 0x62);
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
