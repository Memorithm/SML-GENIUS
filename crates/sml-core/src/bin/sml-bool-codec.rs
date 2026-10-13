//! ENG-1d/BAC: compress and decompress a file with the Boolean arithmetic
//! coder driven by the paged byte n-gram trained on a separate TRAIN file.
//!
//! The compressed size is a physical check of the model's log-loss: the
//! report prints the model's f64 bits, the bits the coder actually applied
//! and the payload size. Decoding needs the identical TRAIN file; the
//! container records its SHA-256, the depth and the escape policy.
//!
//! Usage:
//!   sml-bool-codec encode TRAIN WORK_DIR INPUT NEW_OUTPUT DEPTH POLICY
//!   sml-bool-codec decode TRAIN WORK_DIR INPUT NEW_OUTPUT
//!   sml-bool-codec encode-mix PRIME|- INPUT NEW_OUTPUT
//!   sml-bool-codec decode-mix PRIME|- INPUT NEW_OUTPUT
//! POLICY is `wb`, `swb:<beta>` or `dir:<tau>`. The `-mix` commands use the
//! adaptive Boolean context-mixing predictor (`bit_mixer`), optionally
//! primed by learning PRIME first (`-` for none); decoding needs the same
//! PRIME, whose SHA-256 is recorded in the container.

use std::{
    error::Error,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    time::Instant,
};

use sml_core::{
    bit_mixer::{BitMixer, MixerConfig},
    bool_coder::{
        container_model_id, decode_bytes, decode_with_predictor, encode_bytes,
        encode_with_predictor, CoderError,
    },
    elastic_ngram_paged::{
        build_paged, training_sha256, EscapePolicy, PagedBuild, PagedLimits, PagedNGram,
        MAX_PAGED_DEPTH,
    },
    elastic_ngram_paged_sha::hex,
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const MAX_FILE: u64 = 64 * 1024 * 1024;

fn read(path: &Path) -> Result<Vec<u8>> {
    if fs::metadata(path)?.len() > MAX_FILE {
        return Err("input above 64 MiB".into());
    }
    Ok(fs::read(path)?)
}

fn write_new(path: &Path, data: &[u8]) -> Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(data)?;
    file.sync_all()?;
    Ok(())
}

struct Model {
    paged: PagedNGram,
    depth: usize,
    policy: EscapePolicy,
    id: Vec<u8>,
    dirs: [PathBuf; 2],
}

impl Drop for Model {
    fn drop(&mut self) {
        for dir in &self.dirs {
            let _ = fs::remove_dir_all(dir);
        }
    }
}

fn model_id(train_sha: &[u8; 32], depth: usize, policy: EscapePolicy) -> Vec<u8> {
    format!(
        "sml-paged-wb-chain-v1;train_sha256={};depth={depth};policy={}",
        hex(train_sha),
        policy.label()
    )
    .into_bytes()
}

fn parse_id(id: &[u8]) -> Result<(String, usize, EscapePolicy)> {
    let text = std::str::from_utf8(id)?;
    let mut fields = text.split(';');
    if fields.next() != Some("sml-paged-wb-chain-v1") {
        return Err("unknown model family in container".into());
    }
    let mut take = |key: &str| -> Result<String> {
        let field = fields.next().ok_or("incomplete model id")?;
        let (k, v) = field.split_once('=').ok_or("malformed model id")?;
        if k != key {
            return Err("unexpected model id field".into());
        }
        Ok(v.to_string())
    };
    let sha = take("train_sha256")?;
    let depth = take("depth")?.parse()?;
    let policy = EscapePolicy::from_label(&take("policy")?)?;
    Ok((sha, depth, policy))
}

