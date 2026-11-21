//! Differential tests: production Q4 decoder vs the independent oracle in
//! `sml_core::q4_decode_reference`. Every audit finding of the Grok Q4 audit
//! has a test here (prefix `f<N>_`). No test reads DEV/TEST data or real
//! trained weights: all atlases are synthetic and seeded.
use sml_core::{
    byte_factorization::ByteFactorization,
    q4_decode_reference::{
        assert_topk_conformant, check_topk_impl, exact_top_k, msb_tree_top_k_best_first,
        reference_decisions, reference_heads, ReferenceError, ReferenceQ4, TieRule,
        DEFAULT_TOL_BITS, Q4_PAGE_BYTES,
    },
    sparse_gradient_atlas::{QuantizedSparseAtlas, SparseConfig, SparseGradientAtlas},
};
use std::{
    fs,
    io::{Seek, SeekFrom, Write},
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

const MODES: [ByteFactorization; 2] = [
    ByteFactorization::IndependentBits,
    ByteFactorization::MsbTree,
];

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
    fn byte(&mut self) -> u8 {
        self.next() as u8
    }
}

fn temp_path(label: &str) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "sml-q4-ref-{label}-{}-{nanos}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ))
}

/// A Q4 file on disk plus its oracle; removed on drop.
struct Fixture {
    path: PathBuf,
    oracle: ReferenceQ4,
}
impl Fixture {
    fn new(label: &str, bytes: Vec<u8>) -> Self {
        let path = temp_path(label);
        fs::write(&path, &bytes).unwrap();
        Self {
            path,
            oracle: ReferenceQ4::from_bytes(bytes).unwrap(),
        }
    }
    fn open(&self, factorization: ByteFactorization, cache: usize) -> QuantizedSparseAtlas {
        let config = SparseConfig {
            pages: self.oracle.pages(),
            max_cached_pages: cache,
            learning_rate: 0.1,
        };
        QuantizedSparseAtlas::open_with_factorization(&self.path, config, factorization).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

/// Random atlas families: uniform nibbles, sparse (mostly zero), saturated
/// positive/negative, and "peaked" (large-magnitude, few levels).
fn random_atlas(rng: &mut Rng, pages: usize, family: u64) -> Vec<u8> {
    (0..pages * Q4_PAGE_BYTES)
        .map(|_| match family % 5 {
            0 => rng.byte(),
            1 => {
                if rng.next() % 8 == 0 {
                    rng.byte()
                } else {
                    0
                }
            }
            2 => 0x77,
            3 => 0x88,
            _ => [0x77u8, 0x88, 0x78, 0x87][(rng.next() % 4) as usize],
        })
        .collect()
}

fn contexts(rng: &mut Rng, n: usize) -> Vec<(u8, u8)> {
    let mut out = vec![(0, 0), (0, 255), (255, 0), (255, 255), (b'H', b'i')];
    out.extend((0..n).map(|_| (rng.byte(), rng.byte())));
    out
}

/// Production log2 table: production head probabilities, combined exactly
/// like `predict_next_byte` does (sum of log2 p / log2 (1-p) along the path).
fn production_log2(
    atlas: &mut QuantizedSparseAtlas,
    f: ByteFactorization,
    a: u8,
    b: u8,
) -> [f64; 256] {
    let mut out = [0.0; 256];
    for (byte, score) in out.iter_mut().enumerate() {
        for (head, bit) in f.decisions(byte as u8) {
            let p = atlas.probability(a, b, head).unwrap();
            *score += if bit { p.log2() } else { (1.0 - p).log2() };
        }
    }
    out
}

#[test]
fn f1_distributions_sum_to_one_over_random_atlases() {
    let mut rng = Rng(0x5155_4144);
    let mut checked = 0u32;
    for atlas_index in 0..30u64 {
        let pages = 1 + (rng.next() % 16) as usize;
        let fixture = Fixture::new("sum", random_atlas(&mut rng, pages, atlas_index));
        for (a, b) in contexts(&mut rng, 12) {
            for f in MODES {
                let linear = fixture.oracle.distribution(f, a, b).unwrap();
                let log2 = fixture.oracle.log2_distribution(f, a, b).unwrap();
                let sum: f64 = linear.iter().sum();
                let sum_log: f64 = log2.iter().map(|s| s.exp2()).sum();
                assert!((sum - 1.0).abs() < 1e-12, "{f:?} sum {sum}");
                assert!((sum_log - 1.0).abs() < 1e-12, "{f:?} 2^log2 sum {sum_log}");
                for byte in 0..256 {
                    assert!(linear[byte] > 0.0 && log2[byte].is_finite());
                    let rel = (log2[byte].exp2() - linear[byte]).abs() / linear[byte];
                    // The linear oracle forms 1 - p (cancellation near |z| = 16).
                    assert!(rel < 1e-7, "{f:?} byte {byte} rel {rel}");
                }
                checked += 1;
            }
        }
    }
    assert_eq!(checked, 30 * 17 * 2);
}

#[test]
fn f2_production_heads_and_all_256_byte_scores_match_oracle() {
    let mut rng = Rng(0xdead_beef);
    for atlas_index in 0..20u64 {
        let pages = 1 + (rng.next() % 64) as usize;
        let fixture = Fixture::new("heads", random_atlas(&mut rng, pages, atlas_index));
        for f in MODES {
            // Small caches force evictions between head reads (cache 1, 2, 32).
            for cache in [1, 2, pages.min(32)] {
                let mut atlas = fixture.open(f, cache);
                for (a, b) in contexts(&mut rng, 3) {
                    for head in reference_heads(f) {
                        let got = atlas.probability(a, b, head).unwrap();
                        let want = fixture.oracle.head_probability(a, b, head).unwrap();
                        assert!(
                            (got - want).abs() <= 1e-15,
                            "{f:?} head {head}: {got} vs {want}"
                        );
                    }
                    let prod = production_log2(&mut atlas, f, a, b);
                    let oracle = fixture.oracle.log2_distribution(f, a, b).unwrap();
                    for byte in 0..256 {
                        let err = (prod[byte] - oracle[byte]).abs();
                        assert!(err < DEFAULT_TOL_BITS, "{f:?} byte {byte} err {err}");
                    }
                }
            }
        }
    }
}

#[test]
fn f2_decision_paths_match_closed_form_for_every_byte() {
    for f in MODES {
        for byte in 0..=255u8 {
            assert_eq!(
                f.decisions(byte),
                reference_decisions(f, byte),
                "{f:?} {byte}"
            );
        }
    }
}

#[test]
fn f3_map_decode_is_the_oracle_argmax() {
    let mut rng = Rng(0x00ab_cdef);
    let mut cases = 0u32;
    for atlas_index in 0..40u64 {
        let pages = 1 + (rng.next() % 32) as usize;
        let fixture = Fixture::new("map", random_atlas(&mut rng, pages, atlas_index));
        for f in MODES {
            let mut atlas = fixture.open(f, pages.min(32));
            for (a, b) in contexts(&mut rng, 6) {
                let prediction = atlas.predict_next_byte(a, b).unwrap();
                let top = fixture.oracle.top_k(f, a, b, 2).unwrap();
                if top[0].1 - top[1].1 > DEFAULT_TOL_BITS {
                    assert_eq!(prediction.byte, top[0].0, "{f:?} ctx ({a},{b})");
                }
                assert!((prediction.log2_probability - top[0].1).abs() < DEFAULT_TOL_BITS);
                // MAP equals the oracle argmax byte unless the oracle has a
                // near-tie; exact (mathematical) ties are covered by f12.
                assert!(top[0].1 - top[1].1 > DEFAULT_TOL_BITS || prediction.byte == top[0].0);
                let heads = if f == ByteFactorization::MsbTree {
                    255
                } else {
                    8
                };
                assert_eq!(prediction.evaluated_heads, heads);
                cases += 1;
            }
        }
    }
    assert_eq!(cases, 40 * 2 * 11);
}

#[test]
fn f3_independent_bits_greedy_is_joint_map_but_msb_tree_greedy_is_not() {
    let mut rng = Rng(77);
    let mut msb_greedy_mismatch = 0u32;
    for atlas_index in 0..20u64 {
        let fixture = Fixture::new("greedy", random_atlas(&mut rng, 8, atlas_index * 5));
        for (a, b) in contexts(&mut rng, 20) {
            // IndependentBits: the model is a product of Bernoullis, so the
            // per-bit argmax (ties -> 0, the lowest-byte rule) is the joint MAP.
            let mut greedy = 0u8;
            for bit in 0..8 {
                if fixture.oracle.head_probability(a, b, bit).unwrap() > 0.5 {
                    greedy |= 1 << bit;
                }
            }
            let mut atlas = fixture.open(ByteFactorization::IndependentBits, 8);
            assert_eq!(atlas.predict_next_byte(a, b).unwrap().byte, greedy);
            // MsbTree: follow the locally most probable child from the root.
            let mut node = 1usize;
            while node < 256 {
                let p = fixture.oracle.head_probability(a, b, node).unwrap();
                node = 2 * node + usize::from(p > 0.5);
            }
            let mut tree = fixture.open(ByteFactorization::MsbTree, 8);
            if tree.predict_next_byte(a, b).unwrap().byte != (node - 256) as u8 {
                msb_greedy_mismatch += 1;
            }
        }
    }
    // Greedy descent is not a MAP decoder for the tree: the enumeration in
    // predict_next_byte is needed (or an exact best-first search, see f7).
    assert!(msb_greedy_mismatch > 0);
}

#[test]
fn f4_exact_ties_are_deterministic_lowest_byte() {
    let fixture = Fixture::new("ties", vec![0u8; 2 * Q4_PAGE_BYTES]);
    for f in MODES {
        let table = fixture.oracle.log2_distribution(f, 1, 2).unwrap();
        assert!(table.iter().all(|&s| s == -8.0));
        let top = exact_top_k(&table, 4).unwrap();
        assert_eq!(top.iter().map(|e| e.0).collect::<Vec<_>>(), [0, 1, 2, 3]);
        let mut atlas = fixture.open(f, 2);
        let first = atlas.predict_next_byte(1, 2).unwrap();
        let second = atlas.predict_next_byte(1, 2).unwrap();
        assert_eq!((first.byte, first.log2_probability), (0, -8.0));
        assert_eq!(
            (second.byte, second.log2_probability, second.evaluated_heads),
            (first.byte, first.log2_probability, first.evaluated_heads)
        );
    }
}

#[test]
fn f5_saturated_atlases_stay_finite_and_normalised() {
    // 0x77: every weight +3.5 (max representable); 0x88: every weight -4.0.
    for (label, fill) in [("pos", 0x77u8), ("neg", 0x88u8)] {
        let fixture = Fixture::new(label, vec![fill; Q4_PAGE_BYTES]);
        for f in MODES {
            let mut atlas = fixture.open(f, 1);
            for (a, b) in [(0u8, 0u8), (255, 255), (0x55, 0xaa)] {
                for head in reference_heads(f) {
                    let z = fixture.oracle.head_logit(a, b, head).unwrap();
                    assert!(z.abs() <= 16.0);
                }
                let table = fixture.oracle.log2_distribution(f, a, b).unwrap();
                // Logistic clamp at |z| = 16 bounds every byte above 2^-185.
                assert!(table.iter().all(|&s| s.is_finite() && s > -185.0));
                let sum: f64 = table.iter().map(|s| s.exp2()).sum();
                assert!((sum - 1.0).abs() < 1e-12);
                let p = atlas.predict_next_byte(a, b).unwrap();
                assert!(p.log2_probability.is_finite());
                assert_eq!(p.byte, exact_top_k(&table, 1).unwrap()[0].0);
            }
        }
    }
    // With all 17 weights at -4.0 and matching signs the raw logit is -17,
    // i.e. the +-16 logistic clamp is reachable (saturation, not overflow).
    let fixture = Fixture::new("clamp", vec![0x88; Q4_PAGE_BYTES]);
    assert_eq!(fixture.oracle.head_logit(255, 255, 3).unwrap(), -16.0);
}

/// Writes raw f32 master weights into a fresh 1..N page training atlas and
/// exports Q4 with the production exporter.
fn export_master(weights: &[f32], pages: u64) -> (ReferenceQ4, Vec<u8>) {
    let root = temp_path("master");
    let config = SparseConfig {
        pages,
        max_cached_pages: 1,
        learning_rate: 0.1,
    };
    let mut atlas = SparseGradientAtlas::create(&root, config).unwrap();
    let mut master = fs::OpenOptions::new()
        .write(true)
        .open(root.join("master.f32"))
        .unwrap();
    master.seek(SeekFrom::Start(0)).unwrap();
    for w in weights {
        master.write_all(&w.to_le_bytes()).unwrap();
    }
    master.sync_all().unwrap();
    let out = root.join("weights.q4");
    atlas.export_q4(&out).unwrap();
    let bytes = fs::read(&out).unwrap();
    fs::remove_dir_all(&root).unwrap();
    (ReferenceQ4::from_bytes(bytes.clone()).unwrap(), bytes)
}

#[test]
fn f6_q4_export_nibble_order_scale_and_asymmetric_clipping() {
    // The master is clamped to [-4, 4] by every SGD path, but the Q4 grid is
    // {-4.0, -3.5, ..., +3.5}: +4 cannot be represented.
    let probes: [f32; 12] = [
        -4.0, -3.8, -0.26, -0.25, -0.2, 0.0, 0.2, 0.25, 0.26, 3.7, 3.8, 4.0,
    ];
    let mut weights = vec![0.0f32; 2 * 1024];
    for (i, w) in probes.iter().enumerate() {
        weights[i] = *w;
        weights[1024 + 1023 - i] = -*w; // page 1, reversed, mirrored
    }
    let (oracle, bytes) = export_master(&weights, 2);
    assert_eq!(bytes.len(), 2 * Q4_PAGE_BYTES);
    let got: Vec<f64> = (0..probes.len())
        .map(|s| oracle.weight(0, s).unwrap())
        .collect();
    // round-half-away-from-zero at step 0.5, then clamp to [-8, 7] levels.
    assert_eq!(
        got,
        [-4.0, -4.0, -0.5, -0.5, 0.0, 0.0, 0.0, 0.5, 0.5, 3.5, 3.5, 3.5]
    );
    // Slot 0 is the low nibble of byte 0, slot 1 the high nibble.
    assert_eq!(bytes[0], 0x88); // -8 | -8 << 4
    assert_eq!(bytes[5] & 0x0f, 0x7); // slot 10 = 3.8 -> level 7
    let mirrored: Vec<f64> = (0..probes.len())
        .map(|i| oracle.weight(1, 1023 - i).unwrap())
        .collect();
    assert_eq!(
        mirrored,
        [3.5, 3.5, 0.5, 0.5, 0.0, 0.0, 0.0, -0.5, -0.5, -3.5, -4.0, -4.0]
    );
    // Error budget: |w - q| <= 0.25 everywhere EXCEPT w in (3.75, 4.0] where
    // it reaches 0.5, and the error is one-sided (always shrinks +w).
    for (w, q) in probes.iter().zip(&got) {
        let err = (f64::from(*w) - q).abs();
        if *w > 3.75 {
            assert!(err > 0.25 && err <= 0.5 + 1e-12, "w={w} err={err}");
        } else {
            assert!(err <= 0.25 + 1e-12, "w={w} err={err}");
        }
    }
}

#[test]
fn f7_msb_tree_map_thrashes_a_32_page_lru_and_best_first_is_exact_and_cheaper() {
    let mut rng = Rng(4242);
    let pages = 4096usize;
    let fixture = Fixture::new("lru", random_atlas(&mut rng, pages, 0));
    let (a, b) = (b'H', b'i');
    let distinct = {
        let mut ids: Vec<u64> = (1..256).map(|h| fixture.oracle.page_of(a, b, h)).collect();
        ids.sort_unstable();
        ids.dedup();
        ids.len() as u64
    };
    assert!(distinct > 200, "distinct pages {distinct}");
    let mut atlas = fixture.open(ByteFactorization::MsbTree, 32);
    let first = atlas.predict_next_byte(a, b).unwrap();
    let second = atlas.predict_next_byte(a, b).unwrap();
    // A cyclic scan of >32 distinct pages is the LRU worst case: repeating
    // the SAME context reloads (almost) every page.
    assert!(first.page_loads >= distinct);
    assert!(
        second.page_loads + 32 >= distinct,
        "second {}",
        second.page_loads
    );
    // IndependentBits touches 8 pages, fully cached on repetition.
    let mut ib = fixture.open(ByteFactorization::IndependentBits, 32);
    ib.predict_next_byte(a, b).unwrap();
    assert_eq!(ib.predict_next_byte(a, b).unwrap().page_loads, 0);

    // Exact best-first top-k on the tree, checked by the conformance oracle.
    let mut heads_for_top1 = Vec::new();
    for atlas_index in 0..10u64 {
        let fx = Fixture::new("bf", random_atlas(&mut rng, 16, atlas_index));
        for (ca, cb) in contexts(&mut rng, 4) {
            let mut prod = fx.open(ByteFactorization::MsbTree, 16);
            assert_topk_conformant(
                |x, y, k| {
                    // Production exposes p, not the logit: cross-check p first.
                    for node in 1..256 {
                        let p = prod.probability(x, y, node).unwrap();
                        let q = fx.oracle.head_probability(x, y, node).unwrap();
                        assert!((p - q).abs() <= 1e-15);
                    }
                    msb_tree_top_k_best_first(|node| fx.oracle.head_logit(x, y, node), k)
                        .map(|r| r.0)
                },
                &fx.oracle,
                ByteFactorization::MsbTree,
                (ca, cb),
                7,
            );
            let (_, heads) =
                msb_tree_top_k_best_first(|node| fx.oracle.head_logit(ca, cb, node), 1).unwrap();
            heads_for_top1.push((atlas_index % 5, heads));
        }
    }
    // Peaked atlases (families 2..4) need far fewer than 255 head reads.
    let peaked_max = heads_for_top1
        .iter()
        .filter(|(fam, _)| *fam >= 2)
        .map(|e| e.1)
        .max()
        .unwrap();
    assert!(peaked_max < 255, "peaked best-first heads {peaked_max}");
    eprintln!("best-first heads for top-1 (family, heads): {heads_for_top1:?}");
}

#[test]
fn f8_headerless_q4_accepts_the_wrong_factorization_silently() {
    let mut rng = Rng(8);
    let fixture = Fixture::new("hdr", random_atlas(&mut rng, 4, 0));
    let mut as_ib = fixture.open(ByteFactorization::IndependentBits, 4);
    let mut as_msb = fixture.open(ByteFactorization::MsbTree, 4);
    let mut differ = 0;
    for (a, b) in contexts(&mut rng, 30) {
        let x = as_ib.predict_next_byte(a, b).unwrap();
        let y = as_msb.predict_next_byte(a, b).unwrap();
        differ += usize::from(x.byte != y.byte);
    }
    // Both opens succeed; nothing in the file says which semantics apply.
    assert!(differ > 0);
}

#[test]
fn f9_deterministic_map_generation_is_eventually_periodic() {
    let mut rng = Rng(9);
    let fixture = Fixture::new("cycle", random_atlas(&mut rng, 8, 0));
    let mut atlas = fixture.open(ByteFactorization::IndependentBits, 8);
    let mut seen = std::collections::HashMap::new();
    let (mut a, mut b) = (b'H', b'i');
    let mut period = None;
    for step in 0..=65_536u32 {
        if let Some(first) = seen.insert((a, b), step) {
            period = Some(step - first);
            break;
        }
        let next = atlas.predict_next_byte(a, b).unwrap().byte;
        a = b;
        b = next;
    }
    // Two-byte state + argmax = a map on 65 536 states: greedy generation
    // must loop. Sampling or top-k with a temperature is required for text.
    let period = period.expect("pigeonhole guarantees a repeated state");
    assert!(period >= 1);
    eprintln!("greedy generation period on synthetic atlas: {period}");
}

#[test]
fn f10_first_two_bytes_are_never_scored() {
    let fixture = Fixture::new("short", vec![0u8; Q4_PAGE_BYTES]);
    for f in MODES {
        let mut atlas = fixture.open(f, 1);
        assert_eq!(atlas.evaluate_document(b"").unwrap(), (0.0, 0));
        assert_eq!(atlas.evaluate_document(b"ab").unwrap(), (0.0, 0));
        let (loss, n) = atlas.evaluate_document(b"abc").unwrap();
        assert_eq!(n, 1);
        assert!((loss - 8.0).abs() < 1e-12);
    }
}

#[test]
fn f11_invalid_heads_and_pages_are_rejected() {
    let fixture = Fixture::new("range", vec![0u8; 2 * Q4_PAGE_BYTES]);
    let mut msb = fixture.open(ByteFactorization::MsbTree, 2);
    assert!(msb.probability(0, 0, 0).is_err());
    assert!(msb.probability(0, 0, 256).is_err());
    let mut ib = fixture.open(ByteFactorization::IndependentBits, 2);
    assert!(ib.probability(0, 0, 8).is_err());
    assert!(fixture.oracle.weight(2, 0).is_err());
    assert!(fixture.oracle.head_logit(0, 0, 256).is_err());
    // Train-time expert API: page_id == pages is out of range.
    let root = temp_path("expert");
    let config = SparseConfig {
        pages: 2,
        max_cached_pages: 1,
        learning_rate: 0.1,
    };
    let mut atlas =
        SparseGradientAtlas::create_with_factorization(&root, config, ByteFactorization::MsbTree)
            .unwrap();
    assert!(atlas.probability_on_page(2, 0, 0, 1).is_err());
    assert!(atlas.probability_on_page(1, 0, 0, 0).is_err());
    assert!(atlas.probability_on_page(1, 0, 0, 255).is_ok());
    drop(atlas);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn conformance_harness_accepts_production_and_rejects_a_greedy_top_k() {
    let mut rng = Rng(0x7099);
    for atlas_index in 0..12u64 {
        let fixture = Fixture::new("conf", random_atlas(&mut rng, 8, atlas_index));
        for f in MODES {
            let mut atlas = fixture.open(f, 8);
            for ctx in contexts(&mut rng, 3) {
                // Top-k built only from production head probabilities. The
                // production scoring breaks some mathematical ties by
                // rounding (f12), so only scores/ranks are checked here.
                check_topk_impl(
                    |a, b, k| exact_top_k(&production_log2(&mut atlas, f, a, b), k),
                    &fixture.oracle,
                    f,
                    ctx,
                    &[0, 1, 2, 5, 16, 255, 256, 300],
                    DEFAULT_TOL_BITS,
                    TieRule::WithinTolerance,
                )
                .unwrap();
                // Production MAP as a k = 1 API.
                check_topk_impl(
                    |a, b, _k| {
                        atlas
                            .predict_next_byte(a, b)
                            .map(|p| vec![(p.byte, p.log2_probability)])
                    },
                    &fixture.oracle,
                    f,
                    ctx,
                    &[1],
                    DEFAULT_TOL_BITS,
                    TieRule::WithinTolerance,
                )
                .unwrap();
            }
        }
    }
    // A plausible-but-wrong top-k (bytes in ascending order) must be rejected.
    let fixture = Fixture::new("bad", random_atlas(&mut Rng(3), 4, 0));
    let wrong = check_topk_impl(
        |a, b, k| -> Result<Vec<(u8, f64)>, ReferenceError> {
            let t = fixture
                .oracle
                .log2_distribution(ByteFactorization::MsbTree, a, b)?;
            Ok((0..k.min(256)).map(|i| (i as u8, t[i])).collect())
        },
        &fixture.oracle,
        ByteFactorization::MsbTree,
        (1, 2),
        &[5],
        DEFAULT_TOL_BITS,
        TieRule::WithinTolerance,
    );
    assert!(wrong.is_err());
}

#[test]
fn f12_mathematical_ties_are_not_always_broken_by_lowest_byte() {
    // Q4 logits are multiples of 1/8, so a factor sigmoid(z) on one branch
    // and 1 - sigmoid(-z) on another are mathematically equal but computed
    // as different f64 values, and the 8 log2 factors are summed in path
    // order. Two bytes with EQUAL model probability can then compare
    // unequal, and `predict_next_byte` may return a higher byte than the
    // documented lowest-byte rule. Characterization test (MsbTree): it
    // asserts the current behaviour and must flip when the fix lands.
    let scan = std::env::var("SML_F12_SCAN")
        .ok()
        .and_then(|v| v.parse::<u64>().ok());
    let indices: Vec<u64> = match scan {
        Some(n) => (0..n).collect(),
        None => F12_KNOWN.iter().map(|e| e.0).collect(),
    };
    let (mut exact_ties, mut violations) = (0u32, Vec::new());
    for index in indices {
        let mut rng = Rng(0x7135_0000 + index);
        let fixture = Fixture::new("tie12", random_atlas(&mut rng, 1, index));
        let mut atlas = fixture.open(ByteFactorization::MsbTree, 1);
        for (a, b) in [(0u8, 0u8), (0x41, 0x42), (0xff, 0x00)] {
            let top = fixture
                .oracle
                .top_k(ByteFactorization::MsbTree, a, b, 2)
                .unwrap();
            if top[0].1.to_bits() == top[1].1.to_bits() {
                exact_ties += 1;
                let got = atlas.predict_next_byte(a, b).unwrap().byte;
                if got != top[0].0 {
                    // Prove the tie is mathematical, not an oracle artifact.
                    assert_eq!(
                        signed_logit_multiset(&fixture.oracle, a, b, got),
                        signed_logit_multiset(&fixture.oracle, a, b, top[0].0)
                    );
                    violations.push((index, (a, b), top[0].0, got));
                }
            }
        }
    }
    eprintln!("f12: {exact_ties} exact MAP ties, violations {violations:?}");
    if scan.is_none() {
        assert_eq!(violations, F12_KNOWN.to_vec());
    }
}

/// (atlas index, context, oracle lowest tied byte, production MAP byte),
/// found with `SML_F12_SCAN=8000`.
const F12_KNOWN: &[(u64, (u8, u8), u8, u8)] = &[
    (1469, (0, 0), 142, 214),
    (1479, (0, 0), 164, 170),
    (1981, (0, 0), 176, 183),
    (3494, (0, 0), 155, 157),
];

/// Sorted signed logits (+z for a 1 bit, -z for a 0 bit) along a byte's
/// MSB-tree path: two bytes with equal multisets have exactly equal model
/// probability, independent of floating-point evaluation order.
fn signed_logit_multiset(oracle: &ReferenceQ4, a: u8, b: u8, byte: u8) -> Vec<f64> {
    let mut v: Vec<f64> = reference_decisions(ByteFactorization::MsbTree, byte)
        .iter()
        .map(|&(head, bit)| {
            let z = oracle.head_logit(a, b, head).unwrap();
            if bit {
                z
            } else {
                -z
            }
        })
        .collect();
    v.sort_by(f64::total_cmp);
    v
}
