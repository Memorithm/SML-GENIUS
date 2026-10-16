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
