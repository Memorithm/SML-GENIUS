//! Bounded exact lexical address index; no semantic inference or lossy normalization.
//! Shard identity is language + frozen source + sense ordinal.
use std::collections::BTreeSet;
use std::error::Error;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const BUCKETS: usize = 256;
const MAX_SHARDS: usize = 4096;
const MAX_SHARD: u64 = 16 * 1024 * 1024;
const MAX_RECORDS: usize = 5_000_000;
const V1: &str = "SML_TUTOR_DICTIONARY_SHARD_V1";
const V2: &str = "SML_TUTOR_DICTIONARY_SHARD_V2";

fn valid_hex(s: &str) -> bool {
    !s.is_empty()
        && s.len() % 2 == 0
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn hex(s: &str) -> String {
    const HEX: &[u8] = b"0123456789abcdef";
    let mut out = String::with_capacity(s.len() * 2);
    for b in s.bytes() {
        out.push(char::from(HEX[usize::from(b >> 4)]));
        out.push(char::from(HEX[usize::from(b & 15)]));
    }
    out
}

fn valid_identity(lang: &str, source: &str) -> bool {
    matches!(lang, "fr" | "en") && source.len() == 64 && valid_hex(source)
}

fn bucket(lemma_hex: &str) -> usize {
    // Stable FNV-1a, routing only; equality is always checked on the full key.
    let mut h = 0xcbf29ce484222325u64;
    for b in lemma_hex.bytes() {
        h = (h ^ u64::from(b)).wrapping_mul(0x100000001b3);
    }
    (h & 255) as usize
}

fn regular(path: &Path) -> Result<()> {
    if !fs::symlink_metadata(path)?.file_type().is_file() {
        return Err("nonregular input".into());
    }
    Ok(())
}

fn directory(path: &Path) -> Result<()> {
    if !fs::symlink_metadata(path)?.file_type().is_dir() {
        return Err("nonregular directory".into());
    }
    Ok(())
}

fn header(lang: &str, source: &str, b: usize) -> String {
    format!("SML_TUTOR_GLOBAL_LEXICAL_INDEX_V1\nlang={lang}\nsource={source}\nbucket={b}\n")
}

fn bucket_path(dir: &Path, b: usize) -> PathBuf {
    dir.join(format!("bucket-{b:03}.idx"))
}

fn ordinal(raw: &str) -> Result<u64> {
    let n: u64 = raw.parse()?;
    if n == 0 || raw != n.to_string() {
        return Err("invalid ordinal".into());
    }
    Ok(n)
}

fn read_shard(path: &Path, lang: &str, source: &str) -> Result<String> {
    regular(path)?;
    let mut raw = String::new();
    File::open(path)?
        .take(MAX_SHARD + 1)
        .read_to_string(&mut raw)?;
    if raw.len() as u64 > MAX_SHARD || !raw.ends_with('\n') {
        return Err("invalid shard length".into());
    }
    let mut lines = raw.lines();
    if !matches!(lines.next(), Some(V1 | V2))
        || lines.next() != Some(&format!("lang={lang}"))
        || lines.next() != Some(&format!("source={source}"))
    {
        return Err("invalid shard identity".into());
    }
    Ok(raw)
}

fn project(
    lang: &str,
    source: &str,
    dir: &Path,
    delta: Option<&Path>,
    mut visit: impl FnMut(usize, &str) -> Result<()>,
) -> Result<usize> {
    if !valid_identity(lang, source) {
        return Err("invalid source identity".into());
    }
    directory(dir)?;
    let mut shards = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_str().ok_or("invalid shard filename")?;
        if name.ends_with(".bank") {
            let suffix = name
                .strip_prefix("shard-")
                .and_then(|s| s.strip_suffix(".bank"))
                .ok_or("unexpected bank")?;
            if suffix.len() != 6 || !suffix.bytes().all(|b| b.is_ascii_digit()) {
                return Err("invalid shard filename".into());
            }
            regular(&entry.path())?;
            shards.push((name.to_owned(), entry.path()));
        }
    }
    shards.sort();
    if shards.is_empty() || shards.len() > MAX_SHARDS {
        return Err("invalid shard count".into());
    }
    for (i, (name, _)) in shards.iter().enumerate() {
        if *name != format!("shard-{i:06}.bank") {
            return Err("noncontiguous shard numbering".into());
        }
    }
    if let Some(path) = delta {
        if lang != "en" {
            return Err("unexpected delta language".into());
        }
        shards.push(("delta.bank".into(), path.to_path_buf()));
    }
    let mut ids = BTreeSet::new();
    for (name, path) in shards {
        let raw = read_shard(&path, lang, source)?;
        let mut previous = 0;
        let mut count = 0;
        for row in raw.lines().skip(3) {
            let fields: Vec<_> = row.split('\t').collect();
            let [id, lemma, pos, gloss] = fields.as_slice() else {
                return Err("invalid lexical row".into());
            };
            let n = ordinal(id)?;
            if n <= previous
                || !valid_hex(lemma)
                || !valid_hex(pos)
                || !valid_hex(gloss)
                || !ids.insert(n)
                || ids.len() > MAX_RECORDS
            {
                return Err("invalid or duplicate sense".into());
            }
            previous = n;
            count += 1;
            if count > 4096 {
                return Err("oversized shard".into());
            }
            let line = format!("{lemma}\t{pos}\t{n}\t{name}\n");
            visit(bucket(lemma), &line)?;
        }
        if count == 0 {
            return Err("empty shard".into());
        }
    }
    Ok(ids.len())
}

