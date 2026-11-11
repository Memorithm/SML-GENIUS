//! DEC-16: leave-one-set-out evaluation of the question routers.
//!
//! The four spent reserved sets of DEC-12 to DEC-15 (14 + 50 + 50 + 50 = 164
//! wordings) become folds. For fold k, a router is trained on the base
//! wordings plus the three other sets, and asked on TEST with set k. No new
//! wording is read. Base = DEC-12 training wordings + DEC-13 scope grammar,
//! minus every reserved set.

use crate::dec12_router;
use crate::dec13_scope;
use crate::dec14_anchor;
use crate::dec15_polarity;
use crate::dec4_instr::Intent;

/// A wording set of one intent.
pub type SetFn = fn(Intent) -> &'static [&'static str];

/// The folds, in order.
pub const FOLDS: [(&str, SetFn); 4] = [
    ("S12", dec12_router::reserved),
    ("S13", dec13_scope::reserved),
    ("S14", dec14_anchor::reserved),
    ("S15", dec15_polarity::reserved),
];

fn in_any_fold(intent: Intent, w: &str) -> bool {
    FOLDS.iter().any(|(_, f)| f(intent).contains(&w))
}

/// Base training wordings of `intent` (no reserved set).
pub fn base_training(intent: Intent) -> Vec<String> {
    let mut out = dec12_router::training_wordings(intent);
    out.extend(dec13_scope::scope_grammar(intent));
    let mut kept: Vec<String> = Vec::new();
    for w in out {
        if !in_any_fold(intent, &w) && !kept.contains(&w) {
            kept.push(w);
        }
    }
    kept
}

/// Training wordings of fold `k`: base plus every other fold's set.
pub fn fold_training(intent: Intent, k: usize) -> Vec<String> {
    let mut out = base_training(intent);
    for (j, (_, f)) in FOLDS.iter().enumerate() {
        if j != k {
            for w in f(intent) {
                if !out.iter().any(|x| x == w) {
                    out.push(w.to_string());
                }
            }
        }
    }
    out
}
