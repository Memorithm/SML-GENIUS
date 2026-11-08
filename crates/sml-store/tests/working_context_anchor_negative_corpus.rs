//! CONTEXT-2 G14: deterministic, versioned negative-case corpus for anchor
//! admission under snapshot replacement and policy-version declaration.
//!
//! Every row of the frozen corpus ([`G14_CORPUS_VERSION`]) is evaluated by an
//! **independent pure oracle** and by production
//! [`restore_into_anchored_with_versioned_provenance`]; the outcomes must be
//! identical, and every rejection must leave the target byte-for-byte
//! unchanged (canonical SWC2 encoding, SWE1 seal and all five
//! `WorkingContextStats` counters).
//!
//! Corpus families (frozen):
//! - **cross-stream swap**: a valid foreign-stream SWE1 snapshot presented
//!   for the home stream (home claim, honest foreign claim, unbound
//!   provenance with a required anchor, and a consistently swapped
//!   foreign anchor + foreign bytes pair);
//! - **policy downgrade / unknown version**: declared policy versions `0`,
//!   `2` and `u16::MAX` against the only supported version `1`, including
//!   masking of corrupt bytes and cross-stream bytes (the version gate
//!   runs before the bytes are parsed). There is **no migration**: every
//!   other version is rejected explicitly;
//! - **identical digest, different declared authority**: the anchored
//!   snapshot and anchor digest match exactly, only the declared authority
//!   differs (near collision, all-zero, or an expected authority the
//!   presenter does not hold);
//! - **stale generation**: rollback to an older snapshot, a snapshot newer
//!   than a not-yet-advanced anchor, and a same-generation fork;
//! - **restoration interruption**: truncated / torn envelopes (empty,
//!   header-only, mid-payload, missing trailer byte, trailer bit flip) plus an
//!   exhaustive sweep of every proper prefix and every single-bit flip.
//!
//! Oracle independence: the oracle re-implements the published SWE1 layout
//! check, the G12 provenance order, the G10 anchor order and the G14
//! version gate with its own types. It does not call production
//! `open`, `candidate_of`, `derived_binding`, `admit_provenance`,
//! `admit_with_provenance`, `admit`, `advance` or `check_policy_version`.
//! It shares only the SHA-256 primitive (`sml_core::elastic_ngram_paged_sha`)
//! and takes the stream/generation of each fixture from frozen corpus facts
//! instead of decoding SWC2.
//!
//! Limits: authorities and anchors are in-process mocks. A matching
//! authority tag is an equality binding, **not** a cryptographic origin.
//! SWE1 is unkeyed SHA-256 (corruption detection, not authentication).
//! The declared policy version is a caller assertion, not authenticated.
//! No migration, signature, attestation, hardware counter, transport or
//! E2E claim. `NOT_MEASURED`.

use sml_core::context_ledger::{ContextContentHash, ContextSegmentId, ContextStreamId};
use sml_core::elastic_ngram_paged_sha::sha256;
use sml_store::working_context::{
    WorkingContext, WorkingContextStats, WorkingPolicy, WorkingSegment,
};
use sml_store::working_context_anchor::{
    advance, Admission, AnchorError, AnchorPolicy, AnchorRecord, AnchorState, Candidate,
};
use sml_store::working_context_anchor_provenance::{
    check_policy_version, restore_into_anchored_with_versioned_provenance, AnchorIdentity,
    AuthorityId, DigestDomain, PolicyVersionError, ProvenanceAdmission, ProvenanceError,
    ProvenancePolicy, ProvenanceRestoreError, VersionedRestoreError,
    ANCHOR_PROVENANCE_POLICY_VERSION,
};
use sml_store::working_context_envelope::seal_context;
use sml_store::working_context_wire::encode_context;

/// Version of this frozen negative-case corpus. Bump only when rows,
/// fixtures or expected outcomes change (and re-freeze the digests below).
const G14_CORPUS_VERSION: u16 = 1;
/// The oracle's own notion of the single supported policy version.
const ORACLE_SUPPORTED_POLICY: u16 = 1;

