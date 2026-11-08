//! CONTEXT-2 G15: independent differential tests for G14 error precedence
//! under contradictory rejection causes on the versioned anchor admission
//! path (`restore_into_anchored_with_versioned_provenance`).
//!
//! Each frozen family isolates cause A, isolates cause B, then combines
//! A+B. The combined case must report the higher-precedence typed error
//! (never a third error, never an accept). Every rejection leaves the
//! target byte-for-byte unchanged (canonical SWC2 encoding, SWE1 seal and
//! all five `WorkingContextStats` counters) and must not advance the
//! anchor.
//!
//! Published precedence (independent oracle, same order as G14):
//! 1. declared policy version `< 1` → downgrade, `> 1` → unknown (no
//!    migration; before the bytes are examined);
//! 2. SWE1 layout/digest failure → `Envelope`;
//! 3. G12 provenance: unbound skips; absent; stream (presented or
//!    derived); authority (full 32 bytes); domain (presented or derived
//!    magic);
//! 4. G10: unanchored skips; absent; unverifiable; stream; rollback;
//!    stale; conflict; anchored.
//!
//! Frozen families (this corpus, [`G15_CORPUS_VERSION`]):
//! - **unsupported policy version + corrupt SWE1**: version gate masks a
//!   truncated envelope;
//! - **absent anchor + wrong authority**: G12 authority mismatch precedes
//!   G10 `AnchorAbsent`;
//! - **matching digest + different stream**: G10 stream check precedes
//!   digest equality (a matching digest does not accept or conflict);
//! - **generation rollback + malformed seal**: SWE1 trailer corruption
//!   precedes the G10 rollback check;
//! - **version and provenance mismatch**: version gate masks a wrong
//!   authority and a derived-stream mismatch.
//!
//! Oracle independence: the oracle re-implements the published SWE1
//! layout check, the G12 provenance order, the G10 anchor order and the
//! G14 version gate with its own types. It does not call production
//! `open`, `candidate_of`, `derived_binding`, `admit_provenance`,
//! `admit_with_provenance`, `admit`, `advance` or `check_policy_version`.
//! It shares only the SHA-256 primitive (`sml_core::elastic_ngram_paged_sha`)
//! and takes the stream/generation of each fixture from frozen corpus
//! facts instead of decoding SWC2.
//!
//! Limits: authorities and anchors are in-process mocks. A matching
//! authority tag is an equality binding, **not** a cryptographic origin.
//! SWE1 is unkeyed SHA-256 (corruption detection, not authentication).
//! The declared policy version is a caller assertion, not authenticated.
//! No migration, signature, attestation, hardware counter, transport or
//! E2E claim. `NOT_MEASURED`. This file does not modify the G14 corpus.

use sml_core::context_ledger::{ContextContentHash, ContextSegmentId, ContextStreamId};
use sml_core::elastic_ngram_paged_sha::sha256;
use sml_store::working_context::{
    WorkingContext, WorkingContextStats, WorkingPolicy, WorkingSegment,
};
use sml_store::working_context_anchor::{
    Admission, AnchorError, AnchorPolicy, AnchorRecord, AnchorState,
};
use sml_store::working_context_anchor_provenance::{
    restore_into_anchored_with_versioned_provenance, AnchorIdentity, AuthorityId, DigestDomain,
    PolicyVersionError, ProvenanceAdmission, ProvenanceError, ProvenancePolicy,
    ProvenanceRestoreError, VersionedRestoreError, ANCHOR_PROVENANCE_POLICY_VERSION,
};
use sml_store::working_context_envelope::seal_context;
use sml_store::working_context_wire::encode_context;

/// Version of this frozen differential corpus. Bump only when rows,
/// fixtures or expected outcomes change (and re-freeze the digests below).
const G15_CORPUS_VERSION: u16 = 1;
/// The oracle's own notion of the single supported policy version.
const ORACLE_SUPPORTED_POLICY: u16 = 1;

const STREAM: u64 = 0x4743_0000_0000_0015;
const OTHER: u64 = 0x4743_0000_0000_0051;

