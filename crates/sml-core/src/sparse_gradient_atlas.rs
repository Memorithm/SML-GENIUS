//! Sparse, supervised gradient core for the SML model programme.
//!
//! Unlike an n-gram count bank, these are mutable, learned scalar weights.
//! The train-time master lives on disk as f32; a bounded LRU holds only routed
//! pages in RAM. The 4-bit export is an **evaluation candidate**, not proof
//! of mixed-precision 25B inference or of quantization-aware convergence.
//! Fixed direct hash routing is a bootstrap, NOT a trained MOR router.
use crate::elastic_ngram_paged_sha::{hex, Sha256};
use std::{
    error::Error,
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::Path,
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
pub const PAGE_PARAMETERS: usize = 1024;
pub const TOUCH_BYTES: usize = PAGE_PARAMETERS / 8;
const FEATURES: usize = 17;
const SCALE: f64 = 4.0;
const VERSION: &str = "SML_SPARSE_GRADIENT_V1";

/// Disk-backed capacity and resident-memory controls. The total capacity
/// is *not* the number of weights that were actually updated by training.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SparseConfig {
    pub pages: u64,
    pub max_cached_pages: usize,
    pub learning_rate: f32,
}

impl SparseConfig {
    pub fn validate(self) -> Result<Self> {
        if self.pages == 0 || self.max_cached_pages == 0 || self.max_cached_pages > 256 {
            return Err("invalid page count/cache ceiling".into());
        }
        if self
            .pages
            .checked_mul(PAGE_PARAMETERS as u64)
            .and_then(|n| n.checked_mul(4))
            .is_none()
        {
            return Err("master file length overflow".into());
        }
        if !self.learning_rate.is_finite()
            || !(0.0..=1.0).contains(&self.learning_rate)
            || self.learning_rate == 0.0
        {
            return Err("invalid SGD learning rate".into());
        }
        Ok(self)
    }

    pub fn total_parameters(self) -> u64 {
        self.pages * PAGE_PARAMETERS as u64
    }

    pub fn train_master_bytes(self) -> u64 {
        self.total_parameters() * 4
    }

    pub fn packed_q4_bytes(self) -> u64 {
        self.total_parameters() / 2
    }

