//! CONTEXT-2 G10: **optional** external monotonic-anchor admission contract
//! for SWE1 restores (pure policy, no I/O, no keys).
//!
//! SWE1 ([`crate::working_context_envelope`]) is an unkeyed integrity check:
//! it cannot tell an intact *older* snapshot from the latest one (G9 shows a
//! legitimate rollback restoring successfully). Rollback detection needs
//! state the snapshot store cannot rewind: an external **trusted monotonic
//! anchor** supplied by the caller.
//!
//! This module only defines the admission decision given such an anchor.
//! It does **not** provide, provision, attest, authenticate or persist an
//! anchor, and no hardware monotonic counter exists here. Callers that pass
//! an anchor are responsible for its trust; tests use mock anchors only. All
//! of attestation, provisioning, authentication and hardware monotonic
//! storage are **NOT QUALIFIED**.
//!
//! # Contract
//!
//! - [`AnchorPolicy::Unanchored`] is the existing behaviour: restore by
//!   integrity only, reported as [`Admission::Unanchored`] so a caller can
//!   never mistake it for anti-rollback.
//! - [`AnchorPolicy::Required`] fails closed unless the anchor is
//!   [`AnchorState::Verified`] **and** the candidate matches it exactly:
//!   same stream, same generation, same SWE1 digest.
//!   - absent anchor → [`AnchorError::AnchorAbsent`];
//!   - caller could not verify the anchor → [`AnchorError::AnchorUnverifiable`];
//!   - different stream → [`AnchorError::StreamMismatch`];
//!   - candidate generation below the anchor → [`AnchorError::Rollback`];
//!   - candidate generation above the anchor (anchor not yet advanced, e.g.
//!     crash after the snapshot became durable but before the anchor update)
//!     → [`AnchorError::AnchorStale`]; recovery is an explicit, out-of-band
//!     re-anchor decision, never automatic;
//!   - equal generation, different digest → [`AnchorError::ConflictingHistory`].
//! - The generation of a snapshot is its cumulative `stats.admitted`, which
//!   [`WorkingContext::admit`] increases by exactly one per admission.
//! - [`advance`] computes the next anchor record and refuses non-increasing
//!   generations, stream changes and `u64` overflow.
//! - [`restore_into_anchored`] replaces the target only after envelope,
//!   wire, restore **and** anchor admission all succeed; nothing is replayed.
//!
//! `NOT_MEASURED`.
//!
//! Optional opt-in identity binding (stream, authority, digest domain)
//! lives in [`crate::working_context_anchor_provenance`] and does not
//! change the default [`admit`] / [`advance`] / [`restore_into_anchored`]
//! behaviour.

use core::fmt;

use crate::working_context::WorkingContext;
use crate::working_context_envelope::{open, EnvelopeError, ENVELOPE_DIGEST_BYTES};
use crate::working_context_wire::decode_and_restore;

/// Record held by an external trusted monotonic store (caller-provided).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnchorRecord {
    pub stream_id: u64,
    pub generation: u64,
    /// SWE1 trailer digest of the anchored snapshot.
    pub digest: [u8; 32],
}

/// What the caller could establish about the external anchor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnchorState {
    Absent,
    /// The caller read something but could not verify it.
    Unverifiable,
    /// The caller vouches for this record (mock in tests; trust not qualified).
    Verified(AnchorRecord),
}

/// Restore policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnchorPolicy {
    /// Integrity only; **no** anti-rollback.
    Unanchored,
    /// Fail closed unless a verified anchor matches exactly.
    Required,
}

/// Identity of a candidate snapshot, derived from verified bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Candidate {
    pub stream_id: u64,
    pub generation: u64,
    pub digest: [u8; 32],
}

/// Successful admission outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Admission {
    /// Accepted by integrity only. Not a rollback guarantee.
    Unanchored,
    /// Matched the caller-verified anchor exactly.
    Anchored,
}

