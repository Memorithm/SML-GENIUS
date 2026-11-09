//! A3r restart path and its fail-closed negative controls (prereg sections 4, 5 H2/H2p, 11).
//!
//! At each checkpoint turn the live working context is sealed (SWE1), the
//! process state is dropped, and a fresh empty context is replaced only after
//! envelope, wire, restore and **mock** anchor admission (`NOT_QUALIFIED`).
//! A refused checkpoint stops the session: no prompt is produced from an empty
//! or partial context.

use sml_core::elastic_ngram_paged_sha::sha256;

use super::generator::Session;
use super::plan::{admit_turn, assemble_retained, stream_id, InMemoryRawStore};
use super::{first_u64_le, Ctx2InferError};
use crate::working_context::{WorkingContext, WorkingPolicy};
use crate::working_context_anchor::{
    advance, candidate_of, restore_into_anchored, AnchorPolicy, AnchorRecord, AnchorState,
};
use crate::working_context_envelope::seal_context;

/// Checkpoint turns (prereg section 4, arm A3r).
pub const RESTART_TURNS: [u32; 3] = [12, 24, 36];
/// Domain prefix of the fixed per-seed tamper position.
pub const TAMPER_DOMAIN: &[u8] = b"CTX2-INFER-tamper-v1";

/// What the restarting process is presented with at a checkpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestartControl {
    /// The sealed envelope and a verified mock anchor for it.
    Clean,
    /// XOR `0x01` into the envelope byte at `position % len`.
    FlipByte { turn: u32, position: usize },
    /// Envelope truncated to `len` bytes.
    Truncate { turn: u32, len: usize },
    /// The previous checkpoint's envelope while the anchor has advanced.
    Rollback { turn: u32 },
    /// No anchor record available.
    AnchorAbsent { turn: u32 },
    /// Anchor record read but not verifiable.
    AnchorUnverifiable { turn: u32 },
    /// An envelope sealed from a different stream (another session).
    ForeignEnvelope { turn: u32, envelope: Vec<u8> },
}

impl RestartControl {
    const fn turn(&self) -> Option<u32> {
        match self {
            Self::Clean => None,
            Self::FlipByte { turn, .. }
            | Self::Truncate { turn, .. }
            | Self::Rollback { turn }
            | Self::AnchorAbsent { turn }
            | Self::AnchorUnverifiable { turn }
            | Self::ForeignEnvelope { turn, .. } => Some(*turn),
        }
    }
}

/// Fixed tamper position for a seed: first 8 bytes LE of
/// `SHA-256(TAMPER_DOMAIN || seed_le)` modulo the envelope length.
#[must_use]
pub fn tamper_position(seed: u64, envelope_len: usize) -> usize {
    let mut input = TAMPER_DOMAIN.to_vec();
    input.extend_from_slice(&seed.to_le_bytes());
    let len = envelope_len.max(1) as u64;
    (first_u64_le(&sha256(&input)) % len) as usize
}

/// One accepted checkpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checkpoint {
    pub turn: u32,
    pub generation: u64,
    pub envelope_sha256: [u8; 32],
    pub envelope_len: usize,
}

/// A3r result: history identical in construction to A3, plus checkpoints.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestartOutcome {
    pub history: (Vec<u64>, Vec<u8>),
    pub checkpoints: Vec<Checkpoint>,
}

/// Restore `presented` into `target` under [`AnchorPolicy::Required`].
/// On refusal `target` is left unchanged.
pub fn restore_checkpoint(
    target: &mut WorkingContext,
    presented: &[u8],
    anchor: &AnchorState,
    turn: u32,
) -> Result<(), Ctx2InferError> {
    restore_into_anchored(target, presented, AnchorPolicy::Required, anchor)
        .map(|_| ())
        .map_err(|e| Ctx2InferError::RestartRefused {
            turn,
            reason: format!("{e:?}"),
        })
}

/// Seal the live context and derive the advanced mock anchor record.
pub fn checkpoint(
    ctx: &WorkingContext,
    current: Option<&AnchorRecord>,
    turn: u32,
) -> Result<(Vec<u8>, AnchorRecord), Ctx2InferError> {
    let refused = |reason: String| Ctx2InferError::RestartRefused { turn, reason };
    let sealed = seal_context(ctx).map_err(|e| refused(format!("{e:?}")))?;
    let (_, cand) = candidate_of(&sealed).map_err(|e| refused(format!("{e:?}")))?;
    let next = advance(current, &cand).map_err(|e| refused(format!("{e:?}")))?;
    Ok((sealed, next))
}

/// A3r: A3 with seal / drop / anchored restore at [`RESTART_TURNS`].
pub fn context2_restart_history(
    session: &Session,
    k: u32,
    store: &InMemoryRawStore,
    control: &RestartControl,
) -> Result<RestartOutcome, Ctx2InferError> {
    let stream = stream_id(session)?;
    let policy = WorkingPolicy::CapacityBounded {
        capacity: k as usize,
    };
    let new_ctx = || {
        WorkingContext::new(stream, policy).map_err(|e| Ctx2InferError::Context(format!("{e:?}")))
    };
    let mut ctx = new_ctx()?;
    let mut anchor: Option<AnchorRecord> = None;
    let mut previous: Option<Vec<u8>> = None;
    let mut checkpoints = Vec::new();
    for turn in &session.turns {
        admit_turn(&mut ctx, stream, turn)?;
        let n = turn.number;
        if !RESTART_TURNS.contains(&n) {
            continue;
        }
        let (sealed, next) = checkpoint(&ctx, anchor.as_ref(), n)?;
        anchor = Some(next);
        let mut presented = sealed.clone();
        let mut state = AnchorState::Verified(next);
        if control.turn() == Some(n) {
            match control {
                RestartControl::Clean => {}
                RestartControl::FlipByte { position, .. } => {
                    let p = position % presented.len();
                    presented[p] ^= 0x01;
                }
                RestartControl::Truncate { len, .. } => presented.truncate(*len),
                RestartControl::Rollback { .. } => {
                    presented = previous.clone().ok_or(Ctx2InferError::RestartRefused {
                        turn: n,
                        reason: "no earlier checkpoint to roll back to".to_owned(),
                    })?;
                }
                RestartControl::AnchorAbsent { .. } => state = AnchorState::Absent,
                RestartControl::AnchorUnverifiable { .. } => state = AnchorState::Unverifiable,
                RestartControl::ForeignEnvelope { envelope, .. } => presented = envelope.clone(),
            }
        }
        // Process exit: the live state is dropped; a new process starts empty.
        drop(ctx);
        let mut fresh = new_ctx()?;
        restore_checkpoint(&mut fresh, &presented, &state, n)?;
        ctx = fresh;
        checkpoints.push(Checkpoint {
            turn: n,
            generation: next.generation,
            envelope_sha256: sha256(&sealed),
            envelope_len: sealed.len(),
        });
        previous = Some(sealed);
    }
    Ok(RestartOutcome {
        history: assemble_retained(&ctx, store)?,
        checkpoints,
    })
}
