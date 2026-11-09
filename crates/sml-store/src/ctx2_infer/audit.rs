//! Exact token-ID budget audit (prereg section 4.1). Model-free: the token IDs
//! are data produced by the pinned runtime's `/tokenize`; this module never
//! tokenizes. Every invariant failure is `INVALID` (fail closed).

use std::collections::{BTreeMap, BTreeSet};

use sml_core::elastic_ngram_paged_sha::sha256;

use super::plan::{Arm, CallKey, Phase, Plan};

type QueryGroup = (Phase, u64, u32, u8);
type TemplatePieces<'a> = (&'a [u32], &'a [u32]);

/// Token pieces of one call: `P = T_pre ‖ H ‖ T_post`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenizedCall {
    pub key: CallKey,
    /// SHA-256 of the bytes that were tokenized into `h` (A1: the full history).
    pub source_sha256: [u8; 32],
    pub t_pre: Vec<u32>,
    pub h: Vec<u32>,
    pub t_post: Vec<u32>,
}

impl TokenizedCall {
    /// The exact token-ID array sent to the runtime (no re-tokenization).
    #[must_use]
    pub fn prompt_ids(&self) -> Vec<u32> {
        let mut p = Vec::with_capacity(self.prompt_len());
        p.extend_from_slice(&self.t_pre);
        p.extend_from_slice(&self.h);
        p.extend_from_slice(&self.t_post);
        p
    }

    #[must_use]
    pub fn prompt_len(&self) -> usize {
        self.t_pre.len() + self.h.len() + self.t_post.len()
    }

    /// `sha256(P as little-endian u32 IDs)`.
    #[must_use]
    pub fn prompt_sha256(&self) -> [u8; 32] {
        let bytes: Vec<u8> = self
            .prompt_ids()
            .iter()
            .flat_map(|id| id.to_le_bytes())
            .collect();
        sha256(&bytes)
    }
}

/// A1 history: the last `budget` IDs of the whole-string tokenization.
pub fn a1_history_ids(full: &[u32], budget: usize) -> Result<Vec<u32>, AuditError> {
    if budget > full.len() {
        return Err(AuditError::A1BudgetExceedsHistory {
            budget,
            available: full.len(),
        });
    }
    Ok(full[full.len() - budget..].to_vec())
}

/// Fail-closed audit errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuditError {
    MissingCall(CallKey),
    UnplannedCall(CallKey),
    DuplicateCall(CallKey),
    /// Tokens were produced from bytes other than the planned history.
    SourceMismatch(CallKey),
    /// A0 must carry an empty history.
    NonEmptyA0(CallKey),
    /// Invariant 1: `|H| = B(s, k)`.
    BudgetMismatch {
        key: CallKey,
        expected: usize,
        actual: usize,
    },
    /// A3 queries of one `(session, k)` disagree on `|H|`.
    InconsistentA3Budget(CallKey),
    /// Invariant 2: `T_pre`/`T_post` identical across arms of one query.
    TemplateMismatch(CallKey),
    /// Invariant 3: server-reported prompt tokens differ from `|P|`.
    ServerPromptCount {
        key: CallKey,
        expected: usize,
        reported: usize,
    },
    /// Invariant 4: `|P| + n_predict ≤ n_ctx`.
    ContextOverflow {
        key: CallKey,
        prompt: usize,
        n_predict: u32,
        n_ctx: u32,
    },
    /// Per-`(k, seed)` totals `Σ|P|` differ among A1, A2, A3, A3r.
    UnequalArmTotals {
        phase: Phase,
        seed: u64,
        k: u32,
    },
    A1BudgetExceedsHistory {
        budget: usize,
        available: usize,
    },
}

/// Per-`(phase, arm, k, seed)` totals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ArmTotals {
    pub calls: u64,
    pub prompt_tokens: u64,
    pub history_tokens: u64,
}

/// Audit result (all invariants held).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BudgetReport {
    pub calls: u64,
    pub totals: BTreeMap<(Phase, Arm, Option<u32>, u64), ArmTotals>,
    /// A1 prompts whose `H` equals A2's `H` (reported, no prediction).
    pub a1_equals_a2: u64,
    pub a1_total: u64,
    pub max_prompt_len: usize,
}

