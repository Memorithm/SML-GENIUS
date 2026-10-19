//! Compare MSB tree and independent-bit sparse atlas on disjoint source formats.
//! TRAIN Markdown, DEV Rust code plus independent IID bytes. No SEC corpus.
use sml_core::{
    byte_factorization::ByteFactorization,
    elastic_ngram_paged_sha::{hex, Sha256},
    sparse_gradient_atlas::{QuantizedSparseAtlas, SparseConfig, SparseGradientAtlas},
};
use std::{collections::HashSet, env, error::Error, fs, path::Path, time::Instant};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const TRAIN_FILES: [&str; 3] = ["README.md", "BOOTSTRAP.md", "docs/SML-0-SPEC.md"];
const DEV_FILES: [&str; 3] = [
    "crates/sml-core/src/byte_factorization.rs",
    "crates/sml-core/src/rtlr_cost_gate.rs",
    "crates/sml-core/src/anf_delay_line.rs",
];
const TRAIN_CAP: usize = 16_384;
const DEV_CAP: usize = 16_384;
const EPOCHS: usize = 2;
const IID_LEN: usize = 16_384;

fn sha(data: &[u8]) -> String {
    let mut h = Sha256::default();
    h.update(data);
    hex(&h.finish())
}
fn read_bounded(root: &Path, relative: &str, limit: usize) -> Result<Vec<u8>> {
    if relative.starts_with('/') || relative.contains("..") {
        return Err("unsafe fixture path".into());
    }
    let path = root.join(relative);
    let meta = fs::symlink_metadata(&path)?;
    if !meta.is_file() || meta.file_type().is_symlink() || meta.len() < 3 || meta.len() > 2_000_000 {
        return Err(format!("invalid fixture file: {relative}").into());
    }
    let mut data = fs::read(path)?;
    data.truncate(limit);
    if data.len() < 3 {
        return Err("fixture is shorter than one supervised window".into());
    }
    Ok(data)
}
fn rng_next(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut x = *state;
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}
fn iid_control() -> Vec<u8> {
    let mut state = 0x871e_17ca_456f_1042_u64;
    (0..IID_LEN).map(|_| rng_next(&mut state) as u8).collect()
}
fn load_panels(root: &Path) -> Result<(Vec<Vec<u8>>, Vec<(&'static str, Vec<u8>)>)> {
    let train = TRAIN_FILES
        .iter()
        .map(|path| read_bounded(root, path, TRAIN_CAP))
        .collect::<Result<Vec<_>>>()?;
    let mut dev = DEV_FILES
        .iter()
        .map(|path| Ok((*path, read_bounded(root, path, DEV_CAP)?)))
        .collect::<Result<Vec<_>>>()?;
    dev.push(("iid_splitmix64_unseen", iid_control()));
    let train_hashes: HashSet<_> = train.iter().map(|doc| sha(doc)).collect();
    if train_hashes.len() != train.len()
        || dev.iter().any(|(_, doc)| train_hashes.contains(&sha(doc)))
        || dev.iter().map(|(_, doc)| sha(doc)).collect::<HashSet<_>>().len() != dev.len()
    {
        return Err("TRAIN/DEV fixture SHA collision or duplicate".into());
    }
    Ok((train, dev))
}
fn finite_rate(count: usize, secs: f64) -> Result<f64> {
    if !secs.is_finite() || secs <= 0.0 {
        return Err("nonfinite or zero elapsed seconds".into());
    }
    Ok(count as f64 / secs / 1e6)
}
fn run(root: &Path, output: &Path, seed: u64) -> Result<()> {
    if ![11, 23, 37].contains(&seed) {
        return Err("seed must be 11, 23 or 37".into());
    }
    let (train, dev) = load_panels(root)?;
    fs::create_dir(output)?;
    let config = SparseConfig { pages: 128, max_cached_pages: 128, learning_rate: 0.3 };
    let expected_events = train.iter().map(|d| d.len() - 2).sum::<usize>() * EPOCHS;
    let mut lines = Vec::new();
    for (tag, factorization) in [
        ("independent", ByteFactorization::IndependentBits),
        ("msb_tree", ByteFactorization::MsbTree),
    ] {
        let dir = output.join(tag);
        let mut model = SparseGradientAtlas::create_with_factorization(&dir, config, factorization)?;
        let begin = Instant::now();
        let mut positions = 0_usize;
        // Only TRAIN document ordering changes; both arms receive identical data.
        for epoch in 0..EPOCHS {
            for offset in 0..train.len() {
                let index = (offset + epoch + seed as usize) % train.len();
                let (_, n) = model.train_document(&train[index])?;
                positions += n as usize;
            }
        }
        if positions != expected_events || model.factorization() != factorization {
            return Err("TRAIN position/factorization mismatch".into());
        }
        let train_s = begin.elapsed().as_secs_f64();
        let train_rate = finite_rate(positions, train_s)?;
        let touched = model.counters().distinct_updated_parameters;
        if touched == 0 || touched > config.total_parameters() {
            return Err("invalid touched master parameter accounting".into());
        }
        model.checkpoint(&dir)?;
        model.export_q4(&dir.join("weights.q4"))?;
        drop(model);
        let mut reopened = SparseGradientAtlas::reopen(&dir, config)?;
        if reopened.factorization() != factorization {
            return Err("factorization did not survive checkpoint reopen".into());
        }
        let mut quant = QuantizedSparseAtlas::open_with_factorization(
            &dir.join("weights.q4"), config, factorization,
        )?;
        for (panel, doc) in &dev {
            let tick = Instant::now();
            let (loss, events) = reopened.evaluate_document(doc)?;
            let dev_s = tick.elapsed().as_secs_f64();
            let (q4_loss, q4_events) = quant.evaluate_document(doc)?;
            if events == 0 || events != q4_events || events != (doc.len() - 2) as u64 {
                return Err("Q4/F32 scored different DEV target positions".into());
            }
            let f32_bpc = loss / events as f64;
            let q4_bpc = q4_loss / events as f64;
            if !f32_bpc.is_finite() || !q4_bpc.is_finite() || f32_bpc < 0.0 || q4_bpc < 0.0 {
                return Err("invalid DEV loss".into());
            }
            let eval_rate = finite_rate(events as usize, dev_s)?;
            lines.push(format!(
                "MSB_TRANSFER_V1\t{seed}\t{panel}\t{tag}\t{positions}\t{}\t{touched}\t{}\t{}\t{}\t{}\t{f32_bpc:.9}\t{q4_bpc:.9}\t{train_rate:.6}\t{eval_rate:.6}",
                train.len(), config.total_parameters(), config.train_master_bytes(),
                config.packed_q4_bytes(), sha(doc),
            ));
        }
    }
    if lines.len() != 8 {
        return Err("incomplete source-format transfer comparison".into());
    }
    let manifest = lines.join("\n") + "\n";
    fs::write(output.join("evidence.tsv"), &manifest)?;
    print!("{manifest}");
    Ok(())
}
fn main() -> Result<()> {
    let args: Vec<_> = env::args().collect();
    if args.len() != 4 {
        return Err("usage: sml-msb-transfer-bench REPO_ROOT NEW_OUTPUT_DIR SEED(11|23|37)".into());
    }
    run(Path::new(&args[1]), Path::new(&args[2]), args[3].parse()?)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dev_filenames_and_iid_are_disjoint_and_deterministic() {
        for path in TRAIN_FILES {
            assert!(!DEV_FILES.contains(&path));
        }
        assert_eq!(iid_control().len(), IID_LEN);
        assert_eq!(iid_control(), iid_control());
        assert_ne!(sha(&iid_control()), sha(&vec![0; IID_LEN]));
    }
    #[test]
    fn invalid_timing_and_paths_fail_closed() {
        assert!(finite_rate(1, 0.0).is_err());
        assert!(finite_rate(1, f64::NAN).is_err());
        assert!(finite_rate(65_536, 0.5).unwrap() > 0.1);
        assert!(!Path::new("docs/SML-0-SPEC.md").is_absolute());
    }
}