    /// Upper bound for resident page payload, excluding Vec/OS metadata.
    pub fn resident_payload_bound(self) -> usize {
        self.max_cached_pages * (PAGE_PARAMETERS * 4 + TOUCH_BYTES)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SparseCounters {
    pub supervised_bits: u64,
    pub parameter_updates: u64,
    pub distinct_updated_parameters: u64,
    pub page_loads: u64,
    pub page_evictions: u64,
}

struct Resident {
    id: u64,
    master: Box<[f32; PAGE_PARAMETERS]>,
    touched: [u8; TOUCH_BYTES],
    dirty: bool,
    used: u64,
}

/// Train-time disk atlas. Only selected pages participate in a forward/backward
/// pass. No full model load, no unsafe mmap, no dense attention.
pub struct SparseGradientAtlas {
    config: SparseConfig,
    master: File,
    touches: File,
    cache: Vec<Resident>,
    tick: u64,
    counters: SparseCounters,
}

fn mix(mut x: u64) -> u64 {
    x ^= x >> 30;
    x = x.wrapping_mul(0xbf58476d1ce4e5b9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94d049bb133111eb);
    x ^ (x >> 31)
}

fn route(a: u8, b: u8, bit: usize, pages: u64) -> u64 {
    let address = (u64::from(a) << 16) | (u64::from(b) << 8) | bit as u64;
    mix(address ^ 0x534d4c2d47524144) % pages
}

fn feature_slot(a: u8, b: u8, bit: usize, feature: usize) -> usize {
    let address = (u64::from(a) << 16) | (u64::from(b) << 8) | bit as u64;
    let offset = (mix(address ^ 0xA11FEB1234) as usize) & (PAGE_PARAMETERS - 1);
    (offset + feature * 59) & (PAGE_PARAMETERS - 1)
}

fn feature_sign(a: u8, b: u8, feature: usize) -> f64 {
    match feature {
        0 => 1.0,
        1..=8 => {
            if (a >> (feature - 1)) & 1 != 0 {
                1.0
            } else {
                -1.0
            }
        }
        9..=16 => {
            if (b >> (feature - 9)) & 1 != 0 {
                1.0
            } else {
                -1.0
            }
        }
        _ => unreachable!("17 registered features"),
    }
}

fn sigmoid(x: f64) -> f64 {
    1.0 / (1.0 + (-x.clamp(-16.0, 16.0)).exp())
}

fn binary_loss_bits(probability: f64, target: bool) -> f64 {
    if target {
        -probability.log2()
    } else {
        -(1.0 - probability).log2()
    }
}

fn file_digest(file: &mut File) -> Result<String> {
    file.seek(SeekFrom::Start(0))?;
    let mut hash = Sha256::default();
    let mut chunk = [0u8; 1024 * 1024];
    loop {
        let count = file.read(&mut chunk)?;
        if count == 0 {
            break;
        }
        hash.update(&chunk[..count]);
    }
    Ok(hex(&hash.finish()))
}

impl SparseGradientAtlas {
    /// Creates real train-time master and exact touched-bit files.
    /// File length is allocated logically: filesystem free-space/residency
    /// must be independently checked before large physical training.
    pub fn create(root: &Path, config: SparseConfig) -> Result<Self> {
        let config = config.validate()?;
        fs::create_dir(root)?;
        let master = OpenOptions::new()
            .write(true)
            .read(true)
            .create_new(true)
            .open(root.join("master.f32"))?;
        let touches = OpenOptions::new()
            .write(true)
            .read(true)
            .create_new(true)
            .open(root.join("touched.bit"))?;
        master.set_len(config.train_master_bytes())?;
        touches.set_len(config.pages * TOUCH_BYTES as u64)?;
        Ok(Self {
            config,
            master,
            touches,
            cache: Vec::new(),
            tick: 0,
            counters: SparseCounters::default(),
        })
    }

    pub fn reopen(root: &Path, config: SparseConfig) -> Result<Self> {
        let config = config.validate()?;
        let dir = fs::symlink_metadata(root)?;
        if !dir.is_dir() || dir.file_type().is_symlink() {
            return Err("snapshot directory must not be symlink".into());
        }
        let mut master = OpenOptions::new()
            .read(true)
            .write(true)
            .open(root.join("master.f32"))?;
        let mut touches = OpenOptions::new()
            .read(true)
            .write(true)
            .open(root.join("touched.bit"))?;
        if master.metadata()?.len() != config.train_master_bytes()
            || touches.metadata()?.len() != config.pages * TOUCH_BYTES as u64
        {
            return Err("wrong model file dimensions".into());
        }
        let manifest = fs::read_to_string(root.join("snapshot.tsv"))?;
        let fields: Vec<_> = manifest.trim_end().split('\t').collect();
        if fields.len() != 10
            || fields[0] != VERSION
            || fields[1].parse::<u64>()? != config.pages
            || fields[2].parse::<usize>()? != config.max_cached_pages
            || fields[3].parse::<u32>()? != config.learning_rate.to_bits()
            || fields[8] != file_digest(&mut master)?
            || fields[9] != file_digest(&mut touches)?
        {
            return Err("snapshot signature/configuration mismatch".into());
        }
        let counters = SparseCounters {
            supervised_bits: fields[4].parse()?,
            parameter_updates: fields[5].parse()?,
            distinct_updated_parameters: fields[6].parse()?,
            page_loads: fields[7].parse()?,
            // only cache load and update counts are persisted; evictions restart.
            page_evictions: 0,
        };
        if counters.distinct_updated_parameters > config.total_parameters()
            || counters.parameter_updates < counters.distinct_updated_parameters
        {
            return Err("corrupt update accounting".into());
        }
        Ok(Self {
            config,
            master,
            touches,
            cache: Vec::new(),
            tick: 0,
            counters,
        })
    }

    fn flush_at(&mut self, index: usize) -> Result<()> {
        if !self.cache[index].dirty {
            return Ok(());
        }
        let page = &mut self.cache[index];
        self.master
            .seek(SeekFrom::Start(page.id * (PAGE_PARAMETERS as u64 * 4)))?;
        let mut buf = [0u8; PAGE_PARAMETERS * 4];
        for (slot, weight) in page.master.iter().enumerate() {
            if !weight.is_finite() {
                return Err("nonfinite master weight".into());
            }
            buf[slot * 4..slot * 4 + 4].copy_from_slice(&weight.to_le_bytes());
        }
        self.master.write_all(&buf)?;
        self.touches
            .seek(SeekFrom::Start(page.id * TOUCH_BYTES as u64))?;
        self.touches.write_all(&page.touched)?;
        page.dirty = false;
        Ok(())
    }

    fn cached(&mut self, id: u64) -> Result<usize> {
        self.tick = self.tick.checked_add(1).ok_or("LRU tick overflow")?;
        if let Some(index) = self.cache.iter().position(|page| page.id == id) {
            self.cache[index].used = self.tick;
            return Ok(index);
        }
        if self.cache.len() == self.config.max_cached_pages {
            let victim = self
                .cache
                .iter()
                .enumerate()
                .min_by_key(|(_, p)| p.used)
                .map(|(i, _)| i)
                .ok_or("missing resident page")?;
            self.flush_at(victim)?;
            self.cache.swap_remove(victim);
            self.counters.page_evictions = self
                .counters
                .page_evictions
                .checked_add(1)
                .ok_or("eviction counter overflow")?;
        }
        self.master
            .seek(SeekFrom::Start(id * PAGE_PARAMETERS as u64 * 4))?;
        let mut bytes = [0u8; PAGE_PARAMETERS * 4];
        self.master.read_exact(&mut bytes)?;
        let mut values = Box::new([0f32; PAGE_PARAMETERS]);
        for (index, dest) in values.iter_mut().enumerate() {
            *dest = f32::from_le_bytes(bytes[4 * index..4 * index + 4].try_into()?);
            if !dest.is_finite() {
                return Err("nonfinite restored weight".into());
            }
        }
        let mut touched = [0u8; TOUCH_BYTES];
        self.touches
            .seek(SeekFrom::Start(id * TOUCH_BYTES as u64))?;
        self.touches.read_exact(&mut touched)?;
        self.cache.push(Resident {
            id,
            master: values,
            touched,
            dirty: false,
            used: self.tick,
        });
        self.counters.page_loads = self
            .counters
            .page_loads
            .checked_add(1)
            .ok_or("page load counter overflow")?;
        Ok(self.cache.len() - 1)
    }

    fn bit_probability(&mut self, a: u8, b: u8, bit: usize) -> Result<(usize, f64)> {
        if bit >= 8 {
            return Err("bit output outside 0..8".into());
        }
        let page = self.cached(route(a, b, bit, self.config.pages))?;
        let mut score = 0.0f64;
        for feature in 0..FEATURES {
            let index = feature_slot(a, b, bit, feature);
            score += f64::from(self.cache[page].master[index]) * feature_sign(a, b, feature);
        }
        Ok((page, sigmoid(score / SCALE)))
    }

    /// Online logistic cross-entropy, *actual gradient updates* of selected
    /// master scalars, with the exact first-touch bitmap persisted on disk.
    pub fn train_bit(&mut self, a: u8, b: u8, bit: usize, label: bool) -> Result<f64> {
        let (page, p) = self.bit_probability(a, b, bit)?;
        let loss = binary_loss_bits(p, label);
        let derivative = (p - f64::from(u8::from(label))) / SCALE;
        let eta = f64::from(self.config.learning_rate);
        for feature in 0..FEATURES {
            let index = feature_slot(a, b, bit, feature);
            let step = eta * derivative * feature_sign(a, b, feature);
            let before = self.cache[page].master[index];
            let after = (f64::from(before) - step).clamp(-4.0, 4.0) as f32;
            if !after.is_finite() {
                return Err("nonfinite gradient update".into());
            }
            self.cache[page].master[index] = after;
            let byte = index / 8;
            let mask = 1u8 << (index % 8);
            if self.cache[page].touched[byte] & mask == 0 {
                self.cache[page].touched[byte] |= mask;
                self.counters.distinct_updated_parameters = self
                    .counters
                    .distinct_updated_parameters
                    .checked_add(1)
                    .ok_or("unique touch overflow")?;
            }
            self.counters.parameter_updates = self
                .counters
                .parameter_updates
                .checked_add(1)
                .ok_or("update count overflow")?;
        }
        self.cache[page].dirty = true;
        self.counters.supervised_bits = self
            .counters
            .supervised_bits
            .checked_add(1)
            .ok_or("supervised count overflow")?;
        Ok(loss)
    }

    pub fn train_document(&mut self, document: &[u8]) -> Result<(f64, u64)> {
        let mut loss = 0.0;
        let mut predicted_bytes = 0u64;
        for window in document.windows(3) {
            for bit in 0..8 {
                loss += self.train_bit(window[0], window[1], bit, window[2] & (1 << bit) != 0)?;
            }
            predicted_bytes = predicted_bytes
                .checked_add(1)
                .ok_or("document event overflow")?;
        }
        if !loss.is_finite() {
            return Err("nonfinite training loss".into());
        }
        Ok((loss, predicted_bytes))
    }

    /// Deterministic bounded block-shuffled SGD. Groups each consecutive
    /// block of token prediction positions by routed page to avoid one
    /// physical page fault per bit. Stable sorting preserves temporal
    /// order *within* each page; different pages are independent weights.
    ///
    /// The result is mathematically identical to sequential train_document
    /// when there are no shared weights across different pages. No data
    /// outside the current document influences the prediction context.
    pub fn train_document_grouped(
        &mut self,
        document: &[u8],
        batch_bytes: usize,
    ) -> Result<(f64, u64)> {
        if !(1..=8192).contains(&batch_bytes) {
            return Err("invalid page batch window (1..=8192 prediction bytes)".into());
        }
        let predicted = document.len().saturating_sub(2);
        let mut loss = 0f64;
        for begin in (0..predicted).step_by(batch_bytes) {
            let end = (begin + batch_bytes).min(predicted);
            let mut scheduled = Vec::with_capacity((end - begin) * 8);
            for position in begin..end {
                let a = document[position];
                let b = document[position + 1];
                for bit in 0..8usize {
                    let page = route(a, b, bit, self.config.pages);
                    scheduled.push((page, position, bit));
                }
            }
            scheduled.sort_by_key(|entry| entry.0);
            for (_, position, bit) in scheduled {
                let target = document[position + 2] & (1 << bit) != 0;
                loss += self.train_bit(document[position], document[position + 1], bit, target)?;
            }
        }
        if !loss.is_finite() {
            return Err("nonfinite grouped training loss".into());
        }
        Ok((loss, predicted as u64))
    }

    pub fn evaluate_document(&mut self, document: &[u8]) -> Result<(f64, u64)> {
        let mut loss = 0.0;
        let mut predicted_bytes = 0u64;
        for window in document.windows(3) {
            for bit in 0..8 {
                let (_, p) = self.bit_probability(window[0], window[1], bit)?;
                loss += binary_loss_bits(p, window[2] & (1 << bit) != 0);
            }
            predicted_bytes = predicted_bytes
                .checked_add(1)
                .ok_or("document event overflow")?;
        }
        if !loss.is_finite() {
            return Err("nonfinite DEV loss".into());
        }
        Ok((loss, predicted_bytes))
    }

    /// Exact held-out prediction with page-local data access. Unlike SGD,
    /// score order is immaterial because weights are immutable in this method.
    pub fn evaluate_document_grouped(
        &mut self,
        document: &[u8],
        batch_bytes: usize,
    ) -> Result<(f64, u64)> {
        if !(1..=8192).contains(&batch_bytes) {
            return Err("invalid held-out page batch window".into());
        }
        let predicted = document.len().saturating_sub(2);
        let mut loss = 0f64;
        for begin in (0..predicted).step_by(batch_bytes) {
            let end = (begin + batch_bytes).min(predicted);
            let mut scheduled = Vec::with_capacity((end - begin) * 8);
            for position in begin..end {
                for bit in 0..8usize {
                    scheduled.push((
                        route(
                            document[position],
                            document[position + 1],
                            bit,
                            self.config.pages,
                        ),
                        position,
                        bit,
                    ));
                }
            }
            scheduled.sort_by_key(|entry| entry.0);
            for (_, pos, bit) in scheduled {
                let (_, p) = self.bit_probability(document[pos], document[pos + 1], bit)?;
                loss += binary_loss_bits(p, document[pos + 2] & (1 << bit) != 0);
            }
        }
        if !loss.is_finite() {
            return Err("nonfinite grouped DEV loss".into());
        }
        Ok((loss, predicted as u64))
    }

    #[must_use]
    pub fn counters(&self) -> SparseCounters {
        self.counters
    }

    #[must_use]
    pub fn config(&self) -> SparseConfig {
        self.config
    }

    pub fn flush(&mut self) -> Result<()> {
        for index in 0..self.cache.len() {
            self.flush_at(index)?;
        }
        self.master.sync_all()?;
        self.touches.sync_all()?;
        Ok(())
    }

    /// Fail-closed resumable snapshot. Interrupted writes invalidate the prior
    /// checkpoint by digest, rather than silently resuming inconsistent weights.
    pub fn checkpoint(&mut self, root: &Path) -> Result<()> {
        self.flush()?;
        let master_sha = file_digest(&mut self.master)?;
        let touches_sha = file_digest(&mut self.touches)?;
        let manifest = format!(
            "{VERSION}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{master_sha}\t{touches_sha}\n",
            self.config.pages,
            self.config.max_cached_pages,
            self.config.learning_rate.to_bits(),
            self.counters.supervised_bits,
            self.counters.parameter_updates,
            self.counters.distinct_updated_parameters,
            self.counters.page_loads
        );
        let tmp = root.join("snapshot.tsv.tmp");
        let mut file = OpenOptions::new().write(true).create_new(true).open(&tmp)?;
        file.write_all(manifest.as_bytes())?;
        file.sync_all()?;
        fs::rename(tmp, root.join("snapshot.tsv"))?;
        File::open(root)?.sync_all()?;
        Ok(())
    }

    /// 4-bit signed hard weights; no trained-fraction inflation, no hidden
    /// float master included in inference payload. Quantization error MUST be
    /// measured on independent DEV before claiming an inference candidate.
    pub fn export_q4(&mut self, output: &Path) -> Result<()> {
        self.flush()?;
        self.master.seek(SeekFrom::Start(0))?;
        let mut packed = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(output)?;
        let mut buf = [0u8; PAGE_PARAMETERS * 4];
        for _ in 0..self.config.pages {
            self.master.read_exact(&mut buf)?;
            let mut encoded = [0u8; PAGE_PARAMETERS / 2];
            for (i, item) in encoded.iter_mut().enumerate() {
                let a = f32::from_le_bytes(buf[8 * i..8 * i + 4].try_into()?);
                let b = f32::from_le_bytes(buf[8 * i + 4..8 * i + 8].try_into()?);
                if !a.is_finite() || !b.is_finite() {
                    return Err("cannot quantize nonfinite master".into());
                }
                let qa = (a * 2.0).round().clamp(-8.0, 7.0) as i8;
                let qb = (b * 2.0).round().clamp(-8.0, 7.0) as i8;
                *item = (qa as u8 & 0xf) | ((qb as u8 & 0xf) << 4);
            }
            packed.write_all(&encoded)?;
        }
        packed.sync_all()?;
        Ok(())
    }
}

/// Read-only 4-bit inference atlas. No f32 masters are loaded, no weights are
/// changed, and the page cache contains only bounded 512-byte Q4 pages.
/// Q4 loss is explicitly measured rather than inferred from master loss.
pub struct QuantizedSparseAtlas {
    config: SparseConfig,
    packed: File,
    cache: Vec<QuantizedPage>,
    tick: u64,
    page_loads: u64,
}

struct QuantizedPage {
    id: u64,
    nibble_bytes: [u8; PAGE_PARAMETERS / 2],
    used: u64,
}

impl QuantizedSparseAtlas {
    pub fn open(path: &Path, config: SparseConfig) -> Result<Self> {
        let config = config.validate()?;
        let info = fs::symlink_metadata(path)?;
        if !info.is_file()
            || info.file_type().is_symlink()
            || info.len() != config.packed_q4_bytes()
        {
            return Err("Q4 file must have exact model dimensions".into());
        }
        let packed = File::open(path)?;
        Ok(Self {
            config,
            packed,
            cache: Vec::new(),
            tick: 0,
            page_loads: 0,
        })
    }

    fn load(&mut self, id: u64) -> Result<usize> {
        if id >= self.config.pages {
            return Err("Q4 page outside model atlas".into());
        }
        self.tick = self.tick.checked_add(1).ok_or("Q4 LRU overflow")?;
        if let Some(index) = self.cache.iter().position(|p| p.id == id) {
            self.cache[index].used = self.tick;
            return Ok(index);
        }
        if self.cache.len() == self.config.max_cached_pages {
            let old = self
                .cache
                .iter()
                .enumerate()
                .min_by_key(|(_, p)| p.used)
                .map(|(idx, _)| idx)
                .ok_or("Q4 cache eviction without resident")?;
            self.cache.swap_remove(old);
        }
        let mut bytes = [0u8; PAGE_PARAMETERS / 2];
        self.packed
            .seek(SeekFrom::Start(id * (PAGE_PARAMETERS / 2) as u64))?;
        self.packed.read_exact(&mut bytes)?;
        self.cache.push(QuantizedPage {
            id,
            nibble_bytes: bytes,
            used: self.tick,
        });
        self.page_loads = self.page_loads.checked_add(1).ok_or("Q4 load overflow")?;
        Ok(self.cache.len() - 1)
    }

    pub fn probability(&mut self, a: u8, b: u8, bit: usize) -> Result<f64> {
        if bit >= 8 {
            return Err("invalid output bit".into());
        }
        let page_id = route(a, b, bit, self.config.pages);
        let index = self.load(page_id)?;
        let mut sum = 0f64;
        for feature in 0..FEATURES {
            let slot = feature_slot(a, b, bit, feature);
            let encoded = self.cache[index].nibble_bytes[slot / 2];
            let nibble = if slot % 2 == 0 {
                encoded & 15
            } else {
                encoded >> 4
            };
            let signed = if nibble >= 8 {
                nibble as i8 - 16
            } else {
                nibble as i8
            };
            sum += (f64::from(signed) / 2.0) * feature_sign(a, b, feature);
        }
        Ok(sigmoid(sum / SCALE))
    }

    pub fn evaluate_document(&mut self, document: &[u8]) -> Result<(f64, u64)> {
        let mut loss = 0f64;
        for window in document.windows(3) {
            for bit in 0..8 {
                let p = self.probability(window[0], window[1], bit)?;
                loss += binary_loss_bits(p, window[2] & (1 << bit) != 0);
            }
        }
        if !loss.is_finite() {
            return Err("Q4 inference loss is nonfinite".into());
        }
        Ok((loss, document.len().saturating_sub(2) as u64))
    }

    pub fn evaluate_document_grouped(
        &mut self,
        document: &[u8],
        batch_bytes: usize,
    ) -> Result<(f64, u64)> {
        if !(1..=8192).contains(&batch_bytes) {
            return Err("invalid Q4 evaluation batch window".into());
        }
        let predicted = document.len().saturating_sub(2);
        let mut loss = 0f64;
        for begin in (0..predicted).step_by(batch_bytes) {
            let end = (begin + batch_bytes).min(predicted);
            let mut scheduled = Vec::with_capacity((end - begin) * 8);
            for position in begin..end {
                for bit in 0..8usize {
                    scheduled.push((
                        route(
                            document[position],
                            document[position + 1],
                            bit,
                            self.config.pages,
                        ),
                        position,
                        bit,
                    ));
                }
            }
            scheduled.sort_by_key(|entry| entry.0);
            for (_, pos, bit) in scheduled {
                let probability = self.probability(document[pos], document[pos + 1], bit)?;
                loss += binary_loss_bits(probability, document[pos + 2] & (1 << bit) != 0);
            }
        }
        if !loss.is_finite() {
            return Err("nonfinite grouped Q4 loss".into());
        }
        Ok((loss, predicted as u64))
    }

    pub fn resident_bytes_upper_bound(&self) -> usize {
        self.config.max_cached_pages * (PAGE_PARAMETERS / 2)
    }

    pub fn page_loads(&self) -> u64 {
        self.page_loads
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp(label: &str) -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("sml-learn-{label}-{}-{nonce}", std::process::id()))
    }

    fn cfg(pages: u64, resident: usize) -> SparseConfig {
        SparseConfig {
            pages,
            max_cached_pages: resident,
            learning_rate: 0.3,
        }
    }

    #[test]
    fn supervision_improves_a_real_logloss_and_updates_real_weights() {
        let dir = temp("fit");
        let mut m = SparseGradientAtlas::create(&dir, cfg(32, 2)).unwrap();
        let example = vec![b'A'; 256];
        let (before, n) = m.evaluate_document(&example).unwrap();
        assert!((before / n as f64 - 8.0).abs() < 1e-10);
        for _ in 0..3 {
            m.train_document(&example).unwrap();
        }
        let (after, count) = m.evaluate_document(&example).unwrap();
        assert_eq!(n, count);
        assert!(after < before * 0.7, "before {before} after {after}");
        assert!(m.counters().distinct_updated_parameters > 0);
        assert!(m.counters().distinct_updated_parameters <= m.config().total_parameters());
        assert!(m.counters().parameter_updates > m.counters().distinct_updated_parameters);
        assert_eq!(m.counters().supervised_bits, 3 * (256 - 2) * 8);
        m.checkpoint(&dir).unwrap();
        let mut reopened = SparseGradientAtlas::reopen(&dir, cfg(32, 2)).unwrap();
        let (resumed, _) = reopened.evaluate_document(&example).unwrap();
        assert!((resumed - after).abs() < 1e-9);
        assert_eq!(
            m.counters().distinct_updated_parameters,
            reopened.counters().distinct_updated_parameters
        );
        reopened.export_q4(&dir.join("weights.q4")).unwrap();
        assert_eq!(
            fs::metadata(dir.join("weights.q4")).unwrap().len(),
            32 * 512
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn only_routed_pages_learn_and_caches_are_bounded() {
        let dir = temp("evict");
        let mut m = SparseGradientAtlas::create(&dir, cfg(4096, 1)).unwrap();
        assert_eq!(m.config().total_parameters(), 4_194_304);
        assert_eq!(m.config().packed_q4_bytes(), 2_097_152);
        for n in 0u16..120u16 {
            m.train_bit(n as u8, (n * 3) as u8, (n as usize) % 8, n % 2 == 0)
                .unwrap();
        }
        assert!(m.counters().page_evictions > 0);
        assert!(m.cache.len() <= 1);
        assert!(m.counters().distinct_updated_parameters <= 17 * 120);
        m.checkpoint(&dir).unwrap();
        let mut other = SparseGradientAtlas::reopen(&dir, cfg(4096, 1)).unwrap();
        other.train_bit(7, 16, 2, true).unwrap();
        assert_eq!(other.counters().supervised_bits, 121);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn checksum_and_config_mismatch_fail_closed() {
        let dir = temp("corrupt");
        let mut m = SparseGradientAtlas::create(&dir, cfg(4, 1)).unwrap();
        m.train_bit(2, 3, 1, true).unwrap();
        m.checkpoint(&dir).unwrap();
        assert!(SparseGradientAtlas::reopen(&dir, cfg(8, 1)).is_err());
        assert!(SparseGradientAtlas::reopen(&dir, cfg(4, 2)).is_err());
        let mut weights = OpenOptions::new()
            .write(true)
            .open(dir.join("master.f32"))
            .unwrap();
        weights.seek(SeekFrom::Start(0)).unwrap();
        weights.write_all(&[255; 4]).unwrap();
        weights.sync_all().unwrap();
        assert!(SparseGradientAtlas::reopen(&dir, cfg(4, 1)).is_err());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn grouped_gradient_matches_online_weights_across_page_routing() {
        let first = temp("online");
        let second = temp("grouped");
        let config = cfg(64, 1);
        let mut serial = SparseGradientAtlas::create(&first, config).unwrap();
        let mut grouped = SparseGradientAtlas::create(&second, config).unwrap();
        let bytes: Vec<u8> = (0..3000usize)
            .map(|n| (n.wrapping_mul(13) ^ (n / 7)) as u8)
            .collect();
        let (_, events) = serial.train_document(&bytes).unwrap();
        let (_, grouped_events) = grouped.train_document_grouped(&bytes, 512).unwrap();
        assert_eq!(events, grouped_events);
        assert_eq!(
            serial.counters().supervised_bits,
            grouped.counters().supervised_bits
        );
        assert_eq!(
            serial.counters().distinct_updated_parameters,
            grouped.counters().distinct_updated_parameters
        );
        // Different selected pages have disjoint weights. The result is
        // identical for each page despite reordering across pages.
        serial.flush().unwrap();
        grouped.flush().unwrap();
        assert_eq!(full_master(&first), full_master(&second));
        assert_eq!(
            fs::read(first.join("touched.bit")).unwrap(),
            fs::read(second.join("touched.bit")).unwrap()
        );
        assert!(grouped.counters().page_loads < serial.counters().page_loads);
        let (online_dev, count_online) = serial.evaluate_document(&bytes).unwrap();
        let (grouped_dev, count_grouped) = grouped.evaluate_document_grouped(&bytes, 512).unwrap();
        assert_eq!(count_online, count_grouped);
        assert!(
            (online_dev / count_online as f64 - grouped_dev / count_grouped as f64).abs() < 1e-10
        );
        assert!(grouped.train_document_grouped(&bytes, 0).is_err());
        assert!(grouped.train_document_grouped(&bytes, 8193).is_err());
        fs::remove_dir_all(first).unwrap();
        fs::remove_dir_all(second).unwrap();
    }

    fn full_master(dir: &Path) -> Vec<u8> {
        fs::read(dir.join("master.f32")).unwrap()
    }

    #[test]
    fn q4_is_a_real_bounded_read_only_inference_artifact() {
        let root = temp("q4");
        let config = cfg(256, 2);
        let mut learn = SparseGradientAtlas::create(&root, config).unwrap();
        let bytes = vec![b'Q'; 500];
        learn.train_document_grouped(&bytes, 128).unwrap();
        learn.export_q4(&root.join("weights.q4")).unwrap();
        let before = fs::read(root.join("weights.q4")).unwrap();
        let mut inference = QuantizedSparseAtlas::open(&root.join("weights.q4"), config).unwrap();
        let (loss, n) = inference.evaluate_document(&bytes).unwrap();
        assert_eq!(n, 498);
        assert!(loss.is_finite());
        assert!(loss / (n as f64) < 8.0);
        assert_eq!(inference.resident_bytes_upper_bound(), 1024);
        let (grouped_loss, grouped_n) = inference.evaluate_document_grouped(&bytes, 64).unwrap();
        assert_eq!(grouped_n, n);
        assert!((loss / n as f64 - grouped_loss / grouped_n as f64).abs() < 1e-10);
        assert!(inference.page_loads() > 0);
        assert_eq!(fs::read(root.join("weights.q4")).unwrap(), before);
        assert!(QuantizedSparseAtlas::open(&root.join("weights.q4"), cfg(257, 2)).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn capacity_does_not_allocate_capacity_in_ram() {
        let c = cfg(25_000_000_000u64.div_ceil(PAGE_PARAMETERS as u64), 8);
        assert!(c.validate().is_ok());
        assert!(c.total_parameters() >= 25_000_000_000);
        assert!(c.resident_payload_bound() <= 34_000);
        assert!(c.train_master_bytes() >= 100_000_000_000);
        assert!(c.packed_q4_bytes() >= 12_500_000_000);
    }
}
