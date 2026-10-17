//! Native physical SEC pilot for the first jointly trained MOR + ANF + RTRL cell.
//! Only bounded corpus prefix; source is exploratory and not G0-admitted.
use sml_core::{
    elastic_ngram_paged_sha::{hex, parse_hex, Sha256},
    rtlr_elastic::{RtlrElastic, RtlrElasticConfig, RtlrV2Policy},
    rtrl3_elastic::{ElasticRtrl3, Rtrl3Config},
    sparse_gradient_atlas::SparseConfig,
    sparse_mor_anf_recurrent::{HybridWindowCounts, MorAnfRecurrent, RtrlClipStats},
};
use std::{
    collections::HashSet,
    env,
    error::Error,
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, Read, Write},
    path::Path,
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const HEADER: &str =
    "shard\trow\tsha256\tbytes\tlicense_class\tlicense_source\tidentifier\tcreator_title";
const TRAIN_TARGET: u64 = 384 * 1024;
const DEV_TARGET: u64 = 192 * 1024;
const DOCUMENT_MAX: usize = 2 * 1024 * 1024;
const MATCHED_TRAIN_TARGET: u64 = 64 * 1024;
const MATCHED_DEV_TARGET: u64 = 32 * 1024;
const MATCHED_WINDOW: usize = 128;
const BOUNDARY_WINDOW: usize = 16;
const RTRL2_CLIP_SMALL: f64 = 0.125;
const RTRL2_CLIP_LARGE: f64 = 1.0;
const MATCHED_SAMPLE_MAX: usize = 1024;

#[derive(Clone, Copy, PartialEq, Eq)]
enum PilotMode {
    LegacyRtrl,
    LegacyBptt,
    MatchedRtrl,
    MatchedBptt,
    MatchedHybrid,
    BoundaryRtrl,
    BoundaryBptt,
    BoundaryHybrid1,
    BoundaryHyb2,
    Rtrl2Window16,
    Rtrl2Continuous,
    Rtrl2ClipSmall,
    Rtrl2ClipLarge,
    Rtrl3Paged1,
    Rtrl3Paged8,
    Rtrl3Paged16,
    RtlrElastic1,
    RtlrElastic8,
    RtlrElastic16,
    RtlrV2_1,
    RtlrV2_8,
    RtlrV2_16,
}

impl PilotMode {
    fn is_bptt(self) -> bool {
        matches!(self, Self::LegacyBptt | Self::MatchedBptt)
    }

    fn is_matched(self) -> bool {
        !matches!(self, Self::LegacyRtrl | Self::LegacyBptt)
    }

    fn is_boundary(self) -> bool {
        matches!(
            self,
            Self::BoundaryRtrl
                | Self::BoundaryBptt
                | Self::BoundaryHybrid1
                | Self::BoundaryHyb2
                | Self::Rtrl2Window16
                | Self::Rtrl2Continuous
                | Self::Rtrl2ClipSmall
                | Self::Rtrl2ClipLarge
                | Self::Rtrl3Paged1
                | Self::Rtrl3Paged8
                | Self::Rtrl3Paged16
                | Self::RtlrElastic1
                | Self::RtlrElastic8
                | Self::RtlrElastic16
                | Self::RtlrV2_1
                | Self::RtlrV2_8
                | Self::RtlrV2_16
        )
    }
}

fn sha(path: &Path) -> Result<String> {
    let mut reader = BufReader::with_capacity(1024 * 1024, File::open(path)?);
    let mut state = Sha256::default();
    let mut buf = [0u8; 1024 * 1024];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        state.update(&buf[..n]);
    }
    Ok(hex(&state.finish()))
}