/// Fail-closed anchor errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnchorError {
    AnchorAbsent,
    AnchorUnverifiable,
    StreamMismatch { anchor: u64, candidate: u64 },
    Rollback { anchor: u64, candidate: u64 },
    AnchorStale { anchor: u64, candidate: u64 },
    ConflictingHistory { generation: u64 },
    NonIncreasing { current: u64, next: u64 },
    GenerationOverflow,
    Envelope(EnvelopeError),
}

impl fmt::Display for AnchorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for AnchorError {}

/// Pure admission decision.
pub fn admit(
    policy: AnchorPolicy,
    anchor: &AnchorState,
    candidate: &Candidate,
) -> Result<Admission, AnchorError> {
    match policy {
        AnchorPolicy::Unanchored => Ok(Admission::Unanchored),
        AnchorPolicy::Required => {
            let rec = match anchor {
                AnchorState::Absent => return Err(AnchorError::AnchorAbsent),
                AnchorState::Unverifiable => return Err(AnchorError::AnchorUnverifiable),
                AnchorState::Verified(r) => r,
            };
            if rec.stream_id != candidate.stream_id {
                return Err(AnchorError::StreamMismatch {
                    anchor: rec.stream_id,
                    candidate: candidate.stream_id,
                });
            }
            match candidate.generation.cmp(&rec.generation) {
                core::cmp::Ordering::Less => Err(AnchorError::Rollback {
                    anchor: rec.generation,
                    candidate: candidate.generation,
                }),
                core::cmp::Ordering::Greater => Err(AnchorError::AnchorStale {
                    anchor: rec.generation,
                    candidate: candidate.generation,
                }),
                core::cmp::Ordering::Equal if rec.digest != candidate.digest => {
                    Err(AnchorError::ConflictingHistory {
                        generation: rec.generation,
                    })
                }
                core::cmp::Ordering::Equal => Ok(Admission::Anchored),
            }
        }
    }
}

/// Next anchor record after a durable checkpoint of `next`.
pub fn advance(
    current: Option<&AnchorRecord>,
    next: &Candidate,
) -> Result<AnchorRecord, AnchorError> {
    if let Some(cur) = current {
        if cur.stream_id != next.stream_id {
            return Err(AnchorError::StreamMismatch {
                anchor: cur.stream_id,
                candidate: next.stream_id,
            });
        }
        let floor = cur
            .generation
            .checked_add(1)
            .ok_or(AnchorError::GenerationOverflow)?;
        if next.generation < floor {
            return Err(AnchorError::NonIncreasing {
                current: cur.generation,
                next: next.generation,
            });
        }
    }
    Ok(AnchorRecord {
        stream_id: next.stream_id,
        generation: next.generation,
        digest: next.digest,
    })
}

/// Verify SWE1 + SWC2 + restore and derive the candidate identity.
pub fn candidate_of(bytes: &[u8]) -> Result<(WorkingContext, Candidate), AnchorError> {
    let payload = open(bytes).map_err(AnchorError::Envelope)?;
    let ctx = decode_and_restore(payload)
        .map_err(|e| AnchorError::Envelope(EnvelopeError::Payload(e)))?;
    let mut digest = [0_u8; 32];
    digest.copy_from_slice(&bytes[bytes.len() - ENVELOPE_DIGEST_BYTES..]);
    let snap = ctx.snapshot();
    let cand = Candidate {
        stream_id: snap.stream_id.get(),
        generation: snap.stats.admitted,
        digest,
    };
    Ok((ctx, cand))
}

/// Replace `target` only after envelope, wire, restore and anchor admission.
pub fn restore_into_anchored(
    target: &mut WorkingContext,
    bytes: &[u8],
    policy: AnchorPolicy,
    anchor: &AnchorState,
) -> Result<Admission, AnchorError> {
    let (ctx, cand) = candidate_of(bytes)?;
    let adm = admit(policy, anchor, &cand)?;
    *target = ctx;
    Ok(adm)
}
