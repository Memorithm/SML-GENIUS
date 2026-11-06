//! DEC-11: more training capacity for the DEC-10 heads, and a new reserved set.
//!
//! In DEC-9 and DEC-10, DEV selected the largest epoch count of the grid (8),
//! and accuracy on familiar wordings fell as the grammar grew (0.839, 0.754,
//! 0.681). DEC-11 keeps the DEC-10 grammar and features unchanged and widens
//! only the DEV grid: more epochs and one more weight level. The `reserved`
//! set below was written before any DEC-11 run and is used only on TEST.

use crate::dec4_instr::Intent;

/// Epoch grid of the DEC-11 primary arm (DEC-2 grid: 1, 2, 4, 8).
pub const EPOCH_GRID_11: [usize; 6] = [1, 2, 4, 8, 16, 32];
/// Level grid of the DEC-11 primary arm (DEC-2 grid: 1, 3, 7).
pub const LEVEL_GRID_11: [i64; 4] = [1, 3, 7, 15];

/// Reserved wordings, used only in TEST `reserved`.
pub fn reserved(intent: Intent) -> &'static [&'static str] {
    match intent {
        Intent::Continues => &[
            "Does the passage A run on into B?",
            "Is B what you would read once A is over?",
            "Could B be the next lines after A?",
            "Is B a natural follow-on to A?",
        ],
        Intent::NotContinues => &[
            "Is B a jump away from where A ends?",
            "Would B feel out of sequence after A?",
            "Is B not what follows A?",
            "Is there a gap between A and B in the text?",
        ],
        Intent::Opening => &[
            "Is this taken from the very first page of the paper?",
            "Does this passage appear before most of the article?",
        ],
        Intent::Middle => &[
            "Is this passage drawn from the center of the paper?",
            "Does this text come from neither the opening nor the closing of the article?",
        ],
        Intent::End => &[
            "Is this taken from the final page of the paper?",
            "Does this passage appear after most of the article?",
        ],
    }
}
