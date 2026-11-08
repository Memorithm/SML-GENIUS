//! CONTEXT-2 G12: **optional**, opt-in anchor provenance / identity policy
//! for the G10 [`crate::working_context_anchor`] admission path.
//!
//! G10 binds a caller-verified monotonic record `(stream_id, generation,
//! digest)`. This module adds a separately tested identity binding over
//! `(stream_id, authority, digest_domain)` so a restore can refuse:
//!
//! - a snapshot or identity from a different stream (cross-stream);
//! - an authority identifier that does not match the expected one;
//! - a digest claimed or derived in a different digest domain
//!   (domain confusion).
//!
//! The policy is **opt-in**. [`ProvenancePolicy::Unbound`] (the default)
//! performs no identity check and leaves G10 [`admit`](crate::working_context_anchor::admit),
//! [`advance`](crate::working_context_anchor::advance) and
//! [`restore_into_anchored`](crate::working_context_anchor::restore_into_anchored)
//! behaviour unchanged. Nothing here invents a signature scheme or
//! authenticates caller-supplied identity: [`AuthorityId`] is an opaque
//! equality tag, and [`DigestDomain`] is a four-byte domain separator.
//! SWE1 remains unkeyed SHA-256 (corruption detection, not
//! authentication).
//!
//! # Decision order (fail closed)
//!
//! 1. [`ProvenancePolicy::Unbound`] → [`ProvenanceAdmission::Unbound`]
//!    (presented identity and derived binding are ignored).
//! 2. [`ProvenancePolicy::Bound`] with no presented identity →
//!    [`ProvenanceError::ProvenanceAbsent`].
//! 3. stream mismatch between the expected binding, the presented
//!    identity, or (when supplied) the derived snapshot stream →
//!    [`ProvenanceError::StreamBindingMismatch`].
//! 4. authority identifier mismatch →
//!    [`ProvenanceError::AuthorityMismatch`].
//! 5. digest-domain mismatch between the expected binding, the
//!    presented identity, or (when supplied) the derived envelope
//!    magic → [`ProvenanceError::DigestDomainMismatch`].
//! 6. otherwise [`ProvenanceAdmission::Bound`].
//!
//! [`restore_into_anchored_with_provenance`] derives the stream from the
//! candidate snapshot and the digest domain from the envelope magic
//! (SWE1 after a successful [`candidate_of`](crate::working_context_anchor::candidate_of)),
//! then runs this policy **before** G10 generation/digest admission.
//! The target is replaced only after both checks succeed.
//!
//! # What this is not
//!
//! - Not authentication, attestation, or a signature. A caller who
//!   constructs the expected [`AuthorityId`] is accepted; this module
//!   never verifies a key, MAC or hardware quote.
//! - Not a hardware, TPM or monotonic-counter implementation.
//! - Not an end-to-end protocol or transport claim.
//! - Tests use mock identities only.
//! - The derived digest-domain tag is domain separation, not proof
//!   that the digest is authentic. SWE1 is unkeyed SHA-256.
//!
//! Vector table version: [`ANCHOR_PROVENANCE_POLICY_VERSION`].
//! `NOT_MEASURED`. Attestation, authentication, hardware storage and
//! E2E remain **NOT QUALIFIED**.

use core::fmt;

use crate::working_context::WorkingContext;
use crate::working_context_anchor::{
    admit, candidate_of, Admission, AnchorError, AnchorPolicy, AnchorState, Candidate,
};

/// Policy and frozen-vector version. Bump only when the identity layout
/// or the decision order above changes.
pub const ANCHOR_PROVENANCE_POLICY_VERSION: u16 = 1;

/// Digest-domain tag equal to the SWE1 envelope magic `b"SWE1"`.
///
/// This is a domain separator only. SWE1 is unkeyed SHA-256: corruption
/// detection, not authentication.
pub const DIGEST_DOMAIN_SWE1: DigestDomain = DigestDomain(*b"SWE1");

/// Four-byte digest-domain separator.
///
/// Equality only. The tag does not name a signature scheme and does
/// not authenticate the digest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DigestDomain(pub [u8; 4]);

/// Opaque 32-byte authority identifier.
///
/// Compared for equality only. This is not a public key, not a
/// signature, and not an attestation quote. A caller who constructs
/// the expected bytes is trusted as-is (mock identities in tests).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AuthorityId(pub [u8; 32]);

impl AuthorityId {
    /// Repeat `byte` across all 32 octets. Convenience for mocks.
    pub const fn repeating(byte: u8) -> Self {
        Self([byte; 32])
    }
}

/// Identity binding: stream, authority, digest domain.
///
/// When this value is *presented* by a caller it is a caller assertion,
/// not an authenticated statement. When it is used as the *expected*
/// side of [`ProvenancePolicy::Bound`] it is the policy the caller
/// opted into.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnchorIdentity {
    pub stream_id: u64,
    pub authority: AuthorityId,
    pub digest_domain: DigestDomain,
}

/// Components that can be derived from a candidate snapshot without
/// trusting the caller: stream id and envelope digest-domain tag.
///
/// Authority cannot be derived from the snapshot. Passing a derived
/// binding does not authenticate the caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DerivedBinding {
    pub stream_id: u64,
    pub digest_domain: DigestDomain,
}