fn execute(
    corpus: &Path,
    provenance: &Path,
    expected_sha: &str,
    output: &Path,
    mode: PilotMode,
) -> Result<()> {
    if parse_hex(expected_sha).is_none() || corpus == provenance || output.exists() {
        return Err("overlapping paths, existing output or invalid source SHA".into());
    }
    for path in [corpus, provenance] {
        let meta = fs::symlink_metadata(path)?;
        if !meta.is_file() || meta.file_type().is_symlink() {
            return Err("refuse nonregular/symlink SEC sources".into());
        }
    }
    let digest = sha(corpus)?;
    if digest != expected_sha {
        return Err("source changed since admission".into());
    }
    let bptt = mode.is_bptt();
    let matched = mode.is_matched();
    let boundary_mode = mode.is_boundary();
    let causal_window = if boundary_mode {
        BOUNDARY_WINDOW
    } else {
        MATCHED_WINDOW
    };
    let config = SparseConfig {
        pages: if bptt || matched { 48_829 } else { 4096 },
        max_cached_pages: 32,
        learning_rate: 0.20,
    };
    let mut model = MorAnfRecurrent::create(output, config)?;
    let mut rtrl3 = if matches!(
        mode,
        PilotMode::Rtrl3Paged1 | PilotMode::Rtrl3Paged8 | PilotMode::Rtrl3Paged16
    ) {
        let hidden = match mode {
            PilotMode::Rtrl3Paged1 => 1,
            PilotMode::Rtrl3Paged8 => 8,
            PilotMode::Rtrl3Paged16 => 16,
            _ => unreachable!(),
        };
        Some(ElasticRtrl3::for_mor(Rtrl3Config {
            hidden,
            eligibility_rank: if hidden == 16 {
                4
            } else if hidden == 8 {
                2
            } else {
                1
            },
            mixing_rank: if hidden == 1 { 0 } else { 2 },
            learning_rate: 0.005,
            recurrent_clip_norm: 1e30,
            seed: 11,
        })?)
    } else {
        None
    };
    let mut rtrl3_clipped = 0u64;
    let mut rtlr = if matches!(
        mode,
        PilotMode::RtlrElastic1
            | PilotMode::RtlrElastic8
            | PilotMode::RtlrElastic16
            | PilotMode::RtlrV2_1
            | PilotMode::RtlrV2_8
            | PilotMode::RtlrV2_16
    ) {
        let max_active = match mode {
            PilotMode::RtlrElastic1 | PilotMode::RtlrV2_1 => 1,
            PilotMode::RtlrElastic8 | PilotMode::RtlrV2_8 => 8,
            PilotMode::RtlrElastic16 | PilotMode::RtlrV2_16 => 16,
            _ => unreachable!(),
        };
        let config = RtlrElasticConfig {
            max_active,
            learning_rate: 0.005,
            grow_above_bpc: 4.1,
            shrink_below_bpc: 3.8,
            patience: 2,
            warmup_documents: 2,
            seed: 17,
        };
        let model = if matches!(
            mode,
            PilotMode::RtlrV2_1 | PilotMode::RtlrV2_8 | PilotMode::RtlrV2_16
        ) {
            RtlrElastic::new_v2(
                config,
                RtlrV2Policy {
                    ewma_alpha: 0.25,
                    plateau_tolerance_bpc: 0.05,
                    marginal_cost_bpc_per_cell: 0.001,
                },
                &output.join("rtlr_v2_dormant"),
            )?
        } else {
            RtlrElastic::new(config)?
        };
        Some(model)
    } else {
        None
    };
    let mut reader = BufReader::new(File::open(corpus)?);
    let mut lines = BufReader::new(File::open(provenance)?).lines();
    if lines.next().transpose()?.as_deref() != Some(HEADER) {
        return Err("incorrect provenance schema".into());
    }
    let mut hashes = HashSet::new();
    let mut train_docs = 0u64;
    let mut train_bytes = 0u64;
    let mut train_events = 0u64;
    let mut train_loss = 0f64;
    let mut dev_docs = 0u64;
    let mut dev_bytes = 0u64;
    let mut dev_events = 0u64;
    let mut dev_loss = 0f64;
    let start = std::time::Instant::now();
    let train_target = if matched {
        MATCHED_TRAIN_TARGET
    } else if bptt {
        12 * 1024
    } else {
        TRAIN_TARGET
    };
    let dev_target = if matched {
        MATCHED_DEV_TARGET
    } else if bptt {
        8 * 1024
    } else {
        DEV_TARGET
    };
    let mut train_fingerprint = Sha256::default();
    let mut dev_fingerprint = Sha256::default();
    let mut hybrid_counts = HybridWindowCounts::default();
    let mut clip_stats = RtrlClipStats::default();
    for line in lines.take(1000) {
        if train_bytes >= train_target && dev_bytes >= dev_target {
            break;
        }
        let line = line?;
        let cols: Vec<_> = line.split('\t').collect();
        if cols.len() != 8
            || cols[6].is_empty()
            || !matches!(cols[4], "CC0" | "PD_CLAIM" | "CC_BY_ATTRIBUTION")
        {
            return Err("invalid rights/filing metadata".into());
        }
        let length: usize = cols[3].parse()?;
        if !(128..=8 * 1024 * 1024).contains(&length) {
            return Err("unexpected SEC document length".into());
        }
        let mut bytes = vec![0; length];
        reader.read_exact(&mut bytes)?;
        let mut separator = [0u8; 1];
        reader.read_exact(&mut separator)?;
        if separator != [b'\n'] {
            return Err("corrupt document boundary".into());
        }
        let expected = parse_hex(cols[2]).ok_or("bad SHA ledger")?;
        let mut h = Sha256::default();
        h.update(&bytes);
        if h.finish() != expected || !hashes.insert(expected) {
            return Err("duplicate or altered SEC document".into());
        }
        if length > DOCUMENT_MAX {
            continue;
        }
        // The BPTT arm intentionally limits each verified source report to a
        // small document prefix. Report consumed bytes, not original file size.
        let sample = if bptt || matched {
            &bytes[..bytes.len().min(MATCHED_SAMPLE_MAX)]
        } else {
            &bytes
        };
        if train_bytes < train_target {
            let (loss, events) = if let Some(ref mut state) = rtlr {
                if state.is_v2() {
                    model.train_document_rtlr_v2_paged(sample, state)?
                } else {
                    model.train_document_rtlr_elastic_paged(sample, state)?
                }
            } else if rtrl3.is_some() {
                let (loss, events, clipped) = model.train_document_rtrl3_paged(
                    sample,
                    rtrl3.as_mut().ok_or("missing RTRL-3 trainer")?,
                )?;
                rtrl3_clipped += clipped;
                (loss, events)
            } else if mode == PilotMode::Rtrl2Window16 {
                model.train_document_rtrl_windowed(sample, BOUNDARY_WINDOW)?
            } else if mode == PilotMode::Rtrl2Continuous {
                model.train_document(sample)?
            } else if matches!(mode, PilotMode::Rtrl2ClipSmall | PilotMode::Rtrl2ClipLarge) {
                let threshold = if mode == PilotMode::Rtrl2ClipSmall {
                    RTRL2_CLIP_SMALL
                } else {
                    RTRL2_CLIP_LARGE
                };
                let (loss, events, stats) = model.train_document_rtrl_clipped(sample, threshold)?;
                clip_stats.accumulate(stats)?;
                (loss, events)
            } else if mode == PilotMode::BoundaryHyb2 {
                model.train_document_hyb2_boundary(sample, causal_window)?
            } else if mode == PilotMode::BoundaryHybrid1 {
                let (loss, events, counts) =
                    model.train_document_hybrid_windowed(sample, causal_window)?;
                accumulate_hybrid(&mut hybrid_counts, counts)?;
                (loss, events)
            } else if mode == PilotMode::BoundaryRtrl {
                model.train_document_rtrl_windowed(sample, causal_window)?
            } else if mode == PilotMode::BoundaryBptt {
                model.train_document_bptt(sample, causal_window)?
            } else if mode == PilotMode::MatchedHybrid {
                let (loss, events, counts) =
                    model.train_document_hybrid_windowed(sample, MATCHED_WINDOW)?;
                accumulate_hybrid(&mut hybrid_counts, counts)?;
                (loss, events)
            } else if matched && bptt {
                model.train_document_bptt(sample, MATCHED_WINDOW)?
            } else if matched {
                model.train_document_rtrl_windowed(sample, MATCHED_WINDOW)?
            } else if bptt {
                model.train_document_bptt(sample, MATCHED_WINDOW)?
            } else {
                model.train_document(sample)?
            };
            if matched {
                append_selection_fingerprint(&mut train_fingerprint, &expected, sample);
            }
            train_docs += 1;
            train_bytes += sample.len() as u64;
            train_events += events;
            train_loss += loss;
        } else if dev_bytes < dev_target {
            let (loss, events) = if let Some(ref state) = rtlr {
                model.evaluate_document_rtlr_elastic_paged(sample, state)?
            } else if let Some(ref state) = rtrl3 {
                model.evaluate_document_rtrl3_paged(sample, state)?
            } else if boundary_mode {
                // Equal readout protocol for all four arms: uninterrupted
                // document recurrence. Updates remain frozen on DEV.
                model.evaluate_document(sample)?
            } else if matched {
                model.evaluate_document_windowed(sample, MATCHED_WINDOW)?
            } else {
                model.evaluate_document(sample)?
            };
            if matched {
                append_selection_fingerprint(&mut dev_fingerprint, &expected, sample);
            }
            dev_docs += 1;
            dev_bytes += sample.len() as u64;
            dev_events += events;
            dev_loss += loss;
        }
    }
    if train_docs == 0 || dev_docs == 0 || train_events < 1000 || dev_events < 1000 {
        return Err("missing finite train/heldout samples".into());
    }
    let counters = model.counters();
    let experts = model.atlas_counters();
    if counters.unique_router_parameters == 0
        || counters.recurrent_parameter_updates == 0
        || experts.distinct_updated_parameters == 0
        || counters.supervised_target_bits != train_events * 8
        || counters.routed_expert_updates != train_events * 16
    {
        return Err("joint gradient pathway not confirmed".into());
    }
    if matches!(mode, PilotMode::MatchedHybrid | PilotMode::BoundaryHybrid1)
        && (hybrid_counts.rtrl_windows == 0
            || hybrid_counts.bptt_windows == 0
            || hybrid_counts.rtrl_target_bits + hybrid_counts.bptt_target_bits != train_events * 8)
    {
        return Err("hybrid did not execute both update methods".into());
    }
    if matches!(mode, PilotMode::Rtrl2ClipSmall | PilotMode::Rtrl2ClipLarge) {
        if clip_stats.predicted_bytes != train_events
            || !clip_stats.max_gradient_norm.is_finite()
            || !clip_stats.sum_gradient_norm.is_finite()
        {
            return Err("invalid RTRL-2 clip metrics".into());
        }
        let mut report = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(output.join("rtrl2_norm.tsv"))?;
        writeln!(
            report,
            "version\tclip_norm\tpredicted_bytes\tclipped_steps\tmax_preclip_norm\tmean_preclip_norm"
        )?;
        writeln!(
            report,
            "RTRL2_CLIPPED_V1\t{}\t{}\t{}\t{:.12}\t{:.12}",
            if mode == PilotMode::Rtrl2ClipSmall {
                RTRL2_CLIP_SMALL
            } else {
                RTRL2_CLIP_LARGE
            },
            clip_stats.predicted_bytes,
            clip_stats.clipped_steps,
            clip_stats.max_gradient_norm,
            clip_stats.sum_gradient_norm / clip_stats.predicted_bytes as f64
        )?;
        report.sync_all()?;
    }
    if let Some(ref state) = rtlr {
        if state.is_v2() {
            model.checkpoint_rtlr_v2(output, state)?;
            // Fail closed unless the actual disk-paged experts, trained MOR
            // gate, active recurrence and dormant bank reopen identically.
            let (mut restored, recurrent) = MorAnfRecurrent::reopen_rtlr_v2(output, config)?;
            if restored.counters() != model.counters()
                || recurrent.stats() != state.stats()
                || recurrent.active() != state.active()
            {
                return Err("RTLR-V2 checkpoint roundtrip counters mismatch".into());
            }
            let probe = b"rtlr-elastic-independent-restored-inference-probe";
            let (original, count) = model.evaluate_document_rtlr_elastic_paged(probe, state)?;
            let (reloaded, count_reloaded) =
                restored.evaluate_document_rtlr_elastic_paged(probe, &recurrent)?;
            if original != reloaded || count != count_reloaded {
                return Err("RTLR-V2 checkpoint inference roundtrip mismatch".into());
            }
        } else {
            model.checkpoint(output)?;
        }
    } else {
        model.checkpoint(output)?;
    }
    if let Some(ref state) = rtlr {
        state.write_evidence(&output.join("rtlr_elastic_weights.tsv"))?;
        let stats = state.stats();
        let mut manifest = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(output.join("rtlr_elastic_stats.tsv"))?;
        writeln!(manifest,
            "version\tdocuments\tgrown\tpruned\tactive\tmax_active\tpeak_active\trecurrent_changes\tgate_changes\tsteps\tactive_payload_bytes"
        )?;
        writeln!(
            manifest,
            "RTLR_ELASTIC_V1\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            stats.training_documents,
            stats.grown,
            stats.pruned,
            state.active(),
            state.capacity(),
            stats.peak_active,
            stats.recurrent_parameter_changes,
            stats.gate_parameter_changes,
            stats.recurrent_steps,
            state.active_payload_bytes()
        )?;
        manifest.sync_all()?;
        if state.is_v2() {
            let mut v2 = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(output.join("rtlr_v2_stats.tsv"))?;
            writeln!(v2, "version\tdocuments\tparks\trestores\tpeak_parked\tshadow_train_docs\tactive\tpeak_active\tmax_active")?;
            writeln!(
                v2,
                "RTLR_ELASTIC_V2\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                stats.training_documents,
                stats.bank_parks,
                stats.bank_restores,
                stats.peak_parked,
                stats.train_shadow_documents,
                state.active(),
                stats.peak_active,
                state.capacity()
            )?;
            v2.sync_all()?;
        }
    }
    if let Some(ref state) = rtrl3 {
        if rtrl3_clipped > 0 {
            return Err("RTRL-3 control unexpectedly clipped gradients".into());
        }
        state.write_mor_weights(&output.join("rtrl3_recurrence.tsv"))?;
    }
    if matches!(mode, PilotMode::MatchedHybrid | PilotMode::BoundaryHybrid1) {
        let mut manifest = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(output.join("hybrid_schedule.tsv"))?;
        writeln!(manifest, "version\trtrl_windows\tbptt_windows\trtrl_target_bits\tbptt_target_bits\twindow\trtrl_per_cycle\tcycle_windows")?;
        writeln!(
            manifest,
            "HYBRID_3R_1B_V1\t{}\t{}\t{}\t{}\t{causal_window}\t3\t4",
            hybrid_counts.rtrl_windows,
            hybrid_counts.bptt_windows,
            hybrid_counts.rtrl_target_bits,
            hybrid_counts.bptt_target_bits
        )?;
        manifest.sync_all()?;
    }
    let architecture = match mode {
        PilotMode::LegacyRtrl => "sml_mor_anf_rtrl_v1",
        PilotMode::LegacyBptt => "sml_mor_anf_paged_bptt_v1",
        PilotMode::MatchedRtrl => "sml_mor_anf_paged_rtrl_matched_v2",
        PilotMode::MatchedBptt => "sml_mor_anf_paged_bptt_matched_v2",
        PilotMode::MatchedHybrid => "sml_mor_anf_paged_hybrid_3r_1b_v1",
        PilotMode::BoundaryRtrl => "sml_mor_anf_boundary_rtrl_v3",
        PilotMode::BoundaryBptt => "sml_mor_anf_boundary_bptt_v3",
        PilotMode::BoundaryHybrid1 => "sml_mor_anf_boundary_hyb1_v3",
        PilotMode::BoundaryHyb2 => "sml_mor_anf_boundary_hyb2_v3",
        PilotMode::Rtrl2Window16 => "sml_mor_anf_rtrl2_reset16_control_v1",
        PilotMode::Rtrl2Continuous => "sml_mor_anf_rtrl2_continuous_control_v1",
        PilotMode::Rtrl2ClipSmall => "sml_mor_anf_rtrl2_continuous_clip0125_v1",
        PilotMode::Rtrl2ClipLarge => "sml_mor_anf_rtrl2_continuous_clip1_v1",
        PilotMode::Rtrl3Paged1 => "sml_mor_anf_rtrl3_paged_h1_v1",
        PilotMode::Rtrl3Paged8 => "sml_mor_anf_rtrl3_paged_h8_v1",
        PilotMode::Rtrl3Paged16 => "sml_mor_anf_rtrl3_paged_h16_v1",
        PilotMode::RtlrElastic1 => "sml_mor_anf_rtlr_elastic_max1_v1",
        PilotMode::RtlrElastic8 => "sml_mor_anf_rtlr_elastic_max8_v1",
        PilotMode::RtlrElastic16 => "sml_mor_anf_rtlr_elastic_max16_v1",
        PilotMode::RtlrV2_1 => "sml_mor_anf_rtlr_v2_max1",
        PilotMode::RtlrV2_8 => "sml_mor_anf_rtlr_v2_max8",
        PilotMode::RtlrV2_16 => "sml_mor_anf_rtlr_v2_max16",
    };
    let recurrent_capacity = rtlr.as_ref().map_or_else(
        || {
            rtrl3
                .as_ref()
                .map_or(3, ElasticRtrl3::recurrent_trainable_parameters)
        },
        RtlrElastic::trained_local_parameters,
    );
    let metrics = format!(
        "architecture\tatlas_parameter_capacity\texpert_parameters_updated\texpert_updates\trouter_parameter_capacity\trouter_parameters_updated\trouter_updates\trecurrent_parameter_capacity\trecurrent_updates\ttrain_docs\ttrain_bytes\ttrain_events\ttrain_bpc\theldout_docs\theldout_bytes\theldout_events\theldout_bpc\tuniform_bpc\ttraining_elapsed_seconds\tg0\ttest\n{architecture}\t{}\t{}\t{}\t1024\t{}\t{}\t{}\t{}\t{train_docs}\t{train_bytes}\t{train_events}\t{:.9}\t{dev_docs}\t{dev_bytes}\t{dev_events}\t{:.9}\t8.000000000\t{:.4}\tNOT_ADMITTED\tUNOPENED\n",
        config.total_parameters(), experts.distinct_updated_parameters, experts.parameter_updates,
        counters.unique_router_parameters, counters.router_parameter_updates,
        recurrent_capacity, counters.recurrent_parameter_updates,
        train_loss / train_events as f64,
        dev_loss / dev_events as f64,
        start.elapsed().as_secs_f64()
    );
    let mut report = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output.join("mor_anf_pilot.tsv"))?;
    report.write_all(metrics.as_bytes())?;
    report.sync_all()?;
    print!("{metrics}");
    if matched {
        let mut selection = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(output.join("paired_selection.tsv"))?;
        writeln!(
            selection,
            "version\ttrain_docs\ttrain_bytes\ttrain_events\ttrain_fingerprint\theldout_docs\theldout_bytes\theldout_events\theldout_fingerprint\twindow"
        )?;
        writeln!(
            selection,
            "{}\t{train_docs}\t{train_bytes}\t{train_events}\t{}\t{dev_docs}\t{dev_bytes}\t{dev_events}\t{}\t{causal_window}",
            if boundary_mode { "BOUNDARY_V3" } else { "PAIRED_V2" },
            hex(&train_fingerprint.finish()),
            hex(&dev_fingerprint.finish())
        )?;
        selection.sync_all()?;
    }
    let marker = match mode {
        PilotMode::LegacyRtrl => "SML_MOR_ANF_RTRL_DONE",
        PilotMode::LegacyBptt => "SML_MOR_ANF_BPTT_DONE",
        PilotMode::MatchedRtrl => "SML_MOR_ANF_MATCHED_RTRL_DONE",
        PilotMode::MatchedBptt => "SML_MOR_ANF_MATCHED_BPTT_DONE",
        PilotMode::MatchedHybrid => "SML_MOR_ANF_MATCHED_HYBRID_DONE",
        PilotMode::BoundaryRtrl => "SML_MOR_ANF_BOUNDARY_RTRL_DONE",
        PilotMode::BoundaryBptt => "SML_MOR_ANF_BOUNDARY_BPTT_DONE",
        PilotMode::BoundaryHybrid1 => "SML_MOR_ANF_BOUNDARY_HYB1_DONE",
        PilotMode::BoundaryHyb2 => "SML_MOR_ANF_BOUNDARY_HYB2_DONE",
        PilotMode::Rtrl2Window16 => "SML_MOR_ANF_RTRL2_RESET16_DONE",
        PilotMode::Rtrl2Continuous => "SML_MOR_ANF_RTRL2_CONTINUOUS_DONE",
        PilotMode::Rtrl2ClipSmall => "SML_MOR_ANF_RTRL2_CLIP0125_DONE",
        PilotMode::Rtrl2ClipLarge => "SML_MOR_ANF_RTRL2_CLIP1_DONE",
        PilotMode::Rtrl3Paged1 => "SML_MOR_ANF_RTRL3_PAGED_H1_DONE",
        PilotMode::Rtrl3Paged8 => "SML_MOR_ANF_RTRL3_PAGED_H8_DONE",
        PilotMode::Rtrl3Paged16 => "SML_MOR_ANF_RTRL3_PAGED_H16_DONE",
        PilotMode::RtlrElastic1 => "SML_MOR_ANF_RTLR_ELASTIC_1_DONE",
        PilotMode::RtlrElastic8 => "SML_MOR_ANF_RTLR_ELASTIC_8_DONE",
        PilotMode::RtlrElastic16 => "SML_MOR_ANF_RTLR_ELASTIC_16_DONE",
        PilotMode::RtlrV2_1 => "SML_MOR_ANF_RTLR_V2_1_DONE",
        PilotMode::RtlrV2_8 => "SML_MOR_ANF_RTLR_V2_8_DONE",
        PilotMode::RtlrV2_16 => "SML_MOR_ANF_RTLR_V2_16_DONE",
    };
    println!("{marker} source_sha256={digest} trainable_expert_capacity={} distinct_expert_weights={} router_updated={} recurrent_updates={} test=UNOPENED g0=NOT_ADMITTED",
        config.total_parameters(), experts.distinct_updated_parameters,
        counters.unique_router_parameters, counters.recurrent_parameter_updates);
    Ok(())
}