const DOMAIN_SWE1: [u8; 4] = *b"SWE1";

// ---------------------------------------------------------------------------
// Frozen fixtures.
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Fx {
    /// Home stream, generation 3 (the anchored snapshot).
    Home3,
    /// Home stream, generation 1 (older snapshot; rollback vs Home3).
    Home1,
    /// Foreign stream, generation 3.
    Foreign3,
}

const FIXTURES: [Fx; 3] = [Fx::Home3, Fx::Home1, Fx::Foreign3];

/// Frozen facts the oracle uses instead of decoding SWC2.
fn fx_facts(fx: Fx) -> (u64, u64, u8) {
    match fx {
        Fx::Home3 => (STREAM, 3, 0x15),
        Fx::Home1 => (STREAM, 1, 0x15),
        Fx::Foreign3 => (OTHER, 3, 0x15),
    }
}

/// Frozen SHA-256 of each sealed fixture envelope (reproducibility pin).
fn fx_frozen_sha(fx: Fx) -> &'static str {
    match fx {
        Fx::Home3 => "abbc9fc9b7dcf78a5e83df0b0c56acf529e84c6720b7ee833a776c0f4a7744c5",
        Fx::Home1 => "98f16d6e3606e610ff5c11f4e7668608fa6e8bb13d370ffbb1efc2e7dcc8844c",
        Fx::Foreign3 => "8bc97e23654557d61155fa8c64f492c699a8145093756b08dbacca5301653bc7",
    }
}

/// Frozen SHA-256 over the canonical rendering of the whole corpus.
const FROZEN_CORPUS_SHA256: &str =
    "4dbedab565d4012dfb0845596496b8977e73b8d86bd8dd8882283d1ab62787ec";

fn sample_ctx(stream: u64, n: u64, salt: u8) -> WorkingContext {
    let s = ContextStreamId::new(stream).expect("nonzero stream id");
    let mut ctx = WorkingContext::new(s, WorkingPolicy::CapacityBounded { capacity: 4 }).unwrap();
    for i in 1..=n {
        let mut hash = [salt; 32];
        hash[0] = u8::try_from(i).expect("fixture generation fits u8");
        let seg = WorkingSegment::new(
            ContextSegmentId::new(s, 15_000 + i).unwrap(),
            7 * i,
            ContextContentHash::from_sha256(hash),
            (i << 16) | u64::from(salt) | 1,
        )
        .unwrap();
        ctx.admit(seg).unwrap();
    }
    ctx
}

fn fx_ctx(fx: Fx) -> WorkingContext {
    let (stream, n, salt) = fx_facts(fx);
    sample_ctx(stream, n, salt)
}

fn fx_bytes(fx: Fx) -> Vec<u8> {
    seal_context(&fx_ctx(fx)).unwrap()
}

