//! Read-only, pre-registered next-byte evaluation on the frozen SML-50M DEV
//! roster. Protocol: `docs/SML-INTEGRATED-LOOP-EVAL-PREREGISTRATION.md`.
//!
//! The tool never trains a learned model, never writes files and never opens
//! a TEST split. Count baselines are fitted on TRAIN rows only. In `pinned`
//! mode every input hash, dimension and the S2a reproduction gate is fixed in
//! code; `synthetic` mode exists only for CI fixtures and is labelled so.
//!
//! The Q4 arm reads a stream-fit output directory through the fail-closed
//! `Q4Checkpoint` loader (`--checkpoint DIR`): page count, cache size and
//! factorization come from the sealed `snapshot.tsv`/`metrics.tsv`, and in
//! pinned mode the recorded manifest, DEV roster and `dev_q4_bpc` must equal
//! the pre-registered values. A bare `--q4 FILE` is accepted in synthetic mode
//! only (a headerless Q4 file cannot prove its own factorization).
use sml_core::{
    byte_factorization::ByteFactorization,
    next_byte_eval::{
        check_normalized, file_sha256, load_verified_document, reproduces_exactly, ArmReport,
        CausalBayesMixture, FrozenManifest, LinearMixture, ManifestRole, NextByteEvaluator,
        NextBytePredictor, Order2Counts, Q4AtlasPredictor, Result, UnigramCounts,
        LINEAR_MIXTURE_WEIGHT,
    },
    q4_checkpoint::{CheckpointSchema, Q4Checkpoint},
    sparse_gradient_atlas::{QuantizedSparseAtlas, SparseConfig},
};
use std::{env, path::PathBuf};

const PINNED_MANIFEST_SHA256: &str =
    "739457d01202031d2aed2f79c209073a6c49c4cd1ff1e4be74564252b409a70e";
const PINNED_Q4_SHA256: &str = "3b2a0ea0f665d14afd6867e3cdec8c01fef10a426435d74f33ad0810a7cd427c";
/// S2a `dev_q4_bpc`, RemoteOps run 39504420134, engine b13217ca.
const PINNED_DEV_Q4_BPC: &str = "4.007063135";
const PINNED_DEV_DOCUMENTS: u64 = 583;
const PINNED_DEV_BYTES: u64 = 16_318_912;
const PINNED_DEV_EVENTS: u64 = 16_317_746;
const PINNED_PAGES: u64 = 48_829;
const CACHE_PAGES: usize = 32;
/// Same grouped schedule as the stream fit that produced the S2a value.
const GROUPED_BATCH_BYTES: usize = 4096;
/// Max |trait-path bpc - grouped-path bpc| (float summation order only).
const PATH_CONSISTENCY_TOLERANCE: f64 = 1e-6;
/// NBE-2 (`docs/SML-NEXT-BYTE-NBE2-MIXTURE-PREREGISTRATION.md`): published
/// order-2 DEV value of RemoteOps run 44912119606. Runs with a mixture arm
/// must reproduce it exactly before any mixture row is accepted.
const PINNED_ORDER2_BPC: &str = "3.592544112";
/// Absolute slack per scored position for the NBE-2 theorem gates (float
/// summation order only).
const MIXTURE_GATE_TOLERANCE_PER_BYTE: f64 = 1e-9;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Pinned,
    Synthetic,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Arm {
    Q4Atlas,
    Order2,
    Unigram,
    /// NBE-2: fixed 1/2-1/2 linear mixture of the Q4 atlas and order-2 counts.
    MixHalf,
    /// NBE-2: per-document causal Bayesian average of the same two experts.
    MixBayes,
}

impl Arm {
    fn is_mixture(self) -> bool {
        matches!(self, Arm::MixHalf | Arm::MixBayes)
    }
}