/// Ordered SHA commitment to the exact consumed prefixes, with no SEC text
/// in the artifact. Both arms must have identical fingerprints.
fn append_selection_fingerprint(hasher: &mut Sha256, source: &[u8; 32], sample: &[u8]) {
    let mut prefix_hash = Sha256::default();
    prefix_hash.update(sample);
    hasher.update(source);
    hasher.update(&prefix_hash.finish());
    hasher.update(&(sample.len() as u64).to_le_bytes());
}

fn accumulate_hybrid(total: &mut HybridWindowCounts, add: HybridWindowCounts) -> Result<()> {
    total.rtrl_windows = total
        .rtrl_windows
        .checked_add(add.rtrl_windows)
        .ok_or("hybrid RTRL window count overflow")?;
    total.bptt_windows = total
        .bptt_windows
        .checked_add(add.bptt_windows)
        .ok_or("hybrid BPTT window count overflow")?;
    total.rtrl_target_bits = total
        .rtrl_target_bits
        .checked_add(add.rtrl_target_bits)
        .ok_or("hybrid RTRL target count overflow")?;
    total.bptt_target_bits = total
        .bptt_target_bits
        .checked_add(add.bptt_target_bits)
        .ok_or("hybrid BPTT target count overflow")?;
    Ok(())
}