fn build(lang: &str, source: &str, dir: &Path, delta: Option<&Path>, out: &Path) -> Result<usize> {
    if !valid_identity(lang, source) {
        return Err("invalid identity".into());
    }
    // Output must be a new staging directory. Never rewrite a live index.
    fs::create_dir(out)?;
    let mut writers = Vec::with_capacity(BUCKETS);
    for b in 0..BUCKETS {
        let path = bucket_path(out, b);
        let mut writer = BufWriter::new(File::options().write(true).create_new(true).open(path)?);
        writer.write_all(header(lang, source, b).as_bytes())?;
        writers.push(writer);
    }
    let count = project(lang, source, dir, delta, |b, line| {
        writers[b].write_all(line.as_bytes())?;
        Ok(())
    })?;
    for mut writer in writers {
        writer.flush()?;
        writer.get_ref().sync_all()?;
    }
    Ok(count)
}

fn reader(dir: &Path, lang: &str, source: &str, b: usize) -> Result<BufReader<File>> {
    let path = bucket_path(dir, b);
    regular(&path)?;
    let mut r = BufReader::new(File::open(path)?);
    let expected = header(lang, source, b);
    let mut raw = vec![0; expected.len()];
    r.read_exact(&mut raw)?;
    if raw != expected.as_bytes() {
        return Err("wrong index header".into());
    }
    Ok(r)
}

fn verify(lang: &str, source: &str, dir: &Path, delta: Option<&Path>, out: &Path) -> Result<usize> {
    directory(out)?;
    if fs::read_dir(out)?.count() != BUCKETS {
        return Err("unexpected index files".into());
    }
    let mut readers = (0..BUCKETS)
        .map(|b| reader(out, lang, source, b))
        .collect::<Result<Vec<_>>>()?;
    let count = project(lang, source, dir, delta, |b, line| {
        let mut raw = vec![0; line.len()];
        readers[b].read_exact(&mut raw)?;
        if raw != line.as_bytes() {
            return Err("index projection mismatch".into());
        }
        Ok(())
    })?;
    for mut r in readers {
        if !r.fill_buf()?.is_empty() {
            return Err("unexpected trailing index data".into());
        }
    }
    Ok(count)
}

