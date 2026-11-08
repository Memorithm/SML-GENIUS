//! CONTEXT-2 G17: adversarial composed-mutation qualification of the
//! G16 corpus on the versioned anchor admission path
//! (`restore_into_anchored_with_versioned_provenance`).
//!
//! G16 perturbed **exactly one** of the five published decision gates at
//! a time. G17 starts from the same accepting source shape and perturbs
//! **exactly two** gates simultaneously, using the consecutive pairs
//! assigned on RemoteOps #1208 (comment 6724375732):
//!
//! 1. **version + envelope**
//! 2. **envelope + G12 provenance**
//! 3. **G12 provenance + G10 stream**
//! 4. **G10 stream + G10 rollback**
//!
//! The five published gates, in precedence order:
//!
//! 1. **version** — declared policy `< 1` → downgrade, `> 1` → unknown
//!    (no migration; before the bytes are examined);
//! 2. **SWE1 envelope** — layout/digest failure → `Envelope`;
//! 3. **G12 provenance** — unbound skips; absent; stream (presented or
//!    derived); authority (full 32 bytes); domain (presented or derived
//!    magic);
//! 4. **G10 stream** — after G12, the verified-anchor stream must equal
//!    the candidate stream;
//! 5. **G10 rollback** — after the stream check, a candidate generation
//!    older than the verified anchor is a rollback (this precedes stale
//!    and digest-equality / conflict).
//!
//! Expected typed errors are derived independently from that published
//! order: a composed mutant must report the error of the
//! higher-precedence (earlier) altered gate, never the later gate's
//! error, never a third error, and never an accept. The oracle does
//! **not** call production code to compute expectations.
//!
//! Each composed mutant must be detected by at least one independently
//! expected typed-error assertion and by the nonmutation assertion (the
//! target's canonical SWC2 encoding, SWE1 seal and all five
//! `WorkingContextStats` counters stay byte-for-byte unchanged; the
//! anchor is not advanced). The unmutated positive control must remain
//! accepted and must replace the target with the exact sealed snapshot.
//!
//! Frozen pairwise coverage matrix ([`G17_CORPUS_VERSION`]):
//!
//! | Mutant                                         | Pair                         | Expected typed error  | Detection vectors        |
//! |------------------------------------------------|------------------------------|-----------------------|--------------------------|
//! | `composed_version_unknown_envelope_truncate`   | version + envelope           | `VersionUnknown(2)`   | typed_error, nonmutation |
//! | `composed_version_unknown_envelope_flip`       | version + envelope           | `VersionUnknown(2)`   | typed_error, nonmutation |
//! | `composed_version_downgrade_envelope_truncate` | version + envelope           | `VersionDowngrade(0)` | typed_error, nonmutation |
//! | `composed_version_downgrade_envelope_flip`     | version + envelope           | `VersionDowngrade(0)` | typed_error, nonmutation |
//! | `composed_envelope_truncate_g12_authority`     | envelope + provenance        | `Envelope`            | typed_error, nonmutation |
//! | `composed_envelope_flip_g12_authority`         | envelope + provenance        | `Envelope`            | typed_error, nonmutation |
//! | `composed_envelope_truncate_g12_stream`        | envelope + provenance        | `Envelope`            | typed_error, nonmutation |
//! | `composed_envelope_flip_g12_domain`            | envelope + provenance        | `Envelope`            | typed_error, nonmutation |
//! | `composed_g12_authority_g10_stream`            | provenance + G10 stream      | `ProvAuthority`       | typed_error, nonmutation |
//! | `composed_g12_stream_g10_stream`               | provenance + G10 stream      | `ProvStream`          | typed_error, nonmutation |
//! | `composed_g12_domain_g10_stream`               | provenance + G10 stream      | `ProvDomain`          | typed_error, nonmutation |
//! | `composed_g10_stream_g10_rollback`             | G10 stream + G10 rollback    | `AnchorStream`        | typed_error, nonmutation |
//!
//! `typed_error` kills a mutant that is accepted or that reports a
//! different typed cause. `nonmutation` kills a mutant that writes the
//! target or advances the anchor on a rejection path (and also kills an
//! incorrect accept, because accept replaces the target).
//!
//! The older Home1 snapshot used for the stream+rollback mutant also
//! has a distinct digest; the independently expected error is still
//! `AnchorStream`, because the published G10 order checks stream before
//! generation and generation before digest equality.
//!
//! Oracle independence: the oracle re-implements the published SWE1
//! layout check, the G12 provenance order, the G10 anchor order and the
//! G14 version gate with its own types. It does not call production
//! `open`, `candidate_of`, `derived_binding`, `admit_provenance`,
//! `admit_with_provenance`, `admit`, `advance` or `check_policy_version`.
//! It shares only the SHA-256 primitive (`sml_core::elastic_ngram_paged_sha`)
//! and takes the stream/generation of each fixture from frozen corpus
//! facts instead of decoding SWC2. Expected outcomes are reasoned from
//! the published gate order (G14/G12/G10), not by reading the
//! production function body.
//!
//! Limits: authorities and anchors are in-process mocks. A matching
//! authority tag is an equality binding, **not** a cryptographic origin
//! and **not** authenticated provenance. SWE1 is unkeyed SHA-256
//! (corruption detection, not authentication). The declared policy
//! version is a caller assertion, not authenticated. No migration,
//! signature, attestation, hardware counter, transport or E2E claim.
//! `NOT_MEASURED`. This file does not modify the G14, G15 or G16
//! corpora and does not change production logic unless a concrete
//! defect is demonstrated.
//!
//! Assigned by ChatGPT on RemoteOps #1208 (comment 6724375732).
//! G16 = PR #889, exact head `61613209`, squash `a837fb6b`.

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