struct Args {
    mode: Mode,
    data_root: PathBuf,
    manifest: PathBuf,
    q4: Option<PathBuf>,
    checkpoint: Option<PathBuf>,
    pages: u64,
    factorization: ByteFactorization,
    arms: Vec<Arm>,
    expected_q4_bpc: Option<String>,
    top1: bool,
}

const USAGE: &str = "usage: sml-next-byte-eval --mode pinned|synthetic --data-root DIR --manifest MANIFEST.tsv [--checkpoint STREAM_FIT_OUTPUT_DIR] [--arms q4,order2,unigram[,mix_half,mix_bayes]] [--no-top1]\n  synthetic only: [--q4 WEIGHTS.q4] [--pages N] [--factorization msb|bits] [--expected-q4-bpc X.XXXXXXXXX]";

fn parse(argv: &[String]) -> Result<Args> {
    let mut mode = None;
    let mut data_root = None;
    let mut manifest = None;
    let mut q4 = None;
    let mut checkpoint = None;
    let mut pages = None;
    let mut factorization = None;
    let mut arms = None;
    let mut expected = None;
    let mut top1 = true;
    let mut it = argv.iter().skip(1);
    while let Some(flag) = it.next() {
        if flag == "--no-top1" {
            top1 = false;
            continue;
        }
        let value = it.next().ok_or(USAGE)?;
        match flag.as_str() {
            "--mode" => {
                mode = Some(match value.as_str() {
                    "pinned" => Mode::Pinned,
                    "synthetic" => Mode::Synthetic,
                    _ => return Err(USAGE.into()),
                })
            }
            "--data-root" => data_root = Some(PathBuf::from(value)),
            "--manifest" => manifest = Some(PathBuf::from(value)),
            "--q4" => q4 = Some(PathBuf::from(value)),
            "--checkpoint" => checkpoint = Some(PathBuf::from(value)),
            "--pages" => pages = Some(value.parse::<u64>()?),
            "--factorization" => {
                factorization = Some(match value.as_str() {
                    "msb" => ByteFactorization::MsbTree,
                    "bits" => ByteFactorization::IndependentBits,
                    _ => return Err(USAGE.into()),
                })
            }
            "--arms" => {
                let mut list = Vec::new();
                for name in value.split(',') {
                    let arm = match name {
                        "q4" => Arm::Q4Atlas,
                        "order2" => Arm::Order2,
                        "unigram" => Arm::Unigram,
                        "mix_half" => Arm::MixHalf,
                        "mix_bayes" => Arm::MixBayes,
                        _ => return Err(USAGE.into()),
                    };
                    if list.contains(&arm) {
                        return Err("duplicate arm".into());
                    }
                    list.push(arm);
                }
                arms = Some(list);
            }
            "--expected-q4-bpc" => expected = Some(value.clone()),
            _ => return Err(USAGE.into()),
        }
    }
    let mode = mode.ok_or(USAGE)?;
    let arms = arms.unwrap_or_else(|| vec![Arm::Q4Atlas, Arm::Order2, Arm::Unigram]);
    if q4.is_some() && checkpoint.is_some() {
        return Err("--q4 and --checkpoint are mutually exclusive".into());
    }
    let needs_q4 = arms.iter().any(|a| *a == Arm::Q4Atlas || a.is_mixture());
    if needs_q4 != (q4.is_some() || checkpoint.is_some()) {
        return Err(
            "--checkpoint (or synthetic --q4) is required exactly when the q4 arm is selected"
                .into(),
        );
    }
    if arms.iter().any(|a| a.is_mixture()) {
        // NBE-2: the theorem gates compare mixture losses with both experts'
        // losses on the same run, so both expert rows must be scored first.
        let first_mix = arms.iter().position(|a| a.is_mixture()).unwrap_or(0);
        let before = &arms[..first_mix];
        if !before.contains(&Arm::Q4Atlas) || !before.contains(&Arm::Order2) {
            return Err("mixture arms require q4 and order2 arms listed before them".into());
        }
        if mode == Mode::Pinned && top1 {
            return Err("pinned mixture runs are pre-registered without top-1 (--no-top1)".into());
        }
    }
    if mode == Mode::Pinned && q4.is_some() {
        return Err(
            "pinned mode reads the Q4 arm only through --checkpoint (verified loader)".into(),
        );
    }
    if checkpoint.is_some() && (pages.is_some() || factorization.is_some()) {
        return Err("--checkpoint records pages and factorization; overrides are refused".into());
    }
    let (pages, factorization, expected_q4_bpc) = match mode {
        Mode::Pinned => {
            if pages.is_some() || factorization.is_some() || expected.is_some() {
                return Err("pinned mode fixes pages, factorization and the gate value".into());
            }
            (
                PINNED_PAGES,
                ByteFactorization::MsbTree,
                Some(PINNED_DEV_Q4_BPC.to_string()),
            )
        }
        Mode::Synthetic => (
            pages.unwrap_or(8),
            factorization.unwrap_or(ByteFactorization::MsbTree),
            expected,
        ),
    };
    Ok(Args {
        mode,
        data_root: data_root.ok_or(USAGE)?,
        manifest: manifest.ok_or(USAGE)?,
        q4,
        checkpoint,
        pages,
        factorization,
        arms,
        expected_q4_bpc,
        top1,
    })
}

