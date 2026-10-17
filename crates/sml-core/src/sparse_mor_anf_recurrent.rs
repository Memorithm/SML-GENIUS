//! Trainable sparse Mixture-of-Routers + GF(2) ANF recurrent head.
//!
//! Two paged experts are *actually* evaluated and updated per bit, with an
//! analytic differentiable mixture gate. The recurrent scalar carries causal
//! state across bytes (reset at each document) and receives forward-mode
//! recurrent eligibility gradients. This is a small scientific bridge to SML's
//! ANF/MOR architecture, NOT a 25B language model, and not a discrete 1-bit
//! ANF weight learner. Monomials are fixed; gate/readout weights are learned.
use crate::{
    elastic_ngram_paged_sha::{hex, Sha256},
    sparse_gradient_atlas::{SparseConfig, SparseGradientAtlas},
};
use std::{
    error::Error,
    fs::{self, File, OpenOptions},
    io::Write,
    path::Path,
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const BUCKETS: usize = 256;
const GATE_FEATURES: usize = 4;
const VERSION: &str = "SML_MOR_ANF_RTRL_V1";
const ROUTER_LR: f64 = 0.04;
const RECURRENT_LR: f64 = 0.005;

fn mix(mut x: u64) -> u64 {
    x ^= x >> 30;
    x = x.wrapping_mul(0xbf58476d1ce4e5b9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94d049bb133111eb);
    x ^ (x >> 31)
}

fn sig(x: f64) -> f64 {
    1.0 / (1.0 + (-x.clamp(-16.0, 16.0)).exp())
}

/// GF(2) algebraic normal form:
/// x0*y0 XOR x1*y1 XOR ... XOR x7*y7, genuinely nonlinear over GF(2).
fn anf_monomial(a: u8, b: u8) -> f64 {
    if (a & b).count_ones() & 1 == 0 {
        -1.0
    } else {
        1.0
    }
}

fn signal(b: u8) -> f64 {
    (f64::from(b.count_ones()) - 4.0) / 4.0
}

/// Differentiable recurrent proxy with *online forward-mode* eligibility.
/// The trace is exact for the current frozen-parameter recurrence. Updating
/// parameters between timesteps makes it an online RTRL approximation.
#[derive(Debug, Default, Clone, Copy)]
struct Trace {
    h: f64,
    dh: [f64; 3],
}

impl Trace {
    fn advance(self, a: u8, b: u8, weights: &[f64; 3]) -> Self {
        let x = signal(b);
        let nonlinear = anf_monomial(a, b);
        let h = (weights[0] * self.h + weights[1] * x + weights[2] * nonlinear).tanh();
        let d = 1.0 - h * h;
        Self {
            h,
            dh: [
                d * (self.h + weights[0] * self.dh[0]),
                d * (x + weights[0] * self.dh[1]),
                d * (nonlinear + weights[0] * self.dh[2]),
            ],
        }
    }
}

/// Per-document diagnostics for RTRL-2. A bound on the global Euclidean
/// norm of the *three-dimensional recurrent update gradient* is separate
/// from the MOR gate and sparse expert optimizer. No dense 50M Jacobian.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct RtrlClipStats {
    pub predicted_bytes: u64,
    pub clipped_steps: u64,
    pub max_gradient_norm: f64,
    pub sum_gradient_norm: f64,
}