fn load(train: &[u8], work: &Path, depth: usize, policy: EscapePolicy) -> Result<Model> {
    if depth > MAX_PAGED_DEPTH {
        return Err("depth above 32".into());
    }
    let policy = policy.validate()?;
    fs::create_dir_all(work)?;
    let tag = format!(
        "{}-{}",
        std::process::id(),
        Instant::now().elapsed().as_nanos()
    );
    let store = work.join(format!("bac-store-{tag}"));
    let sort = work.join(format!("bac-sort-{tag}"));
    // Build only the requested depth: deeper levels are never read and
    // would cost construction work and capacity quota.
    let built = build_paged(train, depth, &store, &sort, PagedLimits::default());
    let dirs = [store.clone(), sort];
    let PagedBuild::Published(_) = built? else {
        for dir in &dirs {
            let _ = fs::remove_dir_all(dir);
        }
        return Err("TRAIN exceeds paged capacity limits".into());
    };
    let sha = training_sha256(train);
    Ok(Model {
        paged: PagedNGram::open(&store, sha)?,
        depth,
        policy,
        id: model_id(&sha, depth, policy),
        dirs,
    })
}

fn distribution(model: &mut Model, history: &[u8]) -> std::result::Result<[f64; 256], CoderError> {
    model
        .paged
        .distribution_with(history, model.depth, model.policy)
        .map_err(|e| CoderError::Model(e.to_string()))
}

fn encode(args: &[String]) -> Result<()> {
    let depth: usize = args[6].parse()?;
    let policy = EscapePolicy::from_label(&args[7])?;
    let train = read(Path::new(&args[2]))?;
    let mut model = load(&train, Path::new(&args[3]), depth, policy)?;
    let input = read(Path::new(&args[4]))?;
    let started = Instant::now();
    let id = model.id.clone();
    let encoded = encode_bytes(&input, depth, &id, |h| distribution(&mut model, h))?;
    let seconds = started.elapsed().as_secs_f64();
    write_new(Path::new(&args[5]), &encoded.container)?;
    let n = input.len().max(1) as f64;
    println!(
        "BAC_ENCODE\tinput_bytes={}\tcontainer_bytes={}\tpayload_bytes={}",
        input.len(),
        encoded.container.len(),
        encoded.payload_bytes
    );
    println!(
        "BAC_BITS\tmodel_bpb={:.9}\tcoded_bpb={:.9}\tpayload_bpb={:.9}\tcoder_overhead_bits={:.6}\tflush_bits={:.3}",
        encoded.model_bits / n,
        encoded.coded_bits / n,
        8.0 * encoded.payload_bytes as f64 / n,
        encoded.coded_bits - encoded.model_bits,
        8.0 * encoded.payload_bytes as f64 - encoded.coded_bits
    );
    println!(
        "BAC_MODEL\tdepth={depth}\tpolicy={}\tseconds={seconds:.3}\tbytes_per_second={:.0}",
        policy.label(),
        n / seconds.max(1e-9)
    );
    Ok(())
}

fn decode(args: &[String]) -> Result<()> {
    let container = read(Path::new(&args[4]))?;
    let (train_sha, depth, policy) = parse_id(container_model_id(&container)?)?;
    // Reject a wrong TRAIN before any model construction work.
    let train = read(Path::new(&args[2]))?;
    if hex(&training_sha256(&train)) != train_sha {
        return Err("TRAIN file differs from the one used to encode".into());
    }
    let mut model = load(&train, Path::new(&args[3]), depth, policy)?;
    let started = Instant::now();
    let id = model.id.clone();
    let output = decode_bytes(&container, depth, &id, |h| distribution(&mut model, h))?;
    write_new(Path::new(&args[5]), &output)?;
    println!(
        "BAC_DECODE\toutput_bytes={}\tdepth={depth}\tpolicy={}\tseconds={:.3}\tsha256_verified=YES",
        output.len(),
        policy.label(),
        started.elapsed().as_secs_f64()
    );
    Ok(())
}

fn mix_id(prime: &[u8], config: MixerConfig) -> Vec<u8> {
    format!(
        "{};prime_sha256={};prime_bytes={}",
        config.label(),
        hex(&training_sha256(prime)),
        prime.len()
    )
    .into_bytes()
}

fn read_prime(arg: &str) -> Result<Vec<u8>> {
    if arg == "-" {
        Ok(Vec::new())
    } else {
        read(Path::new(arg))
    }
}

