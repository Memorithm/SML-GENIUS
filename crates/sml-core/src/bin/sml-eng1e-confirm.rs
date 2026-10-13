//! ENG-1e: frozen new-author confirmation with nested TRAIN volumes.
use sml_core::{
    anf_char::{ChunkLayout, LANES},
    elastic_ngram_paged::{
        build_paged, EscapePolicy, PagedBuild, PagedLimits, PagedNGram, ENG1C_VAL_DEPTHS,
    },
    elastic_ngram_paged_sha::{hex, parse_hex, sha256},
    paired_panel::{holm_two_pass, paired_group_interval, paired_summary},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fs::{self, OpenOptions},
    io::Write,
    ops::Range,
    path::Path,
};
type Result<T> = std::result::Result<T, Box<dyn Error>>;
const ROSTER: &str = include_str!("../../../../scripts/eng1e/confirm-roster.tsv");
const MANIFEST: &str = include_str!("../../../../scripts/eng1e/confirm-manifest.tsv");
const PROTOCOL: &str = include_str!("../../../../docs/ENG-1E-CONFIRMATION-PREREGISTRATION.md");
const MANIFEST_SHA: &str = "f8c19a79296b5191a881d5b26c4ba076b1c13f9d8e3cded4924d17ce41e22df0";
const ROSTER_SHA: &str = "5ed558197ce8ce414d8ea298952d527c5b3a0c7e2942fc93f566b8bf0b2498d0";
const VOLUMES: [usize; 3] = [131072, 262144, 524288];
const BETAS: [f64; 6] = [1.0, 0.25, 0.5, 2.0, 4.0, 8.0];
const DELTA: f64 = 0.05;
const COMMON: [&str; 12] = [
    "id",
    "language",
    "genre",
    "train_bytes",
    "manifest_sha256",
    "protocol_sha256",
    "binary_sha256",
    "raw_sha256",
    "body_sha256",
    "train_sha256",
    "dev_sha256",
    "test_sha256",
];
const SEAL_KEYS: [&str; 7] = [
    "schema",
    "dev_tokens",
    "classic",
    "candidate",
    "store_sha256",
    "index_sha256",
    "integrity",
];
const TEST_KEYS: [&str; 11] = [
    "schema",
    "test_tokens",
    "seal_sha256",
    "classic",
    "candidate",
    "store_sha256",
    "index_sha256",
    "classic_bits",
    "candidate_bits",
    "gate_bits",
    "integrity",
];
#[derive(Clone, Debug)]
struct Work {
    id: String,
    language: String,
    genre: String,
    author: String,
    title: String,
    identity: Vec<String>,
}
fn frozen() -> Result<Vec<Work>> {
    if hex(&sha256(MANIFEST.as_bytes())) != MANIFEST_SHA
        || hex(&sha256(ROSTER.as_bytes())) != ROSTER_SHA
    {
        return Err("compiled frozen data drift".into());
    }
    let roster: Vec<_> = ROSTER.lines().skip(1).collect();
    let manifest: Vec<_> = MANIFEST.lines().skip(1).collect();
    if roster.len() != 18 || manifest.len() != 18 {
        return Err("incomplete roster".into());
    }
    let mut works = Vec::new();
    let mut ids = BTreeSet::new();
    let mut authors = BTreeSet::new();
    let mut bodies = BTreeSet::new();
    for (r, m) in roster.iter().zip(manifest) {
        let r: Vec<_> = r.split('\t').collect();
        let m: Vec<_> = m.split('\t').collect();
        if r.len() != 6
            || m.len() != 10
            || r[0] != m[0]
            || !r[0].bytes().all(|b| b.is_ascii_digit())
            || !ids.insert(r[0])
            || !authors.insert(r[3])
            || !bodies.insert(m[4])
            || m[3].parse::<usize>()? < 622592
            || m[1].parse::<usize>()? > 16 * 1024 * 1024
            || [2, 4, 5, 6, 7, 8, 9]
                .iter()
                .any(|i| parse_hex(m[*i]).is_none())
        {
            return Err("invalid frozen work identity".into());
        }
        works.push(Work {
            id: r[0].into(),
            language: r[1].into(),
            genre: r[2].into(),
            author: r[3].into(),
            title: r[4].into(),
            identity: m.iter().map(|s| s.to_string()).collect(),
        });
    }
    for language in ["FR", "EN"] {
        for genre in ["novel", "essay", "science"] {
            if works
                .iter()
                .filter(|w| w.language == language && w.genre == genre)
                .count()
                != 3
            {
                return Err("unbalanced roster".into());
            }
        }
    }
    Ok(works)
}
fn work(id: &str) -> Result<Work> {
    frozen()?
        .into_iter()
        .find(|w| w.id == id)
        .ok_or_else(|| "unregistered author".into())
}
fn volume(value: &str) -> Result<usize> {
    let v = value.parse()?;
    if !VOLUMES.contains(&v) {
        return Err("unregistered TRAIN volume".into());
    }
    Ok(v)
}
fn normalized(raw: &[u8], work: &Work) -> Result<Vec<u8>> {
    let text = std::str::from_utf8(raw)?.replace('\r', "");
    let lines: Vec<_> = text.split_inclusive('\n').collect();
    let marker = |s: &str, p: &str| {
        ["THE", "THIS"]
            .iter()
            .any(|word| s.starts_with(&format!("*** {p} OF {word} PROJECT GUTENBERG EBOOK ")))
    };
    let starts: Vec<_> = lines
        .iter()
        .enumerate()
        .filter(|(_, s)| marker(s, "START"))
        .map(|(i, _)| i)
        .collect();
    let ends: Vec<_> = lines
        .iter()
        .enumerate()
        .filter(|(_, s)| marker(s, "END"))
        .map(|(i, _)| i)
        .collect();
    if starts.len() != 1 || ends.len() != 1 || starts[0] >= ends[0] {
        return Err("ambiguous source markers".into());
    }
    let language = match work.language.as_str() {
        "FR" => "French",
        "EN" => "English",
        _ => return Err("invalid source language".into()),
    };
    for expected in [
        format!("Title: {}", work.title),
        format!("Author: {}", work.author),
        format!("Language: {language}"),
    ] {
        if lines[..starts[0]]
            .iter()
            .filter(|s| s.trim_end_matches('\n') == expected)
            .count()
            != 1
        {
            return Err("source header identity drift".into());
        }
    }
    Ok(lines[starts[0] + 1..ends[0]].concat().into_bytes())
}
struct Inputs {
    work: Work,
    body: Vec<u8>,
    sizes: Vec<usize>,
    dev: Range<usize>,
    test: Range<usize>,
}
fn source(work: Work, dir: &Path) -> Result<Inputs> {
    let path = dir.join(format!("{}.txt", work.id));
    let n = fs::metadata(&path)?.len();
    if n > 16 * 1024 * 1024 || n != work.identity[1].parse::<u64>()? {
        return Err("source raw length drift".into());
    }
    let raw = fs::read(path)?;
    if hex(&sha256(&raw)) != work.identity[2] {
        return Err("raw hash drift".into());
    }
    let body = normalized(&raw, &work)?;
    if body.len() != work.identity[3].parse::<usize>()?
        || body.len() < 622592
        || hex(&sha256(&body)) != work.identity[4]
    {
        return Err("body identity drift".into());
    }
    for (index, size) in VOLUMES.iter().enumerate() {
        if hex(&sha256(&body[..*size])) != work.identity[5 + index] {
            return Err("TRAIN identity drift".into());
        }
    }
    if hex(&sha256(&body[524288..557056])) != work.identity[8]
        || hex(&sha256(&body[557056..622592])) != work.identity[9]
    {
        return Err("evaluation split drift".into());
    }
    Ok(Inputs {
        work,
        body,
        sizes: VOLUMES.to_vec(),
        dev: 524288..557056,
        test: 557056..622592,
    })
}
fn binary_sha() -> Result<String> {
    let p = std::env::current_exe()?;
    if fs::metadata(&p)?.len() > 64 * 1024 * 1024 {
        return Err("binary budget exceeded".into());
    }
    Ok(hex(&sha256(&fs::read(p)?)))
}
fn fresh(path: &Path, text: &str) -> Result<()> {
    let mut f = OpenOptions::new().create_new(true).write(true).open(path)?;
    f.write_all(text.as_bytes())?;
    f.sync_all()?;
    Ok(())
}
fn best(bits: &[f64], end: usize) -> Result<usize> {
    if end == 0 || bits.len() < end || bits.iter().any(|v| !v.is_finite() || *v < 0.0) {
        return Err("invalid selection losses".into());
    }
    let mut chosen = 0;
    for i in 1..end {
        if bits[i] < bits[chosen] {
            chosen = i;
        }
    }
    Ok(chosen)
}
fn threshold(language: &str) -> Result<u64> {
    match language {
        "FR" => Ok(129),
        "EN" => Ok(64),
        _ => Err("unknown gate language".into()),
    }
}
fn gate(events: &[u64; 33], count: u64) -> usize {
    (1..4)
        .rev()
        .find(|i| events[ENG1C_VAL_DEPTHS[*i]] >= count)
        .unwrap_or(0)
}
fn policies(betas: &[f64]) -> Vec<EscapePolicy> {
    betas
        .iter()
        .map(|beta| EscapePolicy::ScaledWittenBell { beta: *beta })
        .collect()
}
fn dev_bits(model: &mut PagedNGram, bytes: &[u8], language: &str) -> Result<Vec<f64>> {
    let layout = ChunkLayout::new(bytes.len())?;
    let arms = policies(&BETAS);
    let count = threshold(language)?;
    let mut total = vec![0.0; 25];
    let mut tokens = 0;
    for lane in 0..LANES {
        let chunk = &bytes[lane * layout.chunk..(lane + 1) * layout.chunk];
        let mut bits = vec![0.0; 25];
        for t in 1..chunk.len() {
            let read = model.target_ladders(&chunk[..t], chunk[t], &arms)?;
            for (b, ladder) in read.iter().enumerate() {
                for (d, depth) in ENG1C_VAL_DEPTHS.iter().enumerate() {
                    bits[b * 4 + d] -= ladder.by_depth[*depth].log2();
                }
            }
            let d = gate(&read[0].context_events, count);
            bits[24] -= read[0].by_depth[ENG1C_VAL_DEPTHS[d]].log2();
            tokens += 1;
        }
        for (sum, value) in total.iter_mut().zip(bits) {
            *sum += value;
        }
    }
    if tokens != layout.tokens() {
        return Err("incomplete DEV targets".into());
    }
    best(&total, 24)?;
    Ok(total)
}
fn test_bits(
    model: &mut PagedNGram,
    bytes: &[u8],
    language: &str,
    classic: usize,
    candidate: usize,
) -> Result<Vec<f64>> {
    if classic >= 4 || candidate >= 24 {
        return Err("invalid sealed choices".into());
    }
    let layout = ChunkLayout::new(bytes.len())?;
    let arms = policies(&[1.0, BETAS[candidate / 4]]);
    let count = threshold(language)?;
    let mut total = vec![0.0; 3];
    let mut tokens = 0;
    for lane in 0..LANES {
        let chunk = &bytes[lane * layout.chunk..(lane + 1) * layout.chunk];
        let mut bits = vec![0.0; 3];
        for t in 1..chunk.len() {
            let read = model.target_ladders(&chunk[..t], chunk[t], &arms)?;
            let g = gate(&read[0].context_events, count);
            for (slot, p) in [
                read[0].by_depth[ENG1C_VAL_DEPTHS[classic]],
                read[1].by_depth[ENG1C_VAL_DEPTHS[candidate % 4]],
                read[0].by_depth[ENG1C_VAL_DEPTHS[g]],
            ]
            .iter()
            .enumerate()
            {
                bits[slot] -= p.log2();
            }
            tokens += 1;
        }
        for (sum, value) in total.iter_mut().zip(bits) {
            *sum += value;
        }
    }
    if tokens != layout.tokens() {
        return Err("incomplete TEST targets".into());
    }
    if total.iter().any(|v| !v.is_finite() || *v < 0.0) {
        return Err("invalid TEST losses".into());
    }
    Ok(total)
}
fn common(
    work: &Work,
    size: usize,
    sizes: &[usize],
    binary: &str,
) -> Result<BTreeMap<String, String>> {
    let index = sizes
        .iter()
        .position(|s| *s == size)
        .ok_or("unknown source volume")?;
    Ok(COMMON
        .iter()
        .map(|s| s.to_string())
        .zip([
            work.id.clone(),
            work.language.clone(),
            work.genre.clone(),
            size.to_string(),
            MANIFEST_SHA.into(),
            hex(&sha256(PROTOCOL.as_bytes())),
            binary.into(),
            work.identity[2].clone(),
            work.identity[4].clone(),
            work.identity[5 + index].clone(),
            work.identity[8].clone(),
            work.identity[9].clone(),
        ])
        .collect())
}
fn field<'a>(meta: &'a BTreeMap<String, String>, key: &str) -> Result<&'a str> {
    meta.get(key)
        .map(String::as_str)
        .ok_or_else(|| format!("missing record field {key}").into())
}
struct Record {
    meta: BTreeMap<String, String>,
    bits: Vec<f64>,
    digest: String,
}
fn read_record(path: &Path, keys: &[&str], dev: bool) -> Result<Record> {
    if fs::metadata(path)?.len() > 65536 {
        return Err("record size budget exceeded".into());
    }
    let text = fs::read_to_string(path)?;
    let mut meta = BTreeMap::new();
    let mut bits = Vec::new();
    for line in text.lines() {
        let f: Vec<_> = line.split('\t').collect();
        if dev && f.first() == Some(&"dev_bits") {
            if f.len() != 3 || f[1].parse::<usize>()? != bits.len() {
                return Err("missing or repeated DEV arm".into());
            }
            let v = f[2].parse::<f64>()?;
            if !v.is_finite() || v < 0.0 {
                return Err("invalid DEV score".into());
            }
            bits.push(v);
        } else if f.len() == 2
            && (COMMON.contains(&f[0]) || keys.contains(&f[0]))
            && !f[1].is_empty()
        {
            if meta.insert(f[0].into(), f[1].into()).is_some() {
                return Err("duplicate record field".into());
            }
        } else {
            return Err("unknown or malformed record line".into());
        }
    }
    if meta.len() != COMMON.len() + keys.len()
        || (dev && bits.len() != 25)
        || (!dev && !bits.is_empty())
        || COMMON
            .iter()
            .chain(keys)
            .any(|key| !meta.contains_key(*key))
        || field(&meta, "integrity")? != "PASS"
    {
        return Err("incomplete record".into());
    }
    Ok(Record {
        meta,
        bits,
        digest: hex(&sha256(text.as_bytes())),
    })
}
fn check_common(
    record: &Record,
    work: &Work,
    size: usize,
    sizes: &[usize],
    binary: &str,
) -> Result<()> {
    for (key, expected) in common(work, size, sizes, binary)? {
        if field(&record.meta, &key)? != expected {
            return Err(format!("{key} record identity drift").into());
        }
    }
    Ok(())
}
fn checked_seal(input: &Inputs, size: usize, path: &Path, binary: &str) -> Result<Record> {
    let r = read_record(path, &SEAL_KEYS, true)?;
    check_common(&r, &input.work, size, &input.sizes, binary)?;
    if field(&r.meta, "schema")? != "eng1e-seal-v1"
        || field(&r.meta, "dev_tokens")?.parse::<u64>()?
            != ChunkLayout::new(input.dev.len())?.tokens()
        || field(&r.meta, "classic")?.parse::<usize>()? != best(&r.bits, 4)?
        || field(&r.meta, "candidate")?.parse::<usize>()? != best(&r.bits, 24)?
        || ["store_sha256", "index_sha256"]
            .iter()
            .any(|key| parse_hex(r.meta.get(*key).map_or("", String::as_str)).is_none())
    {
        return Err("invalid sealed DEV selection".into());
    }
    Ok(r)
}
fn render(meta: BTreeMap<String, String>, bits: &[f64]) -> String {
    let mut text = String::new();
    for (k, v) in meta {
        text.push_str(&format!("{k}\t{v}\n"));
    }
    for (i, b) in bits.iter().enumerate() {
        text.push_str(&format!("dev_bits\t{i}\t{b}\n"));
    }
    text
}
fn store_path(root: &Path, id: &str, size: usize) -> std::path::PathBuf {
    root.join(format!("{id}-{size}-store"))
}
fn seal_input(input: &Inputs, size: usize, root: &Path, out: &Path) -> Result<()> {
    if out.exists() {
        return Err("seal destination exists".into());
    }
    let binary = binary_sha()?;
    let mut meta = common(&input.work, size, &input.sizes, &binary)?;
    fs::create_dir_all(root)?;
    let train = &input.body[..size];
    let store = store_path(root, &input.work.id, size);
    let scratch = root.join(format!("{}-{size}-sort", input.work.id));
    let PagedBuild::Published(built) =
        build_paged(train, 32, &store, &scratch, PagedLimits::default())?
    else {
        return Err("TRAIN capacity stop".into());
    };
    let mut model = PagedNGram::open(&store, sha256(train))?;
    let bits = dev_bits(
        &mut model,
        &input.body[input.dev.clone()],
        &input.work.language,
    )?;
    for (key, value) in [
        ("schema", "eng1e-seal-v1".into()),
        (
            "dev_tokens",
            ChunkLayout::new(input.dev.len())?.tokens().to_string(),
        ),
        ("classic", best(&bits, 4)?.to_string()),
        ("candidate", best(&bits, 24)?.to_string()),
        ("store_sha256", hex(&built.count_store_sha256)),
        ("index_sha256", hex(&built.sparse_index_sha256)),
        ("integrity", "PASS".into()),
    ] {
        meta.insert(key.into(), value);
    }
    fresh(out, &render(meta, &bits))
}
fn test_input(input: &Inputs, size: usize, root: &Path, seals: &Path, out: &Path) -> Result<()> {
    if out.exists() {
        return Err("TEST destination exists".into());
    }
    let binary = binary_sha()?;
    let mut chosen = None;
    // The CLI cannot expose an author's TEST until every volume has a valid DEV seal.
    for volume in &input.sizes {
        let s = checked_seal(
            input,
            *volume,
            &seals.join(format!("{}-{volume}-seal.tsv", input.work.id)),
            &binary,
        )?;
        if *volume == size {
            chosen = Some(s);
        }
    }
    let seal = chosen.ok_or("missing requested volume")?;
    let mut model = PagedNGram::open(
        &store_path(root, &input.work.id, size),
        sha256(&input.body[..size]),
    )?;
    if hex(&model.metadata().count_store_sha256) != field(&seal.meta, "store_sha256")?
        || hex(&model.metadata().sparse_index_sha256) != field(&seal.meta, "index_sha256")?
    {
        return Err("store drift since DEV".into());
    }
    let classic = field(&seal.meta, "classic")?.parse()?;
    let candidate = field(&seal.meta, "candidate")?.parse()?;
    let bits = test_bits(
        &mut model,
        &input.body[input.test.clone()],
        &input.work.language,
        classic,
        candidate,
    )?;
    let mut meta = common(&input.work, size, &input.sizes, &binary)?;
    for (key, value) in [
        ("schema", "eng1e-test-v1".into()),
        (
            "test_tokens",
            ChunkLayout::new(input.test.len())?.tokens().to_string(),
        ),
        ("seal_sha256", seal.digest),
        ("classic", classic.to_string()),
        ("candidate", candidate.to_string()),
        ("store_sha256", field(&seal.meta, "store_sha256")?.into()),
        ("index_sha256", field(&seal.meta, "index_sha256")?.into()),
        ("classic_bits", bits[0].to_string()),
        ("candidate_bits", bits[1].to_string()),
        ("gate_bits", bits[2].to_string()),
        ("integrity", "PASS".into()),
    ] {
        meta.insert(key.into(), value);
    }
    fresh(out, &render(meta, &[]))
}
#[derive(Clone, Debug)]
struct Observation {
    work: Work,
    size: usize,
    tokens: u64,
    classic: f64,
    candidate: f64,
    old_gate: f64,
    classic_index: usize,
    candidate_index: usize,
}
impl Observation {
    fn gain(&self) -> f64 {
        self.classic - self.candidate
    }
}
fn observations(dir: &Path) -> Result<Vec<Observation>> {
    let mut results = Vec::new();
    let mut binary = None;
    let works = frozen()?;
    if fs::read_dir(dir)?
        .filter_map(|p| p.ok())
        .filter(|p| p.file_name().to_string_lossy().ends_with("-test.tsv"))
        .count()
        != 54
        || fs::read_dir(dir)?
            .filter_map(|p| p.ok())
            .filter(|p| p.file_name().to_string_lossy().ends_with("-seal.tsv"))
            .count()
            != 54
    {
        return Err("incomplete or undeclared panel records".into());
    }
    for w in works {
        for (index, size) in VOLUMES.iter().enumerate() {
            let r = read_record(
                &dir.join(format!("{}-{size}-test.tsv", w.id)),
                &TEST_KEYS,
                false,
            )?;
            let b = field(&r.meta, "binary_sha256")?.to_string();
            if parse_hex(&b).is_none() {
                return Err("invalid binary identity".into());
            }
            if binary.as_ref().is_some_and(|prior| prior != &b) {
                return Err("mixed measurement binaries".into());
            }
            binary = Some(b.clone());
            check_common(&r, &w, *size, &VOLUMES, &b)?;
            let seal = read_record(
                &dir.join(format!("{}-{size}-seal.tsv", w.id)),
                &SEAL_KEYS,
                true,
            )?;
            check_common(&seal, &w, *size, &VOLUMES, &b)?;
            if field(&seal.meta, "schema")? != "eng1e-seal-v1"
                || field(&seal.meta, "dev_tokens")?.parse::<u64>()? != 32704
                || field(&seal.meta, "classic")?.parse::<usize>()? != best(&seal.bits, 4)?
                || field(&seal.meta, "candidate")?.parse::<usize>()? != best(&seal.bits, 24)?
                || ["store_sha256", "index_sha256"]
                    .iter()
                    .any(|key| parse_hex(seal.meta.get(*key).map_or("", String::as_str)).is_none())
            {
                return Err("invalid archived DEV selection".into());
            }
            for key in ["classic", "candidate", "store_sha256", "index_sha256"] {
                if field(&r.meta, key)? != field(&seal.meta, key)? {
                    return Err("TEST selection/store differs from seal".into());
                }
            }
            if field(&r.meta, "schema")? != "eng1e-test-v1"
                || field(&r.meta, "seal_sha256")? != seal.digest
                || field(&r.meta, "test_tokens")?.parse::<u64>()? != 65472
                || field(&r.meta, "train_sha256")? != w.identity[5 + index]
            {
                return Err("TEST record drift".into());
            }
            let loss = |key| -> Result<f64> {
                let x = field(&r.meta, key)?.parse::<f64>()?;
                if !x.is_finite() || x < 0.0 {
                    return Err("invalid TEST loss".into());
                }
                Ok(x / 65472.0)
            };
            results.push(Observation {
                work: w.clone(),
                size: *size,
                tokens: 65472,
                classic: loss("classic_bits")?,
                candidate: loss("candidate_bits")?,
                old_gate: loss("gate_bits")?,
                classic_index: field(&r.meta, "classic")?.parse()?,
                candidate_index: field(&r.meta, "candidate")?.parse()?,
            });
        }
    }
    Ok(results)
}
#[derive(Debug)]
struct Author {
    work: Work,
    gain: f64,
    old_gain: f64,
    weight: u64,
}
fn author_blocks(rows: &[Observation]) -> Result<Vec<Author>> {
    if rows.len() != 54 {
        return Err("incomplete case count".into());
    }
    let mut authors = Vec::new();
    let mut seen = BTreeSet::new();
    for w in frozen()? {
        let mut group: Vec<_> = rows.iter().filter(|r| r.work.id == w.id).collect();
        group.sort_by_key(|r| r.size);
        if group.len() != 3
            || group.iter().map(|r| r.size).collect::<Vec<_>>() != VOLUMES
            || group.iter().any(|r| {
                r.work.language != w.language
                    || r.work.genre != w.genre
                    || r.work.author != w.author
                    || r.tokens != 65472
                    || r.classic_index >= 4
                    || r.candidate_index >= 24
                    || [r.classic, r.candidate, r.old_gate]
                        .iter()
                        .any(|v| !v.is_finite() || *v < 0.0)
            })
            || !seen.insert(w.id.clone())
        {
            return Err("invalid repeated author volumes".into());
        }
        authors.push(Author {
            work: w,
            gain: group.iter().map(|r| r.gain()).sum::<f64>() / 3.0,
            old_gain: group.iter().map(|r| r.classic - r.old_gate).sum::<f64>() / 3.0,
            weight: group.iter().map(|r| r.tokens).sum(),
        });
    }
    Ok(authors)
}
fn analyze_rows(rows: &[Observation]) -> Result<String> {
    let authors = author_blocks(rows)?;
    let mut text=String::from("scope\tauthors\tmean_gain_bpb\tci95_low\tci95_high\tp_shifted_delta\twins\tties\tlosses\tworst_gain\told_gate_gain\n");
    let mut pass = true;
    let mut language_p = Vec::new();
    let mut reasons = Vec::new();
    let mut pooled_upper = 0.0;
    for language in ["ALL", "FR", "EN"] {
        let group: Vec<_> = authors
            .iter()
            .filter(|a| language == "ALL" || a.work.language == language)
            .collect();
        let gains: Vec<_> = group.iter().map(|a| a.gain).collect();
        let weights: Vec<_> = group.iter().map(|a| a.weight).collect();
        let raw = paired_summary(&gains, &weights)?;
        let shifted: Vec<_> = gains.iter().map(|g| g - DELTA).collect();
        let test = paired_summary(&shifted, &weights)?;
        let mut strata = Vec::new();
        for lang in ["FR", "EN"] {
            if language != "ALL" && lang != language {
                continue;
            }
            for genre in ["novel", "essay", "science"] {
                strata.push(
                    group
                        .iter()
                        .filter(|a| a.work.language == lang && a.work.genre == genre)
                        .map(|a| a.gain)
                        .collect::<Vec<_>>(),
                );
            }
        }
        let refs: Vec<_> = strata.iter().map(Vec::as_slice).collect();
        let (low, high) = paired_group_interval(&refs)?;
        let old = group.iter().map(|a| a.old_gain).sum::<f64>() / group.len() as f64;
        text.push_str(&format!("{language}\t{}\t{:.12}\t{low:.12}\t{high:.12}\t{:.12}\t{}\t{}\t{}\t{:.12}\t{old:.12}\n",raw.units,raw.mean,test.sign_flip_p,raw.wins,raw.ties,raw.losses,raw.worst));
        if raw.mean < DELTA || low <= DELTA {
            pass = false;
            reasons.push(format!("{language}_effect_or_interval"));
        }
        if language == "ALL" {
            pooled_upper = high;
            if test.sign_flip_p > 0.05 {
                pass = false;
                reasons.push("pooled_sign_flip".into());
            }
        } else {
            language_p.push(test.sign_flip_p);
            if raw.wins < 6 {
                pass = false;
                reasons.push(format!("{language}_wins"));
            }
        }
    }
    if !holm_two_pass(language_p[0], language_p[1]) {
        pass = false;
        reasons.push("language_Holm".into());
    }
    for language in ["FR", "EN"] {
        for genre in ["novel", "essay", "science"] {
            let group: Vec<_> = authors
                .iter()
                .filter(|a| a.work.language == language && a.work.genre == genre)
                .collect();
            let mean = group.iter().map(|a| a.gain).sum::<f64>() / 3.0;
            text.push_str(&format!("STRATUM\t{language}\t{genre}\t3\t{mean:.12}\n"));
            if mean < 0.0 {
                pass = false;
                reasons.push(format!("{language}_{genre}_regression"));
            }
        }
        for size in VOLUMES {
            let group: Vec<_> = rows
                .iter()
                .filter(|r| r.work.language == language && r.size == size)
                .collect();
            let mean = group.iter().map(|r| r.gain()).sum::<f64>() / 9.0;
            text.push_str(&format!("VOLUME\t{language}\t{size}\t9\t{mean:.12}\n"));
            if mean < DELTA {
                pass = false;
                reasons.push(format!("{language}_{size}_effect"));
            }
        }
    }
    for a in &authors {
        text.push_str(&format!(
            "AUTHOR\t{}\t{}\t{}\t{:.12}\t{:.12}\n",
            a.work.id, a.work.language, a.work.genre, a.gain, a.old_gain
        ));
    }
    for r in rows {
        text.push_str(&format!("CASE\t{}\t{}\t{}\t{}\t{}\t{:.12}\t{:.12}\t{:.12}\t{:.12}\tclassic_depth={}\tcandidate_beta={}\tcandidate_depth={}\tboundary={}\n",r.work.id,r.work.language,r.work.genre,r.size,r.tokens,r.classic,r.candidate,r.gain(),r.classic-r.old_gate,ENG1C_VAL_DEPTHS[r.classic_index],BETAS[r.candidate_index/4],ENG1C_VAL_DEPTHS[r.candidate_index%4],[0.25,8.0].contains(&BETAS[r.candidate_index/4])));
    }
    let verdict = if pass {
        "HISTORICAL_FR_EN_PANEL_CONFIRMED"
    } else {
        "SUPERIORITY_NOT_ESTABLISHED"
    };
    text.push_str(&format!("DECISION\t{verdict}\tauthors=18\tcases=54\tdelta=0.05\tpooled_upper_below_zero={}\tpooled_upper_below_delta={}\treasons={}\tscope=HISTORICAL_FR_EN_CONVENIENCE_PANEL\tintegrity=PASS\n",pooled_upper<0.0,pooled_upper<DELTA,if reasons.is_empty(){"none".into()}else{reasons.join(";")}));
    Ok(text)
}
fn run(args: &[String]) -> Result<()> {
    match args.get(1).map(String::as_str){
    Some("seal") if args.len()==7=>{let w=work(&args[2])?;let v=volume(&args[3])?;let i=source(w,Path::new(&args[4]))?;seal_input(&i,v,Path::new(&args[5]),Path::new(&args[6]))},
    Some("test") if args.len()==8=>{let w=work(&args[2])?;let v=volume(&args[3])?;let i=source(w,Path::new(&args[4]))?;test_input(&i,v,Path::new(&args[5]),Path::new(&args[6]),Path::new(&args[7]))},
    Some("analyze") if args.len()==4=>{let rows=observations(Path::new(&args[2]))?;fresh(Path::new(&args[3]),&analyze_rows(&rows)?)},
    _=>Err("usage: sml-eng1e-confirm seal ID BYTES RAW_DIR WORK_DIR NEW_SEAL | test ID BYTES RAW_DIR WORK_DIR SEAL_DIR NEW_TEST | analyze EVIDENCE_DIR NEW_REPORT".into())}
}
fn main() {
    if let Err(error) = run(&std::env::args().collect::<Vec<_>>()) {
        eprintln!("ENG1E_EXECUTION_STOP: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Root(std::path::PathBuf);
    impl Root {
        fn new(name: &str) -> Self {
            let nonce = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path =
                std::env::temp_dir().join(format!("eng1e-{name}-{}-{nonce}", std::process::id()));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Root {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn fixture(scramble_test: bool) -> Inputs {
        let mut body: Vec<_> = (0..8192)
            .map(|i| b"abcabcda ef abcabcda "[(i + i / 71) % 20])
            .collect();
        if scramble_test {
            for byte in &mut body[6144..] {
                *byte = byte.wrapping_mul(31).wrapping_add(7);
            }
        }
        let sizes = vec![1024, 2048, 4096];
        let mut identity = vec![
            "1".into(),
            body.len().to_string(),
            hex(&sha256(&body)),
            body.len().to_string(),
            hex(&sha256(&body)),
        ];
        for size in &sizes {
            identity.push(hex(&sha256(&body[..*size])));
        }
        identity.push(hex(&sha256(&body[4096..6144])));
        identity.push(hex(&sha256(&body[6144..])));
        Inputs {
            work: Work {
                id: "1".into(),
                language: "FR".into(),
                genre: "novel".into(),
                author: "Synthetic".into(),
                title: "Fixture".into(),
                identity,
            },
            body,
            sizes,
            dev: 4096..6144,
            test: 6144..8192,
        }
    }
    #[test]
    fn actual_two_phase_training_seals_parity_test_noninterference_and_corruption() {
        let root = Root::new("two-phase");
        let input = fixture(false);
        let changed = fixture(true);
        let seals = root.0.join("seals");
        let stores = root.0.join("stores");
        fs::create_dir(&seals).unwrap();
        let out = root.0.join("test.tsv");
        assert!(test_input(&input, 1024, &stores, &seals, &out).is_err());
        assert!(!out.exists());
        for size in &input.sizes {
            seal_input(
                &input,
                *size,
                &stores,
                &seals.join(format!("1-{size}-seal.tsv")),
            )
            .unwrap();
        }
        let first = checked_seal(
            &input,
            1024,
            &seals.join("1-1024-seal.tsv"),
            &binary_sha().unwrap(),
        )
        .unwrap();
        let mut model =
            PagedNGram::open(&store_path(&stores, "1", 1024), sha256(&input.body[..1024])).unwrap();
        let before = model.metadata().clone();
        let layout = ChunkLayout::new(input.dev.len()).unwrap();
        let dev = &input.body[input.dev.clone()];
        let mut reference = vec![0.0; 25];
        for lane in 0..LANES {
            let chunk = &dev[lane * layout.chunk..(lane + 1) * layout.chunk];
            let mut bits = vec![0.0; 25];
            for t in 1..chunk.len() {
                let full = model
                    .depth_ladder_readouts_with_betas(&chunk[..t], &BETAS)
                    .unwrap();
                for (b, ladder) in full.iter().enumerate() {
                    for (d, r) in ladder.iter().enumerate() {
                        bits[b * 4 + d] -= r.probabilities[usize::from(chunk[t])].log2();
                    }
                }
                let index = (1..4)
                    .rev()
                    .find(|i| full[0][*i].context_events >= 129)
                    .unwrap_or(0);
                bits[24] -= full[0][index].probabilities[usize::from(chunk[t])].log2();
            }
            for (sum, value) in reference.iter_mut().zip(bits) {
                *sum += value;
            }
        }
        for (actual, expected) in first.bits.iter().zip(reference) {
            assert_eq!(actual.to_bits(), expected.to_bits());
        }
        test_input(&input, 1024, &stores, &seals, &out).unwrap();
        let result = read_record(&out, &TEST_KEYS, false).unwrap();
        assert_eq!(
            field(&result.meta, "classic").unwrap(),
            field(&first.meta, "classic").unwrap()
        );
        assert_eq!(
            field(&result.meta, "candidate").unwrap(),
            field(&first.meta, "candidate").unwrap()
        );
        assert!(test_input(&input, 1024, &stores, &seals, &out).is_err());
        assert_eq!(model.metadata(), &before);
        let other_seals = root.0.join("other-seals");
        let other_stores = root.0.join("other-stores");
        fs::create_dir(&other_seals).unwrap();
        for size in &changed.sizes {
            let path = other_seals.join(format!("1-{size}-seal.tsv"));
            seal_input(&changed, *size, &other_stores, &path).unwrap();
            let old = checked_seal(
                &input,
                *size,
                &seals.join(format!("1-{size}-seal.tsv")),
                &binary_sha().unwrap(),
            )
            .unwrap();
            let new = checked_seal(&changed, *size, &path, &binary_sha().unwrap()).unwrap();
            assert_eq!(old.bits, new.bits);
            for key in ["classic", "candidate", "store_sha256", "index_sha256"] {
                assert_eq!(
                    field(&old.meta, key).unwrap(),
                    field(&new.meta, key).unwrap()
                );
            }
        }
        let other_out = root.0.join("other-test.tsv");
        test_input(&changed, 1024, &other_stores, &other_seals, &other_out).unwrap();
        let other = read_record(&other_out, &TEST_KEYS, false).unwrap();
        assert_ne!(
            field(&result.meta, "classic_bits").unwrap(),
            field(&other.meta, "classic_bits").unwrap()
        );
        assert!(test_input(
            &changed,
            1024,
            &stores,
            &seals,
            &root.0.join("wrong-source.tsv")
        )
        .is_err());
        let text = fs::read_to_string(seals.join("1-1024-seal.tsv")).unwrap();
        let bad = root.0.join("bad-seal.tsv");
        let classic = field(&first.meta, "classic")
            .unwrap()
            .parse::<usize>()
            .unwrap();
        fresh(
            &bad,
            &text.replace(
                &format!("classic\t{classic}\n"),
                &format!("classic\t{}\n", (classic + 1) % 4),
            ),
        )
        .unwrap();
        assert!(checked_seal(&input, 1024, &bad, &binary_sha().unwrap()).is_err());
        let duplicate = root.0.join("duplicate.tsv");
        fresh(&duplicate, &format!("{text}integrity\tPASS\n")).unwrap();
        assert!(read_record(&duplicate, &SEAL_KEYS, true).is_err());
        assert!(checked_seal(
            &input,
            1024,
            &seals.join("1-1024-seal.tsv"),
            &"0".repeat(64)
        )
        .is_err());
        assert!(volume("4096").is_err());
        assert!(work("1").is_err());
        assert!(best(&[f64::NAN; 25], 24).is_err());
        assert_eq!(best(&[1.0; 25], 24).unwrap(), 0);
    }
    fn sample(gain: f64) -> Vec<Observation> {
        frozen()
            .unwrap()
            .into_iter()
            .flat_map(|work| {
                VOLUMES.into_iter().map(move |size| Observation {
                    work: work.clone(),
                    size,
                    tokens: 65472,
                    classic: 2.0,
                    candidate: 2.0 - gain,
                    old_gate: 2.01,
                    classic_index: 0,
                    candidate_index: 21,
                })
            })
            .collect()
    }
    #[test]
    fn author_block_endpoint_positive_negative_small_effect_and_volume_guard() {
        let positive = sample(0.15);
        let authors = author_blocks(&positive).unwrap();
        assert_eq!(authors.len(), 18);
        assert!(authors.iter().all(|a| a.weight == 196416));
        let report = analyze_rows(&positive).unwrap();
        assert!(report.contains("DECISION\tHISTORICAL_FR_EN_PANEL_CONFIRMED"));
        assert!(report.contains("ALL\t18\t0.150000000000"));
        assert!(report.contains("0.000003814697"));
        let negative = analyze_rows(&sample(-0.10)).unwrap();
        assert!(negative.contains("DECISION\tSUPERIORITY_NOT_ESTABLISHED"));
        assert!(negative.contains("pooled_upper_below_zero=true"));
        let weak = analyze_rows(&sample(0.02)).unwrap();
        assert!(weak.contains("SUPERIORITY_NOT_ESTABLISHED"));
        assert!(weak.contains("pooled_upper_below_delta=true"));
        let mut volume_bad = sample(0.20);
        for row in &mut volume_bad {
            if row.work.language == "FR" && row.size == 131072 {
                row.candidate = 2.02;
            }
        }
        let report = analyze_rows(&volume_bad).unwrap();
        assert!(report.contains("FR_131072_effect"));
        assert!(report.contains("SUPERIORITY_NOT_ESTABLISHED"));
        assert!(analyze_rows(&positive[..53]).is_err());
        let mut duplicate = positive.clone();
        duplicate[1] = duplicate[0].clone();
        assert!(analyze_rows(&duplicate).is_err());
        let mut bad = positive.clone();
        bad[0].tokens = 0;
        assert!(analyze_rows(&bad).is_err());
        bad[0].tokens = 65472;
        bad[0].candidate = f64::NAN;
        assert!(analyze_rows(&bad).is_err());
    }
    #[test]
    fn source_hash_and_header_checks_and_incomplete_analysis_are_fail_closed() {
        let root = Root::new("source");
        let mut w = work("54723").unwrap();
        let body = "abc\n".repeat(160000);
        let raw=format!("Title: {}\r\nAuthor: {}\r\nLanguage: French\r\n*** START OF THE PROJECT GUTENBERG EBOOK FIXTURE ***\r\n{}*** END OF THE PROJECT GUTENBERG EBOOK FIXTURE ***\r\n",w.title,w.author,body.replace('\n',"\r\n"));
        w.identity = vec![
            w.id.clone(),
            raw.len().to_string(),
            hex(&sha256(raw.as_bytes())),
            body.len().to_string(),
            hex(&sha256(body.as_bytes())),
        ];
        for size in VOLUMES {
            w.identity.push(hex(&sha256(&body.as_bytes()[..size])));
        }
        w.identity
            .push(hex(&sha256(&body.as_bytes()[524288..557056])));
        w.identity
            .push(hex(&sha256(&body.as_bytes()[557056..622592])));
        fs::write(root.0.join("54723.txt"), &raw).unwrap();
        assert!(source(w.clone(), &root.0).is_ok());
        let wrong = raw.replace("Language: French", "Language: English");
        assert!(normalized(wrong.as_bytes(), &w).is_err());
        fs::write(root.0.join("54723.txt"), wrong).unwrap();
        assert!(source(w, &root.0).is_err());
        assert!(observations(&root.0).is_err());
        assert!(run(&[
            "bin".into(),
            "seal".into(),
            "54723".into(),
            "1024".into(),
            "x".into(),
            "x".into(),
            "x".into()
        ])
        .is_err());
    }
}