impl RtrlClipStats {
    pub fn accumulate(&mut self, other: Self) -> Result<()> {
        self.predicted_bytes = self
            .predicted_bytes
            .checked_add(other.predicted_bytes)
            .ok_or("RTRL-2 count overflow")?;
        self.clipped_steps = self
            .clipped_steps
            .checked_add(other.clipped_steps)
            .ok_or("RTRL-2 clipped-count overflow")?;
        self.max_gradient_norm = self.max_gradient_norm.max(other.max_gradient_norm);
        self.sum_gradient_norm += other.sum_gradient_norm;
        if !self.sum_gradient_norm.is_finite() {
            return Err("nonfinite RTRL-2 diagnostics".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MorAnfCounters {
    pub supervised_target_bits: u64,
    pub routed_expert_updates: u64,
    pub router_parameter_updates: u64,
    pub unique_router_parameters: u64,
    pub recurrent_parameter_updates: u64,
    pub recurrence_steps: u64,
}

#[derive(Debug, Clone)]
struct Head {
    gate: [[f64; GATE_FEATURES]; BUCKETS],
    touched: [u8; BUCKETS],
    recurrence: [f64; 3],
    counters: MorAnfCounters,
}

impl Default for Head {
    fn default() -> Self {
        let mut gate = [[0f64; GATE_FEATURES]; BUCKETS];
        for row in &mut gate {
            row[2] = 0.25;
        }
        Self {
            gate,
            touched: [0u8; BUCKETS],
            recurrence: [0.65, 0.3, 0.2],
            counters: MorAnfCounters::default(),
        }
    }
}

struct MixturePrediction {
    p: f64,
    gate: f64,
    p0: f64,
    p1: f64,
    bucket: usize,
    features: [f64; GATE_FEATURES],
    first: u64,
    second: u64,
}

pub struct MorAnfRecurrent {
    atlas: SparseGradientAtlas,
    head: Head,
}

fn hash_file(path: &Path) -> Result<String> {
    let data = fs::read(path)?;
    let mut digest = Sha256::default();
    digest.update(&data);
    Ok(hex(&digest.finish()))
}

impl MorAnfRecurrent {
    pub fn create(directory: &Path, config: SparseConfig) -> Result<Self> {
        if config.pages < 2 {
            return Err("MOR needs at least two pages".into());
        }
        Ok(Self {
            atlas: SparseGradientAtlas::create(directory, config)?,
            head: Head::default(),
        })
    }

    pub fn counters(&self) -> MorAnfCounters {
        self.head.counters
    }

    pub fn atlas_counters(&self) -> crate::sparse_gradient_atlas::SparseCounters {
        self.atlas.counters()
    }

    fn candidates(&self, a: u8, b: u8, bit: usize) -> (u64, u64, usize) {
        let addr = (u64::from(a) << 16) | (u64::from(b) << 8) | bit as u64;
        let cap = self.atlas.config().pages;
        let first = mix(addr ^ 0x414e462d4d4f522d) % cap;
        let second = (first + 1 + mix(addr ^ 0x726f757465727632) % (cap - 1)) % cap;
        let bucket = (mix(addr ^ 0x9b27bd4f69af3a19) as usize) % BUCKETS;
        (first, second, bucket)
    }

    fn prediction(&mut self, a: u8, b: u8, bit: usize, trace: Trace) -> Result<MixturePrediction> {
        let (first, second, bucket) = self.candidates(a, b, bit);
        let nonlinear = anf_monomial(a, b);
        let feature = [
            1.0,
            nonlinear,
            trace.h,
            if bit & 1 == 0 { -1.0 } else { 1.0 },
        ];
        let theta = &self.head.gate[bucket];
        let logit = theta.iter().zip(feature).map(|(w, f)| w * f).sum::<f64>();
        let gate = sig(logit);
        let p0 = self.atlas.probability_on_page(first, a, b, bit)?;
        let p1 = self.atlas.probability_on_page(second, a, b, bit)?;
        let p = gate * p0 + (1.0 - gate) * p1;
        if !p.is_finite() || !(0.0..1.0).contains(&p) {
            return Err("invalid MOR mixture probability".into());
        }
        Ok(MixturePrediction {
            p,
            gate,
            p0,
            p1,
            bucket,
            features: feature,
            first,
            second,
        })
    }

    fn train_target(
        &mut self,
        a: u8,
        b: u8,
        bit: usize,
        label: bool,
        trace: Trace,
    ) -> Result<(f64, f64)> {
        let MixturePrediction {
            p,
            gate,
            p0,
            p1,
            bucket,
            features,
            first,
            second,
        } = self.prediction(a, b, bit, trace)?;
        let y = f64::from(u8::from(label));
        let loss = if label { -p.log2() } else { -(1.0 - p).log2() };
        // Gradients are for nats rather than bits, differing by ln(2) scale.
        let dl_dp = (p - y) / (p * (1.0 - p));
        let expert0 = dl_dp * gate * p0 * (1.0 - p0) / 4.0;
        let expert1 = dl_dp * (1.0 - gate) * p1 * (1.0 - p1) / 4.0;
        let dgate = dl_dp * (p0 - p1) * gate * (1.0 - gate);
        if !loss.is_finite() || !dgate.is_finite() {
            return Err("nonfinite MOR gradient".into());
        }
        // Snapshot prior recurrent readout coefficient for chain-rule credit.
        let recurrence_credit = dgate * self.head.gate[bucket][2];
        self.atlas
            .apply_expert_gradient(first, a, b, bit, expert0)?;
        self.atlas
            .apply_expert_gradient(second, a, b, bit, expert1)?;
        for (i, feature) in features.into_iter().enumerate() {
            let before = self.head.gate[bucket][i];
            let after = (before - ROUTER_LR * dgate * feature).clamp(-4.0, 4.0);
            if !after.is_finite() {
                return Err("nonfinite MOR parameter".into());
            }
            if after != before {
                if self.head.touched[bucket] & (1 << i) == 0 {
                    self.head.touched[bucket] |= 1 << i;
                    self.head.counters.unique_router_parameters += 1;
                }
                self.head.counters.router_parameter_updates += 1;
            }
            self.head.gate[bucket][i] = after;
        }
        self.head.counters.supervised_target_bits += 1;
        self.head.counters.routed_expert_updates += 2;
        Ok((loss, recurrence_credit))
    }

    pub fn train_document(&mut self, text: &[u8]) -> Result<(f64, u64)> {
        let mut trace = Trace::default();
        let mut loss = 0f64;
        let mut events = 0u64;
        for triple in text.windows(3) {
            trace = trace.advance(triple[0], triple[1], &self.head.recurrence);
            let mut recurrent_derivative = 0f64;
            for bit in 0..8 {
                let (cost, credit) = self.train_target(
                    triple[0],
                    triple[1],
                    bit,
                    (triple[2] >> bit) & 1 == 1,
                    trace,
                )?;
                loss += cost;
                recurrent_derivative += credit;
            }
            // Online RTRL eligibility for a three-parameter recurrent cell.
            // Gradient does not cross parameter updates at previous timesteps.
            for (index, sensitivity) in trace.dh.into_iter().enumerate() {
                let before = self.head.recurrence[index];
                let after =
                    (before - RECURRENT_LR * recurrent_derivative * sensitivity).clamp(-2.0, 2.0);
                if !after.is_finite() {
                    return Err("nonfinite recurrent parameter".into());
                }
                if after != before {
                    self.head.counters.recurrent_parameter_updates += 1;
                }
                self.head.recurrence[index] = after;
            }
            self.head.counters.recurrence_steps += 1;
            events += 1;
        }
        if !loss.is_finite() {
            return Err("nonfinite training document loss".into());
        }
        Ok((loss, events))
    }

    /// RTRL-2: online recurrent eligibility over the **whole source document**
    /// (without 16/128-position resets) and a bounded Euclidean update norm.
    /// Exact for a frozen recurrent cell; with online weight updates this
    /// remains a causal eligibility approximation, not backprop through those
    /// updates. The original RTRL, gate learning and paged experts are unchanged.
    ///
    /// The norm bound is applied only to the recurrent triple's gradient, NOT
    /// to expert or MOR gate gradients. 'clip_norm' must be finite and positive.
    pub fn train_document_rtrl_clipped(
        &mut self,
        text: &[u8],
        clip_norm: f64,
    ) -> Result<(f64, u64, RtrlClipStats)> {
        if text.len() < 3 || !clip_norm.is_finite() || clip_norm <= 0.0 {
            return Err("invalid RTRL-2 document or positive gradient norm bound".into());
        }
        let mut trace = Trace::default();
        let mut loss = 0.0f64;
        let mut stats = RtrlClipStats::default();
        for triple in text.windows(3) {
            trace = trace.advance(triple[0], triple[1], &self.head.recurrence);
            let mut recurrent_derivative = 0.0f64;
            for bit in 0..8 {
                let (cost, credit) = self.train_target(
                    triple[0],
                    triple[1],
                    bit,
                    (triple[2] >> bit) & 1 == 1,
                    trace,
                )?;
                loss += cost;
                recurrent_derivative += credit;
            }
            let gradient = trace
                .dh
                .map(|sensitivity| recurrent_derivative * sensitivity);
            // hypot-style scaling prevents squaring unbounded gradients.
            let max_component = gradient
                .iter()
                .fold(0.0f64, |max, value| max.max(value.abs()));
            if !max_component.is_finite() {
                return Err("nonfinite RTRL-2 recurrent gradient".into());
            }
            let norm = if max_component == 0.0 {
                0.0
            } else {
                max_component
                    * (gradient
                        .iter()
                        .map(|value| (value / max_component).powi(2))
                        .sum::<f64>())
                    .sqrt()
            };
            if !norm.is_finite() {
                return Err("nonfinite RTRL-2 recurrent norm".into());
            }
            let scale = if norm > clip_norm {
                stats.clipped_steps += 1;
                clip_norm / norm
            } else {
                1.0
            };
            stats.max_gradient_norm = stats.max_gradient_norm.max(norm);
            stats.sum_gradient_norm += norm;
            for (index, &value) in gradient.iter().enumerate() {
                let before = self.head.recurrence[index];
                // Preserve the original left-associative SGD expression
                // exactly when clipping is inactive (bitwise control).
                let after = if scale == 1.0 {
                    (before - RECURRENT_LR * recurrent_derivative * trace.dh[index])
                        .clamp(-2.0, 2.0)
                } else {
                    (before - RECURRENT_LR * value * scale).clamp(-2.0, 2.0)
                };
                if !after.is_finite() {
                    return Err("nonfinite RTRL-2 recurrent parameter".into());
                }
                if after != before {
                    self.head.counters.recurrent_parameter_updates += 1;
                }
                self.head.recurrence[index] = after;
            }
            self.head.counters.recurrence_steps += 1;
            stats.predicted_bytes += 1;
        }
        if !loss.is_finite() || !stats.sum_gradient_norm.is_finite() {
            return Err("nonfinite RTRL-2 total loss or norm sum".into());
        }
        Ok((loss, stats.predicted_bytes, stats))
    }

    pub fn evaluate_document(&mut self, text: &[u8]) -> Result<(f64, u64)> {
        let mut trace = Trace::default();
        let mut loss = 0f64;
        let mut events = 0u64;
        for triple in text.windows(3) {
            trace = trace.advance(triple[0], triple[1], &self.head.recurrence);
            for bit in 0..8 {
                let p = self.prediction(triple[0], triple[1], bit, trace)?.p;
                loss += if triple[2] & (1 << bit) != 0 {
                    -p.log2()
                } else {
                    -(1.0 - p).log2()
                };
            }
            events += 1;
        }
        if !loss.is_finite() {
            return Err("nonfinite evaluation loss".into());
        }
        Ok((loss, events))
    }

    fn encode_head(&self) -> String {
        let c = self.head.counters;
        let mut out = format!(
            "{VERSION}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
            self.atlas.config().pages,
            self.atlas.config().max_cached_pages,
            self.atlas.config().learning_rate.to_bits(),
            c.supervised_target_bits,
            c.routed_expert_updates,
            c.router_parameter_updates,
            c.unique_router_parameters,
            c.recurrent_parameter_updates,
            c.recurrence_steps,
            ROUTER_LR.to_bits()
        );
        out.push_str(&format!(
            "R\t{:016x}\t{:016x}\t{:016x}\n",
            self.head.recurrence[0].to_bits(),
            self.head.recurrence[1].to_bits(),
            self.head.recurrence[2].to_bits()
        ));
        for (i, row) in self.head.gate.iter().enumerate() {
            out.push_str(&format!(
                "{i}\t{:016x}\t{:016x}\t{:016x}\t{:016x}\t{}\n",
                row[0].to_bits(),
                row[1].to_bits(),
                row[2].to_bits(),
                row[3].to_bits(),
                self.head.touched[i]
            ));
        }
        out
    }

    /// Paired SHA ledger binds router/recurrence weights to exact atlas
    /// snapshot. A mid-write interruption invalidates rather than resuming.
    pub fn checkpoint(&mut self, root: &Path) -> Result<()> {
        self.atlas.checkpoint(root)?;
        let mut head = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(root.join("mor_anf_head.tsv"))?;
        head.write_all(self.encode_head().as_bytes())?;
        head.sync_all()?;
        let weights_sha = hash_file(&root.join("mor_anf_head.tsv"))?;
        let atlas_sha = hash_file(&root.join("snapshot.tsv"))?;
        let mut manifest = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(root.join("joint.sha256.tsv"))?;
        writeln!(manifest, "{VERSION}\t{atlas_sha}\t{weights_sha}")?;
        manifest.sync_all()?;
        File::open(root)?.sync_all()?;
        Ok(())
    }

    pub fn reopen(root: &Path, config: SparseConfig) -> Result<Self> {
        if config.pages < 2 {
            return Err("MOR needs at least two pages".into());
        }
        let evidence = fs::read_to_string(root.join("joint.sha256.tsv"))?;
        let signature: Vec<_> = evidence.trim_end().split('\t').collect();
        if signature.len() != 3
            || signature[0] != VERSION
            || signature[1] != hash_file(&root.join("snapshot.tsv"))?
            || signature[2] != hash_file(&root.join("mor_anf_head.tsv"))?
        {
            return Err("router/atlas joint SHA mismatch".into());
        }
        let atlas = SparseGradientAtlas::reopen(root, config)?;
        let text = fs::read_to_string(root.join("mor_anf_head.tsv"))?;
        let mut lines = text.lines();
        let headline = lines.next().ok_or("missing MOR header")?;
        let h: Vec<_> = headline.split('\t').collect();
        if h.len() != 11
            || h[0] != VERSION
            || h[1].parse::<u64>()? != config.pages
            || h[2].parse::<usize>()? != config.max_cached_pages
            || h[3].parse::<u32>()? != config.learning_rate.to_bits()
            || h[10].parse::<u64>()? != ROUTER_LR.to_bits()
        {
            return Err("MOR head config mismatch".into());
        }
        let mut head = Head {
            counters: MorAnfCounters {
                supervised_target_bits: h[4].parse()?,
                routed_expert_updates: h[5].parse()?,
                router_parameter_updates: h[6].parse()?,
                unique_router_parameters: h[7].parse()?,
                recurrent_parameter_updates: h[8].parse()?,
                recurrence_steps: h[9].parse()?,
            },
            ..Head::default()
        };
        let recurrence = lines.next().ok_or("missing RTRL weights")?;
        let r: Vec<_> = recurrence.split('\t').collect();
        if r.len() != 4 || r[0] != "R" {
            return Err("bad RTRL record".into());
        }
        for (dest, val) in head.recurrence.iter_mut().zip(&r[1..]) {
            *dest = f64::from_bits(u64::from_str_radix(val, 16)?);
            if !dest.is_finite() || dest.abs() > 2.0 {
                return Err("bad RTRL weight".into());
            }
        }
        let mut unique = 0u64;
        for (idx, row) in head.gate.iter_mut().enumerate() {
            let record = lines.next().ok_or("short MOR head")?;
            let f: Vec<_> = record.split('\t').collect();
            if f.len() != 6 || f[0].parse::<usize>()? != idx {
                return Err("bad MOR bucket index".into());
            }
            for (dest, val) in row.iter_mut().zip(&f[1..5]) {
                *dest = f64::from_bits(u64::from_str_radix(val, 16)?);
                if !dest.is_finite() || dest.abs() > 4.0 {
                    return Err("bad MOR weight".into());
                }
            }
            head.touched[idx] = f[5].parse()?;
            if head.touched[idx] >= 16 {
                return Err("bad router touch mask".into());
            }
            unique += u64::from(head.touched[idx].count_ones());
        }
        if lines.next().is_some()
            || unique != head.counters.unique_router_parameters
            || head.counters.routed_expert_updates != head.counters.supervised_target_bits * 2
            || head.counters.supervised_target_bits != head.counters.recurrence_steps * 8
        {
            return Err("router accounting mismatch".into());
        }
        Ok(Self { atlas, head })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};
    fn path(tag: &str) -> std::path::PathBuf {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("sml-mor-anf-{tag}-{}-{now}", std::process::id()))
    }
    fn config() -> SparseConfig {
        SparseConfig {
            pages: 32,
            max_cached_pages: 2,
            learning_rate: 0.25,
        }
    }

    #[test]
    fn anf_is_nonlinear_and_recurrence_has_correct_eligibility() {
        assert_eq!(anf_monomial(0, 0), -1.0);
        assert_eq!(anf_monomial(1, 1), 1.0);
        assert_eq!(anf_monomial(3, 3), -1.0);
        let params = [0.62, 0.31, 0.17];
        let mut trace = Trace::default();
        let seq = [(0x11, 0x93), (0x9a, 0xf0), (0x12, 0x32), (0x07, 0x14)];
        for (a, b) in seq {
            trace = trace.advance(a, b, &params);
        }
        for k in 0..3 {
            let mut perturbed = params;
            perturbed[k] += 1e-5;
            let mut probe = Trace::default();
            for (a, b) in seq {
                probe = probe.advance(a, b, &perturbed);
            }
            let numeric = (probe.h - trace.h) / 1e-5;
            assert!((trace.dh[k] - numeric).abs() < 2e-5, "parameter {k}");
        }
    }

    #[test]
    fn mor_anf_recurrent_learns_and_checkpoint_is_immutable() {
        let root = path("fit");
        let mut train = MorAnfRecurrent::create(&root, config()).unwrap();
        let doc: Vec<u8> = (0..300usize)
            .map(|n| if n % 17 < 9 { b'a' } else { b'b' })
            .collect();
        let (before, n) = train.evaluate_document(&doc).unwrap();
        assert!((before / n as f64 - 8.0).abs() < 1e-10);
        for _ in 0..3 {
            train.train_document(&doc).unwrap();
        }
        let (after, count) = train.evaluate_document(&doc).unwrap();
        assert_eq!(n, count);
        assert!(after < before * 0.8, "before {before} after {after}");
        let m = train.counters();
        assert_eq!(m.supervised_target_bits, 3 * (300 - 2) * 8);
        assert_eq!(m.routed_expert_updates, 2 * m.supervised_target_bits);
        assert!(m.unique_router_parameters > 0);
        assert!(m.recurrent_parameter_updates > 0);
        assert!(train.atlas_counters().distinct_updated_parameters > 0);
        let expert_count = train.atlas_counters().parameter_updates;
        let router_count = train.counters();
        train.checkpoint(&root).unwrap();
        let mut frozen = MorAnfRecurrent::reopen(&root, config()).unwrap();
        let (holdout, _) = frozen.evaluate_document(&doc).unwrap();
        assert!((holdout - after).abs() < 1e-9);
        assert_eq!(frozen.atlas_counters().parameter_updates, expert_count);
        assert_eq!(frozen.counters(), router_count);
        // Read-only evaluation must not update parameters.
        frozen.evaluate_document(&doc).unwrap();
        assert_eq!(frozen.counters(), router_count);
        let mut corrupted = fs::read(root.join("mor_anf_head.tsv")).unwrap();
        corrupted[70] ^= 1;
        fs::write(root.join("mor_anf_head.tsv"), corrupted).unwrap();
        assert!(MorAnfRecurrent::reopen(&root, config()).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn two_candidates_differ_and_invalid_capacity_is_rejected() {
        let root = path("candidates");
        assert!(MorAnfRecurrent::create(
            &root,
            SparseConfig {
                pages: 1,
                ..config()
            }
        )
        .is_err());
        let mut model = MorAnfRecurrent::create(&root, config()).unwrap();
        for a in 0..10u8 {
            let (first, second, _) = model.candidates(a, 123, 7);
            assert_ne!(first, second);
            assert!(first < 32 && second < 32);
        }
        // A 1-byte document cannot reveal a future target.
        assert_eq!(model.train_document(b"A").unwrap().1, 0);
        assert_eq!(model.evaluate_document(b"A").unwrap().1, 0);
        fs::remove_dir_all(root).unwrap();
    }
}

/// Maximum number of consecutive predicted bytes retained for an exact
/// frozen-weight backward pass. Long documents use truncated BPTT windows.
pub const PAGED_BPTT_MAX_BYTES: usize = 512;

fn validate_paged_window(text: &[u8], window_bytes: usize) -> Result<()> {
    if text.len() < 3 || !(1..=PAGED_BPTT_MAX_BYTES).contains(&window_bytes) {
        return Err("invalid document or bounded recurrent window".into());
    }
    Ok(())
}

/// Pre-registered policy for the first mixed online / frozen-window pilot.
/// Three RTRL windows consolidate fast, one BPTT window consolidates a frozen
/// local history; no label is optimized twice and the split is data-independent.
pub const HYBRID_RTRL_WINDOWS: u64 = 3;
pub const HYBRID_CYCLE_WINDOWS: u64 = 4;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HybridWindowCounts {
    pub rtrl_windows: u64,
    pub bptt_windows: u64,
    pub rtrl_target_bits: u64,
    pub bptt_target_bits: u64,
}

struct PagedBpttFrame {
    a: u8,
    b: u8,
    target: u8,
    previous_h: f64,
    h: f64,
    predictions: Vec<MixturePrediction>,
}

impl MorAnfRecurrent {
    /// Matched control for the bounded BPTT experiment: the established online
    /// RTRL update, reset at exactly the same window boundaries. The two arms
    /// see the same supervised byte positions and 2-byte causal contexts.
    pub fn train_document_rtrl_windowed(
        &mut self,
        text: &[u8],
        window_bytes: usize,
    ) -> Result<(f64, u64)> {
        validate_paged_window(text, window_bytes)?;
        let mut total_loss = 0.0;
        let mut total_events = 0u64;
        for begin in (0..text.len() - 2).step_by(window_bytes) {
            let end = begin.saturating_add(window_bytes).min(text.len() - 2);
            let (loss, events) = self.train_document(&text[begin..end + 2])?;
            total_loss += loss;
            total_events = total_events
                .checked_add(events)
                .ok_or("RTRL event count overflow")?;
        }
        if !total_loss.is_finite() {
            return Err("nonfinite windowed RTRL loss".into());
        }
        Ok((total_loss, total_events))
    }

    /// Mixed optimizer schedule: 3 successive online-RTRL windows followed by
    /// 1 exact truncated-BPTT window, all with identical causal 2-byte input
    /// context and recurrent reset boundaries. No example is replayed.
    ///
    /// This is a mixed *update schedule*, NOT a sum of RTRL+BPTT derivatives
    /// for the same target. It deliberately does not use heldout loss to choose
    /// the mix. Experts, gate and recurrence are one shared physical model.
    pub fn train_document_hybrid_windowed(
        &mut self,
        text: &[u8],
        window_bytes: usize,
    ) -> Result<(f64, u64, HybridWindowCounts)> {
        validate_paged_window(text, window_bytes)?;
        let mut loss_total = 0.0;
        let mut events_total = 0u64;
        let mut counts = HybridWindowCounts::default();
        for (index, begin) in (0..text.len() - 2).step_by(window_bytes).enumerate() {
            let end = begin.saturating_add(window_bytes).min(text.len() - 2);
            let context = &text[begin..end + 2];
            let do_bptt = u64::try_from(index)? % HYBRID_CYCLE_WINDOWS == HYBRID_CYCLE_WINDOWS - 1;
            let (loss, events) = if do_bptt {
                self.train_document_bptt(context, window_bytes)?
            } else {
                self.train_document(context)?
            };
            if !loss.is_finite() || events != u64::try_from(end - begin)? {
                return Err("invalid hybrid window exposure or loss".into());
            }
            let targets = events.checked_mul(8).ok_or("hybrid target overflow")?;
            if do_bptt {
                counts.bptt_windows = counts
                    .bptt_windows
                    .checked_add(1)
                    .ok_or("hybrid BPTT windows overflow")?;
                counts.bptt_target_bits = counts
                    .bptt_target_bits
                    .checked_add(targets)
                    .ok_or("hybrid BPTT targets overflow")?;
            } else {
                counts.rtrl_windows = counts
                    .rtrl_windows
                    .checked_add(1)
                    .ok_or("hybrid RTRL windows overflow")?;
                counts.rtrl_target_bits = counts
                    .rtrl_target_bits
                    .checked_add(targets)
                    .ok_or("hybrid RTRL targets overflow")?;
            }
            events_total = events_total
                .checked_add(events)
                .ok_or("hybrid events overflow")?;
            loss_total += loss;
        }
        if !loss_total.is_finite() || events_total != u64::try_from(text.len() - 2)? {
            return Err("invalid hybrid document aggregate".into());
        }
        Ok((loss_total, events_total, counts))
    }

    /// Read-only heldout evaluator with the **same recurrent reset boundaries**
    /// for both online RTRL and truncated BPTT, without changing learned
    /// parameters. Only the read cache's page-load counters may advance.
    pub fn evaluate_document_windowed(
        &mut self,
        text: &[u8],
        window_bytes: usize,
    ) -> Result<(f64, u64)> {
        validate_paged_window(text, window_bytes)?;
        let mut total_loss = 0.0;
        let mut total_events = 0u64;
        for begin in (0..text.len() - 2).step_by(window_bytes) {
            let end = begin.saturating_add(window_bytes).min(text.len() - 2);
            let (loss, events) = self.evaluate_document(&text[begin..end + 2])?;
            total_loss += loss;
            total_events = total_events
                .checked_add(events)
                .ok_or("heldout event count overflow")?;
        }
        if !total_loss.is_finite() {
            return Err("nonfinite windowed heldout loss".into());
        }
        Ok((total_loss, total_events))
    }

    /// Train the *existing physical paged experts* and learned MOR/RTRL head
    /// under a single truncated sequence BPTT objective. Every window first
    /// freezes both expert predictions and every recurrent state, then computes
    /// the full backward pass before applying ANY parameter updates.
    ///
    /// The address-to-candidate mapping and ANF monomial topology remain fixed.
    /// No claim of learned discrete routing, 25B training, or chat readiness.
    /// The recurrent state resets at every document AND truncation boundary.
    /// The result is summed binary cross-entropy in bits and predicted bytes.
    pub fn train_document_bptt(&mut self, text: &[u8], window_bytes: usize) -> Result<(f64, u64)> {
        if text.len() < 3 || !(1..=PAGED_BPTT_MAX_BYTES).contains(&window_bytes) {
            return Err("invalid BPTT document length or bounded window".into());
        }
        let mut total_loss = 0.0;
        let mut total_events = 0u64;
        let predicted_bytes = text.len() - 2;
        for begin in (0..predicted_bytes).step_by(window_bytes) {
            let end = begin.saturating_add(window_bytes).min(predicted_bytes);
            let mut trace = Trace::default();
            let mut frames = Vec::with_capacity(end - begin);

            // Frozen-parameter forward pass: neither head nor atlas learns yet.
            for position in begin..end {
                let a = text[position];
                let b = text[position + 1];
                let previous_h = trace.h;
                trace = trace.advance(a, b, &self.head.recurrence);
                let mut predictions = Vec::with_capacity(8);
                for bit in 0..8 {
                    predictions.push(self.prediction(a, b, bit, trace)?);
                }
                frames.push(PagedBpttFrame {
                    a,
                    b,
                    target: text[position + 2],
                    previous_h,
                    h: trace.h,
                    predictions,
                });
            }

            let mut gate_grad = [[0.0f64; GATE_FEATURES]; BUCKETS];
            let mut recurrent_grad = [0.0f64; 3];
            // All sparse gradients refer to the same frozen atlas snapshot.
            let mut expert_grad = Vec::with_capacity(frames.len() * 16);
            let mut next_state_credit = 0.0f64;
            let mut window_loss_nats = 0.0f64;

            for frame in frames.iter().rev() {
                let mut state_credit = next_state_credit;
                for (bit, pred) in frame.predictions.iter().enumerate() {
                    let y = f64::from((frame.target >> bit) & 1);
                    let p = pred.p.clamp(1e-12, 1.0 - 1e-12);
                    window_loss_nats -= y * p.ln() + (1.0 - y) * (1.0 - p).ln();
                    let dl_dp = (p - y) / (p * (1.0 - p));
                    let dl_dgate = dl_dp * (pred.p0 - pred.p1) * pred.gate * (1.0 - pred.gate);
                    let dl_dexpert0 = dl_dp * pred.gate * pred.p0 * (1.0 - pred.p0) / 4.0;
                    let dl_dexpert1 = dl_dp * (1.0 - pred.gate) * pred.p1 * (1.0 - pred.p1) / 4.0;

                    for (feature, &value) in pred.features.iter().enumerate() {
                        gate_grad[pred.bucket][feature] += dl_dgate * value;
                    }
                    // The gate is the sole path from expert loss to recurrent
                    // state. Its hidden coefficient is read before any update.
                    state_credit += dl_dgate * self.head.gate[pred.bucket][2];
                    expert_grad.push((pred.first, frame.a, frame.b, bit, dl_dexpert0));
                    expert_grad.push((pred.second, frame.a, frame.b, bit, dl_dexpert1));
                }

                let dz = state_credit * (1.0 - frame.h * frame.h);
                recurrent_grad[0] += dz * frame.previous_h;
                recurrent_grad[1] += dz * signal(frame.b);
                recurrent_grad[2] += dz * anf_monomial(frame.a, frame.b);
                next_state_credit = dz * self.head.recurrence[0];
            }

            let denom = (frames.len() * 8) as f64;
            if !window_loss_nats.is_finite()
                || !recurrent_grad.iter().all(|gradient| gradient.is_finite())
                || !gate_grad
                    .iter()
                    .flatten()
                    .all(|gradient| gradient.is_finite())
                || !expert_grad.iter().all(|entry| {
                    let normalized = entry.4 / denom;
                    normalized.is_finite() && normalized.abs() <= 32.0
                })
            {
                return Err("nonfinite or unbounded BPTT gradient".into());
            }

            // Preflight overflow before touching any trainable parameter.
            let steps = u64::try_from(frames.len())?;
            let targets = steps.checked_mul(8).ok_or("BPTT target overflow")?;
            let updated_targets = self
                .head
                .counters
                .supervised_target_bits
                .checked_add(targets)
                .ok_or("BPTT supervised count overflow")?;
            let updated_experts = self
                .head
                .counters
                .routed_expert_updates
                .checked_add(targets.checked_mul(2).ok_or("BPTT expert count overflow")?)
                .ok_or("BPTT expert count overflow")?;
            let updated_steps = self
                .head
                .counters
                .recurrence_steps
                .checked_add(steps)
                .ok_or("BPTT recurrence count overflow")?;

            // Physical disk-page updates, not synthetic capacity accounting.
            for (page, a, b, bit, derivative) in expert_grad {
                self.atlas
                    .apply_expert_gradient(page, a, b, bit, derivative / denom)?;
            }
            for (bucket, derivatives) in gate_grad.iter().enumerate() {
                for (feature, &derivative) in derivatives.iter().enumerate() {
                    let before = self.head.gate[bucket][feature];
                    let after = (before - ROUTER_LR * derivative / denom).clamp(-4.0, 4.0);
                    if after != before {
                        let mask = 1u8 << feature;
                        if self.head.touched[bucket] & mask == 0 {
                            self.head.touched[bucket] |= mask;
                            self.head.counters.unique_router_parameters += 1;
                        }
                        self.head.counters.router_parameter_updates += 1;
                    }
                    self.head.gate[bucket][feature] = after;
                }
            }
            for (index, &derivative) in recurrent_grad.iter().enumerate() {
                let before = self.head.recurrence[index];
                let after = (before - RECURRENT_LR * derivative / denom).clamp(-2.0, 2.0);
                if after != before {
                    self.head.counters.recurrent_parameter_updates += 1;
                }
                self.head.recurrence[index] = after;
            }
            self.head.counters.supervised_target_bits = updated_targets;
            self.head.counters.routed_expert_updates = updated_experts;
            self.head.counters.recurrence_steps = updated_steps;
            total_loss += window_loss_nats / std::f64::consts::LN_2;
            total_events = total_events
                .checked_add(steps)
                .ok_or("BPTT document event overflow")?;
        }
        if !total_loss.is_finite() {
            return Err("nonfinite BPTT total loss".into());
        }
        Ok((total_loss, total_events))
    }
    /// HYB-2: reverse-mode adjoints within a bounded block, plus forward-mode
    /// RTRL sensitivities carried across blocks for the block-entry state.
    /// The boundary term is (dL/dh_entry) * (dh_entry/dtheta) and is added
    /// exactly once to the locally unrolled recurrent-parameter gradient.
    ///
    /// Frozen-parameter derivatives match full BPTT; the online optimizer
    /// below intentionally changes weights per window, so inherited
    /// sensitivities across updates are *approximate*, not an exact gradient
    /// of the evolving-parameter training trajectory. Gates and atlas experts
    /// use the existing frozen-window gradients and actual disk page updates.
    /// Unlike HYB-1, recurrence state is NOT reset at window boundaries.
    pub fn train_document_hyb2_boundary(
        &mut self,
        text: &[u8],
        window_bytes: usize,
    ) -> Result<(f64, u64)> {
        if text.len() < 3 || !(1..=PAGED_BPTT_MAX_BYTES).contains(&window_bytes) {
            return Err("invalid HYB2 document length or bounded window".into());
        }
        // Boundary state and forward sensitivities survive truncation. They
        // are exact for a frozen model. With per-block weight updates these
        // sensitivities are eligibility approximations (not an exact replay).
        let mut boundary = Trace::default();
        let mut total_loss = 0.0;
        let mut total_events = 0u64;
        let predicted_bytes = text.len() - 2;
        for begin in (0..predicted_bytes).step_by(window_bytes) {
            let end = begin.saturating_add(window_bytes).min(predicted_bytes);
            let mut trace = boundary;
            let mut frames = Vec::with_capacity(end - begin);

            // Frozen-parameter forward pass: neither head nor atlas learns yet.
            for position in begin..end {
                let a = text[position];
                let b = text[position + 1];
                let previous_h = trace.h;
                trace = trace.advance(a, b, &self.head.recurrence);
                let mut predictions = Vec::with_capacity(8);
                for bit in 0..8 {
                    predictions.push(self.prediction(a, b, bit, trace)?);
                }
                frames.push(PagedBpttFrame {
                    a,
                    b,
                    target: text[position + 2],
                    previous_h,
                    h: trace.h,
                    predictions,
                });
            }

            let mut gate_grad = [[0.0f64; GATE_FEATURES]; BUCKETS];
            let mut recurrent_grad = [0.0f64; 3];
            // All sparse gradients refer to the same frozen atlas snapshot.
            let mut expert_grad = Vec::with_capacity(frames.len() * 16);
            let mut next_state_credit = 0.0f64;
            let mut window_loss_nats = 0.0f64;

            for frame in frames.iter().rev() {
                let mut state_credit = next_state_credit;
                for (bit, pred) in frame.predictions.iter().enumerate() {
                    let y = f64::from((frame.target >> bit) & 1);
                    let p = pred.p.clamp(1e-12, 1.0 - 1e-12);
                    window_loss_nats -= y * p.ln() + (1.0 - y) * (1.0 - p).ln();
                    let dl_dp = (p - y) / (p * (1.0 - p));
                    let dl_dgate = dl_dp * (pred.p0 - pred.p1) * pred.gate * (1.0 - pred.gate);
                    let dl_dexpert0 = dl_dp * pred.gate * pred.p0 * (1.0 - pred.p0) / 4.0;
                    let dl_dexpert1 = dl_dp * (1.0 - pred.gate) * pred.p1 * (1.0 - pred.p1) / 4.0;

                    for (feature, &value) in pred.features.iter().enumerate() {
                        gate_grad[pred.bucket][feature] += dl_dgate * value;
                    }
                    // The gate is the sole path from expert loss to recurrent
                    // state. Its hidden coefficient is read before any update.
                    state_credit += dl_dgate * self.head.gate[pred.bucket][2];
                    expert_grad.push((pred.first, frame.a, frame.b, bit, dl_dexpert0));
                    expert_grad.push((pred.second, frame.a, frame.b, bit, dl_dexpert1));
                }

                let dz = state_credit * (1.0 - frame.h * frame.h);
                recurrent_grad[0] += dz * frame.previous_h;
                recurrent_grad[1] += dz * signal(frame.b);
                recurrent_grad[2] += dz * anf_monomial(frame.a, frame.b);
                next_state_credit = dz * self.head.recurrence[0];
            }

            // Reverse-mode adjoint of the block's entry state times its
            // inherited RTRL forward-mode Jacobian: credit for *all* history
            // before the current truncated block, exactly once.
            for (gradient, sensitivity) in recurrent_grad.iter_mut().zip(boundary.dh) {
                *gradient += next_state_credit * sensitivity;
            }
            // Advance both causal state and eligibility, without replaying
            // source bytes. Weight changes after this window make later
            // inherited eligibilities an online approximation.
            boundary = trace;
            let denom = (frames.len() * 8) as f64;
            if !window_loss_nats.is_finite()
                || !recurrent_grad.iter().all(|gradient| gradient.is_finite())
                || !gate_grad
                    .iter()
                    .flatten()
                    .all(|gradient| gradient.is_finite())
                || !expert_grad.iter().all(|entry| {
                    let normalized = entry.4 / denom;
                    normalized.is_finite() && normalized.abs() <= 32.0
                })
            {
                return Err("nonfinite or unbounded BPTT gradient".into());
            }

            // Preflight overflow before touching any trainable parameter.
            let steps = u64::try_from(frames.len())?;
            let targets = steps.checked_mul(8).ok_or("BPTT target overflow")?;
            let updated_targets = self
                .head
                .counters
                .supervised_target_bits
                .checked_add(targets)
                .ok_or("BPTT supervised count overflow")?;
            let updated_experts = self
                .head
                .counters
                .routed_expert_updates
                .checked_add(targets.checked_mul(2).ok_or("BPTT expert count overflow")?)
                .ok_or("BPTT expert count overflow")?;
            let updated_steps = self
                .head
                .counters
                .recurrence_steps
                .checked_add(steps)
                .ok_or("BPTT recurrence count overflow")?;

            // Physical disk-page updates, not synthetic capacity accounting.
            for (page, a, b, bit, derivative) in expert_grad {
                self.atlas
                    .apply_expert_gradient(page, a, b, bit, derivative / denom)?;
            }
            for (bucket, derivatives) in gate_grad.iter().enumerate() {
                for (feature, &derivative) in derivatives.iter().enumerate() {
                    let before = self.head.gate[bucket][feature];
                    let after = (before - ROUTER_LR * derivative / denom).clamp(-4.0, 4.0);
                    if after != before {
                        let mask = 1u8 << feature;
                        if self.head.touched[bucket] & mask == 0 {
                            self.head.touched[bucket] |= mask;
                            self.head.counters.unique_router_parameters += 1;
                        }
                        self.head.counters.router_parameter_updates += 1;
                    }
                    self.head.gate[bucket][feature] = after;
                }
            }
            for (index, &derivative) in recurrent_grad.iter().enumerate() {
                let before = self.head.recurrence[index];
                let after = (before - RECURRENT_LR * derivative / denom).clamp(-2.0, 2.0);
                if after != before {
                    self.head.counters.recurrent_parameter_updates += 1;
                }
                self.head.recurrence[index] = after;
            }
            self.head.counters.supervised_target_bits = updated_targets;
            self.head.counters.routed_expert_updates = updated_experts;
            self.head.counters.recurrence_steps = updated_steps;
            total_loss += window_loss_nats / std::f64::consts::LN_2;
            total_events = total_events
                .checked_add(steps)
                .ok_or("BPTT document event overflow")?;
        }
        if !total_loss.is_finite() {
            return Err("nonfinite HYB2 total loss".into());
        }
        Ok((total_loss, total_events))
    }
}

#[cfg(test)]
mod paged_bptt_tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn model() -> (MorAnfRecurrent, std::path::PathBuf) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("sml-paged-bptt-{}-{now}", std::process::id()));
        let model = MorAnfRecurrent::create(
            &root,
            SparseConfig {
                pages: 32,
                max_cached_pages: 3,
                learning_rate: 0.25,
            },
        )
        .unwrap();
        (model, root)
    }

    #[test]
    fn trainable_paged_experts_and_bptt_head_improve_supervised_loss() {
        let (mut model, root) = model();
        let document: Vec<u8> = (0..98)
            .map(|i| if i % 5 < 3 { b'a' } else { b'b' })
            .collect();
        let (before, events) = model.evaluate_document(&document).unwrap();
        assert_eq!(events, 96);
        for _ in 0..16 {
            let (_, trained_events) = model.train_document_bptt(&document, 32).unwrap();
            assert_eq!(trained_events, events);
        }
        let (after, _) = model.evaluate_document(&document).unwrap();
        assert!(after < before, "before={before} after={after}");
        let head = model.counters();
        let atlas = model.atlas_counters();
        assert_eq!(head.supervised_target_bits, 16 * 96 * 8);
        assert_eq!(head.routed_expert_updates, 2 * head.supervised_target_bits);
        assert_eq!(head.recurrence_steps, 16 * 96);
        assert!(head.router_parameter_updates > 0);
        assert!(head.recurrent_parameter_updates > 0);
        assert!(atlas.distinct_updated_parameters > 0);
        assert_eq!(atlas.supervised_bits, head.routed_expert_updates);
        drop(model);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn recurrent_bptt_gradient_matches_finite_difference_on_paged_experts() {
        let (mut model, root) = model();
        let warmup = b"abcddcbaabcddcbaabcddcba";
        for _ in 0..8 {
            model.train_document_bptt(warmup, 32).unwrap();
        }
        let probe = b"abacabadabacaba";
        let epsilon = 1e-5;
        let original = model.head.recurrence[1];

        model.head.recurrence[1] = original + epsilon;
        let (plus, events) = model.evaluate_document(probe).unwrap();
        model.head.recurrence[1] = original - epsilon;
        let (minus, _) = model.evaluate_document(probe).unwrap();
        model.head.recurrence[1] = original;

        let numerical =
            (plus - minus) / (2.0 * epsilon) * std::f64::consts::LN_2 / (events * 8) as f64;
        model.train_document_bptt(probe, 512).unwrap();
        let analytical = (original - model.head.recurrence[1]) / RECURRENT_LR;
        assert!(
            (analytical - numerical).abs() < 1e-6,
            "analytical={analytical} numerical={numerical}"
        );
        drop(model);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rtrl2_unclipped_matches_original_continuous_rtrl_byte_for_byte() {
        let (mut old, old_dir) = model();
        let (mut evolved, new_dir) = model();
        let sample = b"independent sample for exact online continuous RTRL comparison";
        let (old_loss, old_events) = old.train_document(sample).unwrap();
        let (new_loss, new_events, stats) =
            evolved.train_document_rtrl_clipped(sample, 1e30).unwrap();
        assert!((old_loss - new_loss).abs() < 1e-12);
        assert_eq!(old_events, new_events);
        assert_eq!(stats.predicted_bytes, old_events);
        assert_eq!(stats.clipped_steps, 0);
        assert_eq!(old.counters(), evolved.counters());
        assert_eq!(old.encode_head(), evolved.encode_head());
        let (old_eval, _) = old.evaluate_document(sample).unwrap();
        let (new_eval, _) = evolved.evaluate_document(sample).unwrap();
        assert!((old_eval - new_eval).abs() < 1e-12);
        drop(old);
        drop(evolved);
        fs::remove_dir_all(old_dir).unwrap();
        fs::remove_dir_all(new_dir).unwrap();
    }

    #[test]
    fn rtrl2_aggressive_clip_bounds_online_recurrent_gradient() {
        let (mut clipped, root) = model();
        let sample = b"abababaabbacababcccaccbbbaaaacabbbabababaaaaaabbb";
        let (loss, events, stats) = clipped.train_document_rtrl_clipped(sample, 1e-12).unwrap();
        assert!(loss.is_finite());
        assert_eq!(events, (sample.len() - 2) as u64);
        assert!(stats.clipped_steps > 0);
        assert!(stats.max_gradient_norm > 1e-12);
        assert!(stats.sum_gradient_norm.is_finite());
        assert_eq!(clipped.counters().supervised_target_bits, events * 8);
        assert_eq!(clipped.counters().routed_expert_updates, events * 16);
        assert!(clipped.atlas_counters().distinct_updated_parameters > 0);
        let before = clipped.counters();
        let (_, read_events) = clipped.evaluate_document(sample).unwrap();
        assert_eq!(read_events, events);
        assert_eq!(before, clipped.counters());
        drop(clipped);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rtrl2_rejects_invalid_norms_without_mutating_parameters() {
        let (mut model, root) = model();
        for norm in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            assert!(model.train_document_rtrl_clipped(b"abcd", norm).is_err());
        }
        assert!(model.train_document_rtrl_clipped(b"ab", 1.0).is_err());
        assert_eq!(model.counters(), MorAnfCounters::default());
        assert_eq!(model.atlas_counters().distinct_updated_parameters, 0);
        drop(model);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn matched_windowed_rtrl_has_identical_contexts_and_exact_event_counts() {
        let (mut grouped, root_a) = model();
        let (mut manual, root_b) = model();
        let text = b"twelve sample bytes of causal recurrent testing";
        let (group_loss, grouped_events) = grouped.train_document_rtrl_windowed(text, 7).unwrap();
        let mut manual_loss = 0.0;
        let mut manual_events = 0;
        for begin in (0..text.len() - 2).step_by(7) {
            let end = (begin + 7).min(text.len() - 2);
            let (loss, events) = manual.train_document(&text[begin..end + 2]).unwrap();
            manual_loss += loss;
            manual_events += events;
        }
        assert_eq!(grouped_events, (text.len() - 2) as u64);
        assert_eq!(manual_events, grouped_events);
        assert!((manual_loss - group_loss).abs() < 1e-10);
        assert_eq!(grouped.counters(), manual.counters());
        assert_eq!(
            grouped.atlas_counters().distinct_updated_parameters,
            manual.atlas_counters().distinct_updated_parameters
        );
        let before = grouped.counters();
        let atlas_before = grouped.atlas_counters().distinct_updated_parameters;
        let (evaluation, events) = grouped.evaluate_document_windowed(text, 7).unwrap();
        assert!(evaluation.is_finite());
        assert_eq!(events, grouped_events);
        assert_eq!(grouped.counters(), before);
        assert_eq!(
            grouped.atlas_counters().distinct_updated_parameters,
            atlas_before
        );
        assert!(grouped.train_document_rtrl_windowed(text, 0).is_err());
        assert!(grouped
            .evaluate_document_windowed(text, PAGED_BPTT_MAX_BYTES + 1)
            .is_err());
        drop(grouped);
        drop(manual);
        fs::remove_dir_all(root_a).unwrap();
        fs::remove_dir_all(root_b).unwrap();
    }

    #[test]
    fn matched_bptt_and_rtrl_expose_equal_targets_for_same_windows() {
        let (mut bptt, root_a) = model();
        let (mut rtrl, root_b) = model();
        let text = b"abababccababbcbcabababcabc";
        let (_, bptt_count) = bptt.train_document_bptt(text, 8).unwrap();
        let (_, rtrl_count) = rtrl.train_document_rtrl_windowed(text, 8).unwrap();
        assert_eq!(bptt_count, rtrl_count);
        assert_eq!(bptt_count, (text.len() - 2) as u64);
        assert_eq!(
            bptt.counters().supervised_target_bits,
            rtrl.counters().supervised_target_bits
        );
        assert_eq!(
            bptt.counters().routed_expert_updates,
            rtrl.counters().routed_expert_updates
        );
        drop(bptt);
        drop(rtrl);
        fs::remove_dir_all(root_a).unwrap();
        fs::remove_dir_all(root_b).unwrap();
    }

    #[test]
    fn hybrid_schedule_matches_explicit_online_bptt_sequence() {
        let (mut hybrid, first) = model();
        let (mut explicit, second) = model();
        let text = b"abaabbacacababbccbcabcabcbbacabaaacabbbbcabaaabbbcc";
        let bytes_per_window = 8;
        let (loss, events, counts) = hybrid
            .train_document_hybrid_windowed(text, bytes_per_window)
            .unwrap();
        let mut control_loss = 0.0;
        let mut control_events = 0u64;
        for (index, begin) in (0..text.len() - 2).step_by(bytes_per_window).enumerate() {
            let end = (begin + bytes_per_window).min(text.len() - 2);
            let slice = &text[begin..end + 2];
            let (local_loss, n) =
                if index % HYBRID_CYCLE_WINDOWS as usize == HYBRID_CYCLE_WINDOWS as usize - 1 {
                    explicit
                        .train_document_bptt(slice, bytes_per_window)
                        .unwrap()
                } else {
                    explicit.train_document(slice).unwrap()
                };
            control_loss += local_loss;
            control_events += n;
        }
        assert_eq!(events, control_events);
        assert_eq!(events, (text.len() - 2) as u64);
        assert!((loss - control_loss).abs() < 1e-9);
        assert_eq!(
            counts.rtrl_target_bits + counts.bptt_target_bits,
            events * 8
        );
        assert!(counts.rtrl_windows > 0 && counts.bptt_windows > 0);
        assert_eq!(hybrid.counters(), explicit.counters());
        assert_eq!(
            hybrid.atlas_counters().distinct_updated_parameters,
            explicit.atlas_counters().distinct_updated_parameters
        );
        assert!(hybrid.atlas_counters().distinct_updated_parameters > 0);
        let previous = hybrid.counters();
        let (_, holdout_events) = hybrid
            .evaluate_document_windowed(text, bytes_per_window)
            .unwrap();
        assert_eq!(holdout_events, events);
        assert_eq!(hybrid.counters(), previous);
        drop(hybrid);
        drop(explicit);
        fs::remove_dir_all(first).unwrap();
        fs::remove_dir_all(second).unwrap();
    }

    #[test]
    fn hybrid_rejects_invalid_windows_without_parameter_updates() {
        let (mut model, root) = model();
        assert!(model.train_document_hybrid_windowed(b"ab", 8).is_err());
        assert!(model.train_document_hybrid_windowed(b"abcd", 0).is_err());
        assert!(model
            .train_document_hybrid_windowed(b"abcd", PAGED_BPTT_MAX_BYTES + 1)
            .is_err());
        assert_eq!(model.counters(), MorAnfCounters::default());
        assert_eq!(model.atlas_counters().distinct_updated_parameters, 0);
        drop(model);
        fs::remove_dir_all(root).unwrap();
    }

    // Test the *frozen-parameter mathematical identity* independently of the
    // online atlas optimizer, which changes parameters between blocks.
    fn toy_target(index: usize) -> f64 {
        if index % 7 < 3 {
            0.36
        } else {
            -0.19
        }
    }

    fn toy_data() -> Vec<(u8, u8)> {
        (0..48)
            .map(|i| (((i * 17 + 31) % 256) as u8, ((i * 11 + 79) % 256) as u8))
            .collect()
    }

    fn toy_loss(weights: &[f64; 3], data: &[(u8, u8)]) -> f64 {
        let mut trace = Trace::default();
        let mut total = 0.0;
        for (index, &(a, b)) in data.iter().enumerate() {
            trace = trace.advance(a, b, weights);
            let difference = trace.h - toy_target(index);
            total += difference * difference / 2.0;
        }
        total
    }

    fn toy_reference_full_bptt(weights: &[f64; 3], data: &[(u8, u8)]) -> [f64; 3] {
        let mut trace = Trace::default();
        let mut frames = Vec::new();
        for (index, &(a, b)) in data.iter().enumerate() {
            let previous = trace.h;
            trace = trace.advance(a, b, weights);
            frames.push((previous, trace.h, a, b, toy_target(index)));
        }
        let mut gradient = [0.0; 3];
        let mut credit = 0.0;
        for &(previous, h, a, b, target) in frames.iter().rev() {
            let dz = (h - target + credit) * (1.0 - h * h);
            gradient[0] += dz * previous;
            gradient[1] += dz * signal(b);
            gradient[2] += dz * anf_monomial(a, b);
            credit = dz * weights[0];
        }
        gradient
    }

    fn toy_hyb2_gradient(
        weights: &[f64; 3],
        data: &[(u8, u8)],
        block: usize,
        use_boundary_credit: bool,
    ) -> [f64; 3] {
        let mut boundary = Trace::default();
        let mut gradient = [0.0; 3];
        for (chunk_index, chunk) in data.chunks(block).enumerate() {
            let entry = boundary;
            let mut frames = Vec::with_capacity(chunk.len());
            for (index, &(a, b)) in chunk.iter().enumerate() {
                let previous = boundary.h;
                boundary = boundary.advance(a, b, weights);
                frames.push((
                    previous,
                    boundary.h,
                    a,
                    b,
                    toy_target(chunk_index * block + index),
                ));
            }
            let mut credit = 0.0;
            for &(previous, h, a, b, target) in frames.iter().rev() {
                let dz = (h - target + credit) * (1.0 - h * h);
                gradient[0] += dz * previous;
                gradient[1] += dz * signal(b);
                gradient[2] += dz * anf_monomial(a, b);
                credit = dz * weights[0];
            }
            if use_boundary_credit {
                for (gradient_value, sensitivity) in gradient.iter_mut().zip(entry.dh) {
                    *gradient_value += credit * sensitivity;
                }
            }
        }
        gradient
    }

    #[test]
    fn hyb2_frozen_boundary_gradients_equal_full_bptt_rtrl_and_finite_differences() {
        let weights = [0.93, 0.03, 0.02];
        let data = toy_data();
        let full = toy_reference_full_bptt(&weights, &data);
        let hybrid = toy_hyb2_gradient(&weights, &data, 5, true);
        let truncated = toy_hyb2_gradient(&weights, &data, 5, false);
        let mut missing_boundary_norm = 0.0;
        for k in 0..3 {
            let numerical = {
                let mut high = weights;
                let mut low = weights;
                high[k] += 1e-6;
                low[k] -= 1e-6;
                (toy_loss(&high, &data) - toy_loss(&low, &data)) / 2e-6
            };
            assert!((hybrid[k] - full[k]).abs() < 1e-9);
            assert!((hybrid[k] - numerical).abs() < 1e-6);
            // Exact online RTRL sensitivity at frozen parameters is the
            // same total gradient for the readout loss.
            let mut trace = Trace::default();
            let mut forward = 0.0;
            for (index, &(a, b)) in data.iter().enumerate() {
                trace = trace.advance(a, b, &weights);
                forward += (trace.h - toy_target(index)) * trace.dh[k];
            }
            assert!((hybrid[k] - forward).abs() < 1e-9);
            missing_boundary_norm += (truncated[k] - full[k]).abs();
        }
        assert!(missing_boundary_norm > 0.01, "boundary credit must matter");
    }

    #[test]
    fn hyb2_physical_paged_training_updates_without_double_counting() {
        let (mut model, root) = model();
        let document: Vec<u8> = (0..74)
            .map(|i| if i % 4 == 0 { b'a' } else { b'b' })
            .collect();
        let (loss, events) = model.train_document_hyb2_boundary(&document, 5).unwrap();
        assert!(loss.is_finite() && loss > 0.0);
        assert_eq!(events, (document.len() - 2) as u64);
        assert_eq!(model.counters().supervised_target_bits, events * 8);
        assert_eq!(model.counters().routed_expert_updates, events * 16);
        assert_eq!(model.counters().recurrence_steps, events);
        assert!(model.counters().recurrent_parameter_updates > 0);
        assert!(model.atlas_counters().distinct_updated_parameters > 0);
        assert!(model.train_document_hyb2_boundary(b"ab", 5).is_err());
        assert!(model.train_document_hyb2_boundary(b"abc", 513).is_err());
        let before = model.counters();
        let (_, count) = model.evaluate_document(&document).unwrap();
        assert_eq!(count, events);
        assert_eq!(model.counters(), before);
        drop(model);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn bptt_rejects_unbounded_windows_without_mutation() {
        let (mut model, root) = model();
        let before = model.counters();
        assert!(model.train_document_bptt(b"ab", 8).is_err());
        assert!(model.train_document_bptt(b"abc", 0).is_err());
        assert!(model
            .train_document_bptt(b"abcd", PAGED_BPTT_MAX_BYTES + 1)
            .is_err());
        assert_eq!(before, model.counters());
        assert_eq!(model.atlas_counters().distinct_updated_parameters, 0);
        drop(model);
        fs::remove_dir_all(root).unwrap();
    }
}
