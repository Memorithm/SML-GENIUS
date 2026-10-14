//! Paired, frozen-weight BMX qualification and measurement. No parameter search.
use sml_core::{
    bit_mixer::{BitMixer, MixerConfig, SideInputs, SideModel},
    bool_coder::{decode_with_predictor, encode_with_predictor},
    elastic_ngram_paged_sha::sha256,
    gru_side::GruSide,
};
use std::{error::Error, fs, io::Write, time::Instant};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
struct Uniform;
impl SideModel for Uniform {
    fn label(&self) -> String {
        "uniform-placebo-v1".into()
    }
    fn push_byte(&mut self, _: u8) {}
    fn predict(&self, _: u32, _: u32) -> (i32, u32) {
        (2048, 0)
    }
}
fn hex(bytes: &[u8]) -> String {
    sha256(bytes).iter().map(|b| format!("{b:02x}")).collect()
}
fn build(arm: &str, gru: &GruSide) -> Result<BitMixer> {
    let config = MixerConfig::default();
    let mixer = match arm {
        "v5" => BitMixer::try_new(config),
        "v5-gru" => BitMixer::try_with_side(
            config,
            config.mask_set.masks(),
            Box::new(gru.clone()),
            SideInputs::ProbabilityOnly,
        ),
        "v5-placebo" => BitMixer::try_with_side(
            config,
            config.mask_set.masks(),
            Box::new(Uniform),
            SideInputs::ProbabilityOnly,
        ),
        _ => return Err("unknown arm".into()),
    };
    Ok(mixer?)
}
fn run(args: &[String]) -> Result<()> {
    if args.len() == 4 && args[1] == "oracle" {
        let mut gru = GruSide::from_bytes(&fs::read(&args[2])?)?;
        let input = fs::read(&args[3])?;
        if input.len() > 4096 {
            return Err("oracle input too large".into());
        }
        for step in 0..=input.len() {
            println!(
                "{}",
                gru.probabilities()
                    .iter()
                    .map(|p| format!("{p:.17e}"))
                    .collect::<Vec<_>>()
                    .join("\t")
            );
            if step < input.len() {
                gru.push_byte(input[step]);
            }
        }
        return Ok(());
    }
    if args.len() != 5 {
        return Err(
            "usage: sml-bmx-gru-eval WEIGHTS PRIME INPUT NEW_REPORT | oracle WEIGHTS INPUT".into(),
        );
    }
    let weights = fs::read(&args[1])?;
    let prime = fs::read(&args[2])?;
    let input = fs::read(&args[3])?;
    if weights.len() > 5_000_000 || prime.len() > 65536 || input.is_empty() || input.len() > 65536 {
        return Err("measurement size bounds exceeded".into());
    }
    let gru = GruSide::from_bytes(&weights)?;
    // Reserve output before expensive work; a failed run remains visibly incomplete.
    let mut report = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args[4])?;
    writeln!(report, "# weights_sha256={} prime_sha256={} input_sha256={} binary_sha256={} prime_bytes={} input_bytes={}", hex(&weights), hex(&prime), hex(&input), hex(&fs::read(std::env::current_exe()?)?), prime.len(), input.len())?;
    writeln!(report, "arm\tmodel_bits\tcoded_bits\tpayload_bytes\tcontainer_bytes\tencoder_setup_s\tencode_s\tdecoder_setup_s\tdecode_s\troundtrip\tidentity")?;
    for arm in ["v5", "v5-gru", "v5-placebo"] {
        let setup = Instant::now();
        let mut encoder = build(arm, &gru)?;
        encoder.prime(&prime);
        let identity = encoder.identity();
        let encoder_setup = setup.elapsed().as_secs_f64();
        let timer = Instant::now();
        let encoded = encode_with_predictor(&input, identity.as_bytes(), &mut encoder)?;
        let encode_s = timer.elapsed().as_secs_f64();
        drop(encoder);
        let setup = Instant::now();
        let mut decoder = build(arm, &gru)?;
        decoder.prime(&prime);
        let decoder_setup = setup.elapsed().as_secs_f64();
        let timer = Instant::now();
        let decoded = decode_with_predictor(&encoded.container, identity.as_bytes(), &mut decoder)?;
        let decode_s = timer.elapsed().as_secs_f64();
        if decoded != input {
            return Err("roundtrip mismatch".into());
        }
        writeln!(report, "{arm}\t{:.12}\t{:.12}\t{}\t{}\t{encoder_setup:.6}\t{encode_s:.6}\t{decoder_setup:.6}\t{decode_s:.6}\tPASS\t{identity}", encoded.model_bits, encoded.coded_bits, encoded.payload_bytes, encoded.container.len())?;
        report.flush()?;
        eprintln!("BMX_GRU_COMPLETE arm={arm} roundtrip=PASS");
    }
    writeln!(report, "# COMPLETE")?;
    Ok(())
}
fn main() {
    if let Err(error) = run(&std::env::args().collect::<Vec<_>>()) {
        eprintln!("BMX_GRU_STOP: {error}");
        std::process::exit(1);
    }
}