fn lookup(
    lang: &str,
    source: &str,
    out: &Path,
    lemma: &str,
    pos: Option<&str>,
) -> Result<Vec<(u64, String)>> {
    if !valid_identity(lang, source)
        || lemma.is_empty()
        || lemma.len() > 4 * 1024 * 1024
        || pos.is_some_and(|p| p.is_empty() || p.len() > 4 * 1024 * 1024)
    {
        return Err("invalid lookup key".into());
    }
    directory(out)?;
    let key = hex(lemma);
    let pos = pos.map(hex);
    let b = bucket(&key);
    let mut r = reader(out, lang, source, b)?;
    let mut matches = Vec::new();
    let mut seen = BTreeSet::new();
    let mut line = Vec::new();
    loop {
        line.clear();
        let n = r
            .by_ref()
            .take(MAX_SHARD + 1)
            .read_until(b'\n', &mut line)?;
        if n == 0 {
            break;
        }
        if n as u64 > MAX_SHARD || !line.ends_with(b"\n") {
            return Err("invalid index line".into());
        }
        let text = std::str::from_utf8(&line[..line.len() - 1])?;
        let fields: Vec<_> = text.split('\t').collect();
        let [word, category, id, shard] = fields.as_slice() else {
            return Err("invalid index row".into());
        };
        let id = ordinal(id)?;
        let shard_ok = *shard == "delta.bank"
            || shard
                .strip_prefix("shard-")
                .and_then(|s| s.strip_suffix(".bank"))
                .is_some_and(|s| s.len() == 6 && s.bytes().all(|b| b.is_ascii_digit()));
        if !valid_hex(word) || !valid_hex(category) || bucket(word) != b || !shard_ok {
            return Err("invalid index address".into());
        }
        if *word == key && pos.as_ref().is_none_or(|p| p == category) {
            if !seen.insert(id) || matches.len() >= MAX_RECORDS {
                return Err("duplicate or excessive match".into());
            }
            matches.push((id, (*shard).to_owned()));
        }
    }
    matches.sort();
    Ok(matches)
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [mode, lang, source, dir, delta, out] if matches!(mode.as_str(), "--build" | "--verify") => {
            let delta = (delta != "-").then(|| Path::new(delta));
            let count = if mode == "--build" {
                build(lang, source, Path::new(dir), delta, Path::new(out))?
            } else {
                verify(lang, source, Path::new(dir), delta, Path::new(out))?
            };
            println!("SML_TUTOR_LEXICAL_INDEX=PASS mode={mode} lang={lang} senses={count} buckets={BUCKETS}");
        }
        [mode, lang, source, out, lemma, rest @ ..] if mode == "--lookup" && rest.len() <= 1 => {
            let matches = lookup(
                lang,
                source,
                Path::new(out),
                lemma,
                rest.first().map(String::as_str),
            )?;
            if matches.is_empty() {
                println!("ABSTAIN");
            } else {
                for (id, shard) in matches {
                    println!("{id}\t{shard}");
                }
            }
        }
        _ => return Err("use --build/--verify LANG SHA SHARDS DELTA_OR_- OUT, or --lookup LANG SHA OUT LEMMA [POS]".into()),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);

    struct Temp(PathBuf);
    impl Temp {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!(
                "sml-lexical-index-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&p).unwrap();
            Self(p)
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn shard(path: &Path, source: &str, rows: &[(u64, &str, &str)]) {
        let mut text = format!("{V1}\nlang=en\nsource={source}\n");
        for (id, word, pos) in rows {
            text.push_str(&format!("{id}\t{}\t{}\t61\n", hex(word), hex(pos)));
        }
        fs::write(path, text).unwrap();
    }

    #[test]
    fn exact_cross_shard_lookup_delta_and_replay() {
        let t = Temp::new();
        let dir = t.0.join("shards");
        fs::create_dir(&dir).unwrap();
        let source = "a".repeat(64);
        shard(
            &dir.join("shard-000000.bank"),
            &source,
            &[(1, "bank", "noun"), (2, "café", "noun")],
        );
        shard(
            &dir.join("shard-000001.bank"),
            &source,
            &[(3, "bank", "noun"), (4, "bank", "verb")],
        );
        let delta = t.0.join("delta.bank");
        shard(&delta, &source, &[(5, "bank", "noun")]);
        let out = t.0.join("index");
        assert_eq!(build("en", &source, &dir, Some(&delta), &out).unwrap(), 5);
        assert_eq!(verify("en", &source, &dir, Some(&delta), &out).unwrap(), 5);
        let ids = lookup("en", &source, &out, "bank", Some("noun")).unwrap();
        assert_eq!(
            ids,
            vec![
                (1, "shard-000000.bank".into()),
                (3, "shard-000001.bank".into()),
                (5, "delta.bank".into())
            ]
        );
        assert_eq!(lookup("en", &source, &out, "bank", None).unwrap().len(), 4);
        assert_eq!(lookup("en", &source, &out, "café", None).unwrap().len(), 1);
        assert!(lookup("en", &source, &out, "Bank", None)
            .unwrap()
            .is_empty());
        assert!(lookup("fr", &source, &out, "bank", None).is_err());
        let p = bucket_path(&out, bucket(&hex("bank")));
        File::options()
            .append(true)
            .open(p)
            .unwrap()
            .write_all(b"extra\n")
            .unwrap();
        assert!(verify("en", &source, &dir, Some(&delta), &out).is_err());
        assert!(build("en", &source, &dir, Some(&delta), &out).is_err());
    }

    #[test]
    fn reject_duplicate_ids_across_shards() {
        let t = Temp::new();
        let dir = t.0.join("shards");
        fs::create_dir(&dir).unwrap();
        let source = "b".repeat(64);
        shard(&dir.join("shard-000000.bank"), &source, &[(1, "a", "noun")]);
        shard(&dir.join("shard-000001.bank"), &source, &[(1, "b", "noun")]);
        assert!(build("en", &source, &dir, None, &t.0.join("index")).is_err());
    }
}