fn main() {
    let args: Vec<_> = env::args().collect();
    let result = match args.as_slice() {
        [_, flag, corpus, ledger, digest, output] if flag == "--pilot" => execute(
            Path::new(corpus),
            Path::new(ledger),
            digest,
            Path::new(output),
            PilotMode::LegacyRtrl,
        ),
        [_, flag, corpus, ledger, digest, output] if flag == "--pilot-bptt" => execute(
            Path::new(corpus),
            Path::new(ledger),
            digest,
            Path::new(output),
            PilotMode::LegacyBptt,
        ),
        [_, flag, corpus, ledger, digest, output] if flag == "--pilot-paired-rtrl" => execute(
            Path::new(corpus),
            Path::new(ledger),
            digest,
            Path::new(output),
            PilotMode::MatchedRtrl,
        ),
        [_, flag, corpus, ledger, digest, output] if flag == "--pilot-paired-bptt" => execute(
            Path::new(corpus),
            Path::new(ledger),
            digest,
            Path::new(output),
            PilotMode::MatchedBptt,
        ),
        [_, flag, corpus, ledger, digest, output] if flag == "--pilot-paired-hybrid" => execute(
            Path::new(corpus),
            Path::new(ledger),
            digest,
            Path::new(output),
            PilotMode::MatchedHybrid,
        ),
        [_, flag, corpus, ledger, digest, output] if flag == "--pilot-boundary-rtrl" => execute(
            Path::new(corpus), Path::new(ledger), digest, Path::new(output),
            PilotMode::BoundaryRtrl,
        ),
        [_, flag, corpus, ledger, digest, output] if flag == "--pilot-boundary-bptt" => execute(
            Path::new(corpus), Path::new(ledger), digest, Path::new(output),
            PilotMode::BoundaryBptt,
        ),
        [_, flag, corpus, ledger, digest, output] if flag == "--pilot-boundary-hyb1" => execute(
            Path::new(corpus), Path::new(ledger), digest, Path::new(output),
            PilotMode::BoundaryHybrid1,
        ),
        [_, flag, corpus, ledger, digest, output] if flag == "--pilot-boundary-hyb2" => execute(
            Path::new(corpus), Path::new(ledger), digest, Path::new(output),
            PilotMode::BoundaryHyb2,
        ),
        [_, flag, corpus, ledger, digest, output] if flag == "--pilot-rtrl2-reset16" => execute(
            Path::new(corpus), Path::new(ledger), digest, Path::new(output),
            PilotMode::Rtrl2Window16,
        ),
        [_, flag, corpus, ledger, digest, output] if flag == "--pilot-rtrl2-continuous" => execute(
            Path::new(corpus), Path::new(ledger), digest, Path::new(output),
            PilotMode::Rtrl2Continuous,
        ),
        [_, flag, corpus, ledger, digest, output] if flag == "--pilot-rtrl2-clip0125" => execute(
            Path::new(corpus), Path::new(ledger), digest, Path::new(output),
            PilotMode::Rtrl2ClipSmall,
        ),
        [_, flag, corpus, ledger, digest, output] if flag == "--pilot-rtrl2-clip1" => execute(
            Path::new(corpus), Path::new(ledger), digest, Path::new(output),
            PilotMode::Rtrl2ClipLarge,
        ),
        [_, flag, corpus, ledger, digest, output] if flag == "--pilot-rtrl3-paged1" => execute(
            Path::new(corpus), Path::new(ledger), digest, Path::new(output),
            PilotMode::Rtrl3Paged1,
        ),
        [_, flag, corpus, ledger, digest, output] if flag == "--pilot-rtrl3-paged8" => execute(
            Path::new(corpus), Path::new(ledger), digest, Path::new(output),
            PilotMode::Rtrl3Paged8,
        ),
        [_, flag, corpus, ledger, digest, output] if flag == "--pilot-rtrl3-paged16" => execute(
            Path::new(corpus), Path::new(ledger), digest, Path::new(output),
            PilotMode::Rtrl3Paged16,
        ),
        [_, flag, corpus, ledger, digest, output] if flag == "--pilot-rtlr-elastic1" => execute(
            Path::new(corpus), Path::new(ledger), digest, Path::new(output),
            PilotMode::RtlrElastic1,
        ),
        [_, flag, corpus, ledger, digest, output] if flag == "--pilot-rtlr-elastic8" => execute(
            Path::new(corpus), Path::new(ledger), digest, Path::new(output),
            PilotMode::RtlrElastic8,
        ),
        [_, flag, corpus, ledger, digest, output] if flag == "--pilot-rtlr-elastic16" => execute(
            Path::new(corpus), Path::new(ledger), digest, Path::new(output),
            PilotMode::RtlrElastic16,
        ),
        [_, flag, corpus, ledger, digest, output] if flag == "--pilot-rtlr-v2-1" => execute(
            Path::new(corpus), Path::new(ledger), digest, Path::new(output),
            PilotMode::RtlrV2_1,
        ),
        [_, flag, corpus, ledger, digest, output] if flag == "--pilot-rtlr-v2-8" => execute(
            Path::new(corpus), Path::new(ledger), digest, Path::new(output),
            PilotMode::RtlrV2_8,
        ),
        [_, flag, corpus, ledger, digest, output] if flag == "--pilot-rtlr-v2-16" => execute(
            Path::new(corpus), Path::new(ledger), digest, Path::new(output),
            PilotMode::RtlrV2_16,
        ),
        _ => Err(
            "usage: sml-mor-anf-rtrl-fit (--pilot|--pilot-bptt|--pilot-paired-rtrl|--pilot-paired-bptt|--pilot-paired-hybrid|--pilot-boundary-rtrl|--pilot-boundary-bptt|--pilot-boundary-hyb1|--pilot-boundary-hyb2|--pilot-rtrl2-reset16|--pilot-rtrl2-continuous|--pilot-rtrl2-clip0125|--pilot-rtrl2-clip1|--pilot-rtrl3-paged1|--pilot-rtrl3-paged8|--pilot-rtrl3-paged16|--pilot-rtlr-elastic1|--pilot-rtlr-elastic8|--pilot-rtlr-elastic16|--pilot-rtlr-v2-1|--pilot-rtlr-v2-8|--pilot-rtlr-v2-16) CORPUS provenance SOURCE_SHA256 NEW_OUTPUT_DIR"
                .into(),
        ),
    };
    if let Err(error) = result {
        eprintln!("SML_MOR_ANF_RTRL_FAIL {error}");
        std::process::exit(1);
    }
}
