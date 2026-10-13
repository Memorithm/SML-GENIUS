//! Corpus provenance only: no predictor, fit or scoring dependency.
// The shared module also exposes helpers not needed by this standalone tool.
#[allow(dead_code)]
#[path = "../../crates/sml-core/src/elastic_ngram_paged_sha.rs"]
mod elastic_ngram_paged_sha;
use elastic_ngram_paged_sha::{hex, sha256};
use std::{collections::BTreeSet, error::Error, fs, io::Write, path::Path};
type Result<T> = std::result::Result<T, Box<dyn Error>>;
const ROSTER: &str = include_str!("confirm-roster.tsv");
const END: usize = 622592;
const MAX_RAW: usize = 16 * 1024 * 1024;
const HEADER: &str = "id\traw_bytes\traw_sha256\tbody_bytes\tbody_sha256\ttrain128_sha256\ttrain256_sha256\ttrain512_sha256\tdev_sha256\ttest_sha256";
fn normalized(raw: &[u8], fields: &[&str]) -> Result<Vec<u8>> {
    let text = std::str::from_utf8(raw)?.replace('\r', "");
    let lines: Vec<_> = text.split_inclusive('\n').collect();
    let marker = |line: &str, phase: &str| {
        ["THE", "THIS"].iter().any(|word| {
            line.starts_with(&format!("*** {phase} OF {word} PROJECT GUTENBERG EBOOK "))
        })
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
    let language = match fields[1] {
        "FR" => "French",
        "EN" => "English",
        _ => return Err("invalid language".into()),
    };
    for expected in [
        format!("Title: {}", fields[4]),
        format!("Author: {}", fields[3]),
        format!("Language: {language}"),
    ] {
        if lines[..starts[0]]
            .iter()
            .filter(|s| s.trim_end_matches('\n') == expected)
            .count()
            != 1
        {
            return Err("header identity drift".into());
        }
    }
    let body = lines[starts[0] + 1..ends[0]].concat().into_bytes();
    if body.len() < END {
        return Err("body below three-volume minimum".into());
    }
    Ok(body)
}
fn run(raw_dir: &Path, output: &Path) -> Result<()> {
    if output.exists() {
        return Err("manifest destination exists".into());
    }
    let mut ids = BTreeSet::new();
    let mut authors = BTreeSet::new();
    let mut bodies = BTreeSet::new();
    let mut groups = std::collections::BTreeMap::new();
    let mut lines = vec![HEADER.to_string()];
    for row in ROSTER.lines().skip(1) {
        let f: Vec<_> = row.split('\t').collect();
        if f.len() != 6
            || !f[0].bytes().all(|b| b.is_ascii_digit())
            || !ids.insert(f[0])
            || !authors.insert(f[3])
            || f[5]
                != format!(
                    "https://www.gutenberg.org/cache/epub/{}/pg{}.txt",
                    f[0], f[0]
                )
        {
            return Err("invalid roster identity".into());
        }
        *groups.entry((f[1], f[2])).or_insert(0) += 1;
        let path = raw_dir.join(format!("{}.txt", f[0]));
        if fs::metadata(&path)?.len() > MAX_RAW as u64 {
            return Err("raw budget exceeded".into());
        }
        let raw = fs::read(path)?;
        if raw.len() > MAX_RAW {
            return Err("raw grew beyond budget".into());
        }
        let body = normalized(&raw, &f)?;
        let body_sha = hex(&sha256(&body));
        if !bodies.insert(body_sha.clone()) {
            return Err("duplicate body".into());
        }
        let mut record = vec![
            f[0].to_string(),
            raw.len().to_string(),
            hex(&sha256(&raw)),
            body.len().to_string(),
            body_sha,
        ];
        for size in [131072, 262144, 524288] {
            record.push(hex(&sha256(&body[..size])));
        }
        record.push(hex(&sha256(&body[524288..557056])));
        record.push(hex(&sha256(&body[557056..END])));
        lines.push(record.join("\t"));
    }
    if ids.len() != 18
        || groups.len() != 6
        || ["FR", "EN"].iter().any(|language| {
            ["novel", "essay", "science"]
                .iter()
                .any(|genre| groups.get(&(*language, *genre)) != Some(&3))
        })
    {
        return Err("incomplete or unbalanced panel".into());
    }
    let text = format!("{}\n", lines.join("\n"));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)?;
    file.write_all(text.as_bytes())?;
    file.sync_all()?;
    println!("ENG1E_FINGERPRINT,works=18,manifest_sha256={},fit=NOT_EXECUTED,score=NOT_EXECUTED,integrity=PASS",hex(&sha256(text.as_bytes())));
    Ok(())
}
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let result = if args.len() == 3 {
        run(Path::new(&args[1]), Path::new(&args[2]))
    } else {
        Err("usage: fingerprint RAW_DIR NEW_MANIFEST".into())
    };
    if let Err(error) = result {
        eprintln!("ENG1E_FINGERPRINT_STOP: {error}");
        std::process::exit(1);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_cr_removal_and_markers_header_corruption() {
        let fields = ["1", "FR", "novel", "Author", "Title", "url"];
        let content = "abc\n".repeat(160000);
        let raw = format!("Title: Title\r\nAuthor: Author\r\nLanguage: French\r\n*** START OF THIS PROJECT GUTENBERG EBOOK X ***\r\n{}*** END OF THIS PROJECT GUTENBERG EBOOK X ***\r\n",content.replace('\n',"\r\n"));
        assert_eq!(
            normalized(raw.as_bytes(), &fields).unwrap(),
            content.as_bytes()
        );
        assert!(normalized(
            raw.replace("Language: French", "Language: English")
                .as_bytes(),
            &fields
        )
        .is_err());
        assert!(normalized(
            raw.replace("*** END OF THIS", "*** START OF THIS")
                .as_bytes(),
            &fields
        )
        .is_err());
        assert!(normalized(&[255], &fields).is_err());
        assert!(normalized(
            raw.replace(&content.replace('\n', "\r\n"), "short\r\n")
                .as_bytes(),
            &fields
        )
        .is_err());
    }
}
