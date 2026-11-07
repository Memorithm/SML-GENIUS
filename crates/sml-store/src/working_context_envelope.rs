//! CONTEXT-2 G7: bounded, versioned integrity envelope around an SWC2 wire
//! snapshot (see [`crate::working_context_wire`]).
//!
//! The envelope adds **unkeyed corruption detection** only: a SHA-256 digest
//! over the envelope header and the payload. It detects accidental damage
//! (truncation, bit flips, torn writes) before any wire decode or restore is
//! attempted. It is **not authentication**: anyone can recompute an unkeyed
//! digest, so a deliberately modified payload with a recomputed digest is
//! accepted at this layer (and then still has to pass the wire decoder and
//! [`WorkingContext::restore`]). No keyed MAC/signature exists here because
//! no verified key lifecycle exists; no authenticity claim is made.
//!
//! # Layout (envelope version 1, integers little-endian)
//!
//! | Offset | Size | Field |
//! | --- | --- | --- |
//! | 0 | 4 | magic `b"SWE1"` ([`ENVELOPE_MAGIC`]) |
//! | 4 | 2 | envelope version, must equal [`ENVELOPE_VERSION`] (1) |
//! | 6 | 1 | digest algorithm, must equal [`DIGEST_SHA256_UNKEYED`] (1) |
//! | 7 | 1 | reserved, must be 0 |
//! | 8 | 4 | payload length `L` (u32) |
//! | 12 | `L` | payload: canonical SWC2 v1 bytes |
//! | 12 + `L` | 32 | SHA-256 over bytes `0 .. 12 + L` |
//!
//! # Admission order (fail closed, nothing allocated from unchecked lengths)
//!
//! 1. total length against [`MAX_ENVELOPE_BYTES`] / caller cap, before reading;
//! 2. fixed header: magic, version, algorithm, reserved byte;
//! 3. `L` against [`MAX_SNAPSHOT_BYTES`], then exact total length;
//! 4. digest comparison;
//! 5. only then wire decode, then semantic restore.
//!
//! Live state is replaced only after all five steps succeed
//! ([`restore_into_sealed`]). Opening an envelope never replays admissions:
//! the restored context is exactly the sealed snapshot.
//!
//! `NOT_MEASURED`: no model-quality, context-span or latency claim.

use core::fmt;

use sml_core::elastic_ngram_paged_sha::sha256;

use crate::working_context::WorkingContext;
use crate::working_context_wire::{
    decode_and_restore, encode_context, SnapshotWireError, MAX_SNAPSHOT_BYTES,
};

/// Envelope magic.
pub const ENVELOPE_MAGIC: [u8; 4] = *b"SWE1";
/// Only envelope version accepted.
pub const ENVELOPE_VERSION: u16 = 1;
/// Unkeyed SHA-256 (corruption detection, not authentication).
pub const DIGEST_SHA256_UNKEYED: u8 = 1;
/// Fixed header size.
pub const ENVELOPE_HEADER_BYTES: usize = 12;
/// Digest trailer size.
pub const ENVELOPE_DIGEST_BYTES: usize = 32;
/// Hard cap on a sealed envelope.
pub const MAX_ENVELOPE_BYTES: usize =
    ENVELOPE_HEADER_BYTES + MAX_SNAPSHOT_BYTES + ENVELOPE_DIGEST_BYTES;

/// Fail-closed envelope errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnvelopeError {
    InputTooLarge {
        len: usize,
        max: usize,
    },
    Truncated {
        needed: usize,
        got: usize,
    },
    TrailingBytes {
        extra: usize,
    },
    BadMagic,
    UnknownEnvelopeVersion {
        got: u16,
    },
    UnknownDigestAlgorithm {
        got: u8,
    },
    NonzeroReserved {
        got: u8,
    },
    PayloadTooLarge {
        len: u64,
        max: usize,
    },
    DigestMismatch,
    /// Payload passed the envelope but failed wire decode or restore.
    Payload(SnapshotWireError),
}

impl fmt::Display for EnvelopeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for EnvelopeError {}