/// Live target before every restore attempt: home stream, generation 2,
/// distinct contents so a replacement is always observable.
fn initial_target() -> WorkingContext {
    sample_ctx(STREAM, 2, 0x77)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn trailer(bytes: &[u8]) -> [u8; 32] {
    let mut d = [0_u8; 32];
    d.copy_from_slice(&bytes[bytes.len() - 32..]);
    d
}

// ---------------------------------------------------------------------------
// Independent oracle.
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // published G12 vocabulary; Unbound is a skip path unused by this corpus
enum OProv {
    Unbound,
    Bound,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct OIdent {
    stream: u64,
    authority: [u8; 32],
    domain: [u8; 4],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // published G10 vocabulary; Unanchored is a skip path unused by this corpus
enum OG10 {
    Unanchored,
    Required,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // published G10 vocabulary; Unverifiable is unused by this corpus
enum OAnchor {
    Absent,
    Unverifiable,
    Verified {
        stream: u64,
        generation: u64,
        digest: [u8; 32],
    },
}

/// Outcome vocabulary shared by oracle and classified production.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OOut {
    Accepted {
        anchored: bool,
        bound: bool,
    },
    VersionDowngrade {
        declared: u16,
    },
    VersionUnknown {
        declared: u16,
    },
    Envelope,
    ProvAbsent,
    ProvStream {
        expected: u64,
        presented: u64,
        derived: Option<u64>,
    },
    ProvAuthority {
        expected: [u8; 32],
        presented: [u8; 32],
    },
    ProvDomain {
        expected: [u8; 4],
        presented: [u8; 4],
        derived: Option<[u8; 4]>,
    },
    AnchorAbsent,
    AnchorUnverifiable,
    AnchorStream {
        anchor: u64,
        candidate: u64,
    },
    Rollback {
        anchor: u64,
        candidate: u64,
    },
    Stale {
        anchor: u64,
        candidate: u64,
    },
    Conflict {
        generation: u64,
    },
}

impl OOut {
    fn is_rejection(self) -> bool {
        !matches!(self, OOut::Accepted { .. })
    }
}

/// Published SWE1 v1 layout check (header, exact length, unkeyed digest).
fn oracle_envelope_ok(bytes: &[u8]) -> bool {
    if bytes.len() < 12 + 32 {
        return false;
    }
    if bytes[0..4] != DOMAIN_SWE1 || bytes[4..6] != [1, 0] || bytes[6] != 1 || bytes[7] != 0 {
        return false;
    }
    let l = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]) as usize;
    if bytes.len() != 12 + l + 32 {
        return false;
    }
    sha256(&bytes[..12 + l]) == trailer(bytes)
}

#[derive(Clone, Debug)]
struct Row {
    corpus_version: u16,
    name: &'static str,
    family: &'static str,
    declared_policy: u16,
    fixture: Fx,
    /// `None`: bytes are the sealed fixture; `Some(f)`: mutated bytes.
    mutate: Option<Mutation>,
    g10: OG10,
    anchor: OAnchor,
    prov: OProv,
    expected: OIdent,
    presented: Option<OIdent>,
    expect: OOut,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mutation {
    TruncateHalf,
    FlipLastBit,
}

fn apply(bytes: &[u8], m: Option<Mutation>) -> Vec<u8> {
    match m {
        None => bytes.to_vec(),
        Some(Mutation::TruncateHalf) => bytes[..bytes.len() / 2].to_vec(),
        Some(Mutation::FlipLastBit) => {
            let mut v = bytes.to_vec();
            let last = v.len() - 1;
            v[last] ^= 1;
            v
        }
    }
}

/// Pure oracle, priority-ordered as documented in the module header.
fn oracle(row: &Row, bytes: &[u8], facts: Option<(u64, u64)>) -> OOut {
    if row.declared_policy < ORACLE_SUPPORTED_POLICY {
        return OOut::VersionDowngrade {
            declared: row.declared_policy,
        };
    }
    if row.declared_policy > ORACLE_SUPPORTED_POLICY {
        return OOut::VersionUnknown {
            declared: row.declared_policy,
        };
    }
    if !oracle_envelope_ok(bytes) {
        return OOut::Envelope;
    }
    let (c_stream, c_gen) = facts.expect("valid envelope must have frozen facts");
    let c_digest = trailer(bytes);
    let mut d_domain = [0_u8; 4];
    d_domain.copy_from_slice(&bytes[..4]);
    let bound = match row.prov {
        OProv::Unbound => false,
        OProv::Bound => {
            let p = match row.presented {
                Some(p) => p,
                None => return OOut::ProvAbsent,
            };
            if p.stream != row.expected.stream || c_stream != row.expected.stream {
                return OOut::ProvStream {
                    expected: row.expected.stream,
                    presented: p.stream,
                    derived: Some(c_stream),
                };
            }
            if p.authority != row.expected.authority {
                return OOut::ProvAuthority {
                    expected: row.expected.authority,
                    presented: p.authority,
                };
            }
            if p.domain != row.expected.domain || d_domain != row.expected.domain {
                return OOut::ProvDomain {
                    expected: row.expected.domain,
                    presented: p.domain,
                    derived: Some(d_domain),
                };
            }
            true
        }
    };
    let anchored = match row.g10 {
        OG10::Unanchored => false,
        OG10::Required => match row.anchor {
            OAnchor::Absent => return OOut::AnchorAbsent,
            OAnchor::Unverifiable => return OOut::AnchorUnverifiable,
            OAnchor::Verified {
                stream,
                generation,
                digest,
            } => {
                if stream != c_stream {
                    return OOut::AnchorStream {
                        anchor: stream,
                        candidate: c_stream,
                    };
                }
                if c_gen < generation {
                    return OOut::Rollback {
                        anchor: generation,
                        candidate: c_gen,
                    };
                }
                if c_gen > generation {
                    return OOut::Stale {
                        anchor: generation,
                        candidate: c_gen,
                    };
                }
                if digest != c_digest {
                    return OOut::Conflict { generation };
                }
                true
            }
        },
    };
    OOut::Accepted { anchored, bound }
}

// ---------------------------------------------------------------------------
// Production adapters and classification.
// ---------------------------------------------------------------------------

fn prod_ident(i: OIdent) -> AnchorIdentity {
    AnchorIdentity {
        stream_id: i.stream,
        authority: AuthorityId(i.authority),
        digest_domain: DigestDomain(i.domain),
    }
}

fn prod_anchor(a: OAnchor) -> AnchorState {
    match a {
        OAnchor::Absent => AnchorState::Absent,
        OAnchor::Unverifiable => AnchorState::Unverifiable,
        OAnchor::Verified {
            stream,
            generation,
            digest,
        } => AnchorState::Verified(AnchorRecord {
            stream_id: stream,
            generation,
            digest,
        }),
    }
}

fn classify(r: Result<(Admission, ProvenanceAdmission), VersionedRestoreError>) -> OOut {
    match r {
        Ok((a, p)) => OOut::Accepted {
            anchored: matches!(a, Admission::Anchored),
            bound: matches!(p, ProvenanceAdmission::Bound),
        },
        Err(VersionedRestoreError::PolicyVersion(PolicyVersionError::Downgrade {
            declared,
            supported,
        })) => {
            assert_eq!(supported, ORACLE_SUPPORTED_POLICY);
            OOut::VersionDowngrade { declared }
        }
        Err(VersionedRestoreError::PolicyVersion(PolicyVersionError::Unknown {
            declared,
            supported,
        })) => {
            assert_eq!(supported, ORACLE_SUPPORTED_POLICY);
            OOut::VersionUnknown { declared }
        }
        Err(VersionedRestoreError::Restore(ProvenanceRestoreError::Provenance(e))) => match e {
            ProvenanceError::ProvenanceAbsent => OOut::ProvAbsent,
            ProvenanceError::StreamBindingMismatch {
                expected,
                presented,
                derived,
            } => OOut::ProvStream {
                expected,
                presented,
                derived,
            },
            ProvenanceError::AuthorityMismatch {
                expected,
                presented,
            } => OOut::ProvAuthority {
                expected: expected.0,
                presented: presented.0,
            },
            ProvenanceError::DigestDomainMismatch {
                expected,
                presented,
                derived,
            } => OOut::ProvDomain {
                expected: expected.0,
                presented: presented.0,
                derived: derived.map(|d| d.0),
            },
        },
        Err(VersionedRestoreError::Restore(ProvenanceRestoreError::Anchor(e))) => match e {
            AnchorError::Envelope(_) => OOut::Envelope,
            AnchorError::AnchorAbsent => OOut::AnchorAbsent,
            AnchorError::AnchorUnverifiable => OOut::AnchorUnverifiable,
            AnchorError::StreamMismatch { anchor, candidate } => {
                OOut::AnchorStream { anchor, candidate }
            }
            AnchorError::Rollback { anchor, candidate } => OOut::Rollback { anchor, candidate },
            AnchorError::AnchorStale { anchor, candidate } => OOut::Stale { anchor, candidate },
            AnchorError::ConflictingHistory { generation } => OOut::Conflict { generation },
            other @ (AnchorError::NonIncreasing { .. } | AnchorError::GenerationOverflow) => {
                panic!("restore path must not report advance errors: {other:?}")
            }
        },
    }
}

/// Byte-for-byte fingerprint of the live target: canonical SWC2 encoding,
/// SWE1 seal, and every stats counter.
fn fingerprint(ctx: &WorkingContext) -> (Vec<u8>, Vec<u8>, WorkingContextStats) {
    (
        encode_context(ctx).unwrap(),
        seal_context(ctx).unwrap(),
        ctx.snapshot().stats,
    )
}

fn run_prod(target: &mut WorkingContext, row: &Row, bytes: &[u8]) -> OOut {
    let state = prod_anchor(row.anchor);
    let state_before = state;
    let policy = match row.prov {
        OProv::Unbound => ProvenancePolicy::Unbound,
        OProv::Bound => ProvenancePolicy::Bound(prod_ident(row.expected)),
    };
    let g10p = match row.g10 {
        OG10::Unanchored => AnchorPolicy::Unanchored,
        OG10::Required => AnchorPolicy::Required,
    };
    let presented = row.presented.map(prod_ident);
    let out = classify(restore_into_anchored_with_versioned_provenance(
        target,
        bytes,
        row.declared_policy,
        g10p,
        &state,
        policy,
        presented.as_ref(),
    ));
    assert_eq!(state, state_before, "anchor state must never be advanced");
    out
}

// ---------------------------------------------------------------------------
// The frozen differential corpus.
// ---------------------------------------------------------------------------

fn auth(byte: u8) -> [u8; 32] {
    [byte; 32]
}

fn auth_near(byte: u8) -> [u8; 32] {
    let mut a = [byte; 32];
    a[31] = byte.wrapping_add(1);
    a
}

fn id(stream: u64, authority: [u8; 32]) -> OIdent {
    OIdent {
        stream,
        authority,
        domain: DOMAIN_SWE1,
    }
}

fn verified(fx: Fx) -> OAnchor {
    let (stream, generation, _) = fx_facts(fx);
    OAnchor::Verified {
        stream,
        generation,
        digest: trailer(&fx_bytes(fx)),
    }
}

/// Digest of `digest_of` presented as an anchor on `stream`.
fn verified_digest_on(stream: u64, generation: u64, digest_of: Fx) -> OAnchor {
    OAnchor::Verified {
        stream,
        generation,
        digest: trailer(&fx_bytes(digest_of)),
    }
}

/// Differential triples: isolate A, isolate B, combined A+B. Combined
/// must equal the published winner (named here as `winner`).
struct Triple {
    family: &'static str,
    isolate_a: &'static str,
    isolate_b: &'static str,
    combined: &'static str,
    winner: &'static str,
}

const TRIPLES: [Triple; 7] = [
    Triple {
        family: "version_and_corrupt_swe1",
        isolate_a: "isolate_unknown_v2",
        isolate_b: "isolate_corrupt_swe1",
        combined: "combined_unknown_v2_and_corrupt_swe1",
        winner: "isolate_unknown_v2",
    },
    Triple {
        family: "version_and_corrupt_swe1",
        isolate_a: "isolate_downgrade_v0",
        isolate_b: "isolate_corrupt_swe1",
        combined: "combined_downgrade_v0_and_corrupt_swe1",
        winner: "isolate_downgrade_v0",
    },
    Triple {
        family: "absent_anchor_and_wrong_authority",
        isolate_a: "isolate_absent_anchor",
        isolate_b: "isolate_wrong_authority",
        combined: "combined_absent_and_wrong_authority",
        winner: "isolate_wrong_authority",
    },
    Triple {
        family: "matching_digest_and_different_stream",
        isolate_a: "isolate_conflict_same_stream_other_digest",
        isolate_b: "isolate_stream_and_digest_mismatch",
        combined: "combined_matching_digest_different_stream",
        winner: "isolate_stream_and_digest_mismatch",
    },
    Triple {
        family: "rollback_and_malformed_seal",
        isolate_a: "isolate_rollback",
        isolate_b: "isolate_malformed_seal",
        combined: "combined_rollback_and_malformed_seal",
        winner: "isolate_malformed_seal",
    },
    Triple {
        family: "version_and_provenance_mismatch",
        isolate_a: "isolate_unknown_v2",
        isolate_b: "isolate_wrong_authority",
        combined: "combined_unknown_v2_and_wrong_authority",
        winner: "isolate_unknown_v2",
    },
    Triple {
        family: "version_and_provenance_mismatch",
        isolate_a: "isolate_downgrade_v0",
        isolate_b: "isolate_provenance_stream_mismatch",
        combined: "combined_downgrade_v0_and_stream_mismatch",
        winner: "isolate_downgrade_v0",
    },
];

fn corpus() -> Vec<Row> {
    let a = auth(0xC5);
    let exp = id(STREAM, a);
    let wrong = id(STREAM, auth_near(0xC5));
    let a3 = verified(Fx::Home3);
    let base = Row {
        corpus_version: G15_CORPUS_VERSION,
        name: "",
        family: "",
        declared_policy: ORACLE_SUPPORTED_POLICY,
        fixture: Fx::Home3,
        mutate: None,
        g10: OG10::Required,
        anchor: a3,
        prov: OProv::Bound,
        expected: exp,
        presented: Some(exp),
        expect: OOut::AnchorAbsent,
    };
    let r = |name: &'static str, family: &'static str, f: &dyn Fn(&mut Row)| {
        let mut row = base.clone();
        row.name = name;
        row.family = family;
        f(&mut row);
        row
    };
    vec![
        // --- unsupported policy version + corrupt SWE1 -------------------
        r("isolate_unknown_v2", "version_and_corrupt_swe1", &|x| {
            x.declared_policy = 2;
            x.expect = OOut::VersionUnknown { declared: 2 };
        }),
        r("isolate_downgrade_v0", "version_and_corrupt_swe1", &|x| {
            x.declared_policy = 0;
            x.expect = OOut::VersionDowngrade { declared: 0 };
        }),
        r("isolate_corrupt_swe1", "version_and_corrupt_swe1", &|x| {
            x.mutate = Some(Mutation::TruncateHalf);
            x.expect = OOut::Envelope;
        }),
        r(
            "combined_unknown_v2_and_corrupt_swe1",
            "version_and_corrupt_swe1",
            &|x| {
                x.declared_policy = 2;
                x.mutate = Some(Mutation::TruncateHalf);
                x.expect = OOut::VersionUnknown { declared: 2 };
            },
        ),
        r(
            "combined_downgrade_v0_and_corrupt_swe1",
            "version_and_corrupt_swe1",
            &|x| {
                x.declared_policy = 0;
                x.mutate = Some(Mutation::TruncateHalf);
                x.expect = OOut::VersionDowngrade { declared: 0 };
            },
        ),
        // --- absent anchor + wrong authority -----------------------------
        r(
            "isolate_absent_anchor",
            "absent_anchor_and_wrong_authority",
            &|x| {
                x.anchor = OAnchor::Absent;
                x.expect = OOut::AnchorAbsent;
            },
        ),
        r(
            "isolate_wrong_authority",
            "absent_anchor_and_wrong_authority",
            &|x| {
                x.presented = Some(wrong);
                x.expect = OOut::ProvAuthority {
                    expected: a,
                    presented: auth_near(0xC5),
                };
            },
        ),
        r(
            "combined_absent_and_wrong_authority",
            "absent_anchor_and_wrong_authority",
            &|x| {
                x.anchor = OAnchor::Absent;
                x.presented = Some(wrong);
                x.expect = OOut::ProvAuthority {
                    expected: a,
                    presented: auth_near(0xC5),
                };
            },
        ),
        // --- matching digest + different stream --------------------------
        r(
            "isolate_conflict_same_stream_other_digest",
            "matching_digest_and_different_stream",
            &|x| {
                x.anchor = verified_digest_on(STREAM, 3, Fx::Foreign3);
                x.expect = OOut::Conflict { generation: 3 };
            },
        ),
        r(
            "isolate_stream_and_digest_mismatch",
            "matching_digest_and_different_stream",
            &|x| {
                x.anchor = verified(Fx::Foreign3);
                x.expect = OOut::AnchorStream {
                    anchor: OTHER,
                    candidate: STREAM,
                };
            },
        ),
        r(
            "combined_matching_digest_different_stream",
            "matching_digest_and_different_stream",
            &|x| {
                x.anchor = verified_digest_on(OTHER, 3, Fx::Home3);
                x.expect = OOut::AnchorStream {
                    anchor: OTHER,
                    candidate: STREAM,
                };
            },
        ),
        // --- generation rollback + malformed seal ------------------------
        r("isolate_rollback", "rollback_and_malformed_seal", &|x| {
            x.fixture = Fx::Home1;
            x.expect = OOut::Rollback {
                anchor: 3,
                candidate: 1,
            };
        }),
        r(
            "isolate_malformed_seal",
            "rollback_and_malformed_seal",
            &|x| {
                x.mutate = Some(Mutation::FlipLastBit);
                x.expect = OOut::Envelope;
            },
        ),
        r(
            "combined_rollback_and_malformed_seal",
            "rollback_and_malformed_seal",
            &|x| {
                x.fixture = Fx::Home1;
                x.mutate = Some(Mutation::FlipLastBit);
                x.expect = OOut::Envelope;
            },
        ),
        // --- version and provenance mismatch -----------------------------
        r(
            "isolate_provenance_stream_mismatch",
            "version_and_provenance_mismatch",
            &|x| {
                x.fixture = Fx::Foreign3;
                x.expect = OOut::ProvStream {
                    expected: STREAM,
                    presented: STREAM,
                    derived: Some(OTHER),
                };
            },
        ),
        r(
            "combined_unknown_v2_and_wrong_authority",
            "version_and_provenance_mismatch",
            &|x| {
                x.declared_policy = 2;
                x.presented = Some(wrong);
                x.expect = OOut::VersionUnknown { declared: 2 };
            },
        ),
        r(
            "combined_downgrade_v0_and_stream_mismatch",
            "version_and_provenance_mismatch",
            &|x| {
                x.declared_policy = 0;
                x.fixture = Fx::Foreign3;
                x.expect = OOut::VersionDowngrade { declared: 0 };
            },
        ),
    ]
}

fn row_bytes(row: &Row) -> Vec<u8> {
    apply(&fx_bytes(row.fixture), row.mutate)
}

fn row_facts(row: &Row) -> Option<(u64, u64)> {
    match row.mutate {
        None => {
            let (s, g, _) = fx_facts(row.fixture);
            Some((s, g))
        }
        Some(_) => None,
    }
}

fn by_name<'a>(rows: &'a [Row], name: &str) -> &'a Row {
    rows.iter()
        .find(|r| r.name == name)
        .unwrap_or_else(|| panic!("missing corpus row {name}"))
}

