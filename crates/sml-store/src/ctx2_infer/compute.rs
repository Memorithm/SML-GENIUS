//! Bounded compute envelope (prereg section 8.1), derived from the frozen
//! parameters. No throughput or duration is estimated; these are ceilings.

use super::generator::{QUERIES, SESSIONS_PER_SEED};
use super::plan::K_VALUES;
use super::TEST_SEED_COUNT;

/// Fixed decoding length (prereg section 2).
pub const N_PREDICT: u32 = 8;
/// Fixed runtime context (prereg section 2).
pub const N_CTX: u32 = 4096;
/// Calls per cell-equivalent (one arm, one `k`, one seed, or one precheck pass).
pub const CELL_CALLS: u64 = 384;
/// Wall-clock cap per cell-equivalent, seconds.
pub const CELL_CAP_SECS: u64 = 3600;
/// Transport retries allowed per call (identical token-ID array).
pub const MAX_RETRIES_PER_CALL: u8 = 1;

/// Derived call counts and ceilings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ComputeEnvelope {
    pub naive_grid_calls: u64,
    pub a0_calls: u64,
    pub r_calls: u64,
    pub budgeted_arm_calls: u64,
    pub test_calls: u64,
    pub precheck_calls: u64,
    pub admitted_host_calls: u64,
    pub smoke_calls: u64,
    pub cell_equivalents: u64,
    pub ceiling_secs: u64,
    pub per_seed_ceiling_secs: u64,
    pub precheck_ceiling_secs: u64,
}

impl ComputeEnvelope {
    /// Values written in the prereg (section 8.1). Must equal [`Self::derive`].
    pub const FROZEN: Self = Self {
        naive_grid_calls: 34_560,
        a0_calls: 1_920,
        r_calls: 1_920,
        budgeted_arm_calls: 23_040,
        test_calls: 26_880,
        precheck_calls: 768,
        admitted_host_calls: 27_648,
        smoke_calls: 84,
        cell_equivalents: 72,
        ceiling_secs: 72 * 3600,
        per_seed_ceiling_secs: 14 * 3600,
        precheck_ceiling_secs: 2 * 3600,
    };

    /// Derive every count from the frozen parameters.
    #[must_use]
    pub const fn derive() -> Self {
        let arms = 6_u64;
        let ks = K_VALUES.len() as u64;
        let seeds = TEST_SEED_COUNT as u64;
        let per_seed = SESSIONS_PER_SEED as u64 * QUERIES as u64;
        let a0 = seeds * per_seed;
        let r = seeds * per_seed;
        let budgeted = 4 * ks * seeds * per_seed;
        let test = a0 + r + budgeted;
        let precheck = 2 * per_seed;
        // Smoke: 2 dev sessions, every arm at k = 4, plus one repeat of R.
        let smoke_queries = 2 * QUERIES as u64;
        let smoke = arms * smoke_queries + smoke_queries;
        let cells_per_seed = 2 + 4 * ks;
        let cells = cells_per_seed * seeds + 2;
        Self {
            naive_grid_calls: arms * ks * seeds * per_seed,
            a0_calls: a0,
            r_calls: r,
            budgeted_arm_calls: budgeted,
            test_calls: test,
            precheck_calls: precheck,
            admitted_host_calls: test + precheck,
            smoke_calls: smoke,
            cell_equivalents: cells,
            ceiling_secs: cells * CELL_CAP_SECS,
            per_seed_ceiling_secs: cells_per_seed * CELL_CAP_SECS,
            precheck_ceiling_secs: 2 * CELL_CAP_SECS,
        }
    }

    /// Fail-closed wall-clock check: `Err` once `elapsed_secs` exceeds the
    /// ceiling of `cell_equivalents_started` cell-equivalents.
    pub fn check_wall_clock(
        &self,
        cell_equivalents_started: u64,
        elapsed_secs: u64,
    ) -> Result<(), WallClockExceeded> {
        let cap = cell_equivalents_started
            .min(self.cell_equivalents)
            .saturating_mul(CELL_CAP_SECS);
        if elapsed_secs > cap || elapsed_secs > self.ceiling_secs {
            Err(WallClockExceeded {
                cap_secs: cap.min(self.ceiling_secs),
                elapsed_secs,
            })
        } else {
            Ok(())
        }
    }
}

/// A wall-clock ceiling was hit: the run is `INVALID` (prereg section 10).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WallClockExceeded {
    pub cap_secs: u64,
    pub elapsed_secs: u64,
}
