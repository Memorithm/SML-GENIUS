use sml_core::{
    byte_factorization::ByteFactorization,
    elastic_ngram_paged_sha::{hex, sha256},
    sparse_gradient_atlas::{QuantizedSparseAtlas, SparseConfig},
};
use std::{
    fs,
    path::PathBuf,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

fn fixture(label: &str, bytes: &[u8]) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "sml-q4-byte-{label}-{}-{nanos}.q4",
        std::process::id()
    ));
    fs::write(&path, bytes).unwrap();
    path
}

fn config(pages: u64) -> SparseConfig {
    SparseConfig {
        pages,
        max_cached_pages: pages as usize,
        learning_rate: 0.1,
    }
}

#[test]
fn q4_map_matches_independently_scored_full_byte_distribution() {
    let bytes = (0..(4 * 512))
        .map(|i| (i as u8).wrapping_mul(29).wrapping_add(7))
        .collect::<Vec<_>>();
    let path = fixture("map", &bytes);
    for factorization in [
        ByteFactorization::IndependentBits,
        ByteFactorization::MsbTree,
    ] {
        let mut predicted =
            QuantizedSparseAtlas::open_with_factorization(&path, config(4), factorization).unwrap();
        let mut control =
            QuantizedSparseAtlas::open_with_factorization(&path, config(4), factorization).unwrap();
        let actual = predicted.predict_next_byte(0xa3, 0x71).unwrap();
        let mut best = (0_u8, f64::NEG_INFINITY);
        for candidate in 0..=u8::MAX {
            let mut score = 0.0_f64;
            for (head, bit) in factorization.decisions(candidate) {
                let p = control.probability(0xa3, 0x71, head).unwrap();
                score += if bit { p.log2() } else { (1.0 - p).log2() };
            }
            if score > best.1 {
                best = (candidate, score);
            }
        }
        assert_eq!(actual.byte, best.0);
        assert!((actual.log2_probability - best.1).abs() < 1e-10);
        assert_eq!(
            actual.evaluated_heads,
            match factorization {
                ByteFactorization::IndependentBits => 8,
                ByteFactorization::MsbTree => 255,
            }
        );
        assert!(actual.page_loads <= u64::from(actual.evaluated_heads));
    }
    assert_eq!(fs::read(&path).unwrap(), bytes);
    fs::remove_file(path).unwrap();
}

#[test]
fn q4_equal_probability_ties_choose_zero_without_extra_page_loads() {
    let bytes = vec![0u8; 512];
    let path = fixture("ties", &bytes);
    for factorization in [
        ByteFactorization::IndependentBits,
        ByteFactorization::MsbTree,
    ] {
        let mut atlas =
            QuantizedSparseAtlas::open_with_factorization(&path, config(1), factorization).unwrap();
        let first = atlas.predict_next_byte(0, 0).unwrap();
        assert_eq!(first.byte, 0);
        assert!((first.log2_probability + 8.0).abs() < 1e-10);
        assert_eq!(first.page_loads, 1);
        let second = atlas.predict_next_byte(0, 0).unwrap();
        assert_eq!(second.byte, first.byte);
        assert_eq!(second.log2_probability, first.log2_probability);
        assert_eq!(second.page_loads, 0);
    }
    fs::remove_file(path).unwrap();
}

#[test]
fn q4_cli_uses_real_file_digest_and_refuses_tampering() {
    let bytes = vec![0u8; 512];
    let path = fixture("cli", &bytes);
    let exe = env!("CARGO_BIN_EXE_sml-q4-byte-generate");
    let digest = hex(&sha256(&bytes));
    let success = Command::new(exe)
        .args([
            path.to_str().unwrap(),
            &digest,
            "msb",
            "0102",
            "2",
        ])
        .output()
        .unwrap();
    assert!(success.status.success(), "{:?}", success.stderr);
    let stdout = String::from_utf8(success.stdout).unwrap();
    assert!(stdout.contains("generated_hex=0000\n"));
    assert!(stdout.contains("head_evaluations=510\n"));
    assert!(stdout.contains("SML_Q4_RESEARCH_GENERATION=PASS\n"));

    fs::write(&path, vec![1u8; 512]).unwrap();
    let rejected = Command::new(exe)
        .args([
            path.to_str().unwrap(),
            &digest,
            "msb",
            "0102",
            "2",
        ])
        .output()
        .unwrap();
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("SHA256 mismatch"));
    fs::remove_file(path).unwrap();
}