/// Canonical, deterministic rendering of the corpus (one line per row).
fn render(rows: &[Row]) -> String {
    let mut out = String::new();
    for row in rows {
        out.push_str(&format!(
            "{}|{}|{}|{}|{:?}|{:?}|{}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}\n",
            row.corpus_version,
            row.family,
            row.name,
            row.declared_policy,
            row.fixture,
            row.mutate,
            hex(&sha256(&row_bytes(row))),
            row.g10,
            row.anchor,
            row.prov,
            row.expected,
            row.presented,
            row.expect,
        ));
    }
    out
}

// ---------------------------------------------------------------------------
// Tests.
// ---------------------------------------------------------------------------

#[test]
fn corpus_is_versioned_frozen_and_reproducible() {
    assert_eq!(ANCHOR_PROVENANCE_POLICY_VERSION, ORACLE_SUPPORTED_POLICY);
    for fx in FIXTURES {
        let a = fx_bytes(fx);
        let b = fx_bytes(fx);
        assert_eq!(a, b, "{fx:?}: sealing must be deterministic");
        assert_eq!(
            hex(&sha256(&a)),
            fx_frozen_sha(fx),
            "{fx:?}: frozen fixture sha"
        );
        let (stream, generation, _) = fx_facts(fx);
        let snap = fx_ctx(fx).snapshot();
        assert_eq!(snap.stream_id.get(), stream, "{fx:?}");
        assert_eq!(snap.stats.admitted, generation, "{fx:?}");
    }
    let rows = corpus();
    assert_eq!(rows.len(), 17, "frozen corpus size");
    let mut names: Vec<&str> = rows.iter().map(|r| r.name).collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), rows.len(), "row names are unique");
    for fam in [
        "version_and_corrupt_swe1",
        "absent_anchor_and_wrong_authority",
        "matching_digest_and_different_stream",
        "rollback_and_malformed_seal",
        "version_and_provenance_mismatch",
    ] {
        assert!(
            rows.iter().filter(|r| r.family == fam).count() >= 3,
            "{fam}"
        );
    }
    let rendered = render(&rows);
    assert_eq!(rendered, render(&corpus()), "rendering is deterministic");
    assert_eq!(
        hex(&sha256(rendered.as_bytes())),
        FROZEN_CORPUS_SHA256,
        "frozen corpus digest (bump G15_CORPUS_VERSION and re-freeze on change)"
    );
}