/// Open `dir` through the fail-closed loader and bind it to this evaluation:
/// the checkpoint must have been scored on this exact manifest and DEV roster.
/// Pinned mode additionally requires the S2a continuation with its frozen Q4
/// SHA-256, dimensions, factorization and verbatim `dev_q4_bpc`.
fn verify_checkpoint(
    dir: &std::path::Path,
    pinned: bool,
    manifest_sha256: &str,
    dev_documents: u64,
    dev_events: u64,
) -> Result<Q4Checkpoint> {
    let ckpt = if pinned {
        Q4Checkpoint::open_verified_pinned(dir, PINNED_Q4_SHA256)?
    } else {
        Q4Checkpoint::open_verified(dir)?
    };
    if ckpt.manifest_sha256 != manifest_sha256 {
        return Err("checkpoint was scored on a different manifest".into());
    }
    if ckpt.dev_files != dev_documents || ckpt.dev_events != dev_events {
        return Err("checkpoint DEV roster differs from this manifest's DEV rows".into());
    }
    if pinned
        && (ckpt.schema != CheckpointSchema::Continue
            || ckpt.config.pages != PINNED_PAGES
            || ckpt.config.max_cached_pages != CACHE_PAGES
            || ckpt.factorization != ByteFactorization::MsbTree
            || ckpt.dev_q4_bpc != PINNED_DEV_Q4_BPC)
    {
        return Err("checkpoint is not the pre-registered S2a continuation".into());
    }
    Ok(ckpt)
}