/// Version of this frozen composed-mutation corpus. Bump only when rows,
/// fixtures or expected outcomes change (and re-freeze the digests below).
const G17_CORPUS_VERSION: u16 = 1;
/// The oracle's own notion of the single supported policy version.
const ORACLE_SUPPORTED_POLICY: u16 = 1;

/// G17-owned stream id (distinct from the G15 `…0015` and G16 `…0016` fixtures).
const STREAM: u64 = 0x4743_0000_0000_0017;
const OTHER: u64 = 0x4743_0000_0000_0071;

const DOMAIN_SWE1: [u8; 4] = *b"SWE1";
const DOMAIN_OTHER: [u8; 4] = *b"SWX1";

// ---------------------------------------------------------------------------
// Frozen fixtures.
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Fx {
    /// Home stream, generation 3 (the anchored / positive-control snapshot).
    Home3,
    /// Home stream, generation 1 (older snapshot; rollback vs Home3).
    Home1,
}

const FIXTURES: [Fx; 2] = [Fx::Home3, Fx::Home1];

/// Frozen facts the oracle uses instead of decoding SWC2.
fn fx_facts(fx: Fx) -> (u64, u64, u8) {
    match fx {
        Fx::Home3 => (STREAM, 3, 0x17),
        Fx::Home1 => (STREAM, 1, 0x17),
    }
}

/// Frozen SHA-256 of each sealed fixture envelope (reproducibility pin).
fn fx_frozen_sha(fx: Fx) -> &'static str {
    match fx {
        Fx::Home3 => "c4b2bc9f8c9511da0577273364b89958bfe9ed796c2f8c9ac7b4762cd039f880",
        Fx::Home1 => "107cac73be4a2db6071aa33425479a2b037b31ea402d46e1360277bb1554bb1a",
    }
}

/// Frozen SHA-256 over the canonical rendering of the whole corpus.
const FROZEN_CORPUS_SHA256: &str =
    "15ceba9cdb3ed42f92acd73bdf2208dcd7050298bd3c489a7f5a64ee0b298bb8";