/// Audit every planned call (invariants 1, 2 and 4, completeness, source binding,
/// equal per-arm totals). Invariant 3 is [`check_server_prompt_count`].
pub fn audit_budget(
    plan: &Plan,
    calls: &[TokenizedCall],
    n_ctx: u32,
    n_predict: u32,
) -> Result<BudgetReport, AuditError> {
    let mut by_key: BTreeMap<CallKey, &TokenizedCall> = BTreeMap::new();
    let planned: BTreeMap<CallKey, [u8; 32]> = plan
        .records
        .iter()
        .map(|r| (r.key, r.history_sha256))
        .collect();
    for c in calls {
        if !planned.contains_key(&c.key) {
            return Err(AuditError::UnplannedCall(c.key));
        }
        if by_key.insert(c.key, c).is_some() {
            return Err(AuditError::DuplicateCall(c.key));
        }
    }
    for (key, sha) in &planned {
        let c = by_key.get(key).ok_or(AuditError::MissingCall(*key))?;
        if c.source_sha256 != *sha {
            return Err(AuditError::SourceMismatch(*key));
        }
    }

    // Invariant 4 (checked on the full plan before any call).
    let mut max_prompt_len = 0;
    for c in by_key.values() {
        let p = c.prompt_len();
        max_prompt_len = max_prompt_len.max(p);
        if p as u64 + u64::from(n_predict) > u64::from(n_ctx) {
            return Err(AuditError::ContextOverflow {
                key: c.key,
                prompt: p,
                n_predict,
                n_ctx,
            });
        }
    }

    // Invariant 2: identical template pieces per query group.
    let mut template: BTreeMap<QueryGroup, TemplatePieces<'_>> = BTreeMap::new();
    for c in by_key.values() {
        let entry = template
            .entry(c.key.query_group())
            .or_insert((&c.t_pre, &c.t_post));
        if entry.0 != c.t_pre.as_slice() || entry.1 != c.t_post.as_slice() {
            return Err(AuditError::TemplateMismatch(c.key));
        }
    }

    // Budget B(s, k) = |H_A3|, shared by every query of the session.
    let mut budget: BTreeMap<(Phase, u64, u32, u32), usize> = BTreeMap::new();
    for c in by_key.values() {
        if let (Arm::A3, Some(k)) = (c.key.arm, c.key.k) {
            let e = budget
                .entry((c.key.phase, c.key.seed, c.key.session, k))
                .or_insert(c.h.len());
            if *e != c.h.len() {
                return Err(AuditError::InconsistentA3Budget(c.key));
            }
        }
    }

    // Invariant 1 and A0 emptiness.
    let mut a1_equals_a2 = 0;
    let mut a1_total = 0;
    for c in by_key.values() {
        match (c.key.arm, c.key.k) {
            (Arm::A0, _) if !c.h.is_empty() => return Err(AuditError::NonEmptyA0(c.key)),
            (Arm::A1 | Arm::A2 | Arm::A3r, Some(k)) => {
                let expected = *budget
                    .get(&(c.key.phase, c.key.seed, c.key.session, k))
                    .ok_or(AuditError::MissingCall(CallKey {
                        arm: Arm::A3,
                        ..c.key
                    }))?;
                if c.h.len() != expected {
                    return Err(AuditError::BudgetMismatch {
                        key: c.key,
                        expected,
                        actual: c.h.len(),
                    });
                }
                if c.key.arm == Arm::A1 {
                    a1_total += 1;
                    let a2 = CallKey {
                        arm: Arm::A2,
                        ..c.key
                    };
                    if by_key.get(&a2).is_some_and(|x| x.h == c.h) {
                        a1_equals_a2 += 1;
                    }
                }
            }
            _ => {}
        }
    }

    // Per-arm totals and their equality across the budgeted arms.
    let mut totals: BTreeMap<(Phase, Arm, Option<u32>, u64), ArmTotals> = BTreeMap::new();
    for c in by_key.values() {
        let t = totals
            .entry((c.key.phase, c.key.arm, c.key.k, c.key.seed))
            .or_default();
        t.calls += 1;
        t.prompt_tokens += c.prompt_len() as u64;
        t.history_tokens += c.h.len() as u64;
    }
    let groups: BTreeSet<(Phase, u64, u32)> = totals
        .keys()
        .filter_map(|(phase, _, k, seed)| k.map(|k| (*phase, *seed, k)))
        .collect();
    for (phase, seed, k) in groups {
        let sums: BTreeSet<Option<u64>> = Arm::BUDGETED
            .iter()
            .map(|arm| {
                totals
                    .get(&(phase, *arm, Some(k), seed))
                    .map(|t| t.prompt_tokens)
            })
            .collect();
        if sums.len() != 1 {
            return Err(AuditError::UnequalArmTotals { phase, seed, k });
        }
    }

    Ok(BudgetReport {
        calls: by_key.len() as u64,
        totals,
        a1_equals_a2,
        a1_total,
        max_prompt_len,
    })
}

/// Invariant 3: the runtime-reported prompt token count equals `|P|`.
pub fn check_server_prompt_count(call: &TokenizedCall, reported: usize) -> Result<(), AuditError> {
    if reported == call.prompt_len() {
        Ok(())
    } else {
        Err(AuditError::ServerPromptCount {
            key: call.key,
            expected: call.prompt_len(),
            reported,
        })
    }
}