#[test]
fn isolate_causes_disagree_and_combined_follows_published_precedence() {
    let rows = corpus();
    assert_eq!(TRIPLES.len(), 7, "frozen differential triple count");
    for t in &TRIPLES {
        let a = by_name(&rows, t.isolate_a);
        let b = by_name(&rows, t.isolate_b);
        let c = by_name(&rows, t.combined);
        let w = by_name(&rows, t.winner);
        assert_eq!(c.family, t.family, "{}", t.combined);
        assert_ne!(
            a.expect, b.expect,
            "{} vs {}: isolates must be distinct typed causes",
            a.name, b.name
        );
        assert!(a.expect.is_rejection(), "{}", a.name);
        assert!(b.expect.is_rejection(), "{}", b.name);
        assert!(c.expect.is_rejection(), "{}", c.name);
        assert_eq!(
            c.expect, w.expect,
            "{} must report the higher-precedence error {}",
            c.name, w.name
        );
        assert!(
            c.expect == a.expect || c.expect == b.expect,
            "{} must equal one of the two isolates, not a third error",
            c.name
        );
    }
}

#[test]
fn every_corpus_row_matches_oracle_and_production_without_mutation() {
    for row in corpus() {
        assert_eq!(row.corpus_version, G15_CORPUS_VERSION, "{}", row.name);
        assert!(row.expect.is_rejection(), "{}: negative corpus", row.name);
        let bytes = row_bytes(&row);
        let o = oracle(&row, &bytes, row_facts(&row));
        assert_eq!(o, row.expect, "oracle {}", row.name);

        let mut target = initial_target();
        let before = fingerprint(&target);
        let p = run_prod(&mut target, &row, &bytes);
        assert_eq!(p, o, "production vs oracle {}", row.name);
        assert_eq!(
            fingerprint(&target),
            before,
            "{}: target SWC2/SWE1/stats must be unchanged",
            row.name
        );
    }
}

