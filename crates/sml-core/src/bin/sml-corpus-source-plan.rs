//! SML S5: read-only source-family discovery catalog validator.
//!
//! Metadata and rights CLAIMS are not operator-reviewed training authority.
//! This tool never downloads a source, opens TEST, or runs training.
use std::{
    collections::{BTreeMap, HashSet},
    env,
    error::Error,
    fs,
    path::Path,
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const HEADER: &str =
    "id\tdomain\tpolicy_lane\tsource_class\tsource_url\tterms_url\trevision\treview_state";
const MAX_CATALOG_BYTES: u64 = 1024 * 1024;
const MAX_SOURCES: usize = 10_000;
const LANES: [&str; 7] = [
    "OPEN_REVIEW",
    "ATTRIBUTION_REVIEW",
    "SHAREALIKE_REVIEW",
    "RESEARCH_ONLY_REVIEW",
    "WEB_TDM_REVIEW",
    "MIXED_SOURCE_REVIEW",
    "METADATA_ONLY",
];
const CLASSES: [&str; 4] = ["PUBLISHER", "CURATED_COLLECTION", "AGGREGATE", "MIRROR"];

#[derive(Debug, Default, PartialEq, Eq)]
struct Summary {
    sources: usize,
    revision_pinned: usize,
    by_lane: BTreeMap<String, usize>,
}

fn canonical_name(raw: &str) -> bool {
    !raw.is_empty()
        && raw.len() <= 80
        && raw
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
        && raw.as_bytes()[0].is_ascii_lowercase()
}

fn source_url(raw: &str) -> bool {
    if !raw.starts_with("https://") || raw.len() > 1024 {
        return false;
    }
    let remainder = &raw[8..];
    let host = remainder.split('/').next().unwrap_or("");
    !host.is_empty()
        && host.contains('.')
        && !host.contains(':')
        && !raw.contains('@')
        && !raw.contains('?')
        && !raw.contains('#')
        && !raw
            .bytes()
            .any(|b| b.is_ascii_whitespace() || b.is_ascii_control())
}

fn pinned_revision(raw: &str) -> bool {
    raw.len() == 40
        && raw
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn parse_catalog(raw: &str) -> Result<Summary> {
    let mut lines = raw.lines();
    if lines.next() != Some(HEADER) {
        return Err("invalid S5 catalog header".into());
    }
    let mut summary = Summary::default();
    let mut ids = HashSet::new();
    let mut urls = HashSet::new();
    for line in lines {
        if summary.sources == MAX_SOURCES {
            return Err("S5 catalog exceeds the source cap".into());
        }
        let fields = line.split('\t').collect::<Vec<_>>();
        if fields.len() != 8 {
            return Err("invalid S5 catalog record".into());
        }
        if !canonical_name(fields[0]) || !canonical_name(fields[1]) {
            return Err("invalid S5 source/domain identifier".into());
        }
        if !ids.insert(fields[0]) || !urls.insert(fields[4]) {
            return Err("duplicate S5 id or source URL".into());
        }
        if !LANES.contains(&fields[2]) || !CLASSES.contains(&fields[3]) {
            return Err("unknown S5 lane or origin class".into());
        }
        if !source_url(fields[4]) || !source_url(fields[5]) {
            return Err("S5 publisher or license URL is not canonical HTTPS".into());
        }
        if fields[6] != "PIN_PENDING" && !pinned_revision(fields[6]) {
            return Err("S5 source revision must be pending or pinned SHA-1-shaped ID".into());
        }
        if fields[7] != "SOURCE_CANDIDATE_ONLY" {
            return Err("S5 catalog is not an operator training admission".into());
        }
        summary.sources += 1;
        summary.revision_pinned += usize::from(fields[6] != "PIN_PENDING");
        *summary.by_lane.entry(fields[2].to_owned()).or_default() += 1;
    }
    if summary.sources == 0 {
        return Err("empty S5 source catalog".into());
    }
    Ok(summary)
}

fn run(args: &[String]) -> Result<()> {
    if args.len() != 3 || args[1] != "--catalog" {
        return Err("usage: sml-corpus-source-plan --catalog FILE.tsv".into());
    }
    let path = Path::new(&args[2]);
    let info = fs::symlink_metadata(path)?;
    if !info.is_file() || info.file_type().is_symlink() {
        return Err("catalog must be a regular non-symlink file".into());
    }
    if info.len() == 0 || info.len() > MAX_CATALOG_BYTES {
        return Err("S5 catalog file outside byte budget".into());
    }
    let bytes = fs::read(path)?;
    if bytes.len() as u64 > MAX_CATALOG_BYTES {
        return Err("S5 catalog grew outside byte budget".into());
    }
    let catalog = std::str::from_utf8(&bytes)?;
    let report = parse_catalog(catalog)?;
    println!(
        "SML_CORPUS_CATALOG_V1 sources={} revision_pinned={} rights_certified=false training_authorized=false training_started=false test=UNOPENED",
        report.sources, report.revision_pinned
    );
    for lane in LANES {
        let count = report.by_lane.get(lane).copied().unwrap_or(0);
        println!("SML_CORPUS_CATALOG_LANE lane={lane} source_families={count}");
    }
    Ok(())
}

fn main() {
    let args = env::args().collect::<Vec<_>>();
    if let Err(error) = run(&args) {
        eprintln!("SML_CORPUS_CATALOG_FAIL {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const CATALOG: &str = include_str!("../../../../research/corpus/source_catalog_v1.tsv");

    fn sample() -> String {
        format!(
            "{HEADER}\nfirst_source\tgeneral\tOPEN_REVIEW\tPUBLISHER\thttps://example.org/data\thttps://example.org/terms\tPIN_PENDING\tSOURCE_CANDIDATE_ONLY\nsecond_source\tcyber\tRESEARCH_ONLY_REVIEW\tMIRROR\thttps://example.com/data\thttps://example.com/terms\t1234567890abcdef1234567890abcdef12345678\tSOURCE_CANDIDATE_ONLY\n"
        )
    }

    #[test]
    fn catalog_is_live_metadata_only() {
        let report = parse_catalog(CATALOG).unwrap();
        assert!(report.sources >= 22);
        assert!(report.revision_pinned >= 3);
        assert_eq!(report.by_lane.values().sum::<usize>(), report.sources);
    }

    #[test]
    fn independent_rights_lanes_remain_candidates() {
        let report = parse_catalog(&sample()).unwrap();
        assert_eq!(report.sources, 2);
        assert_eq!(report.revision_pinned, 1);
        assert_eq!(report.by_lane.get("RESEARCH_ONLY_REVIEW"), Some(&1));
    }

    #[test]
    fn unreviewed_candidates_cannot_be_promoted_by_string_claim() {
        let original = sample();
        for forged in [
            "APPROVED",
            "CC0_REVIEWED",
            "ACADEMIC_RESEARCH_ONLY_REVIEWED",
            "TRAIN_OK",
        ] {
            let altered = original.replace("SOURCE_CANDIDATE_ONLY", forged);
            assert!(parse_catalog(&altered).is_err());
        }
    }

    #[test]
    fn rejects_duplicate_names_urls_noncanonical_revisions_and_url_injection() {
        let original = sample();
        for altered in [
            original.replace("second_source", "first_source"),
            original.replace("https://example.com/data", "https://example.org/data"),
            original.replace("PIN_PENDING", "latest"),
            original.replace("https://example.org/data", "http://example.org/data"),
            original.replace("https://example.org/data", "https://user@example.org/data"),
            original.replace("https://example.org/data", "https://example.org:8080/data"),
        ] {
            assert!(parse_catalog(&altered).is_err());
        }
    }

    #[test]
    fn rejects_unknown_lanes_and_broken_schema() {
        let original = sample();
        assert!(parse_catalog(&original.replace("OPEN_REVIEW", "ALL_ALLOWED")).is_err());
        assert!(parse_catalog(&original.replace("PUBLISHER", "EXECUTABLE")).is_err());
        assert!(parse_catalog(&original.replace("\tcyber\t", "\tcyber\n")).is_err());
        assert!(parse_catalog(HEADER).is_err());
    }
}