/// Opt-in provenance policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProvenancePolicy {
    /// Default: no identity binding. Existing G10 behaviour is unchanged.
    Unbound,
    /// Fail closed unless presented (and, when supplied, derived)
    /// identity matches this binding exactly.
    Bound(AnchorIdentity),
}

/// Successful provenance check (or explicit skip).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProvenanceAdmission {
    /// Policy was [`ProvenancePolicy::Unbound`]; no identity check ran.
    Unbound,
    /// Policy was [`ProvenancePolicy::Bound`] and the binding matched.
    Bound,
}

/// Fail-closed provenance errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProvenanceError {
    ProvenanceAbsent,
    StreamBindingMismatch {
        expected: u64,
        presented: u64,
        derived: Option<u64>,
    },
    AuthorityMismatch {
        expected: AuthorityId,
        presented: AuthorityId,
    },
    DigestDomainMismatch {
        expected: DigestDomain,
        presented: DigestDomain,
        derived: Option<DigestDomain>,
    },
}

impl fmt::Display for ProvenanceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for ProvenanceError {}

/// Combined G10 + G12 failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProvenanceRestoreError {
    Anchor(AnchorError),
    Provenance(ProvenanceError),
}

impl fmt::Display for ProvenanceRestoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for ProvenanceRestoreError {}

/// Extract stream id from `candidate` and the digest-domain tag from
/// the leading four bytes of `envelope_bytes` (SWE1 magic after a
/// successful `candidate_of`). Domain separation only, not
/// authentication.
pub fn derived_binding(candidate: &Candidate, envelope_bytes: &[u8]) -> DerivedBinding {
    let mut tag = [0_u8; 4];
    let n = core::cmp::min(4, envelope_bytes.len());
    tag[..n].copy_from_slice(&envelope_bytes[..n]);
    DerivedBinding {
        stream_id: candidate.stream_id,
        digest_domain: DigestDomain(tag),
    }
}

/// Pure provenance decision. Does not call G10 `admit` or `advance`.
pub fn admit_provenance(
    policy: ProvenancePolicy,
    presented: Option<&AnchorIdentity>,
    derived: Option<&DerivedBinding>,
) -> Result<ProvenanceAdmission, ProvenanceError> {
    match policy {
        ProvenancePolicy::Unbound => Ok(ProvenanceAdmission::Unbound),
        ProvenancePolicy::Bound(expected) => {
            let presented = match presented {
                Some(p) => p,
                None => return Err(ProvenanceError::ProvenanceAbsent),
            };
            let derived_stream = derived.map(|d| d.stream_id);
            let stream_ok = presented.stream_id == expected.stream_id
                && derived_stream
                    .map(|stream| stream == expected.stream_id)
                    .unwrap_or(true);
            if !stream_ok {
                return Err(ProvenanceError::StreamBindingMismatch {
                    expected: expected.stream_id,
                    presented: presented.stream_id,
                    derived: derived_stream,
                });
            }
            if presented.authority != expected.authority {
                return Err(ProvenanceError::AuthorityMismatch {
                    expected: expected.authority,
                    presented: presented.authority,
                });
            }
            let derived_domain = derived.map(|d| d.digest_domain);
            let domain_ok = presented.digest_domain == expected.digest_domain
                && derived_domain
                    .map(|domain| domain == expected.digest_domain)
                    .unwrap_or(true);
            if !domain_ok {
                return Err(ProvenanceError::DigestDomainMismatch {
                    expected: expected.digest_domain,
                    presented: presented.digest_domain,
                    derived: derived_domain,
                });
            }
            Ok(ProvenanceAdmission::Bound)
        }
    }
}

/// G12 provenance check then G10 generation/digest admission.
///
/// Either failure is returned before the caller mutates any store.
/// This function does not call [`crate::working_context_anchor::advance`].
pub fn admit_with_provenance(
    policy: AnchorPolicy,
    anchor: &AnchorState,
    candidate: &Candidate,
    provenance: ProvenancePolicy,
    presented: Option<&AnchorIdentity>,
    derived: Option<&DerivedBinding>,
) -> Result<(Admission, ProvenanceAdmission), ProvenanceRestoreError> {
    let prov = admit_provenance(provenance, presented, derived)
        .map_err(ProvenanceRestoreError::Provenance)?;
    let adm = admit(policy, anchor, candidate).map_err(ProvenanceRestoreError::Anchor)?;
    Ok((adm, prov))
}

/// Verify SWE1 + SWC2 + restore, then provenance, then G10 admission.
///
/// Replaces `target` only after every step succeeds. Rejection leaves
/// `target` unchanged (no replay, no partial mutation).
pub fn restore_into_anchored_with_provenance(
    target: &mut WorkingContext,
    bytes: &[u8],
    policy: AnchorPolicy,
    anchor: &AnchorState,
    provenance: ProvenancePolicy,
    presented: Option<&AnchorIdentity>,
) -> Result<(Admission, ProvenanceAdmission), ProvenanceRestoreError> {
    let (ctx, cand) = candidate_of(bytes).map_err(ProvenanceRestoreError::Anchor)?;
    let derived = derived_binding(&cand, bytes);
    let (adm, prov) =
        admit_with_provenance(policy, anchor, &cand, provenance, presented, Some(&derived))?;
    *target = ctx;
    Ok((adm, prov))
}
