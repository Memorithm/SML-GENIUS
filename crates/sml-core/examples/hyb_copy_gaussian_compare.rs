//! Research-only executable for the COPY-G vs fixed-20/80 competitor.
//! This uses SYNTHETIC test data and synthetic heads only. It makes no real
//! corpus or model-quality claim and cannot satisfy sealed TEST admission.
//!
//! cargo run --locked --release -p sml-core --example hyb_copy_gaussian_compare

use sml_core::{
    gb11_p1_stream::Tables,
    hyb_h1d_gaussian_copy::{CopyActivityHistogram, GaussianCopyProfile},
    hyb_h3_engine::{compress, decompress, engine_p1s, oracle_p1s, synthetic, HybridHead},
};
use std::{error::Error, time::Instant};

fn main() -> Result<(), Box<dyn Error>> {
    let mut train = CopyActivityHistogram::default();
    // Calibration is from disjoint synthetic TRAIN strings, not heldout
    // code losses, and it is common to both H-1d seeds.
    for _ in 0..6 {
        train.observe_train_document(&b"ababababcababababc".repeat(250))
            .map_err(|e| format!("TRAIN Gaussian calibration observation: {e:?}"))?;
    }
    let profile = GaussianCopyProfile::fit_train_only(&train)
        .map_err(|e| format!("TRAIN Gaussian calibration was not comparable: {e:?}"))?;
    println!(
        "calibration,counts={:?},amplitude={},weights={:?},active={},effective_exposure={},fixed_exposure={}",
        train.counts(),
        profile.amplitude(),
        profile.weights(),
        train.active_decisions(),
        profile.exposure_units(&train),
        u128::from(train.active_decisions()) * 2048
    );

    let tables = Tables::new();
    let mut model = synthetic::model(&tables);
    println!("seed,document,arm,bytes,container_bytes,bpc,encode_us,decode_us,oracle_mismatches,round_trip");
    let documents = [
        ("repeat", b"abcabcabc---abcabcabc---".repeat(96)),
        ("holdout", synthetic::text(97, 2304)),
        ("binary", (0..2304).map(|i| (i as u8).wrapping_mul(37)).collect()),
    ];
    for seed in [11, 23] {
        let heads = [
            ("neural", HybridHead::single(synthetic::head(seed), 8)),
            ("fixed20", HybridHead::single_with_default_copy(synthetic::head(seed), 8)),
            (
                "gaussian",
                HybridHead::single_with_gaussian_copy(synthetic::head(seed), 8, profile),
            ),
        ];
        for (document, data) in &documents {
            for (arm, head) in &heads {
                let expected = oracle_p1s(&mut model, &tables, head, data);
                let actual = engine_p1s(&model, &tables, head, data);
                let mismatches = actual.iter().zip(&expected).filter(|(a, b)| a != b).count();
                let before = Instant::now();
                let container = compress(&model, &tables, head, data);
                let encode_time = before.elapsed().as_micros();
                let before_decode = Instant::now();
                let decoded = decompress(&model, &tables, head, &container)
                    .map_err(|e| format!("decoder failed: {e:?}"))?;
                let decode_time = before_decode.elapsed().as_micros();
                let round_trip = decoded == *data;
                let bpc = container.len() as f64 * 8.0 / data.len() as f64;
                println!(
                    "{seed},{document},{arm},{},{},{bpc:.6},{encode_time},{decode_time},{mismatches},{round_trip}",
                    data.len(),
                    container.len()
                );
                if mismatches != 0 || !round_trip {
                    return Err("invalid copy competitor oracle/round trip".into());
                }
            }
        }
    }
    Ok(())
}
