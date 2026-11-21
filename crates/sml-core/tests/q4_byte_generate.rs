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
        .args([path.to_str().unwrap(), &digest, "msb", "0102", "2"])
        .output()
        .unwrap();
    assert!(success.status.success(), "{:?}", success.stderr);
    let stdout = String::from_utf8(success.stdout).unwrap();
    assert!(stdout.contains("generated_hex=0000\n"));
    assert!(stdout.contains("head_evaluations=510\n"));
    assert!(stdout.contains("SML_Q4_RESEARCH_GENERATION=PASS\n"));

    fs::write(&path, vec![1u8; 512]).unwrap();
    let rejected = Command::new(exe)
        .args([path.to_str().unwrap(), &digest, "msb", "0102", "2"])
        .output()
        .unwrap();
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("SHA256 mismatch"));
    fs::remove_file(path).unwrap();
}

/// Deterministic SplitMix64 stream for synthetic Q4 atlases.
fn splitmix(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// Synthetic Q4 atlas families for #1014: near-uniform (all weights 0 or
/// small), concentrated (large-magnitude levels), extreme (every nibble
/// +7 or -8), and uniformly random nibbles.
fn family_atlas(family: &str, pages: usize, seed: u64) -> Vec<u8> {
    let mut state = seed;
    (0..pages * 512)
        .map(|_| {
            let r = splitmix(&mut state);
            match family {
                "zero" => 0,
                "near_uniform" => [0x00, 0x01, 0x10, 0xf0, 0x0f][(r % 5) as usize],
                "concentrated" => [0x77, 0x88, 0x78, 0x87, 0x66, 0x99][(r % 6) as usize],
                "extreme_pos" => 0x77,
                "extreme_neg" => 0x88,
                _ => r as u8,
            }
        })
        .collect()
}

#[test]
fn issue_1014_full_distribution_normalised_product_of_decisions_and_map() {
    let families = [
        "zero",
        "near_uniform",
        "concentrated",
        "extreme_pos",
        "extreme_neg",
        "random",
    ];
    let contexts = [
        (0u8, 0u8),
        (0xff, 0xff),
        (b'H', b'i'),
        (0x00, 0xff),
        (0xa3, 0x71),
    ];
    let mut checked = 0u32;
    for (index, family) in families.iter().enumerate() {
        for seed in 0..4u64 {
            let pages = 1 + (seed as usize % 3);
            let bytes = family_atlas(family, pages, 0x1014_0000 + 16 * index as u64 + seed);
            let path = fixture(&format!("dist-{family}"), &bytes);
            for f in [
                ByteFactorization::IndependentBits,
                ByteFactorization::MsbTree,
            ] {
                let mut atlas =
                    QuantizedSparseAtlas::open_with_factorization(&path, config(pages as u64), f)
                        .unwrap();
                for &(a, b) in &contexts {
                    let log2 = atlas.next_byte_log2_distribution(a, b).unwrap();
                    let linear = atlas.next_byte_distribution(a, b).unwrap();
                    // 256 masses sum to 1 (tolerance 1e-12; observed ~1e-15).
                    let sum: f64 = linear.iter().sum();
                    assert!((sum - 1.0).abs() < 1e-12, "{family} {f:?} sum {sum}");
                    for byte in 0..=u8::MAX {
                        let p = linear[usize::from(byte)];
                        assert!(p > 0.0 && log2[usize::from(byte)].is_finite());
                        // Product of the eight conditional decisions, from the
                        // public head probabilities (relative 1e-7: 1 - p is
                        // cancellation-limited at |logit| = 16).
                        let mut product = 1.0;
                        for (head, bit) in f.decisions(byte) {
                            let q = atlas.probability(a, b, head).unwrap();
                            product *= if bit { q } else { 1.0 - q };
                        }
                        assert!(
                            (p - product).abs() <= 1e-7 * product,
                            "{family} {f:?} byte {byte}: {p} vs {product}"
                        );
                    }
                    // Argmax (lowest byte on ties) reproduces predict_next_byte,
                    // exact ties included since the F12 fix (#1019).
                    let mut best = 0usize;
                    for byte in 1..256 {
                        if log2[byte] > log2[best] {
                            best = byte;
                        }
                    }
                    let map = atlas.predict_next_byte(a, b).unwrap();
                    assert_eq!(map.byte as usize, best, "{family} {f:?} ctx ({a},{b})");
                    assert_eq!(map.log2_probability.to_bits(), log2[best].to_bits());
                    checked += 1;
                }
            }
            fs::remove_file(path).unwrap();
        }
    }
    assert_eq!(checked, 6 * 4 * 2 * 5);
}

#[test]
fn issue_1014_zero_atlas_distribution_is_exactly_uniform() {
    let path = fixture("dist-uniform", &[0u8; 512]);
    for f in [
        ByteFactorization::IndependentBits,
        ByteFactorization::MsbTree,
    ] {
        let mut atlas = QuantizedSparseAtlas::open_with_factorization(&path, config(1), f).unwrap();
        let log2 = atlas.next_byte_log2_distribution(7, 9).unwrap();
        assert!(log2.iter().all(|&s| s == -8.0));
        assert_eq!(atlas.predict_next_byte(7, 9).unwrap().byte, 0);
    }
    fs::remove_file(path).unwrap();
}
