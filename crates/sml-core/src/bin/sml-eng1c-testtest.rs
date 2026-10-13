//! Prospective two-phase confirmation. No threshold search and no TEST selection.
use sml_core::{
    anf_char::{ChunkLayout, LANES},
    elastic_ngram_paged::{
        build_paged, EscapePolicy, EvidenceDepthReadout, PagedBuild, PagedLimits, PagedNGram,
        ENG1C_VAL_DEPTHS,
    },
    elastic_ngram_paged_sha::{hex, sha256},
    paired_panel::{holm_two_pass, paired_group_interval, paired_summary},
};
use std::{
    collections::BTreeMap,
    error::Error,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const ROSTER: &str = include_str!("../../../../scripts/eng1c/testtest-roster.tsv");
const MANIFEST_HEADER: &str = "id\traw_bytes\traw_sha256\tbody_bytes\tbody_sha256\ttrain_bytes\ttrain_sha256\tdev_bytes\tdev_sha256\ttest_bytes\ttest_sha256";
const MAX_RAW: u64 = 16 * 1024 * 1024;
const FROZEN_MANIFEST_SHA: &str =
    "ffd768bfe236eb1c7e9cee689d84c50702d551315dfe3327ed9dced10074e990";

#[derive(Clone, Debug)]
struct Work {
    id: String,
    language: String,
    genre: String,
    author: String,
    title: String,
}
fn roster() -> Result<Vec<Work>> {
    let mut works = Vec::new();
    for line in ROSTER.lines().skip(1) {
        let f: Vec<_> = line.split('\t').collect();
        if f.len() != 6 {
            return Err("invalid compiled roster".into());
        }
        works.push(Work {
            id: f[0].into(),
            language: f[1].into(),
            genre: f[2].into(),
            author: f[3].into(),
            title: f[4].into(),
        });
    }
    validate_roster(&works)?;
    Ok(works)
}
fn validate_roster(works: &[Work]) -> Result<()> {
    if works.len() != 12 {
        return Err("incomplete author panel".into());
    }
    let mut ids = std::collections::BTreeSet::new();
    let mut authors = std::collections::BTreeSet::new();
    for w in works {
        if !ids.insert(&w.id)
            || !authors.insert(&w.author)
            || matches!(w.id.as_str(), "17989" | "1342" | "800" | "14155" | "516")
            || !w.id.bytes().all(|b| b.is_ascii_digit())
        {
            return Err("duplicate/excluded author or source".into());
        }
    }
    for language in ["FR", "EN"] {
        for genre in ["novel", "essay", "science"] {
            if works
                .iter()
                .filter(|w| w.language == language && w.genre == genre)
                .count()
                != 2
            {
                return Err("unbalanced language/genre panel".into());
            }
        }
    }
    Ok(())
}
fn read_bounded(path: &Path, max: u64) -> Result<Vec<u8>> {
    if fs::metadata(path)?.len() > max {
        return Err("input exceeds declared size budget".into());
    }
    let bytes = fs::read(path)?;
    if bytes.len() as u64 > max {
        return Err("input grew beyond budget".into());
    }
    Ok(bytes)
}
fn body(raw: &[u8], work: &Work) -> Result<Vec<u8>> {
    let text = std::str::from_utf8(raw)?.replace('\r', "");
    let lines: Vec<_> = text.split_inclusive('\n').collect();
    let starts: Vec<_> = lines
        .iter()
        .enumerate()
        .filter(|(_, s)| s.starts_with("*** START OF THE PROJECT GUTENBERG EBOOK "))
        .map(|(i, _)| i)
        .collect();
    let ends: Vec<_> = lines
        .iter()
        .enumerate()
        .filter(|(_, s)| s.starts_with("*** END OF THE PROJECT GUTENBERG EBOOK "))
        .map(|(i, _)| i)
        .collect();
    if starts.len() != 1 || ends.len() != 1 || starts[0] >= ends[0] {
        return Err("ambiguous Gutenberg body".into());
    }
    let header = &lines[..starts[0]];
    let language = if work.language == "FR" {
        "French"
    } else {
        "English"
    };
    for expected in [
        format!("Title: {}", work.title),
        format!("Author: {}", work.author),
        format!("Language: {language}"),
    ] {
        if header
            .iter()
            .filter(|s| s.trim_end_matches('\n') == expected)
            .count()
            != 1
        {
            return Err(format!("source identity mismatch for {}", work.id).into());
        }
    }
    Ok(lines[starts[0] + 1..ends[0]].concat().into_bytes())
}
#[derive(Clone, Debug)]
struct Splits {
    train: std::ops::Range<usize>,
    dev: std::ops::Range<usize>,
    test: std::ops::Range<usize>,
}
fn splits(n: usize) -> Result<Splits> {
    let a = n.checked_mul(3).ok_or("split overflow")? / 5;
    let b = n.checked_mul(4).ok_or("split overflow")? / 5;
    let s = Splits {
        train: 0..a.min(600000),
        dev: a..b.min(a + 32768),
        test: b..n.min(b + 65536),
    };
    if [&s.train, &s.dev, &s.test].iter().any(|r| r.len() < 4096) {
        return Err("work below frozen minimum split size".into());
    }
    Ok(s)
}
fn fingerprint(work: &Work, raw: &[u8]) -> Result<String> {
    let b = body(raw, work)?;
    let s = splits(b.len())?;
    let mut f = vec![
        work.id.clone(),
        raw.len().to_string(),
        hex(&sha256(raw)),
        b.len().to_string(),
        hex(&sha256(&b)),
    ];
    for r in [s.train, s.dev, s.test] {
        f.push(r.len().to_string());
        f.push(hex(&sha256(&b[r])));
    }
    Ok(f.join("\t"))
}
fn fresh(path: &Path, data: &str) -> Result<()> {
    let mut out = OpenOptions::new().write(true).create_new(true).open(path)?;
    out.write_all(data.as_bytes())?;
    out.sync_all()?;
    Ok(())
}
fn fingerprint_all(raw_dir: &Path, output: &Path) -> Result<()> {
    let mut lines = vec![MANIFEST_HEADER.to_string()];
    let mut bodies = std::collections::BTreeSet::new();
    for work in roster()? {
        let raw = read_bounded(&raw_dir.join(format!("{}.txt", work.id)), MAX_RAW)?;
        let row = fingerprint(&work, &raw)?;
        if !bodies.insert(
            row.split('\t')
                .nth(4)
                .ok_or("missing body identity")?
                .to_string(),
        ) {
            return Err("duplicate normalized corpus".into());
        }
        lines.push(row);
    }
    fresh(output, &format!("{}\n", lines.join("\n")))
}
fn manifest(path: &Path) -> Result<(String, BTreeMap<String, String>)> {
    let bytes = read_bounded(path, 65536)?;
    if hex(&sha256(&bytes)) != FROZEN_MANIFEST_SHA {
        return Err("unregistered corpus manifest".into());
    }
    let text = std::str::from_utf8(&bytes)?;
    let lines: Vec<_> = text.lines().collect();
    let works = roster()?;
    if lines.len() != 13 || lines[0] != MANIFEST_HEADER {
        return Err("incomplete/unknown manifest".into());
    }
    let mut rows = BTreeMap::new();
    let mut body_hashes = std::collections::BTreeSet::new();
    for (line, work) in lines[1..].iter().zip(works) {
        let f: Vec<_> = line.split('\t').collect();
        if f.len() != 11 || f[0] != work.id {
            return Err("undeclared/duplicate/out-of-order manifest row".into());
        }
        for i in [2, 4, 6, 8, 10] {
            if !is_sha(f[i]) {
                return Err("invalid manifest SHA256".into());
            }
        }
        for i in [1, 3, 5, 7, 9] {
            if f[i].parse::<usize>()? == 0 {
                return Err("zero manifest size".into());
            }
        }
        if !body_hashes.insert(f[4]) {
            return Err("duplicate manifest body".into());
        }
        rows.insert(work.id, line.to_string());
    }
    Ok((hex(&sha256(&bytes)), rows))
}
fn is_sha(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
struct Inputs {
    work: Work,
    manifest_sha: String,
    fingerprint: String,
    bytes: Vec<u8>,
    splits: Splits,
}
fn inputs(manifest_path: &Path, raw_dir: &Path, id: &str) -> Result<Inputs> {
    let (manifest_sha, rows) = manifest(manifest_path)?;
    let work = roster()?
        .into_iter()
        .find(|w| w.id == id)
        .ok_or("undeclared work")?;
    let raw = read_bounded(&raw_dir.join(format!("{id}.txt")), MAX_RAW)?;
    let fingerprint = fingerprint(&work, &raw)?;
    if rows.get(id) != Some(&fingerprint) {
        return Err("frozen source/split drift".into());
    }
    let bytes = body(&raw, &work)?;
    let splits = splits(bytes.len())?;
    Ok(Inputs {
        work,
        manifest_sha,
        fingerprint,
        bytes,
        splits,
    })
}
fn binary_sha() -> Result<String> {
    Ok(hex(&sha256(&read_bounded(
        &std::env::current_exe()?,
        64 * 1024 * 1024,
    )?)))
}
fn threshold(language: &str) -> Result<u64> {
    match language {
        "FR" => Ok(129),
        "EN" => Ok(64),
        _ => Err("unknown gate language".into()),
    }
}
fn choice(ladder: &[EvidenceDepthReadout; 4], count: u64) -> usize {
    (1..4)
        .rev()
        .find(|&i| ladder[i].context_events >= count)
        .unwrap_or(0)
}
#[derive(Clone, Debug, Default)]
struct Scores {
    bits: [f64; 5],
    tokens: u64,
    adaptive_depths: [u64; 4],
}
fn score(model: &mut PagedNGram, bytes: &[u8], count: u64, adaptive: bool) -> Result<Scores> {
    let layout = ChunkLayout::new(bytes.len())?;
    let mut result = Scores::default();
    for lane in 0..LANES {
        let chunk = &bytes[lane * layout.chunk..(lane + 1) * layout.chunk];
        let mut lane_bits = [0.0; 5];
        for t in 1..chunk.len() {
            let ladder = model.depth_ladder_readout(&chunk[..t])?;
            let selected = choice(&ladder, count);
            for (i, readout) in ladder.iter().enumerate() {
                let p = readout.probabilities[usize::from(chunk[t])];
                if !p.is_finite() || p <= 0.0 || p > 1.0 {
                    return Err("invalid target probability".into());
                }
                lane_bits[i] -= p.log2();
            }
            if adaptive {
                lane_bits[4] -= ladder[selected].probabilities[usize::from(chunk[t])].log2();
                result.adaptive_depths[selected] += 1;
            }
            result.tokens += 1;
        }
        for (sum, lane) in result.bits.iter_mut().zip(lane_bits) {
            *sum += lane;
        }
    }
    if result.tokens != layout.tokens()
        || result.bits.iter().any(|x| !x.is_finite() || *x < 0.0)
        || (adaptive && result.adaptive_depths.iter().sum::<u64>() != result.tokens)
    {
        return Err("incomplete score".into());
    }
    Ok(result)
}
fn best_dev(bits: &[f64; 4]) -> Result<usize> {
    if bits.iter().any(|v| !v.is_finite() || *v < 0.0) {
        return Err("invalid DEV loss".into());
    }
    let mut best = 0;
    for i in 1..4 {
        if bits[i] < bits[best] {
            best = i;
        }
    }
    Ok(best)
}
const SEAL_KEYS: [&str; 15] = [
    "schema",
    "id",
    "manifest_sha256",
    "binary_sha256",
    "fingerprint",
    "dev_tokens",
    "dev_bits4",
    "dev_bits7",
    "dev_bits12",
    "dev_bits32",
    "selected_depth",
    "min_count",
    "store_sha256",
    "index_sha256",
    "integrity",
];
const TEST_KEYS: [&str; 19] = [
    "schema",
    "id",
    "manifest_sha256",
    "binary_sha256",
    "seal_sha256",
    "fingerprint",
    "selected_depth",
    "min_count",
    "test_tokens",
    "test_bits4",
    "test_bits7",
    "test_bits12",
    "test_bits32",
    "adaptive_bits",
    "adaptive_depth4",
    "adaptive_depth7",
    "adaptive_depth12",
    "adaptive_depth32",
    "integrity",
];
fn write_record(path: &Path, keys: &[&str], values: &[String]) -> Result<()> {
    if keys.len() != values.len() || values.iter().any(|v| v.contains('\n') || v.contains('\r')) {
        return Err("invalid record".into());
    }
    let digest_path = PathBuf::from(format!("{}.sha256", path.display()));
    if path.exists() || digest_path.exists() {
        return Err("record destination exists".into());
    }
    let data: String = keys
        .iter()
        .zip(values)
        .map(|(k, v)| format!("{k}\t{v}\n"))
        .collect();
    fresh(path, &data)?;
    fresh(
        &digest_path,
        &format!("{}\n", hex(&sha256(data.as_bytes()))),
    )
}
fn record(path: &Path, keys: &[&str]) -> Result<(String, Vec<String>)> {
    let data = read_bounded(path, 65536)?;
    let digest = hex(&sha256(&data));
    if fs::read_to_string(format!("{}.sha256", path.display()))? != format!("{digest}\n") {
        return Err("record checksum mismatch".into());
    }
    let text = std::str::from_utf8(&data)?;
    let lines: Vec<_> = text.lines().collect();
    if lines.len() != keys.len() {
        return Err("incomplete record".into());
    }
    let mut values = Vec::new();
    for (line, key) in lines.iter().zip(keys) {
        let (k, v) = line.split_once('\t').ok_or("invalid record field")?;
        if k != *key {
            return Err("unknown/duplicate/out-of-order record key".into());
        }
        values.push(v.into());
    }
    Ok((digest, values))
}
fn seal(i: &Inputs, work_dir: &Path, output: &Path) -> Result<()> {
    if output.exists() || PathBuf::from(format!("{}.sha256", output.display())).exists() {
        return Err("seal destination exists".into());
    }
    fs::create_dir_all(work_dir)?;
    let train = &i.bytes[i.splits.train.clone()];
    let store = work_dir.join(format!("{}-store", i.work.id));
    let scratch = work_dir.join(format!("{}-sort", i.work.id));
    let PagedBuild::Published(meta) =
        build_paged(train, 32, &store, &scratch, PagedLimits::default())?
    else {
        return Err("capacity stop".into());
    };
    let mut model = PagedNGram::open(&store, sha256(train))?;
    // DEV phase does not evaluate the adaptive candidate or open the TEST slice.
    let dev = score(
        &mut model,
        &i.bytes[i.splits.dev.clone()],
        threshold(&i.work.language)?,
        false,
    )?;
    let fixed: [f64; 4] = dev.bits[..4].try_into()?;
    let best = best_dev(&fixed)?;
    let values = vec![
        "testtest-seal-v1".into(),
        i.work.id.clone(),
        i.manifest_sha.clone(),
        binary_sha()?,
        i.fingerprint.clone(),
        dev.tokens.to_string(),
        fixed[0].to_string(),
        fixed[1].to_string(),
        fixed[2].to_string(),
        fixed[3].to_string(),
        ENG1C_VAL_DEPTHS[best].to_string(),
        threshold(&i.work.language)?.to_string(),
        hex(&meta.count_store_sha256),
        hex(&meta.sparse_index_sha256),
        "PASS".into(),
    ];
    write_record(output, &SEAL_KEYS, &values)
}
fn check_seal(i: &Inputs, path: &Path) -> Result<(String, Vec<String>, usize)> {
    let (digest, s) = record(path, &SEAL_KEYS)?;
    if s[0] != "testtest-seal-v1"
        || s[1] != i.work.id
        || s[2] != i.manifest_sha
        || s[3] != binary_sha()?
        || s[4] != i.fingerprint
        || s[14] != "PASS"
        || s[5].parse::<u64>()? != ChunkLayout::new(i.splits.dev.len())?.tokens()
        || s[11].parse::<u64>()? != threshold(&i.work.language)?
    {
        return Err("seal source/predictor/protocol drift".into());
    }
    let dev: [f64; 4] = [s[6].parse()?, s[7].parse()?, s[8].parse()?, s[9].parse()?];
    let best = best_dev(&dev)?;
    if s[10].parse::<usize>()? != ENG1C_VAL_DEPTHS[best] || !is_sha(&s[12]) || !is_sha(&s[13]) {
        return Err("DEV-selected policy/store drift".into());
    }
    Ok((digest, s, best))
}
fn test(i: &Inputs, work_dir: &Path, seal_path: &Path, output: &Path) -> Result<()> {
    if output.exists() || PathBuf::from(format!("{}.sha256", output.display())).exists() {
        return Err("TEST destination exists".into());
    }
    let (seal_sha, s, best) = check_seal(i, seal_path)?;
    let mut model = PagedNGram::open(
        &work_dir.join(format!("{}-store", i.work.id)),
        sha256(&i.bytes[i.splits.train.clone()]),
    )?;
    if s[12] != hex(&model.metadata().count_store_sha256)
        || s[13] != hex(&model.metadata().sparse_index_sha256)
    {
        return Err("sealed TRAIN model drift".into());
    }
    let result = score(
        &mut model,
        &i.bytes[i.splits.test.clone()],
        threshold(&i.work.language)?,
        true,
    )?;
    let mut values = vec![
        "testtest-result-v1".into(),
        i.work.id.clone(),
        i.manifest_sha.clone(),
        binary_sha()?,
        seal_sha,
        i.fingerprint.clone(),
        ENG1C_VAL_DEPTHS[best].to_string(),
        threshold(&i.work.language)?.to_string(),
        result.tokens.to_string(),
    ];
    values.extend(result.bits.iter().map(f64::to_string));
    values.push(result.adaptive_depths[0].to_string());
    values.push(result.adaptive_depths[1].to_string());
    values.push(result.adaptive_depths[2].to_string());
    values.push(result.adaptive_depths[3].to_string());
    values.push("PASS".into());
    write_record(output, &TEST_KEYS, &values)
}
#[derive(Clone, Debug)]
struct Observation {
    work: Work,
    tokens: u64,
    primary: f64,
    control: f64,
}
fn observations(manifest_path: &Path, evidence: &Path) -> Result<Vec<Observation>> {
    let (manifest_sha, rows) = manifest(manifest_path)?;
    let executable = binary_sha()?;
    let mut out = Vec::new();
    for work in roster()? {
        let seal_path = evidence.join(format!("{}-seal.tsv", work.id));
        let result_path = evidence.join(format!("{}-test.tsv", work.id));
        let (seal_digest, s) = record(&seal_path, &SEAL_KEYS)?;
        let (_, r) = record(&result_path, &TEST_KEYS)?;
        let fingerprint = rows.get(&work.id).ok_or("missing admitted work")?;
        let fp: Vec<_> = fingerprint.split('\t').collect();
        let test_tokens = ChunkLayout::new(fp[9].parse()?)?.tokens();
        let dev_tokens = ChunkLayout::new(fp[7].parse()?)?.tokens();
        let fixed_dev = [s[6].parse()?, s[7].parse()?, s[8].parse()?, s[9].parse()?];
        let best = best_dev(&fixed_dev)?;
        if s[0] != "testtest-seal-v1"
            || r[0] != "testtest-result-v1"
            || s[1] != work.id
            || r[1] != work.id
            || s[2] != manifest_sha
            || r[2] != manifest_sha
            || s[3] != executable
            || r[3] != executable
            || r[4] != seal_digest
            || s[4] != *fingerprint
            || r[5] != *fingerprint
            || s[5].parse::<u64>()? != dev_tokens
            || s[10].parse::<usize>()? != ENG1C_VAL_DEPTHS[best]
            || r[6] != s[10]
            || s[11].parse::<u64>()? != threshold(&work.language)?
            || r[7] != s[11]
            || !is_sha(&s[12])
            || !is_sha(&s[13])
            || r[8].parse::<u64>()? != test_tokens
            || s[14] != "PASS"
            || r[18] != "PASS"
        {
            return Err("incomplete/mixed/unsealed panel".into());
        }
        let bits: Vec<f64> = r[9..14]
            .iter()
            .map(|v| v.parse::<f64>())
            .collect::<std::result::Result<_, _>>()?;
        if bits.iter().any(|v| !v.is_finite() || *v < 0.0)
            || r[14..18]
                .iter()
                .map(|v| v.parse::<u64>())
                .collect::<std::result::Result<Vec<_>, _>>()?
                .into_iter()
                .try_fold(0u64, |a, b| a.checked_add(b))
                .ok_or("depth total overflow")?
                != test_tokens
        {
            return Err("invalid TEST metrics".into());
        }
        out.push(Observation {
            work,
            tokens: test_tokens,
            primary: (bits[best] - bits[4]) / test_tokens as f64,
            control: (bits[0] - bits[4]) / test_tokens as f64,
        });
    }
    Ok(out)
}
fn analyze(obs: &[Observation]) -> Result<String> {
    validate_roster(&obs.iter().map(|o| o.work.clone()).collect::<Vec<_>>())?;
    let gains: Vec<_> = obs.iter().map(|o| o.primary).collect();
    let weights: Vec<_> = obs.iter().map(|o| o.tokens).collect();
    let pooled = paired_summary(&gains, &weights)?;
    let fr: Vec<_> = obs
        .iter()
        .filter(|o| o.work.language == "FR")
        .map(|o| o.primary)
        .collect();
    let en: Vec<_> = obs
        .iter()
        .filter(|o| o.work.language == "EN")
        .map(|o| o.primary)
        .collect();
    let interval = paired_group_interval(&[&fr, &en])?;
    let mut passes = pooled.mean >= 0.01 && interval.0 > 0.0 && pooled.sign_flip_p <= 0.05;
    let mut language_p = Vec::new();
    let mut text=String::from("scope\tprimary_gain_bpb\tbootstrap_low\tbootstrap_high\tsign_flip_p\twins\tties\tlosses\tworst_gain\ttoken_weighted_gain\tfixed4_gain\n");
    for language in ["ALL", "FR", "EN"] {
        let selected: Vec<_> = obs
            .iter()
            .filter(|o| language == "ALL" || o.work.language == language)
            .collect();
        let g: Vec<_> = selected.iter().map(|o| o.primary).collect();
        let w: Vec<_> = selected.iter().map(|o| o.tokens).collect();
        let summary = paired_summary(&g, &w)?;
        let bounds = if language == "ALL" {
            interval
        } else {
            paired_group_interval(&[&g])?
        };
        let control = selected.iter().map(|o| o.control).sum::<f64>() / selected.len() as f64;
        passes &= control >= 0.0;
        if language != "ALL" {
            passes &= summary.mean >= 0.01 && bounds.0 > 0.0 && summary.wins >= 4;
            language_p.push(summary.sign_flip_p);
        }
        text.push_str(&format!(
            "{language}\t{:.12}\t{:.12}\t{:.12}\t{}\t{}\t{}\t{}\t{:.12}\t{:.12}\t{:.12}\n",
            summary.mean,
            bounds.0,
            bounds.1,
            summary.sign_flip_p,
            summary.wins,
            summary.ties,
            summary.losses,
            summary.worst,
            summary.weighted_mean,
            control
        ));
    }
    passes &= holm_two_pass(language_p[0], language_p[1]);
    for language in ["FR", "EN"] {
        for genre in ["novel", "essay", "science"] {
            let selected: Vec<_> = obs
                .iter()
                .filter(|o| o.work.language == language && o.work.genre == genre)
                .collect();
            let mean = selected.iter().map(|o| o.primary).sum::<f64>() / selected.len() as f64;
            passes &= mean >= 0.0;
            text.push_str(&format!("STRATUM\t{language}\t{genre}\t{mean:.12}\n"));
        }
    }
    for o in obs {
        text.push_str(&format!(
            "AUTHOR\t{}\t{}\t{}\t{}\t{}\t{:.12}\t{:.12}\n",
            o.work.id, o.work.language, o.work.genre, o.work.author, o.tokens, o.primary, o.control
        ));
    }
    let verdict = if passes {
        "PANEL_CONFIRMATION_PASS"
    } else {
        "PANEL_SUPERIORITY_NOT_ESTABLISHED"
    };
    text.push_str(&format!("DECISION\t{verdict}\tunits=12\toriginal_TEST=UNREAD\tscope=HISTORICAL_FR_EN_PANEL_ONLY\tintegrity=PASS\n"));
    Ok(text)
}
// ---------------------------------------------------------------- ENG-1d
// Exploratory DEV-only grid of escape policies x depths on the TESTTEST
// works. It reuses the frozen manifest/source checks, builds the same TRAIN
// store as `seal`, and scores ONLY the DEV slice. TEST is hashed by the
// manifest check (as in `seal`) but never scored. Outputs are exploration
// for a future preregistration on NEW works, never a confirmation.
const GRID_SCHEMA: &str = "eng1d-dev-grid-v1";
const GRID_DEPTHS: [usize; 12] = [2, 3, 4, 5, 6, 7, 8, 10, 12, 16, 24, 32];
fn grid_policies() -> Vec<EscapePolicy> {
    let mut policies = vec![EscapePolicy::WittenBell];
    for beta in [1.5, 2.0, 3.0, 4.0, 6.0, 8.0, 12.0, 16.0, 24.0, 32.0] {
        policies.push(EscapePolicy::ScaledWittenBell { beta });
    }
    for tau in [1.0, 2.0, 4.0, 8.0, 16.0, 32.0, 64.0, 128.0] {
        policies.push(EscapePolicy::Dirichlet { tau });
    }
    policies
}
/// Total DEV bits per (policy, GRID_DEPTHS index), summed per lane then
/// across lanes exactly like `score`, so the `wb` row at 4/7/12/32 is
/// bitwise comparable with a seal's `dev_bits*`.
fn dev_grid_bits(
    model: &mut PagedNGram,
    dev: &[u8],
    policies: &[EscapePolicy],
) -> Result<(Vec<[f64; GRID_DEPTHS.len()]>, u64)> {
    let layout = ChunkLayout::new(dev.len())?;
    let mut total = vec![[0.0f64; GRID_DEPTHS.len()]; policies.len()];
    let mut tokens = 0u64;
    for lane in 0..LANES {
        let chunk = &dev[lane * layout.chunk..(lane + 1) * layout.chunk];
        let mut lane_bits = vec![[0.0f64; GRID_DEPTHS.len()]; policies.len()];
        for t in 1..chunk.len() {
            let ladders = model.target_ladders(&chunk[..t], chunk[t], policies)?;
            for (bits, ladder) in lane_bits.iter_mut().zip(&ladders) {
                for (slot, &depth) in bits.iter_mut().zip(&GRID_DEPTHS) {
                    *slot -= ladder.by_depth[depth].log2();
                }
            }
            tokens += 1;
        }
        for (sum, lane) in total.iter_mut().zip(&lane_bits) {
            for (s, l) in sum.iter_mut().zip(lane) {
                *s += l;
            }
        }
    }
    if tokens != layout.tokens() || total.iter().flatten().any(|x| !x.is_finite() || *x < 0.0) {
        return Err("incomplete DEV grid".into());
    }
    Ok((total, tokens))
}
fn dev_grid(i: &Inputs, work_dir: &Path, output: &Path) -> Result<()> {
    let digest_path = PathBuf::from(format!("{}.sha256", output.display()));
    if output.exists() || digest_path.exists() {
        return Err("grid destination exists".into());
    }
    fs::create_dir_all(work_dir)?;
    let train = &i.bytes[i.splits.train.clone()];
    let store = work_dir.join(format!("{}-grid-store", i.work.id));
    let scratch = work_dir.join(format!("{}-grid-sort", i.work.id));
    let PagedBuild::Published(meta) =
        build_paged(train, 32, &store, &scratch, PagedLimits::default())?
    else {
        return Err("capacity stop".into());
    };
    let mut model = PagedNGram::open(&store, sha256(train))?;
    let policies = grid_policies();
    let (bits, tokens) = dev_grid_bits(&mut model, &i.bytes[i.splits.dev.clone()], &policies)?;
    let mut data = String::new();
    for (k, v) in [
        ("schema", GRID_SCHEMA.to_string()),
        ("id", i.work.id.clone()),
        ("language", i.work.language.clone()),
        ("manifest_sha256", i.manifest_sha.clone()),
        ("binary_sha256", binary_sha()?),
        ("fingerprint", i.fingerprint.clone()),
        ("train_bytes", train.len().to_string()),
        ("dev_tokens", tokens.to_string()),
        ("store_sha256", hex(&meta.count_store_sha256)),
        ("index_sha256", hex(&meta.sparse_index_sha256)),
        ("test_scored", "NO".to_string()),
    ] {
        data.push_str(&format!("{k}\t{v}\n"));
    }
    for (policy, row) in policies.iter().zip(&bits) {
        for (depth, value) in GRID_DEPTHS.iter().zip(row) {
            data.push_str(&format!("grid\t{}\t{depth}\t{value}\n", policy.label()));
        }
    }
    data.push_str("integrity\tPASS\n");
    fresh(output, &data)?;
    fresh(
        &digest_path,
        &format!("{}\n", hex(&sha256(data.as_bytes()))),
    )
}
fn run(args: &[String]) -> Result<()> {
    match args.get(1).map(String::as_str) {
        Some("fingerprint") if args.len()==4=>fingerprint_all(Path::new(&args[2]),Path::new(&args[3])),
        Some("seal") if args.len()==7=>{ let i=inputs(Path::new(&args[2]),Path::new(&args[3]),&args[5])?; seal(&i,Path::new(&args[4]),Path::new(&args[6])) },
        Some("test") if args.len()==8=>{ let i=inputs(Path::new(&args[2]),Path::new(&args[3]),&args[5])?; test(&i,Path::new(&args[4]),Path::new(&args[6]),Path::new(&args[7])) },
        Some("dev-grid") if args.len()==7=>{ let i=inputs(Path::new(&args[2]),Path::new(&args[3]),&args[5])?; dev_grid(&i,Path::new(&args[4]),Path::new(&args[6])) },
        Some("analyze") if args.len()==5=>{let obs=observations(Path::new(&args[2]),Path::new(&args[3]))?; fresh(Path::new(&args[4]),&analyze(&obs)?)},
        _=>Err("usage: sml-eng1c-testtest fingerprint RAW_DIR NEW_MANIFEST | seal MANIFEST RAW_DIR WORK_DIR ID NEW_SEAL | test MANIFEST RAW_DIR WORK_DIR ID SEAL NEW_RESULT | analyze MANIFEST EVIDENCE_DIR NEW_REPORT | dev-grid MANIFEST RAW_DIR WORK_DIR ID NEW_GRID".into())
    }
}
fn main() {
    if let Err(e) = run(&std::env::args().collect::<Vec<_>>()) {
        eprintln!("TESTTEST_EXECUTION_STOP: {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selection_uses_dev_even_if_test_prefers_another_depth() {
        assert_eq!(best_dev(&[10.0, 9.0, 9.0, 12.0]).unwrap(), 1);
        let test_bits = [1.0, 8.0, 2.0, 3.0];
        let selected = best_dev(&[10.0, 9.0, 9.0, 12.0]).unwrap();
        assert_eq!(test_bits[selected], 8.0);
        assert_eq!(best_dev(&[10.0; 4]).unwrap(), 0);
        assert!(best_dev(&[f64::NAN; 4]).is_err());
    }
    #[test]
    fn frozen_gate_has_no_target_input_and_exact_inclusive_counts() {
        let mut ladder = std::array::from_fn(|i| EvidenceDepthReadout {
            depth: ENG1C_VAL_DEPTHS[i],
            probabilities: [1.0 / 256.0; 256],
            context_events: [999, 128, 64, 0][i],
        });
        assert_eq!(choice(&ladder, 129), 0);
        assert_eq!(choice(&ladder, 128), 1);
        assert_eq!(choice(&ladder, 64), 2);
        ladder[2].probabilities[17] = 0.999;
        assert_eq!(choice(&ladder, 64), 2);
        assert_eq!(threshold("FR").unwrap(), 129);
        assert_eq!(threshold("EN").unwrap(), 64);
    }
    #[test]
    fn incomplete_duplicate_and_wrong_strata_stop() {
        let mut works = roster().unwrap();
        assert!(validate_roster(&works[..11]).is_err());
        works[1].author = works[0].author.clone();
        assert!(validate_roster(&works).is_err());
        let mut works = roster().unwrap();
        works[1].genre = "essay".into();
        assert!(validate_roster(&works).is_err());
        let s = splits(100000).unwrap();
        assert_eq!(
            (s.train, s.dev, s.test),
            (0..60000, 60000..80000, 80000..100000)
        );
        let s = splits(2000000).unwrap();
        assert_eq!(
            (s.train, s.dev, s.test),
            (0..600000, 1200000..1232768, 1600000..1665536)
        );
    }
    #[test]
    fn eng1d_dev_grid_scores_only_dev_and_reproduces_sealed_dev_bits() {
        let root = std::env::temp_dir().join(format!("eng1d-grid-fixture-{}", std::process::id()));
        fs::create_dir(&root).unwrap();
        let work = roster().unwrap().remove(0);
        let text: String = (0..1600)
            .map(|k| format!("le {} de la {} ", k % 37, (k * 7) % 23))
            .collect();
        let content = format!("Title: {}\r\nAuthor: {}\r\nLanguage: French\r\n*** START OF THE PROJECT GUTENBERG EBOOK FIXTURE ***\r\n{text}\r\n*** END OF THE PROJECT GUTENBERG EBOOK FIXTURE ***\r\n", work.title, work.author);
        let raw = content.as_bytes();
        let mut bytes = body(raw, &work).unwrap();
        let sp = splits(bytes.len()).unwrap();
        let i = Inputs {
            work,
            manifest_sha: "a".repeat(64),
            fingerprint: fingerprint(&roster().unwrap()[0], raw).unwrap(),
            splits: sp.clone(),
            bytes: bytes.clone(),
        };
        let seal_path = root.join("seal.tsv");
        seal(&i, &root.join("s"), &seal_path).unwrap();
        let grid_path = root.join("grid.tsv");
        dev_grid(&i, &root.join("g"), &grid_path).unwrap();
        assert!(
            dev_grid(&i, &root.join("g2"), &grid_path).is_err(),
            "no overwrite"
        );
        let text = fs::read_to_string(&grid_path).unwrap();
        assert_eq!(
            fs::read_to_string(format!("{}.sha256", grid_path.display())).unwrap(),
            format!("{}\n", hex(&sha256(text.as_bytes())))
        );
        let (_, s) = record(&seal_path, &SEAL_KEYS).unwrap();
        for (k, depth) in ENG1C_VAL_DEPTHS.iter().enumerate() {
            let line = format!("grid\twb\t{depth}\t{}\n", s[6 + k]);
            assert!(
                text.contains(&line),
                "wb row must equal sealed DEV bits: {line}"
            );
        }
        let rows = text.lines().filter(|l| l.starts_with("grid\t")).count();
        assert_eq!(rows, grid_policies().len() * GRID_DEPTHS.len());
        assert!(text.contains("test_scored\tNO\n") && text.ends_with("integrity\tPASS\n"));

        // TEST bytes cannot influence the grid: scramble TEST, same output.
        for b in &mut bytes[sp.test.clone()] {
            *b = b.wrapping_mul(31).wrapping_add(7);
        }
        let scrambled = Inputs {
            work: i.work.clone(),
            manifest_sha: i.manifest_sha.clone(),
            fingerprint: i.fingerprint.clone(),
            splits: sp,
            bytes,
        };
        let other = root.join("grid-scrambled.tsv");
        dev_grid(&scrambled, &root.join("g3"), &other).unwrap();
        let strip = |t: &str| {
            t.lines()
                .filter(|l| l.starts_with("grid\t"))
                .collect::<Vec<_>>()
                .join("\n")
        };
        assert_eq!(strip(&text), strip(&fs::read_to_string(&other).unwrap()));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn synthetic_two_phase_roundtrip_and_corruption_rejection() {
        let root = std::env::temp_dir().join(format!("testtest-fixture-{}", std::process::id()));
        fs::create_dir(&root).unwrap();
        let work = roster().unwrap().remove(0);
        let content=format!("Title: {}\r\nAuthor: {}\r\nLanguage: French\r\n*** START OF THE PROJECT GUTENBERG EBOOK FIXTURE ***\r\n{}\r\n*** END OF THE PROJECT GUTENBERG EBOOK FIXTURE ***\r\n",work.title,work.author,"abcdefghijklmnop".repeat(1600));
        let raw = content.as_bytes();
        let bytes = body(raw, &work).unwrap();
        let i = Inputs {
            work,
            manifest_sha: "a".repeat(64),
            fingerprint: fingerprint(&roster().unwrap()[0], raw).unwrap(),
            splits: splits(bytes.len()).unwrap(),
            bytes,
        };
        let seal_path = root.join("seal.tsv");
        let test_path = root.join("test.tsv");
        seal(&i, &root, &seal_path).unwrap();
        test(&i, &root, &seal_path, &test_path).unwrap();
        let (_, r) = record(&test_path, &TEST_KEYS).unwrap();
        let oracle = sml_core::elastic_ngram_long::ElasticNGramLong::train(
            &i.bytes[i.splits.train.clone()],
            sml_core::elastic_ngram_long::LongNGramLimits {
                max_depth: 32,
                ..Default::default()
            },
        )
        .unwrap();
        for (j, depth) in ENG1C_VAL_DEPTHS.iter().enumerate() {
            let independent = oracle
                .score(&i.bytes[i.splits.test.clone()], *depth)
                .unwrap();
            let bpb = r[9 + j].parse::<f64>().unwrap() / r[8].parse::<u64>().unwrap() as f64;
            assert!((independent.bits_per_byte - bpb).abs() < 1e-10);
        }
        assert_eq!(
            r[8].parse::<u64>().unwrap(),
            ChunkLayout::new(i.splits.test.len()).unwrap().tokens()
        );
        assert!(test(&i, &root, &seal_path, &test_path).is_err());
        let mut changed = Inputs {
            work: i.work.clone(),
            manifest_sha: i.manifest_sha.clone(),
            fingerprint: i.fingerprint.clone(),
            bytes: i.bytes.clone(),
            splits: i.splits.clone(),
        };
        changed.manifest_sha = "b".repeat(64);
        assert!(check_seal(&changed, &seal_path).is_err());
        fs::write(&seal_path, "corrupt").unwrap();
        assert!(check_seal(&i, &seal_path).is_err());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn panel_gate_is_positive_only_with_all_prespecified_conditions() {
        let mut obs: Vec<_> = roster()
            .unwrap()
            .into_iter()
            .map(|work| Observation {
                work,
                tokens: 6400,
                primary: 0.02,
                control: 0.02,
            })
            .collect();
        assert!(analyze(&obs).unwrap().contains("PANEL_CONFIRMATION_PASS"));
        for o in &mut obs {
            o.primary = 0.001;
        }
        assert!(analyze(&obs)
            .unwrap()
            .contains("PANEL_SUPERIORITY_NOT_ESTABLISHED"));
        for o in &mut obs {
            o.primary = if o.work.language == "FR" { -0.01 } else { 0.03 };
        }
        assert!(analyze(&obs)
            .unwrap()
            .contains("PANEL_SUPERIORITY_NOT_ESTABLISHED"));
    }
    #[test]
    fn source_and_manifest_drift_are_rejected_before_training() {
        let work = roster().unwrap().remove(0);
        assert!(body(b"invalid source", &work).is_err());
        let manifest_path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/eng1c/testtest-manifest.tsv");
        assert_eq!(manifest(&manifest_path).unwrap().1.len(), 12);
        let root =
            std::env::temp_dir().join(format!("testtest-manifest-drift-{}", std::process::id()));
        fs::write(&root, "id\twrong\n").unwrap();
        assert!(manifest(&root).is_err());
        fs::remove_file(root).unwrap();
    }
}