/// Seal arbitrary payload bytes (callers normally pass canonical SWC2 bytes).
pub fn seal(payload: &[u8]) -> Result<Vec<u8>, EnvelopeError> {
    if payload.len() > MAX_SNAPSHOT_BYTES {
        return Err(EnvelopeError::PayloadTooLarge {
            len: payload.len() as u64,
            max: MAX_SNAPSHOT_BYTES,
        });
    }
    let len = u32::try_from(payload.len()).map_err(|_| EnvelopeError::PayloadTooLarge {
        len: payload.len() as u64,
        max: MAX_SNAPSHOT_BYTES,
    })?;
    let mut out = Vec::with_capacity(ENVELOPE_HEADER_BYTES + payload.len() + ENVELOPE_DIGEST_BYTES);
    out.extend_from_slice(&ENVELOPE_MAGIC);
    out.extend_from_slice(&ENVELOPE_VERSION.to_le_bytes());
    out.push(DIGEST_SHA256_UNKEYED);
    out.push(0);
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(payload);
    let digest = sha256(&out);
    out.extend_from_slice(&digest);
    Ok(out)
}

/// Encode a live context to SWC2 and seal it.
pub fn seal_context(ctx: &WorkingContext) -> Result<Vec<u8>, EnvelopeError> {
    let payload = encode_context(ctx).map_err(EnvelopeError::Payload)?;
    seal(&payload)
}

/// Verify the envelope with the default cap and return the payload slice.
pub fn open(bytes: &[u8]) -> Result<&[u8], EnvelopeError> {
    open_with_max(bytes, MAX_ENVELOPE_BYTES)
}

/// [`open`] with a caller byte cap (clamped to [`MAX_ENVELOPE_BYTES`]).
pub fn open_with_max(bytes: &[u8], max_bytes: usize) -> Result<&[u8], EnvelopeError> {
    let max = max_bytes.min(MAX_ENVELOPE_BYTES);
    if bytes.len() > max {
        return Err(EnvelopeError::InputTooLarge {
            len: bytes.len(),
            max,
        });
    }
    if bytes.len() < ENVELOPE_HEADER_BYTES {
        return Err(EnvelopeError::Truncated {
            needed: ENVELOPE_HEADER_BYTES,
            got: bytes.len(),
        });
    }
    if bytes[0..4] != ENVELOPE_MAGIC {
        return Err(EnvelopeError::BadMagic);
    }
    let version = u16::from_le_bytes([bytes[4], bytes[5]]);
    if version != ENVELOPE_VERSION {
        return Err(EnvelopeError::UnknownEnvelopeVersion { got: version });
    }
    if bytes[6] != DIGEST_SHA256_UNKEYED {
        return Err(EnvelopeError::UnknownDigestAlgorithm { got: bytes[6] });
    }
    if bytes[7] != 0 {
        return Err(EnvelopeError::NonzeroReserved { got: bytes[7] });
    }
    let len = u64::from(u32::from_le_bytes([
        bytes[8], bytes[9], bytes[10], bytes[11],
    ]));
    if len > MAX_SNAPSHOT_BYTES as u64 {
        return Err(EnvelopeError::PayloadTooLarge {
            len,
            max: MAX_SNAPSHOT_BYTES,
        });
    }
    // len <= MAX_SNAPSHOT_BYTES, so this cannot overflow.
    let payload_end = ENVELOPE_HEADER_BYTES + len as usize;
    let needed = payload_end + ENVELOPE_DIGEST_BYTES;
    if bytes.len() < needed {
        return Err(EnvelopeError::Truncated {
            needed,
            got: bytes.len(),
        });
    }
    if bytes.len() > needed {
        return Err(EnvelopeError::TrailingBytes {
            extra: bytes.len() - needed,
        });
    }
    let expected = sha256(&bytes[..payload_end]);
    if !digest_eq(&expected, &bytes[payload_end..]) {
        return Err(EnvelopeError::DigestMismatch);
    }
    Ok(&bytes[ENVELOPE_HEADER_BYTES..payload_end])
}

/// Verify, wire-decode and restore. Nothing is restored unless all succeed.
pub fn open_and_restore(bytes: &[u8]) -> Result<WorkingContext, EnvelopeError> {
    let payload = open(bytes)?;
    decode_and_restore(payload).map_err(EnvelopeError::Payload)
}

/// Replace `target` only after full envelope + wire + restore success.
pub fn restore_into_sealed(target: &mut WorkingContext, bytes: &[u8]) -> Result<(), EnvelopeError> {
    let restored = open_and_restore(bytes)?;
    *target = restored;
    Ok(())
}

/// Length-independent-time comparison of two 32-byte digests (not a security
/// claim; the digest is unkeyed).
fn digest_eq(a: &[u8; 32], b: &[u8]) -> bool {
    if b.len() != 32 {
        return false;
    }
    a.iter().zip(b).fold(0_u8, |acc, (x, y)| acc | (x ^ y)) == 0
}