fn sample_ctx(stream: u64, n: u64, salt: u8) -> WorkingContext {
    let s = ContextStreamId::new(stream).expect("nonzero stream id");
    let mut ctx = WorkingContext::new(s, WorkingPolicy::CapacityBounded { capacity: 4 }).unwrap();
    for i in 1..=n {
        let mut hash = [salt; 32];
        hash[0] = u8::try_from(i).expect("fixture generation fits u8");
        let seg = WorkingSegment::new(
            ContextSegmentId::new(s, 17_000 + i).unwrap(),
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
    sample_ctx(STREAM, 2, 0x67)
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
    Required,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OAnchor {
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
    AnchorStream {
        anchor: u64,
        candidate: u64,
    },
    Rollback {
        anchor: u64,
        candidate: u64,
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

/// The five published decision gates. G16 perturbs one at a time; G17
/// perturbs two at a time along the consecutive pairs below.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Gate {
    Version,
    Swe1Envelope,
    G12Provenance,
    G10Stream,
    G10Rollback,
}

const ALL_GATES: [Gate; 5] = [
    Gate::Version,
    Gate::Swe1Envelope,
    Gate::G12Provenance,
    Gate::G10Stream,
    Gate::G10Rollback,
];

/// Consecutive pairwise compositions assigned for G17.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pair {
    VersionEnvelope,
    EnvelopeProvenance,
    ProvenanceG10Stream,
    StreamRollback,
}

impl Pair {
    /// The two altered gates, listed in published precedence order
    /// (earlier = higher priority).
    fn gates(self) -> [Gate; 2] {
        match self {
            Pair::VersionEnvelope => [Gate::Version, Gate::Swe1Envelope],
            Pair::EnvelopeProvenance => [Gate::Swe1Envelope, Gate::G12Provenance],
            Pair::ProvenanceG10Stream => [Gate::G12Provenance, Gate::G10Stream],
            Pair::StreamRollback => [Gate::G10Stream, Gate::G10Rollback],
        }
    }
}

const ALL_PAIRS: [Pair; 4] = [
    Pair::VersionEnvelope,
    Pair::EnvelopeProvenance,
    Pair::ProvenanceG10Stream,
    Pair::StreamRollback,
];

/// Assertion that independently kills a mutant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DetectionVector {
    /// Production (and the independent oracle) must report the expected
    /// typed error, not an accept and not a different cause.
    TypedError,
    /// Rejection must leave SWC2, SWE1, stats and the anchor untouched.
    Nonmutation,
}

#[derive(Clone, Debug)]
struct Row {
    corpus_version: u16,
    name: &'static str,
    family: &'static str,
    /// `None` on the positive-control source; `Some` on each composed mutant.
    pair: Option<Pair>,
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
        OProv::Bound => {
            let p = match row.presented {
                Some(p) => p,
                None => panic!("{}: G17 corpus always presents an identity", row.name),
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
        OG10::Required => match row.anchor {
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
                    panic!(
                        "{}: G17 corpus has no stale follow-up (anchor {generation}, candidate {c_gen})",
                        row.name
                    );
                }
                if digest != c_digest {
                    panic!(
                        "{}: G17 corpus has no conflict follow-up at matching generation",
                        row.name
                    );
                }
                true
            }
        },
    };
    OOut::Accepted { anchored, bound }
}

/// Which published gate produced `out`? Used to check that a composed
/// mutant's independently expected error belongs to the higher-precedence
/// altered gate, not the later one.
fn error_gate(out: OOut) -> Gate {
    match out {
        OOut::VersionDowngrade { .. } | OOut::VersionUnknown { .. } => Gate::Version,
        OOut::Envelope => Gate::Swe1Envelope,
        OOut::ProvStream { .. } | OOut::ProvAuthority { .. } | OOut::ProvDomain { .. } => {
            Gate::G12Provenance
        }
        OOut::AnchorStream { .. } => Gate::G10Stream,
        OOut::Rollback { .. } => Gate::G10Rollback,
        OOut::Accepted { .. } => panic!("accepted is not a typed error"),
    }
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
            other => panic!("unexpected provenance error on G17 path: {other:?}"),
        },
        Err(VersionedRestoreError::Restore(ProvenanceRestoreError::Anchor(e))) => match e {
            AnchorError::Envelope(_) => OOut::Envelope,
            AnchorError::StreamMismatch { anchor, candidate } => {
                OOut::AnchorStream { anchor, candidate }
            }
            AnchorError::Rollback { anchor, candidate } => OOut::Rollback { anchor, candidate },
            other => panic!("unexpected anchor error on G17 path: {other:?}"),
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
        OProv::Bound => ProvenancePolicy::Bound(prod_ident(row.expected)),
    };
    let g10p = match row.g10 {
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
// The frozen composed-mutation corpus.
// ---------------------------------------------------------------------------

fn auth(byte: u8) -> [u8; 32] {
    [byte; 32]
}

fn auth_near(byte: u8) -> [u8; 32] {
    let mut a = [byte; 32];
    a[31] = byte.wrapping_add(1);
    a
}

fn id(stream: u64, authority: [u8; 32], domain: [u8; 4]) -> OIdent {
    OIdent {
        stream,
        authority,
        domain,
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

/// Digest of `digest_of` presented as an anchor on `stream` at `generation`.
fn verified_digest_on(stream: u64, generation: u64, digest_of: Fx) -> OAnchor {
    OAnchor::Verified {
        stream,
        generation,
        digest: trailer(&fx_bytes(digest_of)),
    }
}

/// Frozen composed mutant → (pair, detection vectors). Documented in
/// the module header and re-checked by the pairwise matrix test.
struct MatrixEntry {
    mutant: &'static str,
    pair: Pair,
    vectors: &'static [DetectionVector],
}

const BOTH: &[DetectionVector] = &[DetectionVector::TypedError, DetectionVector::Nonmutation];

const MATRIX: [MatrixEntry; 12] = [
    MatrixEntry {
        mutant: "composed_version_unknown_envelope_truncate",
        pair: Pair::VersionEnvelope,
        vectors: BOTH,
    },
    MatrixEntry {
        mutant: "composed_version_unknown_envelope_flip",
        pair: Pair::VersionEnvelope,
        vectors: BOTH,
    },
    MatrixEntry {
        mutant: "composed_version_downgrade_envelope_truncate",
        pair: Pair::VersionEnvelope,
        vectors: BOTH,
    },
    MatrixEntry {
        mutant: "composed_version_downgrade_envelope_flip",
        pair: Pair::VersionEnvelope,
        vectors: BOTH,
    },
    MatrixEntry {
        mutant: "composed_envelope_truncate_g12_authority",
        pair: Pair::EnvelopeProvenance,
        vectors: BOTH,
    },
    MatrixEntry {
        mutant: "composed_envelope_flip_g12_authority",
        pair: Pair::EnvelopeProvenance,
        vectors: BOTH,
    },
    MatrixEntry {
        mutant: "composed_envelope_truncate_g12_stream",
        pair: Pair::EnvelopeProvenance,
        vectors: BOTH,
    },
    MatrixEntry {
        mutant: "composed_envelope_flip_g12_domain",
        pair: Pair::EnvelopeProvenance,
        vectors: BOTH,
    },
    MatrixEntry {
        mutant: "composed_g12_authority_g10_stream",
        pair: Pair::ProvenanceG10Stream,
        vectors: BOTH,
    },
    MatrixEntry {
        mutant: "composed_g12_stream_g10_stream",
        pair: Pair::ProvenanceG10Stream,
        vectors: BOTH,
    },
    MatrixEntry {
        mutant: "composed_g12_domain_g10_stream",
        pair: Pair::ProvenanceG10Stream,
        vectors: BOTH,
    },
    MatrixEntry {
        mutant: "composed_g10_stream_g10_rollback",
        pair: Pair::StreamRollback,
        vectors: BOTH,
    },
];

fn corpus() -> Vec<Row> {
    let a = auth(0xC7);
    let exp = id(STREAM, a, DOMAIN_SWE1);
    let a3 = verified(Fx::Home3);
    let accept = OOut::Accepted {
        anchored: true,
        bound: true,
    };
    let base = Row {
        corpus_version: G17_CORPUS_VERSION,
        name: "",
        family: "",
        pair: None,
        declared_policy: ORACLE_SUPPORTED_POLICY,
        fixture: Fx::Home3,
        mutate: None,
        g10: OG10::Required,
        anchor: a3,
        prov: OProv::Bound,
        expected: exp,
        presented: Some(exp),
        expect: accept,
    };
    let r = |name: &'static str, family: &'static str, pair: Option<Pair>, f: &dyn Fn(&mut Row)| {
        let mut row = base.clone();
        row.name = name;
        row.family = family;
        row.pair = pair;
        f(&mut row);
        row
    };
    vec![
        r("positive_control_accept", "positive_control", None, &|_| {}),
        r(
            "composed_version_unknown_envelope_truncate",
            "composed_version_envelope",
            Some(Pair::VersionEnvelope),
            &|x| {
                x.declared_policy = 2;
                x.mutate = Some(Mutation::TruncateHalf);
                x.expect = OOut::VersionUnknown { declared: 2 };
            },
        ),
        r(
            "composed_version_unknown_envelope_flip",
            "composed_version_envelope",
            Some(Pair::VersionEnvelope),
            &|x| {
                x.declared_policy = 2;
                x.mutate = Some(Mutation::FlipLastBit);
                x.expect = OOut::VersionUnknown { declared: 2 };
            },
        ),
        r(
            "composed_version_downgrade_envelope_truncate",
            "composed_version_envelope",
            Some(Pair::VersionEnvelope),
            &|x| {
                x.declared_policy = 0;
                x.mutate = Some(Mutation::TruncateHalf);
                x.expect = OOut::VersionDowngrade { declared: 0 };
            },
        ),
        r(
            "composed_version_downgrade_envelope_flip",
            "composed_version_envelope",
            Some(Pair::VersionEnvelope),
            &|x| {
                x.declared_policy = 0;
                x.mutate = Some(Mutation::FlipLastBit);
                x.expect = OOut::VersionDowngrade { declared: 0 };
            },
        ),
        r(
            "composed_envelope_truncate_g12_authority",
            "composed_envelope_provenance",
            Some(Pair::EnvelopeProvenance),
            &|x| {
                x.mutate = Some(Mutation::TruncateHalf);
                x.presented = Some(id(STREAM, auth_near(0xC7), DOMAIN_SWE1));
                x.expect = OOut::Envelope;
            },
        ),
        r(
            "composed_envelope_flip_g12_authority",
            "composed_envelope_provenance",
            Some(Pair::EnvelopeProvenance),
            &|x| {
                x.mutate = Some(Mutation::FlipLastBit);
                x.presented = Some(id(STREAM, auth_near(0xC7), DOMAIN_SWE1));
                x.expect = OOut::Envelope;
            },
        ),
        r(
            "composed_envelope_truncate_g12_stream",
            "composed_envelope_provenance",
            Some(Pair::EnvelopeProvenance),
            &|x| {
                x.mutate = Some(Mutation::TruncateHalf);
                x.presented = Some(id(OTHER, a, DOMAIN_SWE1));
                x.expect = OOut::Envelope;
            },
        ),
        r(
            "composed_envelope_flip_g12_domain",
            "composed_envelope_provenance",
            Some(Pair::EnvelopeProvenance),
            &|x| {
                x.mutate = Some(Mutation::FlipLastBit);
                x.presented = Some(id(STREAM, a, DOMAIN_OTHER));
                x.expect = OOut::Envelope;
            },
        ),
        r(
            "composed_g12_authority_g10_stream",
            "composed_provenance_g10_stream",
            Some(Pair::ProvenanceG10Stream),
            &|x| {
                x.presented = Some(id(STREAM, auth_near(0xC7), DOMAIN_SWE1));
                x.anchor = verified_digest_on(OTHER, 3, Fx::Home3);
                x.expect = OOut::ProvAuthority {
                    expected: a,
                    presented: auth_near(0xC7),
                };
            },
        ),
        r(
            "composed_g12_stream_g10_stream",
            "composed_provenance_g10_stream",
            Some(Pair::ProvenanceG10Stream),
            &|x| {
                x.presented = Some(id(OTHER, a, DOMAIN_SWE1));
                x.anchor = verified_digest_on(OTHER, 3, Fx::Home3);
                x.expect = OOut::ProvStream {
                    expected: STREAM,
                    presented: OTHER,
                    derived: Some(STREAM),
                };
            },
        ),
        r(
            "composed_g12_domain_g10_stream",
            "composed_provenance_g10_stream",
            Some(Pair::ProvenanceG10Stream),
            &|x| {
                x.presented = Some(id(STREAM, a, DOMAIN_OTHER));
                x.anchor = verified_digest_on(OTHER, 3, Fx::Home3);
                x.expect = OOut::ProvDomain {
                    expected: DOMAIN_SWE1,
                    presented: DOMAIN_OTHER,
                    derived: Some(DOMAIN_SWE1),
                };
            },
        ),
        r(
            "composed_g10_stream_g10_rollback",
            "composed_stream_rollback",
            Some(Pair::StreamRollback),
            &|x| {
                // Older snapshot on the home stream plus a foreign-stream
                // verified anchor: G12 still matches (presented / derived
                // stream stay on STREAM) so both G10 stream and G10
                // rollback are live. Stream precedes generation, which
                // precedes digest equality (Home1 also has a distinct
                // digest).
                x.fixture = Fx::Home1;
                x.anchor = verified_digest_on(OTHER, 3, Fx::Home3);
                x.expect = OOut::AnchorStream {
                    anchor: OTHER,
                    candidate: STREAM,
                };
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

fn source(rows: &[Row]) -> &Row {
    by_name(rows, "positive_control_accept")
}

/// Which of the five published gates differ between the source and `row`.
fn gate_deltas(src: &Row, row: &Row) -> Vec<Gate> {
    let mut d = Vec::new();
    if row.declared_policy != src.declared_policy {
        d.push(Gate::Version);
    }
    if row.mutate != src.mutate {
        d.push(Gate::Swe1Envelope);
    }
    let (src_stream, src_gen, _) = fx_facts(src.fixture);
    let (row_stream, row_gen, _) = fx_facts(row.fixture);
    if row.prov != src.prov
        || row.expected != src.expected
        || row.presented != src.presented
        || row_stream != src_stream
    {
        d.push(Gate::G12Provenance);
    }
    let (src_a_stream, src_a_gen, _) = match src.anchor {
        OAnchor::Verified {
            stream,
            generation,
            digest: _,
        } => (stream, generation, ()),
    };
    let (row_a_stream, row_a_gen, _) = match row.anchor {
        OAnchor::Verified {
            stream,
            generation,
            digest: _,
        } => (stream, generation, ()),
    };
    if row_a_stream != src_a_stream || row_stream != src_stream {
        d.push(Gate::G10Stream);
    }
    if (row_gen, row_a_gen) != (src_gen, src_a_gen) {
        d.push(Gate::G10Rollback);
    }
    d
}

/// Canonical, deterministic rendering of the corpus (one line per row).
fn render(rows: &[Row]) -> String {
    let mut out = String::new();
    for row in rows {
        out.push_str(&format!(
            "{}|{}|{}|{:?}|{}|{:?}|{:?}|{}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}\n",
            row.corpus_version,
            row.family,
            row.name,
            row.pair,
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
    assert_eq!(
        rows.len(),
        13,
        "frozen corpus size: 1 source + 12 composed mutants"
    );
    let mut names: Vec<&str> = rows.iter().map(|r| r.name).collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), rows.len(), "row names are unique");
    let positives: Vec<_> = rows.iter().filter(|r| r.pair.is_none()).collect();
    assert_eq!(positives.len(), 1, "exactly one positive-control source");
    assert_eq!(positives[0].name, "positive_control_accept");
    assert!(!positives[0].expect.is_rejection());
    let mutants: Vec<_> = rows.iter().filter(|r| r.pair.is_some()).collect();
    assert_eq!(mutants.len(), 12, "frozen composed-mutant count");
    for pair in ALL_PAIRS {
        assert!(
            mutants.iter().any(|r| r.pair == Some(pair)),
            "every assigned pair must have at least one mutant: {pair:?}"
        );
    }
    let rendered = render(&rows);
    assert_eq!(rendered, render(&corpus()), "rendering is deterministic");
    assert_eq!(
        hex(&sha256(rendered.as_bytes())),
        FROZEN_CORPUS_SHA256,
        "frozen corpus digest (bump G17_CORPUS_VERSION and re-freeze on change)"
    );
}

#[test]
fn each_composed_mutant_perturbs_exactly_two_published_gates() {
    let rows = corpus();
    let src = source(&rows);
    assert_eq!(
        src.expect,
        OOut::Accepted {
            anchored: true,
            bound: true,
        }
    );
    assert!(gate_deltas(src, src).is_empty(), "source vs itself");
    for row in &rows {
        if row.name == src.name {
            continue;
        }
        let deltas = gate_deltas(src, row);
        let pair = row.pair.expect("composed mutant must name its pair");
        assert_eq!(
            deltas.as_slice(),
            pair.gates(),
            "{} must perturb exactly the declared pair {:?}, got {deltas:?}",
            row.name,
            pair
        );
        assert!(
            row.expect.is_rejection(),
            "{}: composed mutant is a rejection",
            row.name
        );
        assert_eq!(
            error_gate(row.expect),
            pair.gates()[0],
            "{}: expected typed error must belong to the higher-precedence gate of {:?}",
            row.name,
            pair
        );
    }
    // Every published gate appears in at least one composed pair.
    let mut seen = Vec::new();
    for row in &rows {
        if let Some(pair) = row.pair {
            seen.extend_from_slice(&pair.gates());
        }
    }
    for g in ALL_GATES {
        assert!(seen.contains(&g), "composed pairs must cover gate {g:?}");
    }
}

#[test]
fn every_corpus_row_matches_oracle_and_production() {
    let rows = corpus();
    for row in &rows {
        assert_eq!(row.corpus_version, G17_CORPUS_VERSION, "{}", row.name);
        let bytes = row_bytes(row);
        let o = oracle(row, &bytes, row_facts(row));
        assert_eq!(o, row.expect, "oracle {}", row.name);

        let mut target = initial_target();
        let before = fingerprint(&target);
        let p = run_prod(&mut target, row, &bytes);
        assert_eq!(p, o, "production vs oracle {}", row.name);
        if row.expect.is_rejection() {
            assert_eq!(
                fingerprint(&target),
                before,
                "{}: target SWC2/SWE1/stats must be unchanged",
                row.name
            );
        } else {
            assert_eq!(fingerprint(&target), fingerprint(&fx_ctx(Fx::Home3)));
            assert_eq!(
                seal_context(&target).unwrap(),
                bytes,
                "no replay, exact snapshot"
            );
        }
    }
}

#[test]
fn pairwise_gate_coverage_matrix_kills_every_composed_mutant() {
    let rows = corpus();
    assert_eq!(MATRIX.len(), 12, "frozen pairwise matrix size");
    let mut seen_pairs = Vec::new();
    for entry in &MATRIX {
        assert!(
            !entry.vectors.is_empty(),
            "{}: at least one detection vector",
            entry.mutant
        );
        let row = by_name(&rows, entry.mutant);
        assert_eq!(row.pair, Some(entry.pair), "{}", entry.mutant);
        assert_eq!(
            gate_deltas(source(&rows), row).as_slice(),
            entry.pair.gates(),
            "{} pairwise gates",
            entry.mutant
        );
        assert!(row.expect.is_rejection(), "{}", entry.mutant);
        assert_eq!(
            error_gate(row.expect),
            entry.pair.gates()[0],
            "{}: matrix expected error must be the higher-precedence gate",
            entry.mutant
        );
        seen_pairs.push(entry.pair);

        let bytes = row_bytes(row);
        let o = oracle(row, &bytes, row_facts(row));
        let mut target = initial_target();
        let before = fingerprint(&target);
        let p = run_prod(&mut target, row, &bytes);

        for vector in entry.vectors {
            match vector {
                DetectionVector::TypedError => {
                    assert_eq!(o, row.expect, "oracle typed_error {}", entry.mutant);
                    assert_eq!(p, row.expect, "production typed_error {}", entry.mutant);
                }
                DetectionVector::Nonmutation => {
                    assert_eq!(fingerprint(&target), before, "nonmutation {}", entry.mutant);
                }
            }
        }
    }
    for pair in ALL_PAIRS {
        assert!(
            seen_pairs.contains(&pair),
            "matrix must cover assigned pair {pair:?}"
        );
    }

    // Metamorphic: after exercising every composed mutant, the unmutated
    // source is still accepted and still replaces the target exactly.
    let src = source(&rows);
    let bytes = row_bytes(src);
    let o = oracle(src, &bytes, row_facts(src));
    assert_eq!(o, src.expect, "positive control oracle after mutants");
    let mut target = initial_target();
    let p = run_prod(&mut target, src, &bytes);
    assert_eq!(p, o, "positive control production after mutants");
    assert_eq!(fingerprint(&target), fingerprint(&fx_ctx(Fx::Home3)));
    assert_eq!(seal_context(&target).unwrap(), bytes);
}

#[test]
fn authority_tag_is_in_process_equality_not_authenticated_provenance() {
    // Documentary / structural check: the G17 authority tag is a 32-byte
    // in-process equality binding. A single-byte difference is enough to
    // fail the G12 authority gate. This is not a signature, MAC, or
    // authenticated origin, and it does not speak for transport or
    // hardware provenance.
    let matching = auth(0xC7);
    let near = auth_near(0xC7);
    assert_ne!(matching, near);
    assert_eq!(&matching[..31], &near[..31]);
    assert_eq!(matching[31].wrapping_add(1), near[31]);
    assert_eq!(matching.len(), 32);
    let rows = corpus();
    let src = source(&rows);
    assert_eq!(src.expected.authority, matching);
    assert_eq!(src.presented.map(|p| p.authority), Some(matching));
    // The composed authority follow-ups present `near` against expected
    // `matching`. That is equality-tag disagreement, not an
    // authentication failure.
    let auth_row = by_name(&rows, "composed_g12_authority_g10_stream");
    assert_eq!(auth_row.expected.authority, matching);
    assert_eq!(
        auth_row.presented.map(|p| p.authority),
        Some(near),
        "authority mutant presents an in-process near-tag, not a signed identity"
    );
    assert_eq!(
        auth_row.expect,
        OOut::ProvAuthority {
            expected: matching,
            presented: near,
        }
    );
}
