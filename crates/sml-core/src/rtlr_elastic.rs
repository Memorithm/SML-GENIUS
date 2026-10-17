//! RTLR-ELASTIC: demand-allocated, uncoupled recurrent cells with exact local
//! forward eligibility (RTRL) and a learned bounded residual readout.
//!
//! This is a research algorithm: independent cells are exact w.r.t. their own
//! local parameters for frozen weights; they do not implement full cross-cell
//! RTRL. Structural decisions use TRAIN loss only, at document boundaries.
//! The original paged MOR/ANF experts remain completely outside this module.
use crate::elastic_ngram_paged_sha::{hex, Sha256};
use std::{
    error::Error,
    fs,
    io::Write,
    path::{Path, PathBuf},
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const LOCAL: usize = 3;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RtlrElasticConfig {
    pub max_active: usize,
    pub learning_rate: f64,
    pub grow_above_bpc: f64,
    pub shrink_below_bpc: f64,
    pub patience: u64,
    pub warmup_documents: u64,
    pub seed: u64,
}

impl RtlrElasticConfig {
    pub fn validate(self) -> Result<Self> {
        if !(1..=64).contains(&self.max_active)
            || !self.learning_rate.is_finite()
            || !(0.0..=0.05).contains(&self.learning_rate)
            || self.learning_rate == 0.0
            || !self.grow_above_bpc.is_finite()
            || !self.shrink_below_bpc.is_finite()
            || self.shrink_below_bpc < 0.0
            || self.shrink_below_bpc >= self.grow_above_bpc
            || self.grow_above_bpc > 8.0
            || !(1..=64).contains(&self.patience)
            || self.warmup_documents > 1024
            || self.seed == 0
        {
            return Err("invalid RTLR-ELASTIC bounded training policy".into());
        }
        Ok(self)
    }
}

#[derive(Clone, Debug)]
struct Cell {
    weights: [f64; LOCAL],
    h: f64,
    sensitivity: [f64; LOCAL],
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RtlrElasticStats {
    pub training_documents: u64,
    pub grown: u64,
    pub pruned: u64,
    pub peak_active: usize,
    pub recurrent_parameter_changes: u64,
    pub gate_parameter_changes: u64,
    pub recurrent_steps: u64,
    pub bank_parks: u64,
    pub bank_restores: u64,
    pub peak_parked: usize,
    pub train_shadow_documents: u64,
}

/// TRAIN-only V2 width scheduler. All rates are predetermined, not tuned on
/// the already inspected SEC DEV set. Costs are in bits/byte/cell.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RtlrV2Policy {
    pub ewma_alpha: f64,
    pub plateau_tolerance_bpc: f64,
    pub marginal_cost_bpc_per_cell: f64,
}

impl RtlrV2Policy {
    pub fn validate(self) -> Result<Self> {
        if !self.ewma_alpha.is_finite()
            || !(0.0..=1.0).contains(&self.ewma_alpha)
            || self.ewma_alpha == 0.0
            || !self.plateau_tolerance_bpc.is_finite()
            || !(0.0..=1.0).contains(&self.plateau_tolerance_bpc)
            || !self.marginal_cost_bpc_per_cell.is_finite()
            || !(0.0..=0.5).contains(&self.marginal_cost_bpc_per_cell)
        {
            return Err("invalid RTLR-V2 TRAIN-only marginal policy".into());
        }
        Ok(self)
    }
}

#[derive(Clone, Debug)]
struct RtlrV2State {
    policy: RtlrV2Policy,
    bank: PathBuf,
    train_ewma: Option<f64>,
}

#[derive(Clone, Debug)]
pub struct RtlrElastic {
    config: RtlrElasticConfig,
    cells: Vec<Cell>,
    residual_gate: f64,
    grow_streak: u64,
    shrink_streak: u64,
    stats: RtlrElasticStats,
    v2: Option<RtlrV2State>,
    multiscale: bool,
}

fn sha_bytes(bytes: &[u8]) -> String {
    let mut hash = Sha256::default();
    hash.update(bytes);
    hex(&hash.finish())
}

fn read_f64(field: &str) -> Result<f64> {
    if field.len() != 16 || !field.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("RTLR-V2 bad float encoding".into());
    }
    let value = f64::from_bits(u64::from_str_radix(field, 16)?);
    if !value.is_finite() {
        return Err("RTLR-V2 nonfinite float".into());
    }
    Ok(value)
}

fn read_cols<'a>(
    lines: &mut impl Iterator<Item = &'a str>,
    prefix: &str,
    fields: usize,
) -> Result<Vec<&'a str>> {
    let next = lines.next().ok_or("RTLR-V2 truncated checkpoint")?;
    let items: Vec<_> = next.split('\t').collect();
    if items.len() != fields || items[0] != prefix {
        return Err("RTLR-V2 malformed checkpoint record".into());
    }
    Ok(items)
}

fn signal(b: u8) -> f64 {
    (f64::from(b.count_ones()) - 4.0) / 4.0
}

fn anf(a: u8, b: u8) -> f64 {
    if (a & b).count_ones() & 1 == 0 {
        -1.0
    } else {
        1.0
    }
}

fn new_cell(index: usize, seed: u64) -> Cell {
    let weights = if index == 0 {
        [0.65, 0.3, 0.2]
    } else {
        // Deterministically break exact state symmetry at the moment a new
        // cell is allocated. No RNG stream or external initialization file.
        let mut x = seed ^ (index as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15);
        x ^= x >> 30;
        x = x.wrapping_mul(0xbf58_476d_1ce4_e5b9);
        x ^= x >> 27;
        x = x.wrapping_mul(0x94d0_49bb_1331_11eb);
        x ^= x >> 31;
        let offset = (x % 1024) as f64 / 1024.0 - 0.5;
        [
            0.65 + 0.25 * offset,
            0.3 - 0.20 * offset,
            0.2 + 0.30 * offset,
        ]
    };
    Cell {
        weights,
        h: 0.0,
        sensitivity: [0.0; LOCAL],
    }
}

