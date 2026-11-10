//! CYCLOID-5: bounded, reproducible, real-source host access observations.
//!
//! Run using only approved SML-GENIUS main code through RemoteOps on Dell:
//! cargo run --locked --release -p sml-runtime --bin sml-cycloid-physical-probe -- 3 256
//!
//! The benchmark creates its own synthetic SMLWGT01 page, materializes it
//! through the real ElasticRamRuntime, and executes both read orders.
//! Output is a single compact JSON report with *host-call durations only*.
//! No model inference, confidential weight or machine-performance claim.

use std::{
    env,
    error::Error,
    fs,
    hint::black_box,
    path::PathBuf,
    process::ExitCode,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use sml_core::{
    cycloid_route::CycloidBudget,
    elastic_weights::{
        ElasticWeightPage, ElasticWeightPlanner, ElasticWeightPolicy, ResidencyTier,
        WeightPrecision,
    },
    mor::MorDecision,
    PageId,
};
use sml_runtime::{
    build_weight_artifact,
    cycloid_access::{
        collect_balanced_mor_page_profiles, CycloidPageProfile, CycloidProbeOrder,
        MAX_CYCLOID_BALANCED_PAIRS, MIN_CYCLOID_BALANCED_PAIRS,
    },
    cycloid_dispatch::{CycloidAccessMode, FrozenCycloidAccess},
    ElasticRamRuntime, RamMaterializationBudget, WeightArtifactCatalog, WeightArtifactEntry,
};

const MAX_PAIRS: u8 = 16;
const MIN_PAYLOAD_KIB: usize = 64;
const MAX_PAYLOAD_KIB: usize = 1024;
const SCHEMA: &str = "sml.cycloid.physical-host-probe/v1";
/// Repetitions per timed batch: avoids reporting per-call clock noise as I/O latency.
const BORROW_BATCH_REPETITIONS: usize = 128;

struct TemporaryDirectory(PathBuf);

impl TemporaryDirectory {
    fn create() -> Result<Self, Box<dyn Error>> {
        let parent = env::var_os("RUNNER_TEMP")
            .map(PathBuf::from)
            .unwrap_or_else(env::temp_dir);
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let path = parent.join(format!("sml-cycloid-probe-{}-{nanos}", std::process::id()));
        fs::create_dir(&path)?;
        Ok(Self(path))
    }
}

impl Drop for TemporaryDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Quantiles {
    median_ns: u128,
    p95_ns: u128,
}

fn quantiles(values: &[u128]) -> Result<Quantiles, Box<dyn Error>> {
    if values.is_empty() {
        return Err("no measurements to summarize".into());
    }
    let mut data = values.to_vec();
    data.sort_unstable();
    let n = data.len();
    let p95_index = (n * 95).div_ceil(100) - 1;
    Ok(Quantiles {
        median_ns: data[n / 2],
        p95_ns: data[p95_index],
    })
}

fn synthetic_payload(bytes: usize) -> Vec<u8> {
    let mut state = 0x8c16_d73bu32;
    (0..bytes)
        .map(|_| {
            // Fixed, deterministic fixture; neither cryptography nor model data.
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state as u8
        })
        .collect()
}

fn arguments() -> Result<(u8, usize), Box<dyn Error>> {
    let mut args = env::args().skip(1);
    let pairs: u8 = args
        .next()
        .ok_or("usage: sml-cycloid-physical-probe <pairs 3..16> <payload_kib 64..1024>")?
        .parse()?;
    let kib: usize = args.next().ok_or("missing payload_kib")?.parse()?;
    if args.next().is_some() {
        return Err("unexpected additional CLI arguments".into());
    }
    if !(MIN_CYCLOID_BALANCED_PAIRS..=MAX_PAIRS).contains(&pairs)
        || pairs > MAX_CYCLOID_BALANCED_PAIRS
        || !(MIN_PAYLOAD_KIB..=MAX_PAYLOAD_KIB).contains(&kib)
    {
        return Err("invalid pairs or payload limit".into());
    }
    Ok((pairs, kib))
}

fn selected_page() -> MorDecision {
    MorDecision {
        active_routers: Vec::new(),
        ranked_experts: Vec::new(),
        selected_experts: vec![PageId(0)],
        noise_ppm: 0,
        structural_disagreement_ppm: 0,
        expert_mass_threshold_ppm: 0,
        request_precision_escalation: false,
        hysteresis_retained_previous_primary: false,
    }
}

/// Measure exactly the same immutable frozen resident page in both API modes.
/// "Borrow" performs only the authorization/identity checks and returns a
/// reference; "copy+CRC" performs those checks then copies and checks the
/// payload. These are intentionally different workloads, not a physical
/// DRAM-throughput or complete neural-inference benchmark.
fn timed_resident_batch(
    runtime: &ElasticRamRuntime,
    decision: &MorDecision,
    frozen: &FrozenCycloidAccess,
    borrow: bool,
) -> Result<u128, Box<dyn Error>> {
    let started = Instant::now();
    for _ in 0..BORROW_BATCH_REPETITIONS {
        if borrow {
            let bytes = frozen.borrow_committed_resident(runtime, decision)?;
            black_box(bytes);
        } else {
            let read = frozen.read(runtime, decision)?;
            if read.mode != CycloidAccessMode::VerifiedResident {
                return Err("copy baseline unexpectedly selected disk".into());
            }
            black_box(read.payload);
        }
    }
    Ok(started.elapsed().as_nanos())
}

/// AB/BA batches, preserving both execution orders instead of pooling away
/// cache effects. Each batch consists of 128 repeated *actual* API calls.
fn timed_resident_pair(
    runtime: &ElasticRamRuntime,
    decision: &MorDecision,
    frozen: &FrozenCycloidAccess,
    borrow_first: bool,
) -> Result<(u128, u128), Box<dyn Error>> {
    if borrow_first {
        let borrow_ns = timed_resident_batch(runtime, decision, frozen, true)?;
        let copy_crc_ns = timed_resident_batch(runtime, decision, frozen, false)?;
        Ok((borrow_ns, copy_crc_ns))
    } else {
        let copy_crc_ns = timed_resident_batch(runtime, decision, frozen, false)?;
        let borrow_ns = timed_resident_batch(runtime, decision, frozen, true)?;
        Ok((borrow_ns, copy_crc_ns))
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let (pairs, kib) = arguments()?;
    let payload_bytes = kib.checked_mul(1024).ok_or("payload overflow")?;
    let parameters = u64::try_from(payload_bytes)?
        .checked_mul(8)
        .ok_or("parameter overflow")?;
    let payload = synthetic_payload(payload_bytes);
    let artifact =
        build_weight_artifact(PageId(0), 0, WeightPrecision::Boolean, parameters, &payload)?;

    // The temporary directory is created atomically, owned only by this
    // invocation, and removed on normal success/error; no production paths.
    let temporary = TemporaryDirectory::create()?;
    let file = temporary.0.join("synthetic-sml-page.smlwgt");
    fs::write(&file, &artifact)?;
    let entry = WeightArtifactEntry {
        page_id: PageId(0),
        representation_version: 0,
        precision: WeightPrecision::Boolean,
        path: file,
    };
    let page = ElasticWeightPage::new(
        PageId(0),
        parameters,
        WeightPrecision::Boolean,
        ResidencyTier::Disk,
        0,
    )?;
    let planner = ElasticWeightPlanner::new(
        vec![page],
        ElasticWeightPolicy {
            ram_limit_bytes: u64::try_from(payload_bytes)?,
            vram_limit_bytes: 0,
            max_active_parameters: parameters,
            ternary_threshold_ppm: 500_000,
            residual4_threshold_ppm: 900_000,
            vram_threshold_ppm: 1_000_000,
        },
    )?;
    let mut runtime = ElasticRamRuntime::new(planner)?;
    let catalog = WeightArtifactCatalog::from_retained_bytes(vec![artifact])?;
    let planned = runtime.planner().plan_cycle(&[PageId(0)], 1)?;
    let stage = runtime.stage(
        &planned,
        &catalog,
        RamMaterializationBudget {
            max_files_read: 1,
            max_total_payload_bytes: u64::try_from(payload_bytes)?,
            max_total_artifact_bytes: u64::try_from(payload_bytes + 64)?,
        },
    )?;
    runtime.commit(stage)?;

    let selection = selected_page();
    let observations = collect_balanced_mor_page_profiles(
        &runtime,
        &selection,
        &entry,
        u64::try_from(payload_bytes)?,
        pairs,
    )?;

    let mut file_ns = Vec::with_capacity(observations.samples.len());
    let mut ram_ns = Vec::with_capacity(observations.samples.len());
    let mut file_when_file_first = Vec::with_capacity(usize::from(pairs));
    let mut ram_when_file_first = Vec::with_capacity(usize::from(pairs));
    let mut file_when_ram_first = Vec::with_capacity(usize::from(pairs));
    let mut ram_when_ram_first = Vec::with_capacity(usize::from(pairs));
    let mut file_first = 0_usize;
    let mut ram_first = 0_usize;
    for observation in &observations.samples {
        if !observation.profile.byte_exact_match
            || observation.profile.page_id != PageId(0)
            || observation.profile.payload_bytes != u64::try_from(payload_bytes)?
        {
            return Err("source integrity or selected page drift".into());
        }
        file_ns.push(observation.profile.file_read_verify_ns);
        ram_ns.push(observation.profile.resident_copy_verify_ns);
        match observation.order {
            CycloidProbeOrder::FileThenRam => {
                file_first += 1;
                file_when_file_first.push(observation.profile.file_read_verify_ns);
                ram_when_file_first.push(observation.profile.resident_copy_verify_ns);
            }
            CycloidProbeOrder::RamThenFile => {
                ram_first += 1;
                file_when_ram_first.push(observation.profile.file_read_verify_ns);
                ram_when_ram_first.push(observation.profile.resident_copy_verify_ns);
            }
        }
    }
    if file_first != usize::from(pairs) || ram_first != usize::from(pairs) {
        return Err("AB/BA order imbalance".into());
    }
    // CYCLOID-8 measures the real no-copy borrow API separately. A
    // deliberately synthetic *routing* cost panel selects the already
    // verified resident path; those fixture costs are NEVER reported as
    // observed latency. Freeze independently re-verifies disk/RAM identity.
    let diagnostic_selection_fixture = [CycloidPageProfile {
        page_id: PageId(0),
        representation_version: 0,
        precision: WeightPrecision::Boolean,
        payload_bytes: u64::try_from(payload_bytes)?,
        artifact_bytes_read: u64::try_from(payload_bytes + 64)?,
        file_read_verify_ns: 1_000_000,
        resident_copy_verify_ns: 1,
        byte_exact_match: true,
    }; 5];
    let frozen = FrozenCycloidAccess::freeze(
        &runtime,
        &selection,
        &entry,
        &diagnostic_selection_fixture,
        CycloidBudget {
            max_hops: 3,
            max_expansions: 20,
            max_read_bytes: u64::try_from(payload_bytes + 64)?,
        },
        1,
        1,
    )?;
    if frozen.mode() != CycloidAccessMode::VerifiedResident {
        return Err("synthetic selection fixture failed to authorize resident-only probe".into());
    }
    let borrowed = frozen.borrow_committed_resident(&runtime, &selection)?;
    let committed = runtime
        .ram_page(PageId(0))
        .ok_or("resident page disappeared")?;
    if borrowed != payload.as_slice()
        || !std::ptr::eq(borrowed.as_ptr(), committed.payload().as_ptr())
    {
        return Err("borrow pointer or byte identity differs from committed RAM".into());
    }

    let mut borrowed_batches = Vec::with_capacity(usize::from(pairs) * 2);
    let mut copied_batches = Vec::with_capacity(usize::from(pairs) * 2);
    let mut borrowed_first_batches = Vec::with_capacity(usize::from(pairs));
    let mut copied_first_batches = Vec::with_capacity(usize::from(pairs));
    for pair in 0..pairs {
        let first = pair % 2 == 0;
        for borrow_first in [first, !first] {
            let (borrow_ns, copy_crc_ns) =
                timed_resident_pair(&runtime, &selection, &frozen, borrow_first)?;
            borrowed_batches.push(borrow_ns);
            copied_batches.push(copy_crc_ns);
            if borrow_first {
                borrowed_first_batches.push(borrow_ns);
            } else {
                copied_first_batches.push(borrow_ns);
            }
        }
    }
    let borrow_batch = quantiles(&borrowed_batches)?;
    let copy_crc_batch = quantiles(&copied_batches)?;
    let borrow_first = quantiles(&borrowed_first_batches)?;
    let copy_first = quantiles(&copied_first_batches)?;

    let files = quantiles(&file_ns)?;
    let resident = quantiles(&ram_ns)?;
    let file_in_file_first = quantiles(&file_when_file_first)?;
    let ram_in_file_first = quantiles(&ram_when_file_first)?;
    let file_in_ram_first = quantiles(&file_when_ram_first)?;
    let ram_in_ram_first = quantiles(&ram_when_ram_first)?;
    println!(
        "{{\"schema\":\"{SCHEMA}\",\"scope\":\"synthetic_sml_page_verified_host_calls_only\",\
         \"model_inference_measured\":false,\"physical_dram_bytes_measured\":false,\
         \"page_id\":0,\"payload_bytes\":{payload_bytes},\"artifact_bytes\":{},\
         \"pairs\":{pairs},\"samples\":{},\
         \"file_then_ram_samples\":{file_first},\"ram_then_file_samples\":{ram_first},\
         \"file_median_ns\":{},\"file_p95_ns\":{},\
         \"resident_median_ns\":{},\"resident_p95_ns\":{},\
         \"file_when_file_first_median_ns\":{},\"file_when_file_first_p95_ns\":{},\
         \"resident_when_file_first_median_ns\":{},\"resident_when_file_first_p95_ns\":{},\
         \"file_when_ram_first_median_ns\":{},\"file_when_ram_first_p95_ns\":{},\
         \"resident_when_ram_first_median_ns\":{},\"resident_when_ram_first_p95_ns\":{},\
         \"all_payloads_byte_exact\":true,\
         \"borrow_mode\":\"synthetic_selection_fixture_only\",\
         \"borrow_zero_copy_pointer_exact\":true,\
         \"borrow_batch_repetitions\":{BORROW_BATCH_REPETITIONS},\
         \"borrow_batch_samples\":{},\
         \"borrow_batch_median_ns\":{},\"borrow_batch_p95_ns\":{},\
         \"copy_crc_batch_median_ns\":{},\"copy_crc_batch_p95_ns\":{},\
         \"borrow_when_borrow_first_batch_median_ns\":{},\
         \"borrow_when_borrow_first_batch_p95_ns\":{},\
         \"borrow_when_copy_first_batch_median_ns\":{},\
         \"borrow_when_copy_first_batch_p95_ns\":{}}}",
        payload_bytes + 64,
        observations.samples.len(),
        files.median_ns,
        files.p95_ns,
        resident.median_ns,
        resident.p95_ns,
        file_in_file_first.median_ns,
        file_in_file_first.p95_ns,
        ram_in_file_first.median_ns,
        ram_in_file_first.p95_ns,
        file_in_ram_first.median_ns,
        file_in_ram_first.p95_ns,
        ram_in_ram_first.median_ns,
        ram_in_ram_first.p95_ns,
        borrowed_batches.len(),
        borrow_batch.median_ns,
        borrow_batch.p95_ns,
        copy_crc_batch.median_ns,
        copy_crc_batch.p95_ns,
        borrow_first.median_ns,
        borrow_first.p95_ns,
        copy_first.median_ns,
        copy_first.p95_ns,
    );
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("CYCLOID_PHYSICAL_PROBE_REJECTED: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paired_borrow_batches_execute_on_same_committed_page() {
        let payload = vec![0xa5; 64];
        let artifact =
            build_weight_artifact(PageId(0), 0, WeightPrecision::Boolean, 512, &payload).unwrap();
        let temporary = TemporaryDirectory::create().unwrap();
        let path = temporary.0.join("borrow-test.smlwgt");
        fs::write(&path, &artifact).unwrap();
        let entry = WeightArtifactEntry {
            page_id: PageId(0),
            representation_version: 0,
            precision: WeightPrecision::Boolean,
            path,
        };
        let planner = ElasticWeightPlanner::new(
            vec![ElasticWeightPage::new(
                PageId(0),
                512,
                WeightPrecision::Boolean,
                ResidencyTier::Disk,
                0,
            )
            .unwrap()],
            ElasticWeightPolicy {
                ram_limit_bytes: 64,
                vram_limit_bytes: 0,
                max_active_parameters: 512,
                ternary_threshold_ppm: 500_000,
                residual4_threshold_ppm: 900_000,
                vram_threshold_ppm: 1_000_000,
            },
        )
        .unwrap();
        let mut runtime = ElasticRamRuntime::new(planner).unwrap();
        let catalog = WeightArtifactCatalog::from_retained_bytes(vec![artifact]).unwrap();
        let plan = runtime.planner().plan_cycle(&[PageId(0)], 1).unwrap();
        let staged = runtime
            .stage(&plan, &catalog, RamMaterializationBudget::default())
            .unwrap();
        runtime.commit(staged).unwrap();

        let selection = selected_page();
        let samples = [CycloidPageProfile {
            page_id: PageId(0),
            representation_version: 0,
            precision: WeightPrecision::Boolean,
            payload_bytes: 64,
            artifact_bytes_read: 128,
            file_read_verify_ns: 1_000_000,
            resident_copy_verify_ns: 1,
            byte_exact_match: true,
        }; 5];
        let frozen = FrozenCycloidAccess::freeze(
            &runtime,
            &selection,
            &entry,
            &samples,
            CycloidBudget {
                max_hops: 3,
                max_expansions: 20,
                max_read_bytes: 128,
            },
            1,
            1,
        )
        .unwrap();
        assert_eq!(frozen.mode(), CycloidAccessMode::VerifiedResident);
        assert_eq!(
            frozen
                .borrow_committed_resident(&runtime, &selection)
                .unwrap(),
            payload.as_slice()
        );
        let (borrow_first, copy_second) =
            timed_resident_pair(&runtime, &selection, &frozen, true).unwrap();
        let (borrow_second, copy_first) =
            timed_resident_pair(&runtime, &selection, &frozen, false).unwrap();
        let borrow_summary = quantiles(&[borrow_first, borrow_second]).unwrap();
        let copy_summary = quantiles(&[copy_first, copy_second]).unwrap();
        assert!(borrow_summary.p95_ns >= borrow_summary.median_ns);
        assert!(copy_summary.p95_ns >= copy_summary.median_ns);
        let authoritative = runtime.ram_page(PageId(0)).unwrap().payload();
        assert!(std::ptr::eq(
            frozen
                .borrow_committed_resident(&runtime, &selection)
                .unwrap()
                .as_ptr(),
            authoritative.as_ptr()
        ));
    }

    #[test]
    fn quantiles_are_deterministic_for_small_frozen_panel() {
        let values = [300, 100, 400, 200, 500];
        assert_eq!(
            quantiles(&values).unwrap(),
            Quantiles {
                median_ns: 300,
                p95_ns: 500
            }
        );
    }

    #[test]
    fn synthetic_fixture_is_nonzero_and_reproducible() {
        let first = synthetic_payload(1024);
        let second = synthetic_payload(1024);
        assert_eq!(first, second);
        assert!(first.iter().any(|value| *value != 0));
        assert_eq!(first.len(), 1024);
    }

    #[test]
    fn no_measurement_is_not_a_performance_result() {
        assert!(quantiles(&[]).is_err());
    }
}