fn run(args: &Args) -> Result<()> {
    let pinned = args.mode == Mode::Pinned;
    let manifest = FrozenManifest::load(
        &args.data_root,
        &args.manifest,
        pinned.then_some(PINNED_MANIFEST_SHA256),
    )?;
    let dev_entries = manifest.role(ManifestRole::Dev).collect::<Vec<_>>();
    let dev = dev_entries
        .iter()
        .map(|entry| load_verified_document(entry))
        .collect::<Result<Vec<_>>>()?;
    let dev_bytes = dev.iter().map(|d| d.len() as u64).sum::<u64>();
    let dev_events = dev
        .iter()
        .map(|d| d.len().saturating_sub(2) as u64)
        .sum::<u64>();
    if dev.is_empty() || dev_events == 0 {
        return Err("no DEV positions to score".into());
    }
    if pinned
        && (dev.len() as u64 != PINNED_DEV_DOCUMENTS
            || dev_bytes != PINNED_DEV_BYTES
            || dev_events != PINNED_DEV_EVENTS)
    {
        return Err("DEV roster differs from the pre-registered 583-document split".into());
    }
    let config = SparseConfig {
        pages: args.pages,
        max_cached_pages: CACHE_PAGES,
        learning_rate: 0.15,
    };
    let checkpoint = match &args.checkpoint {
        Some(dir) => Some(verify_checkpoint(
            dir,
            pinned,
            &manifest.manifest_sha256,
            dev.len() as u64,
            dev_events,
        )?),
        None => None,
    };
    let q4_sha = match (&args.q4, &checkpoint) {
        (Some(path), None) => {
            let sha = file_sha256(path)?;
            if pinned && sha != PINNED_Q4_SHA256 {
                return Err("Q4 SHA-256 differs from the pre-registered S2a pin".into());
            }
            Some(sha)
        }
        (None, Some(ckpt)) => Some(ckpt.q4_sha256.clone()),
        _ => None,
    };
    // Synthetic runs from a checkpoint gate on its own recorded dev_q4_bpc
    // unless an explicit value is given; pinned runs always use the pin.
    let expected_q4_bpc = match (&args.expected_q4_bpc, &checkpoint) {
        (Some(value), _) => Some(value.clone()),
        (None, Some(ckpt)) => Some(ckpt.dev_q4_bpc.clone()),
        (None, None) => None,
    };
    let open_q4 = || -> Result<Option<QuantizedSparseAtlas>> {
        match (&args.q4, &checkpoint) {
            (Some(path), None) => Ok(Some(QuantizedSparseAtlas::open_with_factorization(
                path,
                config,
                args.factorization,
            )?)),
            (None, Some(ckpt)) => Ok(Some(ckpt.open_atlas()?)),
            _ => Ok(None),
        }
    };
    let factorization = checkpoint
        .as_ref()
        .map_or(args.factorization, |c| c.factorization);
    println!(
        "NEXT_BYTE_EVAL_V1 mode={} manifest_sha256={} q4_sha256={} dev_documents={} dev_bytes={dev_bytes} dev_events={dev_events} train_documents={} test=UNOPENED promotion=NOT_PROMOTED",
        if pinned { "PINNED" } else { "SYNTHETIC_UNPINNED" },
        manifest.manifest_sha256,
        q4_sha.as_deref().unwrap_or("NA"),
        dev.len(),
        manifest.role(ManifestRole::Train).count(),
    );
    if let Some(ckpt) = &checkpoint {
        println!(
            "CHECKPOINT_V1 schema={} metrics_sha256={} snapshot_sha256={} q4_sha256={} pages={} cache_pages={} factorization={:?} recorded_dev_q4_bpc={} recorded_dev_f32_bpc={} parent_q4_sha256={} loader=q4_checkpoint::open_verified{}",
            ckpt.schema.as_str(),
            ckpt.metrics_sha256,
            ckpt.snapshot_sha256,
            ckpt.q4_sha256,
            ckpt.config.pages,
            ckpt.config.max_cached_pages,
            ckpt.factorization,
            ckpt.dev_q4_bpc,
            ckpt.dev_f32_bpc,
            ckpt.parent_q4_sha256.as_deref().unwrap_or("NA"),
            if pinned { "_pinned" } else { "" },
        );
    }

    // 1. Reproduction gate on a cold atlas, through the exact S2a scoring path.
    let mut gate_bpc = None;
    if let Some(mut atlas) = open_q4()? {
        let mut loss = 0f64;
        let mut events = 0u64;
        for document in &dev {
            let (l, e) = atlas.evaluate_document_grouped(document, GROUPED_BATCH_BYTES)?;
            loss += l;
            events = events.checked_add(e).ok_or("gate event overflow")?;
        }
        if events != dev_events || !loss.is_finite() {
            return Err("gate scored a different number of DEV positions".into());
        }
        let measured = loss / events as f64;
        let status = match &expected_q4_bpc {
            Some(expected) if reproduces_exactly(measured, expected) => "PASS",
            Some(_) => "FAIL",
            None => "NOT_APPLICABLE",
        };
        println!(
            "REPRODUCTION_GATE measured_dev_q4_bpc={measured:.9} expected={} status={status} grouped_page_loads={}",
            expected_q4_bpc.as_deref().unwrap_or("NA"),
            atlas.page_loads(),
        );
        if status == "FAIL" {
            return Err("RUN_INVALID: atlas arm did not reproduce the S2a DEV Q4 value".into());
        }
        gate_bpc = Some(measured);
    }

    // 2. Count baselines: TRAIN rows only, frozen before the first DEV byte.
    let need_counts = args
        .arms
        .iter()
        .any(|a| matches!(a, Arm::Order2 | Arm::Unigram) || a.is_mixture());
    let mut order2 = None;
    let mut unigram = None;
    if need_counts {
        let mut o2 = args
            .arms
            .iter()
            .any(|a| *a == Arm::Order2 || a.is_mixture())
            .then(Order2Counts::default);
        let mut u1 = args
            .arms
            .contains(&Arm::Unigram)
            .then(UnigramCounts::default);
        for entry in manifest.role(ManifestRole::Train) {
            let document = load_verified_document(entry)?;
            if let Some(model) = o2.as_mut() {
                model.fit_document(&document)?;
            }
            if let Some(model) = u1.as_mut() {
                model.fit_document(&document)?;
            }
        }
        order2 = o2;
        unigram = u1;
    }

    // 3. Every arm through the same trait and evaluator, in pre-registered order.
    println!("{}", ArmReport::tsv_header());
    let mut losses: Vec<(Arm, f64, u64, u64)> = Vec::new();
    for arm in &args.arms {
        let mut q4_predictor;
        let mut mix_half;
        let mut mix_bayes;
        let predictor: &mut dyn NextBytePredictor = match arm {
            Arm::Q4Atlas => {
                let atlas = open_q4()?.ok_or("missing Q4 input")?;
                q4_predictor = Q4AtlasPredictor::new("q4_atlas_s2a", atlas, factorization);
                &mut q4_predictor
            }
            Arm::Order2 => order2.as_mut().ok_or("order2 not fitted")?,
            Arm::Unigram => unigram.as_mut().ok_or("unigram not fitted")?,
            Arm::MixHalf => {
                let atlas = open_q4()?.ok_or("missing Q4 input")?;
                mix_half = LinearMixture::new(
                    "mix_q4_order2_linear_half",
                    Q4AtlasPredictor::new("q4_atlas_s2a", atlas, factorization),
                    order2.as_mut().ok_or("order2 not fitted")?,
                    LINEAR_MIXTURE_WEIGHT,
                )?;
                &mut mix_half
            }
            Arm::MixBayes => {
                let atlas = open_q4()?.ok_or("missing Q4 input")?;
                mix_bayes = CausalBayesMixture::new(
                    "mix_q4_order2_bayes_doc",
                    Q4AtlasPredictor::new("q4_atlas_s2a", atlas, factorization),
                    order2.as_mut().ok_or("order2 not fitted")?,
                );
                &mut mix_bayes
            }
        };
        let mut evaluator = NextByteEvaluator::new(predictor, args.top1);
        for document in &dev {
            evaluator.score_document(predictor, document)?;
        }
        let report = evaluator.finish(predictor)?;
        if report.scored_bytes != dev_events {
            return Err("arm scored a different number of DEV positions".into());
        }
        if *arm == Arm::Q4Atlas {
            let trait_bpc = report.bits_per_byte().ok_or("empty atlas report")?;
            let grouped = gate_bpc.ok_or("atlas arm without gate")?;
            if (trait_bpc - grouped).abs() > PATH_CONSISTENCY_TOLERANCE {
                return Err("RUN_INVALID: trait path disagrees with grouped S2a path".into());
            }
        }
        // Post-scoring sanity probe (does not affect reported page loads).
        let probe = &dev[0][..dev[0].len().min(2)];
        if probe.len() == 2 {
            if arm.is_mixture() {
                // Stateful mixtures start a fresh document for the probe.
                predictor.begin_document()?;
            }
            check_normalized(predictor, probe)?;
        }
        println!("{}", report.tsv_row());
        losses.push((
            *arm,
            report.total_loss_bits,
            report.scored_bytes,
            report.documents,
        ));
    }
    if args.arms.iter().any(|a| a.is_mixture()) {
        mixture_gates(&losses, pinned)?;
    }
    println!(
        "NEXT_BYTE_EVAL_DONE arms={} test=UNOPENED promotion=NOT_PROMOTED",
        args.arms.len()
    );
    Ok(())
}