impl RtlrElastic {
    pub fn new(config: RtlrElasticConfig) -> Result<Self> {
        let config = config.validate()?;
        Ok(Self {
            config,
            cells: vec![new_cell(0, config.seed)],
            residual_gate: 0.125,
            grow_streak: 0,
            shrink_streak: 0,
            v2: None,
            multiscale: false,
            stats: RtlrElasticStats {
                peak_active: 1,
                ..RtlrElasticStats::default()
            },
        })
    }

    /// Create a new private non-symlink bank. Its files persist parked
    /// *weights*, while volatile recurrence state and eligibility are reset
    /// on reactivation. This is not a complete resumable model checkpoint.
    pub fn new_v2(
        config: RtlrElasticConfig,
        policy: RtlrV2Policy,
        bank_directory: &Path,
    ) -> Result<Self> {
        let policy = policy.validate()?;
        let metadata = bank_directory.symlink_metadata();
        if metadata.is_ok() {
            return Err("RTLR-V2 bank path already exists".into());
        }
        let mut model = Self::new(config)?;
        fs::create_dir(bank_directory)?;
        model.v2 = Some(RtlrV2State {
            policy,
            bank: bank_directory.to_path_buf(),
            train_ewma: None,
        });
        Ok(model)
    }

    /// A complete, read-only-verifiable recurrent checkpoint tied to a
    /// previously sealed *physical* MOR/atlas joint manifest. This is not
    /// a self-contained model: use MorAnfRecurrent::reopen_rtlr_v2.
    /// Snapshots must occur at completed document boundaries.
    pub fn checkpoint_v2(&self, root: &Path) -> Result<()> {
        let state = self.v2.as_ref().ok_or("RTLR-V2 checkpoint requires v2")?;
        let root_meta = fs::symlink_metadata(root)?;
        if !root_meta.is_dir()
            || root_meta.file_type().is_symlink()
            || fs::canonicalize(&state.bank)? != fs::canonicalize(root.join("rtlr_v2_dormant"))?
        {
            return Err("RTLR-V2 checkpoint must bind its own physical model bank".into());
        }
        let joint = fs::read(root.join("joint.sha256.tsv"))?;
        let mut body = String::from(if self.multiscale { "RTLR_V3_MULTISCALE_V1\n" } else { "RTLR_V2_FULL_V1\n" });
        body.push_str(&format!(
            "C\t{}\t{:016x}\t{:016x}\t{:016x}\t{}\t{}\t{}\n",
            self.config.max_active,
            self.config.learning_rate.to_bits(),
            self.config.grow_above_bpc.to_bits(),
            self.config.shrink_below_bpc.to_bits(),
            self.config.patience,
            self.config.warmup_documents,
            self.config.seed
        ));
        let p = state.policy;
        body.push_str(&format!(
            "P\t{:016x}\t{:016x}\t{:016x}\n",
            p.ewma_alpha.to_bits(),
            p.plateau_tolerance_bpc.to_bits(),
            p.marginal_cost_bpc_per_cell.to_bits()
        ));
        let ewma = state
            .train_ewma
            .map(|v| format!("{:016x}", v.to_bits()))
            .unwrap_or_else(|| "-".to_owned());
        body.push_str(&format!(
            "D\t{:016x}\t{}\t{}\t{}\n",
            self.residual_gate.to_bits(),
            self.grow_streak,
            self.shrink_streak,
            ewma
        ));
        let c = self.stats;
        body.push_str(&format!(
            "S\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
            c.training_documents,
            c.grown,
            c.pruned,
            c.peak_active,
            c.recurrent_parameter_changes,
            c.gate_parameter_changes,
            c.recurrent_steps,
            c.bank_parks,
            c.bank_restores,
            c.peak_parked,
            c.train_shadow_documents
        ));
        body.push_str(&format!("A\t{}\n", self.cells.len()));
        for (i, cell) in self.cells.iter().enumerate() {
            let v = [
                cell.weights[0],
                cell.weights[1],
                cell.weights[2],
                cell.h,
                cell.sensitivity[0],
                cell.sensitivity[1],
                cell.sensitivity[2],
            ];
            if v.iter().any(|n| !n.is_finite())
                || cell.weights.iter().any(|n| !(-2.0..=2.0).contains(n))
            {
                return Err("RTLR-V2 invalid active checkpoint cell".into());
            }
            body.push_str(&format!("W\t{i}"));
            for value in v {
                body.push_str(&format!("\t{:016x}", value.to_bits()));
            }
            body.push('\n');
        }
        let mut parked = 0usize;
        for i in self.cells.len()..self.capacity() {
            let file = self.bank_path(i)?;
            match fs::symlink_metadata(&file) {
                Ok(meta) => {
                    if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > 512 {
                        return Err("RTLR-V2 unsafe parked checkpoint file".into());
                    }
                    let data = fs::read(&file)?;
                    self.verify_parked_bytes(i, &data)?;
                    body.push_str(&format!("B\t{i}\t{}\n", sha_bytes(&data)));
                    parked += 1;
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
        }
        for i in 1..self.active() {
            if fs::symlink_metadata(self.bank_path(i)?).is_ok() {
                return Err("RTLR-V2 duplicate active/dormant cell".into());
            }
        }
        if c.bank_parks.checked_sub(c.bank_restores) != Some(parked as u64)
            || c.grown.checked_sub(c.pruned).and_then(|v| v.checked_add(1))
                != Some(self.active() as u64)
        {
            return Err("RTLR-V2 checkpoint capacity conservation mismatch".into());
        }
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(root.join("rtlr_v2_full.tsv"))?;
        file.write_all(body.as_bytes())?;
        file.sync_all()?;
        let mut manifest = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(root.join("rtlr_v2_joint.sha256.tsv"))?;
        writeln!(
            manifest,
            "RTLR_V2_JOIN_V1\t{}\t{}",
            sha_bytes(&joint),
            sha_bytes(body.as_bytes())
        )?;
        manifest.sync_all()?;
        fs::File::open(root)?.sync_all()?;
        Ok(())
    }

    fn verify_parked_bytes(&self, index: usize, bytes: &[u8]) -> Result<[f64; LOCAL]> {
        let text = std::str::from_utf8(bytes)?;
        let (record, sum) = text
            .split_once("sha256\t")
            .ok_or("RTLR-V2 missing parked hash")?;
        if sha_bytes(record.as_bytes()) != sum.trim_end_matches('\n') || !sum.ends_with('\n') {
            return Err("RTLR-V2 parked cell SHA mismatch".into());
        }
        let cols: Vec<_> = record.trim_end_matches('\n').split('\t').collect();
        if cols.len() != 5
            || cols[0] != "RTLR_ELASTIC_PARKED_V2"
            || cols[1].parse::<usize>()? != index
        {
            return Err("RTLR-V2 bad parked cell schema".into());
        }
        let mut weights = [0f64; LOCAL];
        for (out, field) in weights.iter_mut().zip(cols[2..].iter()) {
            *out = read_f64(field)?;
            if !(-2.0..=2.0).contains(out) {
                return Err("RTLR-V2 out-of-range parked weight".into());
            }
        }
        Ok(weights)
    }

    /// Reopen the full active topology, TRAIN policy, online eligibility and
    /// SHA-checked bank after the physical MOR/atlas snapshot was verified.
    /// No data are modified while validating; no silent scalar fallback.
    pub fn reopen_v2(root: &Path) -> Result<Self> {
        let root_meta = fs::symlink_metadata(root)?;
        if !root_meta.is_dir() || root_meta.file_type().is_symlink() {
            return Err("RTLR-V2 invalid checkpoint directory".into());
        }
        let signature = fs::read_to_string(root.join("rtlr_v2_joint.sha256.tsv"))?;
        let fields: Vec<_> = signature.trim_end_matches('\n').split('\t').collect();
        if fields.len() != 3 || fields[0] != "RTLR_V2_JOIN_V1" {
            return Err("RTLR-V2 missing integrated manifest".into());
        }
        let joint = fs::read(root.join("joint.sha256.tsv"))?;
        let meta = fs::symlink_metadata(root.join("rtlr_v2_full.tsv"))?;
        if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > 16384 {
            return Err("RTLR-V2 unsafe checkpoint body".into());
        }
        let raw = fs::read(root.join("rtlr_v2_full.tsv"))?;
        if fields[1] != sha_bytes(&joint) || fields[2] != sha_bytes(&raw) {
            return Err("RTLR-V2 integrated SHA mismatch".into());
        }
        let content = std::str::from_utf8(&raw)?;
        let mut lines = content.lines();
        let multiscale = match lines.next() {
            Some("RTLR_V2_FULL_V1") => false,
            Some("RTLR_V3_MULTISCALE_V1") => true,
            _ => return Err("RTLR unknown checkpoint schema".into()),
        };
        let c = read_cols(&mut lines, "C", 8)?;
        let config = RtlrElasticConfig {
            max_active: c[1].parse()?,
            learning_rate: read_f64(c[2])?,
            grow_above_bpc: read_f64(c[3])?,
            shrink_below_bpc: read_f64(c[4])?,
            patience: c[5].parse()?,
            warmup_documents: c[6].parse()?,
            seed: c[7].parse()?,
        }
        .validate()?;
        let p = read_cols(&mut lines, "P", 4)?;
        let policy = RtlrV2Policy {
            ewma_alpha: read_f64(p[1])?,
            plateau_tolerance_bpc: read_f64(p[2])?,
            marginal_cost_bpc_per_cell: read_f64(p[3])?,
        }
        .validate()?;
        let d = read_cols(&mut lines, "D", 5)?;
        let gate = read_f64(d[1])?;
        let grow_streak: u64 = d[2].parse()?;
        let shrink_streak: u64 = d[3].parse()?;
        let ewma = if d[4] == "-" {
            None
        } else {
            Some(read_f64(d[4])?)
        };
        if !(0.0..=0.5).contains(&gate)
            || ewma.is_some_and(|x| !(0.0..=16.0).contains(&x))
            || grow_streak >= config.patience
            || shrink_streak >= config.patience
        {
            return Err("RTLR-V2 invalid saved scheduler".into());
        }
        let st = read_cols(&mut lines, "S", 12)?;
        let nums: Vec<u64> = st[1..]
            .iter()
            .map(|v| v.parse::<u64>())
            .collect::<std::result::Result<_, _>>()?;
        let stats = RtlrElasticStats {
            training_documents: nums[0],
            grown: nums[1],
            pruned: nums[2],
            peak_active: usize::try_from(nums[3])?,
            recurrent_parameter_changes: nums[4],
            gate_parameter_changes: nums[5],
            recurrent_steps: nums[6],
            bank_parks: nums[7],
            bank_restores: nums[8],
            peak_parked: usize::try_from(nums[9])?,
            train_shadow_documents: nums[10],
        };
        let count = read_cols(&mut lines, "A", 2)?[1].parse::<usize>()?;
        if !(1..=config.max_active).contains(&count)
            || stats.peak_active < count
            || stats.peak_active > config.max_active
            || stats.train_shadow_documents != stats.training_documents
            || stats
                .grown
                .checked_sub(stats.pruned)
                .and_then(|v| v.checked_add(1))
                != Some(count as u64)
        {
            return Err("RTLR-V2 impossible active count".into());
        }
        let mut cells = Vec::with_capacity(count);
        for i in 0..count {
            let record = read_cols(&mut lines, "W", 9)?;
            if record[1].parse::<usize>()? != i {
                return Err("RTLR-V2 bad recurrent cell index".into());
            }
            let mut v = [0.0f64; 7];
            for (dest, field) in v.iter_mut().zip(record[2..].iter()) {
                *dest = read_f64(field)?;
            }
            if v[..3].iter().any(|w| !(-2.0..=2.0).contains(w)) {
                return Err("RTLR-V2 recurrent coefficient outside range".into());
            }
            cells.push(Cell {
                weights: [v[0], v[1], v[2]],
                h: v[3],
                sensitivity: [v[4], v[5], v[6]],
            });
        }
        let bank = root.join("rtlr_v2_dormant");
        let dir = fs::symlink_metadata(&bank)?;
        if !dir.is_dir() || dir.file_type().is_symlink() {
            return Err("RTLR-V2 invalid dormant checkpoint bank".into());
        }
        let mut model = Self::new(config)?;
        model.cells = cells;
        model.multiscale = multiscale;
        model.residual_gate = gate;
        model.grow_streak = grow_streak;
        model.shrink_streak = shrink_streak;
        model.stats = stats;
        model.v2 = Some(RtlrV2State {
            policy,
            bank: bank.clone(),
            train_ewma: ewma,
        });
        let mut parked = 0u64;
        let mut recorded = std::collections::BTreeSet::new();
        for line in lines {
            let cols: Vec<_> = line.split('\t').collect();
            if cols.len() != 3 || cols[0] != "B" {
                return Err("RTLR-V2 invalid dormant record".into());
            }
            let index: usize = cols[1].parse()?;
            if index < count || index >= config.max_active || !recorded.insert(index) {
                return Err("RTLR-V2 invalid/duplicate parked index".into());
            }
            let path = model.bank_path(index)?;
            let meta = fs::symlink_metadata(&path)?;
            if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > 512 {
                return Err("RTLR-V2 unsafe dormant cell".into());
            }
            let data = fs::read(&path)?;
            if sha_bytes(&data) != cols[2] {
                return Err("RTLR-V2 dormant file mismatches joint snapshot".into());
            }
            model.verify_parked_bytes(index, &data)?;
            parked += 1;
        }
        // Reject unmanifested files, including symlinks and irrelevant
        // entries; no extra dormant state may escape the integrity seal.
        let mut observed = 0u64;
        for item in fs::read_dir(&bank)? {
            let item = item?;
            if !item.file_type()?.is_file()
                || !recorded.contains(
                    &item
                        .file_name()
                        .to_string_lossy()
                        .strip_prefix("cell-")
                        .and_then(|x| x.strip_suffix(".tsv"))
                        .and_then(|x| x.parse::<usize>().ok())
                        .ok_or("RTLR-V2 unknown dormant entry")?,
                )
            {
                return Err("RTLR-V2 extraneous or linked dormant file".into());
            }
            observed += 1;
        }
        if observed != parked
            || stats.bank_parks.checked_sub(stats.bank_restores) != Some(parked)
            || stats.peak_parked < parked as usize
        {
            return Err("RTLR-V2 dormant accounting mismatch".into());
        }
        Ok(model)
    }

    /// Opt-in V3 exact-local independent recurrent time scales.
    /// Anchor leak=1 remains the original exact RTRL-2 scalar path.
    pub fn new_v3(
        config: RtlrElasticConfig, policy: RtlrV2Policy, bank_directory: &Path,
    ) -> Result<Self> {
        let mut model = Self::new_v2(config, policy, bank_directory)?;
        model.multiscale = true;
        Ok(model)
    }

    pub fn is_multiscale(&self) -> bool {
        self.multiscale
    }

    /// Fixed, reproducible leak rates 1, 1/4, 1/16, ..., independent
    /// of learned weights, TRAIN labels and width-controller decisions.
    pub fn leak_rate(&self, index: usize) -> Result<f64> {
        if index >= self.active() {
            return Err("RTLR-V3 invalid or inactive cell index".into());
        }
        Ok(if self.multiscale && index > 0 {
            4.0f64.powi(-(index.min(12) as i32))
        } else {
            1.0
        })
    }

    pub fn anchor_state(&self) -> f64 {
        self.cells[0].h
    }

    pub fn is_v2(&self) -> bool {
        self.v2.is_some()
    }

    fn bank_path(&self, index: usize) -> Result<PathBuf> {
        let v2 = self.v2.as_ref().ok_or("RTLR-V2 bank not enabled")?;
        if index == 0 || index >= self.capacity() {
            return Err("RTLR-V2 invalid dormant cell index".into());
        }
        let meta = fs::symlink_metadata(&v2.bank)?;
        if !meta.is_dir() || meta.file_type().is_symlink() {
            return Err("RTLR-V2 bank directory replaced or linked".into());
        }
        Ok(v2.bank.join(format!("cell-{index:03}.tsv")))
    }

    fn park_last_cell(&mut self) -> Result<()> {
        let index = self.cells.len().checked_sub(1).ok_or("empty cell set")?;
        let path = self.bank_path(index)?;
        let cell = &self.cells[index];
        // Hex IEEE 754 roundtrip, plus explicit SHA256 to reject bit-flips.
        let record = format!(
            "RTLR_ELASTIC_PARKED_V2\t{index}\t{:016x}\t{:016x}\t{:016x}\n",
            cell.weights[0].to_bits(),
            cell.weights[1].to_bits(),
            cell.weights[2].to_bits()
        );
        let mut hash = Sha256::default();
        hash.update(record.as_bytes());
        let digest = hex(&hash.finish());
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        output.write_all(record.as_bytes())?;
        writeln!(output, "sha256\t{digest}")?;
        output.sync_all()?;
        self.cells.pop();
        self.stats.pruned += 1;
        self.stats.bank_parks += 1;
        self.stats.peak_parked = self
            .stats
            .peak_parked
            .max((self.stats.bank_parks - self.stats.bank_restores) as usize);
        Ok(())
    }

    fn restore_or_allocate(&mut self) -> Result<()> {
        if self.active() >= self.capacity() {
            return Err("RTLR-V2 capacity exceeded".into());
        }
        let index = self.active();
        let path = self.bank_path(index)?;
        let cell = match fs::symlink_metadata(&path) {
            Ok(meta) => {
                if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > 512 {
                    return Err("RTLR-V2 unsafe parked cell file".into());
                }
                let contents = fs::read_to_string(&path)?;
                let (record, digest_line) = contents
                    .split_once("sha256\t")
                    .ok_or("RTLR-V2 missing cell digest")?;
                let digest = digest_line
                    .strip_suffix('\n')
                    .ok_or("bad cell digest suffix")?;
                let mut hash = Sha256::default();
                hash.update(record.as_bytes());
                if hex(&hash.finish()) != digest {
                    return Err("RTLR-V2 parked cell SHA256 mismatch".into());
                }
                let parts: Vec<_> = record.trim_end_matches('\n').split('\t').collect();
                if parts.len() != 5
                    || parts[0] != "RTLR_ELASTIC_PARKED_V2"
                    || parts[1] != index.to_string()
                {
                    return Err("RTLR-V2 invalid cell schema or index".into());
                }
                let mut weights = [0.0; LOCAL];
                for (slot, string) in weights.iter_mut().zip(&parts[2..]) {
                    *slot = f64::from_bits(u64::from_str_radix(string, 16)?);
                    if !slot.is_finite() || !(-2.0..=2.0).contains(slot) {
                        return Err("RTLR-V2 invalid parked weight".into());
                    }
                }
                fs::remove_file(&path)?;
                self.stats.bank_restores += 1;
                Cell {
                    weights,
                    h: 0.0,
                    sensitivity: [0.0; LOCAL],
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => new_cell(index, self.config.seed),
            Err(e) => return Err(e.into()),
        };
        self.cells.push(cell);
        self.stats.grown += 1;
        self.stats.peak_active = self.stats.peak_active.max(self.active());
        Ok(())
    }

    /// A paired *TRAIN-only* comparison between the active recurrent
    /// feature and an anchor-feature shadow prediction under the same
    /// then-current experts/router weights. This is NOT an independently
    /// trained counterfactual RTRL-2 model and must not be described as one.
    pub fn observe_train_document_v2(
        &mut self,
        live_bpc: f64,
        shadow_anchor_bpc: f64,
    ) -> Result<()> {
        if self.v2.is_none() {
            return Err("RTLR-V2 policy disabled".into());
        }
        if !live_bpc.is_finite()
            || !shadow_anchor_bpc.is_finite()
            || !(0.0..=16.0).contains(&live_bpc)
            || !(0.0..=16.0).contains(&shadow_anchor_bpc)
        {
            return Err("invalid RTLR-V2 TRAIN paired loss".into());
        }
        let previous = self.v2.as_ref().and_then(|v| v.train_ewma);
        let policy = self.v2.as_ref().ok_or("missing v2")?.policy;
        self.stats.training_documents += 1;
        self.stats.train_shadow_documents += 1;
        let stagnated =
            previous.is_some_and(|base| live_bpc >= base - policy.plateau_tolerance_bpc);
        let active = self.active();
        let benefit = shadow_anchor_bpc - live_bpc;
        let net_benefit = benefit - policy.marginal_cost_bpc_per_cell * (active - 1) as f64;
        if self.stats.training_documents > self.config.warmup_documents {
            if active > 1 && net_benefit <= 0.0 {
                self.shrink_streak += 1;
                self.grow_streak = 0;
                if self.shrink_streak >= self.config.patience {
                    self.park_last_cell()?;
                    self.shrink_streak = 0;
                }
            } else if stagnated && active < self.capacity() {
                self.grow_streak += 1;
                self.shrink_streak = 0;
                if self.grow_streak >= self.config.patience {
                    self.restore_or_allocate()?;
                    self.grow_streak = 0;
                }
            } else {
                self.grow_streak = 0;
                self.shrink_streak = 0;
            }
        }
        self.v2.as_mut().ok_or("v2 disappeared")?.train_ewma =
            Some(previous.map_or(live_bpc, |mean| {
                policy.ewma_alpha * live_bpc + (1.0 - policy.ewma_alpha) * mean
            }));
        Ok(())
    }

    pub fn active(&self) -> usize {
        self.cells.len()
    }

    pub fn capacity(&self) -> usize {
        self.config.max_active
    }

    pub fn stats(&self) -> RtlrElasticStats {
        self.stats
    }

    pub fn trained_local_parameters(&self) -> usize {
        LOCAL * self.cells.len() + usize::from(self.cells.len() > 1)
    }

    /// Logical f64 cell payload: 3 weights, 1 hidden scalar and 3 derivatives
    /// per active cell, plus one residual gate. This excludes Vec overhead.
    pub fn active_payload_bytes(&self) -> usize {
        (7 * self.cells.len() + 1) * size_of::<f64>()
    }

    pub fn reset_state(&mut self) {
        for cell in &mut self.cells {
            cell.h = 0.0;
            cell.sensitivity.fill(0.0);
        }
    }

    fn readout(&self) -> f64 {
        let anchor = self.cells[0].h;
        if self.cells.len() == 1 {
            return anchor;
        }
        let extra_mean =
            self.cells[1..].iter().map(|c| c.h).sum::<f64>() / (self.cells.len() - 1) as f64;
        anchor + self.residual_gate * (extra_mean - anchor)
    }

    fn advance(&mut self, a: u8, b: u8, derivatives: bool) -> Result<f64> {
        let x = signal(b);
        let phi = anf(a, b);
        for (index, cell) in self.cells.iter_mut().enumerate() {
            let prior = cell.h;
            let leak = if self.multiscale && index > 0 {
                4.0f64.powi(-(index.min(12) as i32))
            } else {
                1.0
            };
            let candidate = (cell.weights[0] * prior
                + cell.weights[1] * x
                + cell.weights[2] * phi)
                .tanh();
            cell.h = if leak == 1.0 {
                candidate
            } else {
                prior + leak * (candidate - prior)
            };
            if derivatives {
                let d = 1.0 - candidate * candidate;
                if leak == 1.0 {
                    cell.sensitivity = [
                        d * (prior + cell.weights[0] * cell.sensitivity[0]),
                        d * (x + cell.weights[0] * cell.sensitivity[1]),
                        d * (phi + cell.weights[0] * cell.sensitivity[2]),
                    ];
                } else {
                    let carry = (1.0 - leak) + leak * d * cell.weights[0];
                    cell.sensitivity = [
                        carry * cell.sensitivity[0] + leak * d * prior,
                        carry * cell.sensitivity[1] + leak * d * x,
                        carry * cell.sensitivity[2] + leak * d * phi,
                    ];
                }
            }
            if !cell.h.is_finite()
                || (derivatives && cell.sensitivity.iter().any(|v| !v.is_finite()))
            {
                return Err("RTLR-ELASTIC nonfinite recurrent state".into());
            }
        }
        let feature = self.readout();
        if !feature.is_finite() {
            return Err("RTLR-ELASTIC nonfinite readout".into());
        }
        Ok(feature)
    }

    /// Call exactly once per predicted byte, before consuming target labels.
    pub fn advance_train(&mut self, a: u8, b: u8) -> Result<f64> {
        self.advance(a, b, true)
    }

    /// Read-only recurrent state evolution (on a disposable clone); no
    /// sensitivity or weight changes, not even UORO random-sign evolution.
    pub fn advance_inference(&mut self, a: u8, b: u8) -> Result<f64> {
        self.advance(a, b, false)
    }

    /// Train local exact eligibility and the bounded residual readout using
    /// the unchanged MOR gradient credit from all 8 target bits.
    pub fn learn_credit(&mut self, credit: f64) -> Result<u64> {
        if !credit.is_finite() {
            return Err("RTLR-ELASTIC nonfinite MOR gradient credit".into());
        }
        let extra = self.cells.len() - 1;
        let anchor = self.cells[0].h;
        let extra_mean = if extra > 0 {
            self.cells[1..].iter().map(|c| c.h).sum::<f64>() / extra as f64
        } else {
            0.0
        };
        let mut changed = 0u64;
        for (index, cell) in self.cells.iter_mut().enumerate() {
            let readout_weight = if extra == 0 {
                1.0
            } else if index == 0 {
                1.0 - self.residual_gate
            } else {
                self.residual_gate / extra as f64
            };
            for p in 0..LOCAL {
                let before = cell.weights[p];
                let after = if extra == 0 {
                    // Preserve the exact left-associative RTRL-2 SGD path.
                    (before - self.config.learning_rate * credit * cell.sensitivity[p])
                        .clamp(-2.0, 2.0)
                } else {
                    (before
                        - self.config.learning_rate * credit * readout_weight * cell.sensitivity[p])
                        .clamp(-2.0, 2.0)
                };
                if !after.is_finite() {
                    return Err("RTLR-ELASTIC nonfinite recurrent update".into());
                }
                if after != before {
                    changed += 1;
                }
                cell.weights[p] = after;
            }
        }
        if extra > 0 {
            let old = self.residual_gate;
            self.residual_gate =
                (old - self.config.learning_rate * credit * (extra_mean - anchor)).clamp(0.0, 0.5);
            if !self.residual_gate.is_finite() {
                return Err("RTLR-ELASTIC nonfinite residual gate".into());
            }
            if self.residual_gate != old {
                changed += 1;
                self.stats.gate_parameter_changes += 1;
            }
        }
        self.stats.recurrent_steps += 1;
        self.stats.recurrent_parameter_changes += changed;
        Ok(changed)
    }

    /// TRAIN-only structural scheduler, called after a whole training
    /// document. Expansion and contraction occur at document boundaries,
    /// never by inspecting read-only DEV loss. Patience prevents oscillation.
    pub fn observe_train_document(&mut self, loss_bpc: f64) -> Result<()> {
        if !loss_bpc.is_finite() || !(0.0..=8.0).contains(&loss_bpc) {
            return Err("invalid TRAIN-only document bit loss".into());
        }
        self.stats.training_documents += 1;
        if self.stats.training_documents <= self.config.warmup_documents {
            return Ok(());
        }
        if loss_bpc > self.config.grow_above_bpc && self.active() < self.config.max_active {
            self.grow_streak += 1;
            self.shrink_streak = 0;
            if self.grow_streak >= self.config.patience {
                self.cells
                    .push(new_cell(self.cells.len(), self.config.seed));
                self.stats.grown += 1;
                self.stats.peak_active = self.stats.peak_active.max(self.active());
                self.grow_streak = 0;
            }
        } else if loss_bpc < self.config.shrink_below_bpc && self.active() > 1 {
            self.shrink_streak += 1;
            self.grow_streak = 0;
            if self.shrink_streak >= self.config.patience {
                self.cells.pop();
                self.stats.pruned += 1;
                self.shrink_streak = 0;
            }
        } else {
            self.grow_streak = 0;
            self.shrink_streak = 0;
        }
        Ok(())
    }

    /// Fails closed on incomplete or malformed sidecar paths.
    /// A full snapshot loader including the physical atlas is not implemented.
    pub fn write_evidence(&self, path: &std::path::Path) -> Result<()> {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(path)?;
        writeln!(
            file,
            "RTLR_ELASTIC_EXACT_LOCAL_V1\tactive={}\tmax={}\tgate={:.17e}\tseed={}",
            self.active(),
            self.capacity(),
            self.residual_gate,
            self.config.seed
        )?;
        writeln!(file, "index\ta\tb\tc")?;
        for (index, cell) in self.cells.iter().enumerate() {
            writeln!(
                file,
                "{index}\t{:.17e}\t{:.17e}\t{:.17e}",
                cell.weights[0], cell.weights[1], cell.weights[2]
            )?;
        }
        file.sync_all()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(max_active: usize) -> RtlrElasticConfig {
        RtlrElasticConfig {
            max_active,
            learning_rate: 0.005,
            grow_above_bpc: 4.1,
            shrink_below_bpc: 3.8,
            patience: 2,
            warmup_documents: 2,
            seed: 17,
        }
    }

    #[test]
    fn scalar_exact_trace_and_update_match_original_rtrl2() {
        let mut rt = RtlrElastic::new(config(1)).unwrap();
        let mut w = [0.65, 0.3, 0.2];
        let mut state = 0.0;
        let mut sens = [0.0; LOCAL];
        for (a, b) in [(b'a', b'b'), (b'c', b'd'), (b'e', b'f'), (b'g', b'h')] {
            let old = state;
            state = (w[0] * state + w[1] * signal(b) + w[2] * anf(a, b)).tanh();
            let d = 1.0 - state * state;
            sens = [
                d * (old + w[0] * sens[0]),
                d * (signal(b) + w[0] * sens[1]),
                d * (anf(a, b) + w[0] * sens[2]),
            ];
            assert_eq!(rt.advance_train(a, b).unwrap(), state);
            let credit = 0.017;
            let changed = rt.learn_credit(credit).unwrap();
            assert!(changed > 0);
            for (p, weight) in w.iter_mut().enumerate() {
                *weight = (*weight - 0.005 * credit * sens[p]).clamp(-2.0, 2.0);
            }
            assert_eq!(rt.cells[0].weights, w);
        }
        assert_eq!(rt.active(), 1);
        assert_eq!(rt.stats().grown, 0);
        assert_eq!(rt.trained_local_parameters(), 3);
    }

    #[test]
    fn training_only_demand_growth_and_retraction_are_bounded() {
        let mut rt = RtlrElastic::new(config(4)).unwrap();
        assert_eq!(rt.active(), 1);
        rt.observe_train_document(7.0).unwrap();
        rt.observe_train_document(7.0).unwrap();
        assert_eq!(rt.active(), 1);
        for _ in 0..12 {
            rt.observe_train_document(7.0).unwrap();
        }
        assert_eq!(rt.active(), 4);
        assert_eq!(rt.stats().grown, 3);
        assert_eq!(rt.stats().peak_active, 4);
        for _ in 0..8 {
            rt.observe_train_document(2.0).unwrap();
        }
        assert_eq!(rt.active(), 1);
        assert_eq!(rt.stats().pruned, 3);
        assert_eq!(rt.active_payload_bytes(), 64);
    }

    #[test]
    fn multi_state_training_is_finite_and_read_only_deterministic() {
        let mut rt = RtlrElastic::new(config(8)).unwrap();
        for _ in 0..12 {
            rt.observe_train_document(6.0).unwrap();
        }
        assert!(rt.active() > 1);
        for (a, b) in [(b'a', b'b'), (b'b', b'c'), (b'c', b'd')] {
            assert!(rt.advance_train(a, b).unwrap().is_finite());
            rt.learn_credit(0.25).unwrap();
        }
        let before = rt.stats();
        let weights: Vec<_> = rt.cells.iter().map(|c| c.weights).collect();
        let mut first = rt.clone();
        first.reset_state();
        let mut second = rt.clone();
        second.reset_state();
        for &(a, b) in &[(b'a', b'z'), (b'c', b'd'), (b'e', b'f')] {
            assert_eq!(
                first.advance_inference(a, b).unwrap(),
                second.advance_inference(a, b).unwrap()
            );
        }
        assert_eq!(rt.stats(), before);
        assert_eq!(
            rt.cells.iter().map(|c| c.weights).collect::<Vec<_>>(),
            weights
        );
    }

    #[test]
    fn v2_parks_restores_identical_trained_weights_without_resident_dormant_cells() {
        let unique = format!(
            "rtlr-v2-bank-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let root = std::env::temp_dir().join(unique);
        let mut cfg = config(4);
        cfg.patience = 1;
        cfg.warmup_documents = 1;
        let policy = RtlrV2Policy {
            ewma_alpha: 0.5,
            plateau_tolerance_bpc: 0.1,
            marginal_cost_bpc_per_cell: 0.001,
        };
        let mut engine = RtlrElastic::new_v2(cfg, policy, &root).unwrap();
        engine.observe_train_document_v2(4.5, 4.5).unwrap();
        engine.observe_train_document_v2(4.5, 4.5).unwrap();
        assert_eq!(engine.active(), 2);
        engine.advance_train(b'a', b'z').unwrap();
        engine.learn_credit(0.17).unwrap();
        let saved = engine.cells[1].weights;
        engine.observe_train_document_v2(5.5, 5.4).unwrap();
        assert_eq!(engine.active(), 1);
        assert_eq!(engine.stats().bank_parks, 1);
        assert!(root.join("cell-001.tsv").exists());
        assert_eq!(engine.active_payload_bytes(), 64);
        engine.observe_train_document_v2(5.5, 5.5).unwrap();
        assert_eq!(engine.active(), 2);
        assert_eq!(engine.stats().bank_restores, 1);
        assert_eq!(engine.cells[1].weights, saved);
        assert_eq!(engine.cells[1].h, 0.0);
        assert!(!root.join("cell-001.tsv").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn v2_fails_closed_on_corrupted_parked_weights_and_preserves_active_state() {
        let root = std::env::temp_dir().join(format!(
            "rtlr-v2-corrupt-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut cfg = config(3);
        cfg.patience = 1;
        cfg.warmup_documents = 1;
        let policy = RtlrV2Policy {
            ewma_alpha: 0.25,
            plateau_tolerance_bpc: 0.1,
            marginal_cost_bpc_per_cell: 0.001,
        };
        let mut engine = RtlrElastic::new_v2(cfg, policy, &root).unwrap();
        engine.observe_train_document_v2(4.0, 4.0).unwrap();
        engine.observe_train_document_v2(4.0, 4.0).unwrap();
        engine.observe_train_document_v2(4.5, 4.4).unwrap();
        assert_eq!(engine.active(), 1);
        fs::write(root.join("cell-001.tsv"), "tampered").unwrap();
        assert!(engine.observe_train_document_v2(4.5, 4.5).is_err());
        assert_eq!(engine.active(), 1);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn v2_rejects_insecure_existing_bank_and_invalid_policy() {
        let root = std::env::temp_dir().join(format!(
            "rtlr-v2-existing-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        assert!(RtlrElastic::new_v2(
            config(3),
            RtlrV2Policy {
                ewma_alpha: 0.2,
                plateau_tolerance_bpc: 0.1,
                marginal_cost_bpc_per_cell: 0.001,
            },
            &root
        )
        .is_err());
        fs::remove_dir_all(root).unwrap();
    }

    fn multiscale_test_policy() -> RtlrV2Policy {
        RtlrV2Policy {
            ewma_alpha: 0.25, plateau_tolerance_bpc: 0.05,
            marginal_cost_bpc_per_cell: 0.001,
        }
    }

    fn multiscale_test_root(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "rtlr-v3-{tag}-{}-{}", std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ))
    }

    #[test]
    fn v3_scalar_is_bit_exact_and_cell_rates_are_distinct() {
        let mut old = RtlrElastic::new(config(1)).unwrap();
        let root = multiscale_test_root("anchor");
        let mut multi = RtlrElastic::new_v3(config(4), multiscale_test_policy(), &root).unwrap();
        for i in 1..4 {
            multi.cells.push(new_cell(i, 17));
        }
        assert_eq!(multi.leak_rate(0).unwrap(), 1.0);
        assert_eq!(multi.leak_rate(1).unwrap(), 0.25);
        assert_eq!(multi.leak_rate(2).unwrap(), 0.0625);
        assert_eq!(multi.leak_rate(3).unwrap(), 0.015625);
        for &(a,b) in &[(b'a',b'z'),(b'b',b'r'),(b'c',b'v')] {
            old.advance_train(a,b).unwrap();
            multi.advance_train(a,b).unwrap();
            assert_eq!(old.cells[0].h.to_bits(), multi.cells[0].h.to_bits());
            assert_eq!(old.cells[0].sensitivity, multi.cells[0].sensitivity);
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn v3_forward_local_gradient_matches_centered_finite_difference() {
        let root = multiscale_test_root("jac");
        let mut m = RtlrElastic::new_v3(config(4), multiscale_test_policy(), &root).unwrap();
        m.cells.push(new_cell(1,17));
        m.cells.push(new_cell(2,17));
        let steps = [(b'a',b'q'),(b'b',b'r'),(b'c',b's'),
            (b'd',b't'),(b'e',b'u'),(b'f',b'v')];
        for &(a,b) in &steps {
            m.advance_train(a,b).unwrap();
        }
        for i in 1..3 {
            for p in 0..3 {
                let mut plus = m.clone();
                let mut minus = m.clone();
                plus.reset_state();
                minus.reset_state();
                const EPS: f64 = 1e-6;
                plus.cells[i].weights[p] += EPS;
                minus.cells[i].weights[p] -= EPS;
                for &(a,b) in &steps {
                    plus.advance_inference(a,b).unwrap();
                    minus.advance_inference(a,b).unwrap();
                }
                let numeric = (plus.cells[i].h-minus.cells[i].h)/(2.0*EPS);
                assert!((numeric-m.cells[i].sensitivity[p]).abs()<1e-8,
                    "cell={i} parameter={p} numeric={numeric} analytic={}",
                    m.cells[i].sensitivity[p]);
            }
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn v3_long_lag_impulse_survives_when_scalar_anchor_has_decayed() {
        let root=multiscale_test_root("impulse");
        let mut m=RtlrElastic::new_v3(config(8),multiscale_test_policy(),&root).unwrap();
        for i in 1..5 {m.cells.push(new_cell(i,17));}
        for cell in &mut m.cells {
            cell.weights=[0.65,0.0,0.0];
            cell.h=0.8;
            cell.sensitivity=[0.0;3];
        }
        for _ in 0..128 {m.advance_inference(0,0).unwrap();}
        assert!(m.cells[0].h.abs()<1e-20);
        assert!(m.cells[4].h>0.1);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn validation_rejects_invalid_policy_and_nan_credit() {
        let mut invalid = config(0);
        assert!(RtlrElastic::new(invalid).is_err());
        invalid.max_active = 8;
        invalid.grow_above_bpc = 3.0;
        assert!(RtlrElastic::new(invalid).is_err());
        let mut rt = RtlrElastic::new(config(2)).unwrap();
        assert!(rt.learn_credit(f64::NAN).is_err());
        assert!(rt.observe_train_document(f64::INFINITY).is_err());
        assert_eq!(rt.active(), 1);
        assert_eq!(rt.stats().training_documents, 0);
    }
}