const STREAM: u64 = 0x4743_0000_0000_0014;
const OTHER: u64 = 0x4743_0000_0000_0041;

const DOMAIN_SWE1: [u8; 4] = *b"SWE1";

// ---------------------------------------------------------------------------
// Frozen fixtures.
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Fx {
    /// Home stream, generation 3 (the anchored snapshot).
    Home3,
    /// Home stream, generation 1 (older snapshot).
    Home1,
    /// Home stream, generation 4 (newer than the anchor).
    Home4,
    /// Home stream, generation 3, different contents (fork).
    Fork3,
    /// Foreign stream, generation 3, same salt as `Home3`.
    Foreign3,
}

const FIXTURES: [Fx; 5] = [Fx::Home3, Fx::Home1, Fx::Home4, Fx::Fork3, Fx::Foreign3];

/// Frozen facts the oracle uses instead of decoding SWC2.
fn fx_facts(fx: Fx) -> (u64, u64, u8) {
    match fx {
        Fx::Home3 => (STREAM, 3, 0x14),
        Fx::Home1 => (STREAM, 1, 0x14),
        Fx::Home4 => (STREAM, 4, 0x14),
        Fx::Fork3 => (STREAM, 3, 0x41),
        Fx::Foreign3 => (OTHER, 3, 0x14),
    }
}

/// Frozen SHA-256 of each sealed fixture envelope (reproducibility pin).
fn fx_frozen_sha(fx: Fx) -> &'static str {
    match fx {
        Fx::Home3 => "479bd4466992c12dcfc4736a9e561ea96d1d2f408db125eb96536e06257a818f",
        Fx::Home1 => "8413364c833d8c4f116c4ccc3ce41194155399df76c1176d8e21ea981e594b5c",
        Fx::Home4 => "e4d2ec97ab59439fa0d11fbacec60b4a14aeb1942a3e4c97594a4efba4907eb4",
        Fx::Fork3 => "01cb4be368025cbc132177a9becd787c6def8b4297fb8617a4ab73edc846a435",
        Fx::Foreign3 => "d164bf67a89d9f14cfe5d11a3733c0f28a7fbf6f31fdb7f7c378c57551902b1a",
    }
}

/// Frozen SHA-256 over the canonical rendering of the whole corpus.
const FROZEN_CORPUS_SHA256: &str =
    "ba29da3d8c14c713e028bc6c420317d6ea1a52a62d5c0113334a60bf6845e28a";

