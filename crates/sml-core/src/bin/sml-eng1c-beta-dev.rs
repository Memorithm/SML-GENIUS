//! Frozen exploratory beta × depth grid. Only registered TRAIN and DEV inputs.
use sml_core::{
    anf_char::{ChunkLayout, LANES},
    elastic_ngram_paged::{
        build_paged, EvidenceDepthReadout, PagedBuild, PagedLimits, PagedNGram, ENG1C_VAL_DEPTHS,
    },
    elastic_ngram_paged_sha::{hex, sha256},
    eng1c_dev::{
        extract_train_dev, frozen_dev_corpora, frozen_dev_corpus, DevCorpus, DEV_MANIFEST_SHA,
    },
};
use std::{
    collections::BTreeMap,
    error::Error,
    fs::{self, File, OpenOptions},
    io::Write,
    path::Path,
};
type Result<T> = std::result::Result<T, Box<dyn Error>>;
const BETAS: [f64; 6] = [1.0, 0.25, 0.5, 2.0, 4.0, 8.0];
const GRID_ARMS: usize = 24;
const TOTAL_ARMS: usize = 25;
const PROTOCOL: &str = include_str!("../../../../docs/ENG-1C-BETA-DEV-GRID-PROTOCOL.md");
fn policy_name(i: usize) -> String {
    if i == GRID_ARMS {
        "OLD_GATE_FR129_EN64".into()
    } else {
        format!("WB_BETA_{}_DEPTH_{}", BETAS[i / 4], ENG1C_VAL_DEPTHS[i % 4])
    }
}
fn gate_count(language: &str) -> Result<u64> {
    match language {
        "FR" => Ok(129),
        "EN" => Ok(64),
        _ => Err("unknown gate language".into()),
    }
}
fn gate(ladder: &[EvidenceDepthReadout; 4], count: u64) -> usize {
    (1..4)
        .rev()
        .find(|&i| ladder[i].context_events >= count)
        .unwrap_or(0)
}
fn best(bits: &[f64], end: usize) -> Result<usize> {
    if end == 0 || bits.len() < end || bits.iter().any(|x| !x.is_finite() || *x < 0.0) {
        return Err("invalid grid loss".into());
    }
    let mut index = 0;
    for i in 1..end {
        if bits[i] < bits[index] {
            index = i;
        }
    }
    Ok(index)
}
fn fresh(path: &Path, text: &str) -> Result<()> {
    let mut f = OpenOptions::new().write(true).create_new(true).open(path)?;
    f.write_all(text.as_bytes())?;
    f.sync_all()?;
    Ok(())
}
fn binary_sha() -> Result<String> {
    let p = std::env::current_exe()?;
    if fs::metadata(&p)?.len() > 64 * 1024 * 1024 {
        return Err("binary budget exceeded".into());
    }
    Ok(hex(&sha256(&fs::read(p)?)))
}
#[derive(Clone, Debug)]
struct GridScores {
    bits: Vec<f64>,
    tokens: u64,
    old_gate_depths: [u64; 4],
}
fn score_grid(model: &mut PagedNGram, dev: &[u8], language: &str) -> Result<GridScores> {
    let layout = ChunkLayout::new(dev.len())?;
    let min_count = gate_count(language)?;
    let mut result = GridScores {
        bits: vec![0.0; TOTAL_ARMS],
        tokens: 0,
        old_gate_depths: [0; 4],
    };
    for lane in 0..LANES {
        let chunk = &dev[lane * layout.chunk..(lane + 1) * layout.chunk];
        let mut lane_bits = [0.0; TOTAL_ARMS];
        for t in 1..chunk.len() {
            let readouts = model.depth_ladder_readouts_with_betas(&chunk[..t], &BETAS)?;
            let label = usize::from(chunk[t]);
            for (b, ladder) in readouts.iter().enumerate() {
                for (d, r) in ladder.iter().enumerate() {
                    lane_bits[b * 4 + d] -= r.probabilities[label].log2();
                }
            }
            let chosen = gate(&readouts[0], min_count);
            lane_bits[GRID_ARMS] -= readouts[0][chosen].probabilities[label].log2();
            result.old_gate_depths[chosen] += 1;
            result.tokens += 1;
        }
        for (total, lane) in result.bits.iter_mut().zip(lane_bits) {
            *total += lane;
        }
    }
    if result.tokens != layout.tokens()
        || result.old_gate_depths.iter().sum::<u64>() != result.tokens
    {
        return Err("incomplete DEV grid".into());
    }
    best(&result.bits, GRID_ARMS)?;
    Ok(result)
}
fn checked_splits(
    work: &DevCorpus,
    train_path: &Path,
    dev_path: &Path,
) -> Result<(Vec<u8>, Vec<u8>)> {
    if fs::metadata(train_path)?.len() != work.train_bytes as u64
        || fs::metadata(dev_path)?.len() != work.dev_bytes as u64
    {
        return Err("wrong TRAIN/DEV split length".into());
    }
    let train = fs::read(train_path)?;
    let dev = fs::read(dev_path)?;
    work.verify_splits(&train, &dev)?;
    Ok((train, dev))
}
fn prepare(id: &str, raw: &Path, train: &Path, dev: &Path) -> Result<()> {
    let work = frozen_dev_corpus(id)?;
    if train == dev
        || train.exists()
        || dev.exists()
        || fs::metadata(raw)?.len() != work.raw_bytes as u64
    {
        return Err("invalid source length/new split destinations".into());
    }
    let (training, development) = extract_train_dev(File::open(raw)?, &work)?;
    let mut t = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(train)?;
    t.write_all(&training)?;
    t.sync_all()?;
    let mut d = OpenOptions::new().create_new(true).write(true).open(dev)?;
    d.write_all(&development)?;
    d.sync_all()?;
    println!("BETA_DEV_PREPARE,id={id},train_sha256={},dev_sha256={},tail=UNREAD,test_file=NOT_CREATED,integrity=PASS",work.train_sha256,work.dev_sha256);
    Ok(())
}
fn grid(
    id: &str,
    train_path: &Path,
    dev_path: &Path,
    store: &Path,
    sort: &Path,
    out: &Path,
) -> Result<()> {
    let work = frozen_dev_corpus(id)?;
    if out.exists() {
        return Err("grid destination exists".into());
    }
    let (train, dev) = checked_splits(&work, train_path, dev_path)?;
    let PagedBuild::Published(meta) = build_paged(&train, 32, store, sort, PagedLimits::default())?
    else {
        return Err("TRAIN capacity stop".into());
    };
    let mut model = PagedNGram::open(store, sha256(&train))?;
    let stats = score_grid(&mut model, &dev, &work.language)?;
    let classic = best(&stats.bits, 4)?;
    let selected = best(&stats.bits, GRID_ARMS)?;
    let mut output=format!("BETA_DEV_META,id={id},language={},genre={},train_bytes={},train_sha256={},dev_bytes={},dev_sha256={},tokens={},manifest_sha256={DEV_MANIFEST_SHA},protocol_sha256={},binary_sha256={},store_sha256={},index_sha256={},policy_count={TOTAL_ARMS},quality_source=EXPLORATORY_DEV_ONLY,test=UNREAD,integrity=PASS\n",work.language,work.genre,work.train_bytes,work.train_sha256,work.dev_bytes,work.dev_sha256,stats.tokens,hex(&sha256(PROTOCOL.as_bytes())),binary_sha()?,hex(&meta.count_store_sha256),hex(&meta.sparse_index_sha256));
    for (i, bits) in stats.bits.iter().enumerate() {
        let (beta, depth) = if i == GRID_ARMS {
            ("1".into(), "ADAPTIVE".into())
        } else {
            (
                BETAS[i / 4].to_string(),
                ENG1C_VAL_DEPTHS[i % 4].to_string(),
            )
        };
        output.push_str(&format!("BETA_DEV_POLICY,id={id},policy={},beta={beta},depth={depth},tokens={},total_bits={bits},bpb={:.12},integrity=PASS\n",policy_name(i),stats.tokens,bits/stats.tokens as f64));
    }
    output.push_str(&format!("BETA_DEV_SELECTION,id={id},classic={},candidate={},gain_bpb={:.12},beta_boundary={},old_gate_depth4={},old_gate_depth7={},old_gate_depth12={},old_gate_depth32={},decision=EXPLORATORY_DEV_SELECTION_ONLY,test=UNREAD,integrity=PASS\n",policy_name(classic),policy_name(selected),(stats.bits[classic]-stats.bits[selected])/stats.tokens as f64,[0.25,8.0].contains(&BETAS[selected/4]),stats.old_gate_depths[0],stats.old_gate_depths[1],stats.old_gate_depths[2],stats.old_gate_depths[3]));
    fresh(out, &output)
}
fn fields(line: &str, prefix: &str) -> Result<BTreeMap<String, String>> {
    let mut map = BTreeMap::new();
    for pair in line
        .strip_prefix(prefix)
        .ok_or("wrong grid record type")?
        .split(',')
    {
        let (k, v) = pair.split_once('=').ok_or("invalid grid record field")?;
        if map.insert(k.into(), v.into()).is_some() {
            return Err("duplicate grid field".into());
        }
    }
    Ok(map)
}
fn field<'a>(map: &'a BTreeMap<String, String>, key: &str) -> Result<&'a str> {
    map.get(key)
        .map(String::as_str)
        .ok_or_else(|| format!("missing {key}").into())
}
struct Observed {
    work: DevCorpus,
    bits: Vec<f64>,
    tokens: u64,
}
fn read_grid(path: &Path, work: DevCorpus, executable: &str) -> Result<Observed> {
    if fs::metadata(path)?.len() > 65536 {
        return Err("grid evidence budget exceeded".into());
    }
    let raw = fs::read_to_string(path)?;
    let lines: Vec<_> = raw.lines().collect();
    if lines.len() != TOTAL_ARMS + 2 {
        return Err("incomplete grid evidence".into());
    }
    let meta = fields(lines[0], "BETA_DEV_META,")?;
    for (key, expected) in [
        ("id", work.id.as_str()),
        ("language", work.language.as_str()),
        ("genre", work.genre.as_str()),
        ("train_sha256", work.train_sha256.as_str()),
        ("dev_sha256", work.dev_sha256.as_str()),
        ("manifest_sha256", DEV_MANIFEST_SHA),
        ("binary_sha256", executable),
        ("quality_source", "EXPLORATORY_DEV_ONLY"),
        ("test", "UNREAD"),
        ("integrity", "PASS"),
    ] {
        if field(&meta, key)? != expected {
            return Err(format!("grid {key} identity drift").into());
        }
    }
    if field(&meta, "protocol_sha256")? != hex(&sha256(PROTOCOL.as_bytes()))
        || field(&meta, "train_bytes")?.parse::<usize>()? != work.train_bytes
        || field(&meta, "dev_bytes")?.parse::<usize>()? != work.dev_bytes
        || field(&meta, "policy_count")?.parse::<usize>()? != TOTAL_ARMS
    {
        return Err("grid metadata drift".into());
    }
    let tokens = ChunkLayout::new(work.dev_bytes)?.tokens();
    if field(&meta, "tokens")?.parse::<u64>()? != tokens {
        return Err("grid denominator drift".into());
    }
    let mut bits = Vec::new();
    for (i, line) in lines[1..=TOTAL_ARMS].iter().enumerate() {
        let row = fields(line, "BETA_DEV_POLICY,")?;
        let (beta, depth) = if i == GRID_ARMS {
            ("1".into(), "ADAPTIVE".into())
        } else {
            (
                BETAS[i / 4].to_string(),
                ENG1C_VAL_DEPTHS[i % 4].to_string(),
            )
        };
        if field(&row, "id")? != work.id
            || field(&row, "policy")? != policy_name(i)
            || field(&row, "beta")? != beta
            || field(&row, "depth")? != depth
            || field(&row, "tokens")?.parse::<u64>()? != tokens
            || field(&row, "integrity")? != "PASS"
        {
            return Err("missing/duplicate/changed grid policy".into());
        }
        let value = field(&row, "total_bits")?.parse::<f64>()?;
        let displayed = field(&row, "bpb")?.parse::<f64>()?;
        if !value.is_finite()
            || value < 0.0
            || !displayed.is_finite()
            || (displayed - value / tokens as f64).abs() > 1e-12
        {
            return Err("invalid grid probability score".into());
        }
        bits.push(value);
    }
    let selection = fields(lines[TOTAL_ARMS + 1], "BETA_DEV_SELECTION,")?;
    if field(&selection, "id")? != work.id
        || field(&selection, "classic")? != policy_name(best(&bits, 4)?)
        || field(&selection, "candidate")? != policy_name(best(&bits, GRID_ARMS)?)
        || field(&selection, "decision")? != "EXPLORATORY_DEV_SELECTION_ONLY"
        || field(&selection, "test")? != "UNREAD"
        || field(&selection, "integrity")? != "PASS"
    {
        return Err("grid selection drift".into());
    }
    Ok(Observed { work, bits, tokens })
}
fn summarize(dir: &Path, out: &Path) -> Result<()> {
    let executable = binary_sha()?;
    let mut observed = Vec::new();
    for work in frozen_dev_corpora()? {
        observed.push(read_grid(
            &dir.join(format!("{}-grid.txt", work.id)),
            work,
            &executable,
        )?);
    }
    let mut text=String::from("scope\tauthors\tclassic_dev_bpb\tselected_beta_dev_bpb\tselected_gain_bpb\told_gate_gain_bpb\n");
    for language in ["ALL", "FR", "EN"] {
        for bin in ["ALL", "small", "medium", "large"] {
            let group: Vec<_> = observed
                .iter()
                .filter(|o| {
                    (language == "ALL" || o.work.language == language)
                        && (bin == "ALL" || o.work.size_stratum() == bin)
                })
                .collect();
            if group.is_empty() {
                continue;
            }
            let n = group.len() as f64;
            let baseline = group
                .iter()
                .map(|o| Ok(o.bits[best(&o.bits, 4)?] / o.tokens as f64))
                .collect::<Result<Vec<_>>>()?
                .iter()
                .sum::<f64>()
                / n;
            let candidate = group
                .iter()
                .map(|o| Ok(o.bits[best(&o.bits, GRID_ARMS)?] / o.tokens as f64))
                .collect::<Result<Vec<_>>>()?
                .iter()
                .sum::<f64>()
                / n;
            let old = group
                .iter()
                .map(|o| o.bits[GRID_ARMS] / o.tokens as f64)
                .sum::<f64>()
                / n;
            text.push_str(&format!(
                "{language}_{bin}\t{}\t{baseline:.12}\t{candidate:.12}\t{:.12}\t{:.12}\n",
                group.len(),
                baseline - candidate,
                baseline - old
            ));
            for i in 0..GRID_ARMS {
                let mean = group
                    .iter()
                    .map(|o| o.bits[i] / o.tokens as f64)
                    .sum::<f64>()
                    / n;
                text.push_str(&format!(
                    "ARM\t{language}_{bin}\t{}\t{mean:.12}\t{:.12}\n",
                    policy_name(i),
                    baseline - mean
                ));
            }
        }
    }
    for o in observed {
        let classic = best(&o.bits, 4)?;
        let chosen = best(&o.bits, GRID_ARMS)?;
        text.push_str(&format!(
            "AUTHOR\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{:.12}\tboundary={}\n",
            o.work.id,
            o.work.language,
            o.work.size_stratum(),
            o.work.train_bytes,
            o.tokens,
            policy_name(classic),
            policy_name(chosen),
            (o.bits[classic] - o.bits[chosen]) / o.tokens as f64,
            [0.25, 8.0].contains(&BETAS[chosen / 4])
        ));
    }
    text.push_str("DECISION\tEXPLORATORY_DEV_GRID_ONLY\tselection_optimism=UNMEASURED\ttest=UNREAD\tintegrity=PASS\n");
    fresh(out, &text)
}
fn run(args: &[String]) -> Result<()> {
    match args.get(1).map(String::as_str){
    Some("prepare") if args.len()==6=>prepare(&args[2],Path::new(&args[3]),Path::new(&args[4]),Path::new(&args[5])),
    Some("grid") if args.len()==8=>grid(&args[2],Path::new(&args[3]),Path::new(&args[4]),Path::new(&args[5]),Path::new(&args[6]),Path::new(&args[7])),
    Some("summarize") if args.len()==4=>summarize(Path::new(&args[2]),Path::new(&args[3])),
    _=>Err("usage: sml-eng1c-beta-dev prepare ID RAW NEW_TRAIN NEW_DEV | grid ID TRAIN DEV NEW_STORE NEW_SORT NEW_GRID | summarize GRID_DIR NEW_SUMMARY; no TEST scoring command".into())
}
}
fn main() {
    if let Err(e) = run(&std::env::args().collect::<Vec<_>>()) {
        eprintln!("BETA_DEV_EXECUTION_STOP: {e}");
        std::process::exit(1);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn frozen_grid_order_endpoints_ties_and_test_command_rejection() {
        assert_eq!(BETAS, [1.0, 0.25, 0.5, 2.0, 4.0, 8.0]);
        assert_eq!(best(&[1.0; TOTAL_ARMS], GRID_ARMS).unwrap(), 0);
        assert_eq!(policy_name(0), "WB_BETA_1_DEPTH_4");
        assert_eq!(policy_name(23), "WB_BETA_8_DEPTH_32");
        assert_eq!(policy_name(24), "OLD_GATE_FR129_EN64");
        for mode in ["test", "--test", "--beta", "--depth"] {
            assert!(run(&["program".into(), mode.into()]).is_err());
        }
    }
    #[test]
    fn beta_one_dev_aggregates_match_legacy_and_grid_selection_is_nested() {
        let root = std::env::temp_dir().join(format!("beta-dev-fixture-{}", std::process::id()));
        fs::create_dir(&root).unwrap();
        let train = b"abcdaabcdefgabcdefgh".repeat(256);
        let dev = b"bcdaabcdefghhijklmno".repeat(128);
        assert!(matches!(
            build_paged(
                &train,
                32,
                &root.join("store"),
                &root.join("sort"),
                PagedLimits::default()
            )
            .unwrap(),
            PagedBuild::Published(_)
        ));
        let mut model = PagedNGram::open(&root.join("store"), sha256(&train)).unwrap();
        let measured = score_grid(&mut model, &dev, "FR").unwrap();
        let layout = ChunkLayout::new(dev.len()).unwrap();
        let mut legacy = [0.0; 4];
        for lane in 0..LANES {
            let chunk = &dev[lane * layout.chunk..(lane + 1) * layout.chunk];
            let mut bits = [0.0; 4];
            for t in 1..chunk.len() {
                let ladder = model.depth_ladder_readout(&chunk[..t]).unwrap();
                for (i, r) in ladder.iter().enumerate() {
                    bits[i] -= r.probabilities[usize::from(chunk[t])].log2();
                }
            }
            for (i, b) in bits.iter().enumerate() {
                legacy[i] += b;
            }
        }
        for (i, b) in legacy.iter().enumerate() {
            assert_eq!(b.to_bits(), measured.bits[i].to_bits());
        }
        assert!(
            measured.bits[best(&measured.bits, GRID_ARMS).unwrap()]
                <= measured.bits[best(&measured.bits, 4).unwrap()]
        );
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn wrong_split_identity_is_rejected_before_model_creation() {
        let root = std::env::temp_dir().join(format!("beta-dev-bad-split-{}", std::process::id()));
        fs::create_dir(&root).unwrap();
        let work = frozen_dev_corpus("17489").unwrap();
        let train = root.join("train");
        let dev = root.join("dev");
        fs::write(&train, vec![0; work.train_bytes]).unwrap();
        fs::write(&dev, vec![0; work.dev_bytes]).unwrap();
        assert!(grid(
            &work.id,
            &train,
            &dev,
            &root.join("store"),
            &root.join("sort"),
            &root.join("grid")
        )
        .is_err());
        assert!(!root.join("store").exists());
        assert!(!root.join("sort").exists());
        fs::remove_dir_all(root).unwrap();
    }
}