#[test]
fn positive_control_matching_digest_same_stream_is_accepted() {
    // Control for the matching-digest family: when stream and digest both
    // match, the snapshot is accepted. This is not a rejection cause.
    let a = auth(0xC5);
    let exp = id(STREAM, a);
    let row = Row {
        corpus_version: G15_CORPUS_VERSION,
        name: "positive_matching_digest_same_stream",
        family: "matching_digest_and_different_stream",
        declared_policy: ORACLE_SUPPORTED_POLICY,
        fixture: Fx::Home3,
        mutate: None,
        g10: OG10::Required,
        anchor: verified(Fx::Home3),
        prov: OProv::Bound,
        expected: exp,
        presented: Some(exp),
        expect: OOut::Accepted {
            anchored: true,
            bound: true,
        },
    };
    let bytes = row_bytes(&row);
    let o = oracle(&row, &bytes, row_facts(&row));
    assert_eq!(o, row.expect, "oracle");
    let mut target = initial_target();
    let p = run_prod(&mut target, &row, &bytes);
    assert_eq!(p, o, "production");
    assert_eq!(fingerprint(&target), fingerprint(&fx_ctx(Fx::Home3)));
    assert_eq!(
        seal_context(&target).unwrap(),
        bytes,
        "no replay, exact snapshot"
    );
}