fn primed(prime: &[u8], config: MixerConfig) -> Result<BitMixer> {
    let mut mixer = BitMixer::try_new(config)?;
    mixer.prime(prime);
    Ok(mixer)
}

fn encode_mix(args: &[String]) -> Result<()> {
    let config = MixerConfig::default();
    let prime = read_prime(&args[2])?;
    let input = read(Path::new(&args[3]))?;
    let started = Instant::now();
    let mut mixer = primed(&prime, config)?;
    let prime_seconds = started.elapsed().as_secs_f64();
    let started = Instant::now();
    let encoded = encode_with_predictor(&input, &mix_id(&prime, config), &mut mixer)?;
    let seconds = started.elapsed().as_secs_f64();
    write_new(Path::new(&args[4]), &encoded.container)?;
    let n = input.len().max(1) as f64;
    println!(
        "BMX_ENCODE\tinput_bytes={}\tprime_bytes={}\tcontainer_bytes={}\tpayload_bytes={}",
        input.len(),
        prime.len(),
        encoded.container.len(),
        encoded.payload_bytes
    );
    println!(
        "BMX_BITS\tmodel_bpb={:.9}\tpayload_bpb={:.9}\tflush_bits={:.3}",
        encoded.model_bits / n,
        8.0 * encoded.payload_bytes as f64 / n,
        8.0 * encoded.payload_bytes as f64 - encoded.coded_bits
    );
    println!(
        "BMX_SPEED\tprime_seconds={prime_seconds:.3}\tcode_seconds={seconds:.3}\tbytes_per_second={:.0}",
        n / seconds.max(1e-9)
    );
    Ok(())
}

fn decode_mix(args: &[String]) -> Result<()> {
    let prime = read_prime(&args[2])?;
    let container = read(Path::new(&args[3]))?;
    let stored = container_model_id(&container)?;
    // Select the released configuration whose identity the container
    // carries, so older (bmx-v2, bmx-v3, bmx-v4) archives remain decodable.
    let (config, id) = MixerConfig::released()
        .into_iter()
        .map(|config| (config, mix_id(&prime, config)))
        .find(|(_, id)| id.as_slice() == stored)
        .ok_or("PRIME or mixer configuration differs from the one used to encode")?;
    let started = Instant::now();
    let mut mixer = primed(&prime, config)?;
    let output = decode_with_predictor(&container, &id, &mut mixer)?;
    write_new(Path::new(&args[4]), &output)?;
    println!(
        "BMX_DECODE\toutput_bytes={}\tseconds={:.3}\tsha256_verified=YES",
        output.len(),
        started.elapsed().as_secs_f64()
    );
    Ok(())
}

fn run(args: &[String]) -> Result<()> {
    match args.get(1).map(String::as_str) {
        Some("encode") if args.len() == 8 => encode(args),
        Some("decode") if args.len() == 6 => decode(args),
        Some("encode-mix") if args.len() == 5 => encode_mix(args),
        Some("decode-mix") if args.len() == 5 => decode_mix(args),
        _ => Err("usage: sml-bool-codec encode TRAIN WORK_DIR INPUT NEW_OUTPUT DEPTH POLICY | decode TRAIN WORK_DIR INPUT NEW_OUTPUT | encode-mix PRIME|- INPUT NEW_OUTPUT | decode-mix PRIME|- INPUT NEW_OUTPUT".into()),
    }
}

