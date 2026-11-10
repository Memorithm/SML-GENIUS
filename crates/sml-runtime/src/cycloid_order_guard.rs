//! CYCLOID-6: do not promote an indirect page-read route on pooled timing alone.
//!
//! Freeze the existing CYCLOID-3 path only after CYCLOID-4's two balanced
//! source-access orders have been checked independently. An indirect RAM path
//! must dominate the direct-file path even after lookup+dispatch overhead:
//! resident p95 + overhead < file median **in each read order**.
//! These are host-call observations and caller-declared overheads, not model
//! throughput, DRAM bus counts, statistical confidence or attested benchmarks.

use core::fmt;

use sml_core::{cycloid_route::CycloidBudget, mor::MorDecision};

use crate::{
    cycloid_access::{
        collect_balanced_mor_page_profiles, CycloidBalancedProfiles, CycloidPageProfileError,
        CycloidProbeOrder, MAX_CYCLOID_BALANCED_PAIRS, MIN_CYCLOID_BALANCED_PAIRS,
    },
    cycloid_dispatch::{CycloidAccessMode, CycloidDispatchError, FrozenCycloidAccess},
    ram::{ElasticRamRuntime, WeightArtifactEntry},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CycloidOrderQuantiles {
    pub file_median_ns: u128,
    pub file_p95_ns: u128,
    pub resident_median_ns: u128,
    pub resident_p95_ns: u128,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CycloidOrderGuardEvidence {
    pub pairs: u8,
    pub file_then_ram: CycloidOrderQuantiles,
    pub ram_then_file: CycloidOrderQuantiles,
    pub declared_relay_lookup_ns: u64,
    pub declared_dispatch_ns: u64,
    /// A false value means the direct path was chosen. It is not proof that
    /// direct physical I/O is inherently faster under all cache conditions.
    pub indirect_robust_in_both_orders: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuardedCycloidAccess {
    pub frozen: FrozenCycloidAccess,
    pub evidence: CycloidOrderGuardEvidence,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CycloidOrderGuardError {
    InvalidOrderPanel,
    InvalidTime,
    /// Pooled medians can select a longer path which is not robust when
    /// source-read order is explicitly stratified: fail closed.
    IndirectNotOrderRobust,
    ArithmeticOverflow,
    Profile(CycloidPageProfileError),
    Dispatch(CycloidDispatchError),
}

impl fmt::Display for CycloidOrderGuardError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for CycloidOrderGuardError {}

impl From<CycloidPageProfileError> for CycloidOrderGuardError {
    fn from(value: CycloidPageProfileError) -> Self {
        Self::Profile(value)
    }
}

impl From<CycloidDispatchError> for CycloidOrderGuardError {
    fn from(value: CycloidDispatchError) -> Self {
        Self::Dispatch(value)
    }
}

fn quantiles(values: &mut [u128]) -> Result<(u128, u128), CycloidOrderGuardError> {
    if values.len() < usize::from(MIN_CYCLOID_BALANCED_PAIRS)
        || values.iter().any(|value| *value == 0)
    {
        return Err(CycloidOrderGuardError::InvalidTime);
    }
    values.sort_unstable();
    let p95_index = (values.len() * 95).div_ceil(100) - 1;
    Ok((values[values.len() / 2], values[p95_index]))
}

fn per_order(
    panel: &CycloidBalancedProfiles,
) -> Result<(CycloidOrderQuantiles, CycloidOrderQuantiles), CycloidOrderGuardError> {
    if !(MIN_CYCLOID_BALANCED_PAIRS..=MAX_CYCLOID_BALANCED_PAIRS).contains(&panel.pairs)
        || panel.samples.len() != usize::from(panel.pairs) * 2
    {
        return Err(CycloidOrderGuardError::InvalidOrderPanel);
    }
    let mut file_when_file_first = Vec::with_capacity(usize::from(panel.pairs));
    let mut ram_when_file_first = Vec::with_capacity(usize::from(panel.pairs));
    let mut file_when_ram_first = Vec::with_capacity(usize::from(panel.pairs));
    let mut ram_when_ram_first = Vec::with_capacity(usize::from(panel.pairs));
    for (index, pair) in panel.samples.chunks_exact(2).enumerate() {
        let expected = if index % 2 == 0 {
            CycloidProbeOrder::FileThenRam
        } else {
            CycloidProbeOrder::RamThenFile
        };
        if pair[0].order != expected || pair[1].order != expected.reverse() {
            return Err(CycloidOrderGuardError::InvalidOrderPanel);
        }
        for observation in pair {
            let source = observation.profile;
            if !source.byte_exact_match
                || source.file_read_verify_ns == 0
                || source.resident_copy_verify_ns == 0
            {
                return Err(CycloidOrderGuardError::InvalidTime);
            }
            match observation.order {
                CycloidProbeOrder::FileThenRam => {
                    file_when_file_first.push(source.file_read_verify_ns);
                    ram_when_file_first.push(source.resident_copy_verify_ns);
                }
                CycloidProbeOrder::RamThenFile => {
                    file_when_ram_first.push(source.file_read_verify_ns);
                    ram_when_ram_first.push(source.resident_copy_verify_ns);
                }
            }
        }
    }
    let (file_a_med, file_a_p95) = quantiles(&mut file_when_file_first)?;
    let (ram_a_med, ram_a_p95) = quantiles(&mut ram_when_file_first)?;
    let (file_b_med, file_b_p95) = quantiles(&mut file_when_ram_first)?;
    let (ram_b_med, ram_b_p95) = quantiles(&mut ram_when_ram_first)?;
    Ok((
        CycloidOrderQuantiles {
            file_median_ns: file_a_med,
            file_p95_ns: file_a_p95,
            resident_median_ns: ram_a_med,
            resident_p95_ns: ram_a_p95,
        },
        CycloidOrderQuantiles {
            file_median_ns: file_b_med,
            file_p95_ns: file_b_p95,
            resident_median_ns: ram_b_med,
            resident_p95_ns: ram_b_p95,
        },
    ))
}

/// Validate an already obtained balanced panel and freeze an access plan.
///
/// All raw timings remain untrusted caller data; this is a deterministic
/// decision/validation adapter. The underlying freeze re-verifies exact
/// disk/RAM bytes before binding page, version and RAM generation.
pub fn freeze_order_guarded(
    runtime: &ElasticRamRuntime,
    decision: &MorDecision,
    artifact: &WeightArtifactEntry,
    panel: &CycloidBalancedProfiles,
    budget: CycloidBudget,
    relay_lookup_ns: u64,
    online_dispatch_ns: u64,
) -> Result<GuardedCycloidAccess, CycloidOrderGuardError> {
    let (file_then_ram, ram_then_file) = per_order(panel)?;
    let frozen = FrozenCycloidAccess::freeze(
        runtime,
        decision,
        artifact,
        &panel.profile_values(),
        budget,
        relay_lookup_ns,
        online_dispatch_ns,
    )?;

    let indirect = frozen.mode() == CycloidAccessMode::VerifiedResident;
    if indirect {
        let overhead = u128::from(relay_lookup_ns)
            .checked_add(u128::from(online_dispatch_ns))
            .ok_or(CycloidOrderGuardError::ArithmeticOverflow)?;
        // Neither a tie nor a worse p95 qualifies indirect access. We must
        // reject a pooled-median win that reverses under one read order.
        for stratum in [file_then_ram, ram_then_file] {
            let conservative_indirect = stratum
                .resident_p95_ns
                .checked_add(overhead)
                .ok_or(CycloidOrderGuardError::ArithmeticOverflow)?;
            if conservative_indirect >= stratum.file_median_ns {
                return Err(CycloidOrderGuardError::IndirectNotOrderRobust);
            }
        }
    }

    Ok(GuardedCycloidAccess {
        frozen,
        evidence: CycloidOrderGuardEvidence {
            pairs: panel.pairs,
            file_then_ram,
            ram_then_file,
            declared_relay_lookup_ns: relay_lookup_ns,
            declared_dispatch_ns: online_dispatch_ns,
            indirect_robust_in_both_orders: indirect,
        },
    })
}

/// Actually collect new paired reads and return the panel with its optional
/// qualified frozen decision. No caller-injected timing samples enter this
/// method. Lookup and dispatch overheads are STILL caller-declared, not
/// automatically measured or attested.
pub fn collect_and_freeze_order_guarded(
    runtime: &ElasticRamRuntime,
    decision: &MorDecision,
    artifact: &WeightArtifactEntry,
    max_payload_bytes: u64,
    pairs: u8,
    budget: CycloidBudget,
    relay_lookup_ns: u64,
    online_dispatch_ns: u64,
) -> Result<(GuardedCycloidAccess, CycloidBalancedProfiles), CycloidOrderGuardError> {
    let panel =
        collect_balanced_mor_page_profiles(runtime, decision, artifact, max_payload_bytes, pairs)?;
    let chosen = freeze_order_guarded(
        runtime,
        decision,
        artifact,
        &panel,
        budget,
        relay_lookup_ns,
        online_dispatch_ns,
    )?;
    Ok((chosen, panel))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        sync::atomic::{AtomicU64, Ordering},
    };

    use sml_core::{
        elastic_weights::{
            ElasticWeightPage, ElasticWeightPlanner, ElasticWeightPolicy, ResidencyTier,
            WeightPrecision,
        },
        PageId,
    };

    use crate::{
        artifact::build_weight_artifact,
        cycloid_access::{CycloidOrderedProfile, CycloidPageProfile},
        ram::{RamMaterializationBudget, WeightArtifactCatalog},
    };

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn runtime() -> ElasticRamRuntime {
        let page = ElasticWeightPage::new(
            PageId(0),
            8,
            WeightPrecision::Boolean,
            ResidencyTier::Disk,
            0,
        )
        .unwrap();
        let planner = ElasticWeightPlanner::new(
            vec![page],
            ElasticWeightPolicy {
                ram_limit_bytes: 16,
                vram_limit_bytes: 0,
                max_active_parameters: 8,
                ternary_threshold_ppm: 500_000,
                residual4_threshold_ppm: 900_000,
                vram_threshold_ppm: 1_000_000,
            },
        )
        .unwrap();
        let mut runtime = ElasticRamRuntime::new(planner).unwrap();
        let artifact =
            build_weight_artifact(PageId(0), 0, WeightPrecision::Boolean, 8, &[0xa5]).unwrap();
        let catalog = WeightArtifactCatalog::from_retained_bytes(vec![artifact]).unwrap();
        let plan = runtime.planner().plan_cycle(&[PageId(0)], 1).unwrap();
        let stage = runtime
            .stage(&plan, &catalog, RamMaterializationBudget::default())
            .unwrap();
        runtime.commit(stage).unwrap();
        runtime
    }

    fn authorized_file() -> WeightArtifactEntry {
        let suffix = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "sml-cycloid-6-{}-{suffix}.smlwgt",
            std::process::id()
        ));
        fs::write(
            &path,
            build_weight_artifact(PageId(0), 0, WeightPrecision::Boolean, 8, &[0xa5]).unwrap(),
        )
        .unwrap();
        WeightArtifactEntry {
            page_id: PageId(0),
            representation_version: 0,
            precision: WeightPrecision::Boolean,
            path,
        }
    }

    fn mor() -> MorDecision {
        MorDecision {
            active_routers: vec![],
            ranked_experts: vec![],
            selected_experts: vec![PageId(0)],
            noise_ppm: 0,
            structural_disagreement_ppm: 0,
            expert_mass_threshold_ppm: 0,
            request_precision_escalation: false,
            hysteresis_retained_previous_primary: false,
        }
    }

    fn budget() -> CycloidBudget {
        CycloidBudget {
            max_hops: 3,
            max_expansions: 20,
            max_read_bytes: 100,
        }
    }

    fn panel(file_first: (u128, u128), ram_first: (u128, u128)) -> CycloidBalancedProfiles {
        let mut samples = Vec::new();
        for index in 0..3 {
            let first = if index % 2 == 0 {
                CycloidProbeOrder::FileThenRam
            } else {
                CycloidProbeOrder::RamThenFile
            };
            for order in [first, first.reverse()] {
                let (file_ns, ram_ns) = match order {
                    CycloidProbeOrder::FileThenRam => file_first,
                    CycloidProbeOrder::RamThenFile => ram_first,
                };
                samples.push(CycloidOrderedProfile {
                    order,
                    profile: CycloidPageProfile {
                        page_id: PageId(0),
                        representation_version: 0,
                        precision: WeightPrecision::Boolean,
                        payload_bytes: 1,
                        artifact_bytes_read: 65,
                        file_read_verify_ns: file_ns,
                        resident_copy_verify_ns: ram_ns,
                        byte_exact_match: true,
                    },
                });
            }
        }
        CycloidBalancedProfiles { pairs: 3, samples }
    }

    #[test]
    fn strong_indirect_dominance_under_both_orders_executes_ram() {
        let resident = runtime();
        let entry = authorized_file();
        let guarded = freeze_order_guarded(
            &resident,
            &mor(),
            &entry,
            &panel((1000, 100), (900, 110)),
            budget(),
            10,
            20,
        )
        .unwrap();
        assert_eq!(guarded.frozen.mode(), CycloidAccessMode::VerifiedResident);
        assert!(guarded.evidence.indirect_robust_in_both_orders);
        let loaded = guarded.frozen.read(&resident, &mor()).unwrap();
        assert_eq!(loaded.payload, [0xa5]);
        assert_eq!(loaded.resident_bytes_copied, 1);
        fs::remove_file(entry.path).unwrap();
    }

    #[test]
    fn pooled_winner_reversed_by_second_order_is_blocked() {
        let resident = runtime();
        let entry = authorized_file();
        let error = freeze_order_guarded(
            &resident,
            &mor(),
            &entry,
            &panel((1000, 100), (500, 490)),
            budget(),
            10,
            20,
        )
        .unwrap_err();
        assert_eq!(error, CycloidOrderGuardError::IndirectNotOrderRobust);
        fs::remove_file(entry.path).unwrap();
    }

    #[test]
    fn direct_path_remains_authorized_when_lags_are_inconclusive() {
        let resident = runtime();
        let entry = authorized_file();
        let guarded = freeze_order_guarded(
            &resident,
            &mor(),
            &entry,
            &panel((40, 100), (50, 120)),
            budget(),
            10,
            20,
        )
        .unwrap();
        assert_eq!(guarded.frozen.mode(), CycloidAccessMode::VerifiedFile);
        assert!(!guarded.evidence.indirect_robust_in_both_orders);
        fs::remove_file(entry.path).unwrap();
    }

    #[test]
    fn malformed_order_and_invalid_times_fail_closed() {
        let resident = runtime();
        let entry = authorized_file();
        let mut malformed = panel((1000, 100), (900, 100));
        malformed.samples[1].order = CycloidProbeOrder::FileThenRam;
        assert_eq!(
            freeze_order_guarded(&resident, &mor(), &entry, &malformed, budget(), 10, 20),
            Err(CycloidOrderGuardError::InvalidOrderPanel)
        );
        let mut invalid = panel((1000, 100), (900, 100));
        invalid.samples[1].profile.resident_copy_verify_ns = 0;
        assert_eq!(
            freeze_order_guarded(&resident, &mor(), &entry, &invalid, budget(), 10, 20),
            Err(CycloidOrderGuardError::InvalidTime)
        );
        fs::remove_file(entry.path).unwrap();
    }

    #[test]
    fn exact_identity_failure_cannot_pass_robust_order_guard() {
        let resident = runtime();
        let entry = authorized_file();
        let mut forged = panel((1000, 100), (900, 100));
        forged.samples[2].profile.representation_version = 42;
        assert_eq!(
            freeze_order_guarded(&resident, &mor(), &entry, &forged, budget(), 10, 20),
            Err(CycloidOrderGuardError::Dispatch(
                CycloidDispatchError::InvalidSample
            ))
        );
        fs::remove_file(entry.path).unwrap();
    }
}