/// NBE-2 validity gates, evaluated after every arm row is printed. A failure
/// makes the whole run INVALID (non-zero exit; rows are discarded).
/// - O2: pinned runs reproduce the published order-2 DEV value exactly.
/// - M1: `L_half <= (L_q4 + L_o2) / 2` (concavity of log2).
/// - M2: `L_bayes <= min(L_q4, L_o2) + documents` bits (prior 1/2 per document).
fn mixture_gates(losses: &[(Arm, f64, u64, u64)], pinned: bool) -> Result<()> {
    let find = |arm: Arm| losses.iter().find(|(a, ..)| *a == arm).copied();
    let (_, l_q4, n, _) = find(Arm::Q4Atlas).ok_or("mixture gates need the q4 row")?;
    let (_, l_o2, n_o2, _) = find(Arm::Order2).ok_or("mixture gates need the order2 row")?;
    if n == 0 || n != n_o2 {
        return Err("RUN_INVALID: experts scored different positions".into());
    }
    let tolerance = MIXTURE_GATE_TOLERANCE_PER_BYTE * n as f64;
    let o2_bpc = l_o2 / n as f64;
    let o2_status = if !pinned {
        "NOT_APPLICABLE"
    } else if reproduces_exactly(o2_bpc, PINNED_ORDER2_BPC) {
        "PASS"
    } else {
        "FAIL"
    };
    let mut ok = o2_status != "FAIL";
    let mut line = format!(
        "MIXTURE_GATES order2_bpc={o2_bpc:.9} order2_expected={} order2_repro={o2_status}",
        if pinned { PINNED_ORDER2_BPC } else { "NA" }
    );
    if let Some((_, l_half, n_half, _)) = find(Arm::MixHalf) {
        let bound = 0.5 * (l_q4 + l_o2);
        let pass = n_half == n && l_half <= bound + tolerance;
        ok &= pass;
        line.push_str(&format!(
            " m1_jensen={} m1_slack_bits={:.6}",
            if pass { "PASS" } else { "FAIL" },
            bound - l_half
        ));
    }
    if let Some((_, l_bayes, n_bayes, docs)) = find(Arm::MixBayes) {
        let bound = l_q4.min(l_o2) + docs as f64;
        let pass = n_bayes == n && l_bayes <= bound + tolerance;
        ok &= pass;
        line.push_str(&format!(
            " m2_bayes_bound={} m2_slack_bits={:.6}",
            if pass { "PASS" } else { "FAIL" },
            bound - l_bayes
        ));
    }
    println!("{line}");
    if ok {
        Ok(())
    } else {
        Err("RUN_INVALID: an NBE-2 mixture validity gate failed".into())
    }
}

fn main() {
    let argv = env::args().collect::<Vec<_>>();
    let result = parse(&argv).and_then(|args| run(&args));
    if let Err(error) = result {
        eprintln!("NEXT_BYTE_EVAL_FAIL {error}");
        std::process::exit(1);
    }
}
