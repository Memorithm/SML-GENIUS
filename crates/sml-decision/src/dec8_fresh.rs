//! DEC-8: a fresh reserved set of question wordings.
//!
//! The DEC-4 `held_out` wordings were read by four runs (DEC-4 to DEC-7) and
//! the DEC-7 negation rule was written while they were known. These wordings
//! were written after DEC-7, before any run touches them, and are frozen by
//! the DEC-8 preregistration. They are used only on TEST, as the `fresh`
//! variant; TRAIN and DEV still use `dec4_instr::seen`, so the heads are the
//! DEC-7 heads unchanged.

use crate::dec4_instr::Intent;

/// Fresh wordings, used only in TEST `fresh`.
pub fn fresh(intent: Intent) -> &'static [&'static str] {
    match intent {
        Intent::Continues => &[
            "Is B the continuation of A?",
            "Does A lead straight into B?",
            "Could B be the sentence that comes after A in the original text?",
            "Is B the passage that immediately follows A?",
        ],
        Intent::NotContinues => &[
            "Is B unrelated to what comes after A?",
            "Does B fail to follow A?",
            "Is it incorrect to say that B comes after A?",
            "Was B not written right after A?",
        ],
        Intent::Opening => &[
            "Is this the first part of the paper?",
            "Does this passage open the article?",
        ],
        Intent::Middle => &[
            "Is this passage from neither the start nor the end of the article?",
            "Does this text sit halfway through the paper?",
        ],
        Intent::End => &[
            "Is this the last part of the article?",
            "Does this passage conclude the paper?",
        ],
    }
}
