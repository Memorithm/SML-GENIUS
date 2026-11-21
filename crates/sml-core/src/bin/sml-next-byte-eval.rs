//! Read-only, pre-registered next-byte evaluation on the frozen SML-50M DEV
//! roster. Protocol: `docs/SML-INTEGRATED-LOOP-EVAL-PREREGISTRATION.md`.
//!
//! The tool never trains a learned model, never writes files and never opens
//! a TEST split. Count baselines are fitted on TRAIN rows only. In `pinned`
//! mode every input hash, dimension and the S2a reproduction gate is fixed in
//! code; `synthetic` mode exists only for CI fixtures and is labelled so.
use sml_core::{
    byte_factorization::ByteFactorization,
    next_byte_eval::{
        check_normalized, file_sha256, load_verified_document, reproduces_exactly, ArmReport,
        FrozenManifest, ManifestRole, NextByteEvaluator, NextBytePredictor, Order2Counts,
        Q4AtlasPredictor, Result, UnigramCounts,
    },
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
}

struct Args {
    mode: Mode,
    data_root: PathBuf,
    manifest: PathBuf,
    q4: Option<PathBuf>,
    pages: u64,
    factorization: ByteFactorization,
    arms: Vec<Arm>,
    expected_q4_bpc: Option<String>,
    top1: bool,
}

const USAGE: &str = "usage: sml-next-byte-eval --mode pinned|synthetic --data-root DIR --manifest MANIFEST.tsv [--q4 WEIGHTS.q4] [--arms q4,order2,unigram] [--no-top1]\n  synthetic only: [--pages N] [--factorization msb|bits] [--expected-q4-bpc X.XXXXXXXXX]";

fn parse(argv: &[String]) -> Result<Args> {
    let mut mode = None;
    let mut data_root = None;
    let mut manifest = None;
    let mut q4 = None;
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
    if arms.contains(&Arm::Q4Atlas) != q4.is_some() {
        return Err("--q4 is required exactly when the q4 arm is selected".into());
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
        pages,
        factorization,
        arms,
        expected_q4_bpc,
        top1,
    })
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
    let q4_sha = match &args.q4 {
        Some(path) => {
            let sha = file_sha256(path)?;
            if pinned && sha != PINNED_Q4_SHA256 {
                return Err("Q4 SHA-256 differs from the pre-registered S2a pin".into());
            }
            Some(sha)
        }
        None => None,
    };
    println!(
        "NEXT_BYTE_EVAL_V1 mode={} manifest_sha256={} q4_sha256={} dev_documents={} dev_bytes={dev_bytes} dev_events={dev_events} train_documents={} test=UNOPENED promotion=NOT_PROMOTED",
        if pinned { "PINNED" } else { "SYNTHETIC_UNPINNED" },
        manifest.manifest_sha256,
        q4_sha.as_deref().unwrap_or("NA"),
        dev.len(),
        manifest.role(ManifestRole::Train).count(),
    );

    // 1. Reproduction gate on a cold atlas, through the exact S2a scoring path.
    let mut gate_bpc = None;
    if let Some(path) = &args.q4 {
        let mut atlas =
            QuantizedSparseAtlas::open_with_factorization(path, config, args.factorization)?;
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
        let status = match &args.expected_q4_bpc {
            Some(expected) if reproduces_exactly(measured, expected) => "PASS",
            Some(_) => "FAIL",
            None => "NOT_APPLICABLE",
        };
        println!(
            "REPRODUCTION_GATE measured_dev_q4_bpc={measured:.9} expected={} status={status} grouped_page_loads={}",
            args.expected_q4_bpc.as_deref().unwrap_or("NA"),
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
        .any(|a| matches!(a, Arm::Order2 | Arm::Unigram));
    let mut order2 = None;
    let mut unigram = None;
    if need_counts {
        let mut o2 = args.arms.contains(&Arm::Order2).then(Order2Counts::default);
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
    for arm in &args.arms {
        let mut q4_predictor;
        let predictor: &mut dyn NextBytePredictor = match arm {
            Arm::Q4Atlas => {
                let path = args.q4.as_ref().ok_or("missing Q4 path")?;
                let atlas = QuantizedSparseAtlas::open_with_factorization(
                    path,
                    config,
                    args.factorization,
                )?;
                q4_predictor = Q4AtlasPredictor::new("q4_atlas_s2a", atlas, args.factorization);
                &mut q4_predictor
            }
            Arm::Order2 => order2.as_mut().ok_or("order2 not fitted")?,
            Arm::Unigram => unigram.as_mut().ok_or("unigram not fitted")?,
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
            check_normalized(predictor, probe)?;
        }
        println!("{}", report.tsv_row());
    }
    println!(
        "NEXT_BYTE_EVAL_DONE arms={} test=UNOPENED promotion=NOT_PROMOTED",
        args.arms.len()
    );
    Ok(())
}

fn main() {
    let argv = env::args().collect::<Vec<_>>();
    let result = parse(&argv).and_then(|args| run(&args));
    if let Err(error) = result {
        eprintln!("NEXT_BYTE_EVAL_FAIL {error}");
        std::process::exit(1);
    }
}