fn sample_ctx(stream: u64, n: u64, salt: u8) -> WorkingContext {
    let s = ContextStreamId::new(stream).expect("nonzero stream id");
    let mut ctx = WorkingContext::new(s, WorkingPolicy::CapacityBounded { capacity: 4 }).unwrap();
    for i in 1..=n {
        let mut hash = [salt; 32];
        hash[0] = u8::try_from(i).expect("fixture generation fits u8");
        let seg = WorkingSegment::new(
            ContextSegmentId::new(s, 14_000 + i).unwrap(),
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
enum OG10 {
    Unanchored,
    Required,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
    TruncateTo(usize),
    TruncateHalf,
    DropLast,
    FlipLastBit,
}

fn apply(bytes: &[u8], m: Option<Mutation>) -> Vec<u8> {
    match m {
        None => bytes.to_vec(),
        Some(Mutation::TruncateTo(n)) => bytes[..n].to_vec(),
        Some(Mutation::TruncateHalf) => bytes[..bytes.len() / 2].to_vec(),
        Some(Mutation::DropLast) => bytes[..bytes.len() - 1].to_vec(),
        Some(Mutation::FlipLastBit) => {
            let mut v = bytes.to_vec();
            let last = v.len() - 1;
            v[last] ^= 1;
            v
        }
    }
}

/// Pure oracle, priority-ordered:
/// 1. declared version `< 1` → downgrade, `> 1` → unknown (no migration,
///    before the bytes are examined);
/// 2. SWE1 layout/digest failure → `Envelope`;
/// 3. G12 provenance: unbound skips; absent; stream (presented or derived);
///    authority (full 32 bytes); domain (presented or derived magic);
/// 4. G10: unanchored skips; absent; unverifiable; stream; rollback;
///    stale; conflict; anchored.
#[allow(clippy::too_many_arguments)]
fn oracle(
    declared: u16,
    bytes: &[u8],
    facts: Option<(u64, u64)>,
    g10: OG10,
    anchor: OAnchor,
    prov: OProv,
    expected: OIdent,
    presented: Option<OIdent>,
) -> OOut {
    if declared < ORACLE_SUPPORTED_POLICY {
        return OOut::VersionDowngrade { declared };
    }
    if declared > ORACLE_SUPPORTED_POLICY {
        return OOut::VersionUnknown { declared };
    }
    if !oracle_envelope_ok(bytes) {
        return OOut::Envelope;
    }
    let (c_stream, c_gen) = facts.expect("valid envelope must have frozen facts");
    let c_digest = trailer(bytes);
    let mut d_domain = [0_u8; 4];
    d_domain.copy_from_slice(&bytes[..4]);
    let bound = match prov {
        OProv::Unbound => false,
        OProv::Bound => {
            let p = match presented {
                Some(p) => p,
                None => return OOut::ProvAbsent,
            };
            if p.stream != expected.stream || c_stream != expected.stream {
                return OOut::ProvStream {
                    expected: expected.stream,
                    presented: p.stream,
                    derived: Some(c_stream),
                };
            }
            if p.authority != expected.authority {
                return OOut::ProvAuthority {
                    expected: expected.authority,
                    presented: p.authority,
                };
            }
            if p.domain != expected.domain || d_domain != expected.domain {
                return OOut::ProvDomain {
                    expected: expected.domain,
                    presented: p.domain,
                    derived: Some(d_domain),
                };
            }
            true
        }
    };
    let anchored = match g10 {
        OG10::Unanchored => false,
        OG10::Required => match anchor {
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

#[allow(clippy::too_many_arguments)]
fn run_prod(
    target: &mut WorkingContext,
    declared: u16,
    bytes: &[u8],
    g10: OG10,
    anchor: OAnchor,
    prov: OProv,
    expected: OIdent,
    presented: Option<OIdent>,
) -> OOut {
    let state = prod_anchor(anchor);
    let state_before = state;
    let policy = match prov {
        OProv::Unbound => ProvenancePolicy::Unbound,
        OProv::Bound => ProvenancePolicy::Bound(prod_ident(expected)),
    };
    let g10p = match g10 {
        OG10::Unanchored => AnchorPolicy::Unanchored,
        OG10::Required => AnchorPolicy::Required,
    };
    let presented = presented.map(prod_ident);
    let out = classify(restore_into_anchored_with_versioned_provenance(
        target,
        bytes,
        declared,
        g10p,
        &state,
        policy,
        presented.as_ref(),
    ));
    assert_eq!(state, state_before, "anchor state must never be advanced");
    out
}

// ---------------------------------------------------------------------------
// The frozen corpus.
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

fn corpus() -> Vec<Row> {
    let a = auth(0xC1);
    let exp = id(STREAM, a);
    let a3 = verified(Fx::Home3);
    let base = Row {
        corpus_version: G14_CORPUS_VERSION,
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
        // --- cross-stream swap -------------------------------------------
        r("swap_foreign_bytes_home_claim", "cross_stream_swap", &|x| {
            x.fixture = Fx::Foreign3;
            x.expect = OOut::ProvStream {
                expected: STREAM,
                presented: STREAM,
                derived: Some(OTHER),
            };
        }),
        r(
            "swap_foreign_bytes_foreign_claim",
            "cross_stream_swap",
            &|x| {
                x.fixture = Fx::Foreign3;
                x.presented = Some(id(OTHER, a));
                x.expect = OOut::ProvStream {
                    expected: STREAM,
                    presented: OTHER,
                    derived: Some(OTHER),
                };
            },
        ),
        r(
            "swap_foreign_bytes_unbound_required_anchor",
            "cross_stream_swap",
            &|x| {
                x.fixture = Fx::Foreign3;
                x.prov = OProv::Unbound;
                x.expect = OOut::AnchorStream {
                    anchor: STREAM,
                    candidate: OTHER,
                };
            },
        ),
        r(
            "swap_foreign_bytes_and_foreign_anchor",
            "cross_stream_swap",
            &|x| {
                x.fixture = Fx::Foreign3;
                x.anchor = verified(Fx::Foreign3);
                x.presented = Some(id(OTHER, a));
                x.expect = OOut::ProvStream {
                    expected: STREAM,
                    presented: OTHER,
                    derived: Some(OTHER),
                };
            },
        ),
        // --- policy downgrade / unknown version ---------------------------
        r(
            "policy_downgrade_v0_otherwise_valid",
            "policy_version",
            &|x| {
                x.declared_policy = 0;
                x.expect = OOut::VersionDowngrade { declared: 0 };
            },
        ),
        r(
            "policy_unknown_v2_otherwise_valid",
            "policy_version",
            &|x| {
                x.declared_policy = 2;
                x.expect = OOut::VersionUnknown { declared: 2 };
            },
        ),
        r(
            "policy_unknown_max_otherwise_valid",
            "policy_version",
            &|x| {
                x.declared_policy = u16::MAX;
                x.expect = OOut::VersionUnknown { declared: u16::MAX };
            },
        ),
        r(
            "policy_unknown_v2_before_corrupt_bytes",
            "policy_version",
            &|x| {
                x.declared_policy = 2;
                x.mutate = Some(Mutation::TruncateHalf);
                x.expect = OOut::VersionUnknown { declared: 2 };
            },
        ),
        r(
            "policy_downgrade_v0_before_cross_stream",
            "policy_version",
            &|x| {
                x.declared_policy = 0;
                x.fixture = Fx::Foreign3;
                x.expect = OOut::VersionDowngrade { declared: 0 };
            },
        ),
        r(
            "policy_unknown_v2_unbound_unanchored",
            "policy_version",
            &|x| {
                x.declared_policy = 2;
                x.g10 = OG10::Unanchored;
                x.prov = OProv::Unbound;
                x.expect = OOut::VersionUnknown { declared: 2 };
            },
        ),
        // --- identical digest, different declared authority ---------------
        r(
            "same_digest_near_collision_authority",
            "same_digest_other_authority",
            &|x| {
                x.presented = Some(id(STREAM, auth_near(0xC1)));
                x.expect = OOut::ProvAuthority {
                    expected: a,
                    presented: auth_near(0xC1),
                };
            },
        ),
        r(
            "same_digest_zero_authority",
            "same_digest_other_authority",
            &|x| {
                x.presented = Some(id(STREAM, auth(0x00)));
                x.expect = OOut::ProvAuthority {
                    expected: a,
                    presented: auth(0x00),
                };
            },
        ),
        r(
            "same_digest_expected_authority_not_held",
            "same_digest_other_authority",
            &|x| {
                x.expected = id(STREAM, auth(0xB2));
                x.expect = OOut::ProvAuthority {
                    expected: auth(0xB2),
                    presented: a,
                };
            },
        ),
        r(
            "same_digest_absent_authority_claim",
            "same_digest_other_authority",
            &|x| {
                x.presented = None;
                x.expect = OOut::ProvAbsent;
            },
        ),
        // --- stale generation ---------------------------------------------
        r("stale_rollback_gen1_vs_anchor3", "stale_generation", &|x| {
            x.fixture = Fx::Home1;
            x.expect = OOut::Rollback {
                anchor: 3,
                candidate: 1,
            };
        }),
        r("stale_rollback_gen1_unbound", "stale_generation", &|x| {
            x.fixture = Fx::Home1;
            x.prov = OProv::Unbound;
            x.expect = OOut::Rollback {
                anchor: 3,
                candidate: 1,
            };
        }),
        r("stale_anchor_gen4_not_advanced", "stale_generation", &|x| {
            x.fixture = Fx::Home4;
            x.expect = OOut::Stale {
                anchor: 3,
                candidate: 4,
            };
        }),
        r(
            "stale_fork_same_generation_other_digest",
            "stale_generation",
            &|x| {
                x.fixture = Fx::Fork3;
                x.expect = OOut::Conflict { generation: 3 };
            },
        ),
        r("stale_anchor_absent", "stale_generation", &|x| {
            x.anchor = OAnchor::Absent;
            x.expect = OOut::AnchorAbsent;
        }),
        r("stale_anchor_unverifiable", "stale_generation", &|x| {
            x.anchor = OAnchor::Unverifiable;
            x.expect = OOut::AnchorUnverifiable;
        }),
        // --- restoration interruption --------------------------------------
        r("interrupted_empty", "restoration_interruption", &|x| {
            x.mutate = Some(Mutation::TruncateTo(0));
            x.expect = OOut::Envelope;
        }),
        r(
            "interrupted_header_only",
            "restoration_interruption",
            &|x| {
                x.mutate = Some(Mutation::TruncateTo(12));
                x.expect = OOut::Envelope;
            },
        ),
        r(
            "interrupted_mid_payload",
            "restoration_interruption",
            &|x| {
                x.mutate = Some(Mutation::TruncateHalf);
                x.expect = OOut::Envelope;
            },
        ),
        r(
            "interrupted_missing_last_trailer_byte",
            "restoration_interruption",
            &|x| {
                x.mutate = Some(Mutation::DropLast);
                x.expect = OOut::Envelope;
            },
        ),
        r(
            "torn_write_trailer_bit_flip",
            "restoration_interruption",
            &|x| {
                x.mutate = Some(Mutation::FlipLastBit);
                x.expect = OOut::Envelope;
            },
        ),
        r(
            "interrupted_unbound_unanchored_still_rejected",
            "restoration_interruption",
            &|x| {
                x.mutate = Some(Mutation::TruncateHalf);
                x.g10 = OG10::Unanchored;
                x.prov = OProv::Unbound;
                x.expect = OOut::Envelope;
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
    assert_eq!(rows.len(), 26, "frozen corpus size");
    let mut names: Vec<&str> = rows.iter().map(|r| r.name).collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), rows.len(), "row names are unique");
    for fam in [
        "cross_stream_swap",
        "policy_version",
        "same_digest_other_authority",
        "stale_generation",
        "restoration_interruption",
    ] {
        assert!(
            rows.iter().filter(|r| r.family == fam).count() >= 4,
            "{fam}"
        );
    }
    let rendered = render(&rows);
    assert_eq!(rendered, render(&corpus()), "rendering is deterministic");
    assert_eq!(
        hex(&sha256(rendered.as_bytes())),
        FROZEN_CORPUS_SHA256,
        "frozen corpus digest (bump G14_CORPUS_VERSION and re-freeze on change)"
    );
}

#[test]
fn every_corpus_row_is_rejected_identically_by_oracle_and_production() {
    for row in corpus() {
        assert_eq!(row.corpus_version, G14_CORPUS_VERSION, "{}", row.name);
        assert!(row.expect.is_rejection(), "{}: negative corpus", row.name);
        let bytes = row_bytes(&row);
        let o = oracle(
            row.declared_policy,
            &bytes,
            row_facts(&row),
            row.g10,
            row.anchor,
            row.prov,
            row.expected,
            row.presented,
        );
        assert_eq!(o, row.expect, "oracle {}", row.name);

        let mut target = initial_target();
        let before = fingerprint(&target);
        let p = run_prod(
            &mut target,
            row.declared_policy,
            &bytes,
            row.g10,
            row.anchor,
            row.prov,
            row.expected,
            row.presented,
        );
        assert_eq!(p, o, "production vs oracle {}", row.name);
        assert_eq!(
            fingerprint(&target),
            before,
            "{}: target bytes and counters must be unchanged",
            row.name
        );
    }
}

#[test]
fn positive_controls_replace_target_exactly_and_match_oracle() {
    let a = auth(0xC1);
    let exp = id(STREAM, a);
    let home = fx_bytes(Fx::Home3);
    let cases: [(OG10, OProv, Fx, OOut); 3] = [
        (
            OG10::Required,
            OProv::Bound,
            Fx::Home3,
            OOut::Accepted {
                anchored: true,
                bound: true,
            },
        ),
        (
            OG10::Required,
            OProv::Unbound,
            Fx::Home3,
            OOut::Accepted {
                anchored: true,
                bound: false,
            },
        ),
        // Documented limit: without opt-in provenance and anchor, a
        // foreign-stream snapshot is accepted by integrity alone.
        (
            OG10::Unanchored,
            OProv::Unbound,
            Fx::Foreign3,
            OOut::Accepted {
                anchored: false,
                bound: false,
            },
        ),
    ];
    for (g10, prov, fx, want) in cases {
        let bytes = if fx == Fx::Home3 {
            home.clone()
        } else {
            fx_bytes(fx)
        };
        let (s, g, _) = fx_facts(fx);
        let o = oracle(
            ORACLE_SUPPORTED_POLICY,
            &bytes,
            Some((s, g)),
            g10,
            verified(Fx::Home3),
            prov,
            exp,
            Some(exp),
        );
        assert_eq!(o, want, "oracle {g10:?}/{prov:?}/{fx:?}");
        let mut target = initial_target();
        let p = run_prod(
            &mut target,
            ORACLE_SUPPORTED_POLICY,
            &bytes,
            g10,
            verified(Fx::Home3),
            prov,
            exp,
            Some(exp),
        );
        assert_eq!(p, o, "production {g10:?}/{prov:?}/{fx:?}");
        assert_eq!(fingerprint(&target), fingerprint(&fx_ctx(fx)));
        assert_eq!(
            seal_context(&target).unwrap(),
            bytes,
            "no replay, exact snapshot"
        );
    }
}

#[test]
fn exhaustive_interruption_sweep_matches_oracle_without_mutation() {
    let a = auth(0xC1);
    let exp = id(STREAM, a);
    let bytes = fx_bytes(Fx::Home3);
    let mut target = initial_target();
    let before = fingerprint(&target);
    let mut cases = 0_usize;
    let mut check = |candidate: &[u8], what: String| {
        let o = oracle(
            ORACLE_SUPPORTED_POLICY,
            candidate,
            None,
            OG10::Required,
            verified(Fx::Home3),
            OProv::Bound,
            exp,
            Some(exp),
        );
        assert_eq!(o, OOut::Envelope, "oracle {what}");
        let p = run_prod(
            &mut target,
            ORACLE_SUPPORTED_POLICY,
            candidate,
            OG10::Required,
            verified(Fx::Home3),
            OProv::Bound,
            exp,
            Some(exp),
        );
        assert_eq!(p, o, "production {what}");
        assert_eq!(fingerprint(&target), before, "{what}: unchanged");
        cases += 1;
    };
    for cut in 0..bytes.len() {
        check(&bytes[..cut], format!("prefix {cut}"));
    }
    for byte in 0..bytes.len() {
        for bit in 0..8 {
            let mut v = bytes.clone();
            v[byte] ^= 1 << bit;
            check(&v, format!("flip {byte}:{bit}"));
        }
    }
    let mut extended = bytes.clone();
    extended.push(0);
    check(&extended, "trailing byte".to_string());
    assert_eq!(cases, bytes.len() * 9 + 1);
}

#[test]
fn policy_version_gate_rejects_every_other_version_without_migration() {
    let mut downgrades = 0_u32;
    let mut unknowns = 0_u32;
    for declared in 0..=u16::MAX {
        let prod = check_policy_version(declared);
        let want = if declared < ORACLE_SUPPORTED_POLICY {
            downgrades += 1;
            Err(PolicyVersionError::Downgrade {
                declared,
                supported: ORACLE_SUPPORTED_POLICY,
            })
        } else if declared > ORACLE_SUPPORTED_POLICY {
            unknowns += 1;
            Err(PolicyVersionError::Unknown {
                declared,
                supported: ORACLE_SUPPORTED_POLICY,
            })
        } else {
            Ok(())
        };
        assert_eq!(prod, want, "declared {declared}");
    }
    assert_eq!(downgrades, 1);
    assert_eq!(unknowns, u32::from(u16::MAX) - 1);
}

#[test]
fn snapshot_replacement_sequence_rejections_leave_state_and_anchor_intact() {
    // Live sequence: restore Home3 under the anchor, then attempt every
    // negative corpus row from that state, then advance legitimately.
    let a = auth(0xC1);
    let exp = id(STREAM, a);
    let mut target = initial_target();
    let ok = run_prod(
        &mut target,
        ORACLE_SUPPORTED_POLICY,
        &fx_bytes(Fx::Home3),
        OG10::Required,
        verified(Fx::Home3),
        OProv::Bound,
        exp,
        Some(exp),
    );
    assert_eq!(
        ok,
        OOut::Accepted {
            anchored: true,
            bound: true
        }
    );
    let settled = fingerprint(&target);
    for row in corpus() {
        let p = run_prod(
            &mut target,
            row.declared_policy,
            &row_bytes(&row),
            row.g10,
            row.anchor,
            row.prov,
            row.expected,
            row.presented,
        );
        assert_eq!(p, row.expect, "{}", row.name);
        assert_eq!(fingerprint(&target), settled, "{}: unchanged", row.name);
    }

    // Legitimate replacement: advance the anchor to Home4, then restore.
    let home3 = fx_bytes(Fx::Home3);
    let home4 = fx_bytes(Fx::Home4);
    let cur = AnchorRecord {
        stream_id: STREAM,
        generation: 3,
        digest: trailer(&home3),
    };
    let next = Candidate {
        stream_id: STREAM,
        generation: 4,
        digest: trailer(&home4),
    };
    let rec = advance(Some(&cur), &next).unwrap();
    let a4 = OAnchor::Verified {
        stream: rec.stream_id,
        generation: rec.generation,
        digest: rec.digest,
    };
    // A declared-version change at the moment of replacement is refused;
    // nothing migrates and the settled state survives.
    for declared in [0_u16, 2, u16::MAX] {
        let p = run_prod(
            &mut target,
            declared,
            &home4,
            OG10::Required,
            a4,
            OProv::Bound,
            exp,
            Some(exp),
        );
        assert!(p.is_rejection(), "declared {declared}");
        assert_eq!(fingerprint(&target), settled, "declared {declared}");
    }
    // The previous anchored snapshot is now a rollback.
    let p = run_prod(
        &mut target,
        ORACLE_SUPPORTED_POLICY,
        &home3,
        OG10::Required,
        a4,
        OProv::Bound,
        exp,
        Some(exp),
    );
    assert_eq!(
        p,
        OOut::Rollback {
            anchor: 4,
            candidate: 3
        }
    );
    assert_eq!(fingerprint(&target), settled);
    let p = run_prod(
        &mut target,
        ORACLE_SUPPORTED_POLICY,
        &home4,
        OG10::Required,
        a4,
        OProv::Bound,
        exp,
        Some(exp),
    );
    assert_eq!(
        p,
        OOut::Accepted {
            anchored: true,
            bound: true
        }
    );
    assert_eq!(fingerprint(&target), fingerprint(&fx_ctx(Fx::Home4)));
}

#[test]
fn matching_authority_tag_is_equality_binding_not_cryptographic_origin() {
    // Any caller who constructs the expected authority bytes is admitted:
    // the tag is compared for equality only. This test pins that limit.
    let a = auth(0xC1);
    let forged = OIdent {
        stream: STREAM,
        authority: [0xC1; 32],
        domain: *b"SWE1",
    };
    assert_eq!(forged, id(STREAM, a));
    let mut target = initial_target();
    let p = run_prod(
        &mut target,
        ORACLE_SUPPORTED_POLICY,
        &fx_bytes(Fx::Home3),
        OG10::Required,
        verified(Fx::Home3),
        OProv::Bound,
        id(STREAM, a),
        Some(forged),
    );
    assert_eq!(
        p,
        OOut::Accepted {
            anchored: true,
            bound: true
        }
    );
}