fn main() {
    if let Err(e) = run(&std::env::args().collect::<Vec<_>>()) {
        eprintln!("BAC_STOP: {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_id_round_trips_and_rejects_foreign_ids() {
        let policy = EscapePolicy::ScaledWittenBell { beta: 16.0 };
        let id = model_id(&[7; 32], 12, policy);
        let (sha, depth, parsed) = parse_id(&id).unwrap();
        assert_eq!(
            (sha.as_str(), depth, parsed),
            (hex(&[7; 32]).as_str(), 12, policy)
        );
        assert!(parse_id(b"other;train_sha256=00;depth=1;policy=wb").is_err());
        assert!(parse_id(b"sml-paged-wb-chain-v1;depth=1;train_sha256=00;policy=wb").is_err());
        assert!(parse_id(b"sml-paged-wb-chain-v1;train_sha256=00;depth=1;policy=dir:0").is_err());
    }

    /// Container written by the released bmx-v2 `encode-mix` (main at
    /// 1fdb900) for `golden_text()` without priming.
    const BMX_V2_GOLDEN: &[u8] = include_bytes!("../../tests/fixtures/bmx-v2-golden.bmx");

    /// Container written by the released bmx-v3 `encode-mix` (main at
    /// e89dcd6) for `golden_text()` without priming.
    const BMX_V3_GOLDEN: &[u8] = include_bytes!("../../tests/fixtures/bmx-v3-golden.bmx");
    /// Container written by the released bmx-v4 encoder (main at 618549c)
    /// for `golden_text()` without priming.
    const BMX_V4_GOLDEN: &[u8] = include_bytes!("../../tests/fixtures/bmx-v4-golden.bmx");

    fn golden_text() -> Vec<u8> {
        (0..800)
            .flat_map(|k: u32| {
                format!("golden {} line of bmx v2 compatibility text. ", k % 97).into_bytes()
            })
            .collect()
    }

    #[test]
    fn released_bmx_v2_containers_still_decode_and_v2_encoding_is_unchanged() {
        let root = std::env::temp_dir().join(format!("bmx-golden-{}", std::process::id()));
        fs::create_dir(&root).unwrap();
        let s = |p: &Path| p.display().to_string();
        let packed = root.join("golden.bmx");
        let out = root.join("golden.txt");
        fs::write(&packed, BMX_V2_GOLDEN).unwrap();
        run(&[
            "x".into(),
            "decode-mix".into(),
            "-".into(),
            s(&packed),
            s(&out),
        ])
        .unwrap();
        assert_eq!(fs::read(&out).unwrap(), golden_text());
        // The v2 configuration still produces the released bytes exactly.
        let v2 = MixerConfig::released()[3];
        let mut mixer = BitMixer::try_new(v2).unwrap();
        let encoded = encode_with_predictor(&golden_text(), &mix_id(&[], v2), &mut mixer).unwrap();
        assert_eq!(encoded.container, BMX_V2_GOLDEN);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn released_bmx_v3_containers_still_decode_and_v3_encoding_is_unchanged() {
        let root = std::env::temp_dir().join(format!("bmx3-golden-{}", std::process::id()));
        fs::create_dir(&root).unwrap();
        let s = |p: &Path| p.display().to_string();
        let packed = root.join("golden.bmx");
        let out = root.join("golden.txt");
        fs::write(&packed, BMX_V3_GOLDEN).unwrap();
        run(&[
            "x".into(),
            "decode-mix".into(),
            "-".into(),
            s(&packed),
            s(&out),
        ])
        .unwrap();
        assert_eq!(fs::read(&out).unwrap(), golden_text());
        let v3 = MixerConfig::released()[2];
        let mut mixer = BitMixer::try_new(v3).unwrap();
        let encoded = encode_with_predictor(&golden_text(), &mix_id(&[], v3), &mut mixer).unwrap();
        assert_eq!(encoded.container, BMX_V3_GOLDEN);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn released_bmx_v4_containers_still_decode_and_v4_encoding_is_unchanged() {
        let root = std::env::temp_dir().join(format!("bmx4-golden-{}", std::process::id()));
        fs::create_dir(&root).unwrap();
        let s = |p: &Path| p.display().to_string();
        let packed = root.join("golden.bmx");
        let out = root.join("golden.txt");
        fs::write(&packed, BMX_V4_GOLDEN).unwrap();
        run(&[
            "x".into(),
            "decode-mix".into(),
            "-".into(),
            s(&packed),
            s(&out),
        ])
        .unwrap();
        assert_eq!(fs::read(&out).unwrap(), golden_text());
        let v4 = MixerConfig::released()[1];
        let mut mixer = BitMixer::try_new(v4).unwrap();
        let encoded = encode_with_predictor(&golden_text(), &mix_id(&[], v4), &mut mixer).unwrap();
        assert_eq!(encoded.container, BMX_V4_GOLDEN);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn paged_model_round_trip_through_files() {
        let root = std::env::temp_dir().join(format!("bac-cli-{}", std::process::id()));
        fs::create_dir(&root).unwrap();
        let text: Vec<u8> = (0..6000)
            .flat_map(|k: u32| format!("le {} de la {} ", k % 41, (k * 7) % 29).into_bytes())
            .collect();
        let (train, input) = text.split_at(text.len() * 3 / 4);
        let train_path = root.join("train.txt");
        let input_path = root.join("input.txt");
        fs::write(&train_path, train).unwrap();
        fs::write(&input_path, input).unwrap();
        let s = |p: &Path| p.display().to_string();
        let packed = root.join("input.bac");
        let restored = root.join("restored.txt");
        let work = root.join("work");
        run(&[
            "x".into(),
            "encode".into(),
            s(&train_path),
            s(&work),
            s(&input_path),
            s(&packed),
            "7".into(),
            "swb:8".into(),
        ])
        .unwrap();
        run(&[
            "x".into(),
            "decode".into(),
            s(&train_path),
            s(&work),
            s(&packed),
            s(&restored),
        ])
        .unwrap();
        assert_eq!(fs::read(&restored).unwrap(), input);
        assert!(fs::metadata(&packed).unwrap().len() < input.len() as u64 / 3);
        // No overwrite; a different TRAIN cannot decode.
        assert!(run(&[
            "x".into(),
            "decode".into(),
            s(&train_path),
            s(&work),
            s(&packed),
            s(&restored)
        ])
        .is_err());
        let other_train = root.join("other.txt");
        fs::write(&other_train, &train[1..]).unwrap();
        assert!(run(&[
            "x".into(),
            "decode".into(),
            s(&other_train),
            s(&root.join("never-built")),
            s(&packed),
            s(&root.join("x.txt"))
        ])
        .is_err());
        // The TRAIN mismatch is detected before any store is built.
        assert!(!root.join("never-built").exists());
        // A shallow model is built only to its own depth and still round-trips.
        let shallow = root.join("input-d2.bac");
        let shallow_out = root.join("restored-d2.txt");
        run(&[
            "x".into(),
            "encode".into(),
            s(&train_path),
            s(&work),
            s(&input_path),
            s(&shallow),
            "2".into(),
            "dir:16".into(),
        ])
        .unwrap();
        run(&[
            "x".into(),
            "decode".into(),
            s(&train_path),
            s(&work),
            s(&shallow),
            s(&shallow_out),
        ])
        .unwrap();
        assert_eq!(fs::read(&shallow_out).unwrap(), input);
        // Adaptive mixer, unprimed and primed; a wrong PRIME is refused.
        for (prime, tag) in [(String::from("-"), "cold"), (s(&train_path), "warm")] {
            let packed = root.join(format!("input-{tag}.bmx"));
            let out = root.join(format!("restored-{tag}.txt"));
            run(&[
                "x".into(),
                "encode-mix".into(),
                prime.clone(),
                s(&input_path),
                s(&packed),
            ])
            .unwrap();
            run(&["x".into(), "decode-mix".into(), prime, s(&packed), s(&out)]).unwrap();
            assert_eq!(fs::read(&out).unwrap(), input);
        }
        assert!(run(&[
            "x".into(),
            "decode-mix".into(),
            s(&other_train),
            s(&root.join("input-warm.bmx")),
            s(&root.join("bad.txt"))
        ])
        .is_err());
        assert!(run(&[
            "x".into(),
            "decode-mix".into(),
            "-".into(),
            s(&root.join("input-warm.bmx")),
            s(&root.join("bad2.txt"))
        ])
        .is_err());
        // Temporary stores are cleaned up.
        assert_eq!(fs::read_dir(&work).unwrap().count(), 0);
        fs::remove_dir_all(root).unwrap();
    }
}
