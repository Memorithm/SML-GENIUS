//! Bounded read-only Q4 next-byte generation using the *trained* sparse atlas.
//! Research-only: fixed two-byte hash routing, no SBG/MOR/25B model claims.
use sml_core::{
    byte_factorization::ByteFactorization,
    elastic_ngram_paged_sha::{hex, parse_hex, Sha256},
    sparse_gradient_atlas::{QuantizedSparseAtlas, SparseConfig},
};
use std::{
    env,
    error::Error,
    fs::{self, File},
    io::Read,
    path::Path,
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const MAX_Q4_BYTES: u64 = 64 * 1024 * 1024;
const PAGE_Q4_BYTES: u64 = 512;

fn main() -> Result<()> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.len() != 5 {
        return Err(
            "usage: sml-q4-byte-generate WEIGHTS.q4 SHA256 independent|msb SEED_HEX_4 COUNT_1_128"
                .into(),
        );
    }
    let path = Path::new(&args[0]);
    let expected = parse_hex(&args[1]).ok_or("invalid Q4 SHA256")?;
    if hex(&expected) != args[1] {
        return Err("Q4 SHA256 must be canonical lowercase hex".into());
    }
    let factorization = match args[2].as_str() {
        "independent" => ByteFactorization::IndependentBits,
        "msb" => ByteFactorization::MsbTree,
        _ => return Err("invalid factorization (independent or msb)".into()),
    };
    let seed = &args[3];
    if seed.len() != 4 || !seed.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("seed must contain exactly two bytes as four hex digits".into());
    }
    let mut a = u8::from_str_radix(&seed[0..2], 16)?;
    let mut b = u8::from_str_radix(&seed[2..4], 16)?;
    let count: usize = args[4].parse()?;
    if !(1..=128).contains(&count) {
        return Err("requested output length exceeds bounded 1..128 envelope".into());
    }

    let stat = fs::symlink_metadata(path)?;
    if !stat.is_file() || stat.file_type().is_symlink()
        || stat.len() == 0
        || stat.len() > MAX_Q4_BYTES
        || stat.len() % PAGE_Q4_BYTES != 0
    {
        return Err("Q4 artifact is not a regular, bounded, page-aligned file".into());
    }
    let mut file = File::open(path)?;
    let mut hash = Sha256::default();
    let mut buffer = [0_u8; 1024 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    if hash.finish() != expected {
        return Err("Q4 artifact SHA256 mismatch".into());
    }
    if file.metadata()?.len() != stat.len() {
        return Err("Q4 file length changed during integrity check".into());
    }

    let pages = stat.len() / PAGE_Q4_BYTES;
    let config = SparseConfig {
        pages,
        max_cached_pages: (pages as usize).min(32),
        learning_rate: 0.1,
    };
    let mut model = QuantizedSparseAtlas::open_with_factorization(path, config, factorization)?;
    let mut generated = String::with_capacity(count * 2);
    let mut log2_probability = 0.0;
    let mut head_evaluations = 0_u64;
    let mut q4_page_loads = 0_u64;
    for _ in 0..count {
        let prediction = model.predict_next_byte(a, b)?;
        use std::fmt::Write as _;
        write!(&mut generated, "{:02x}", prediction.byte)?;
        log2_probability += prediction.log2_probability;
        head_evaluations += u64::from(prediction.evaluated_heads);
        q4_page_loads += prediction.page_loads;
        a = b;
        b = prediction.byte;
    }
    println!("schema=sml.q4-byte-generate/v1");
    println!("source_sha256={}", args[1]);
    println!("factorization={}", args[2]);
    println!("model_pages={pages}");
    println!("seed_hex={seed}");
    println!("generated_hex={generated}");
    println!("generated_bytes={count}");
    println!("selected_sequence_log2_probability={log2_probability:.9}");
    println!("head_evaluations={head_evaluations}");
    println!("q4_page_loads={q4_page_loads}");
    println!("SML_Q4_RESEARCH_GENERATION=PASS");
    println!("SML_INTEGRATED_MODEL_QUALITY=NOT_MEASURED");
    Ok(())
}
