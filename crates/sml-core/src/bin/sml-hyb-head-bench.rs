//! Same-host latency of the scaled ternary head inference paths.
//!
//! `synthetic RECORDS` times the paths on `replay_records` with a head
//! trained on them; `trace TRACE.bin HEAD.tsv [LIMIT]` times them on the
//! first `LIMIT` records of a GB-11 P1 trace with an exported head
//! (`head-<base>-seed<seed>.tsv` of `sml-hyb-h1b-s5h` / `sml-hyb-h1c-s5h`).
//! Records are loaded first, so file reads are not timed.
//!
//! Every path produces, per byte, the eight P(bit = 1) values a coder would
//! consume on the true MSB path (P1 squash table after the correction):
//!
//! * `p1_squash`: P1 alone, the shared floor of every path;
//! * `bitplane_alloc`: `ScaledHead::raw` (H-1b kernel, allocates per call);
//! * `fast`: `FastHead` (H-1c), inputs rebuilt per depth;
//! * `fused`: `FusedHead`, inputs shared across depths.
//!
//! Before timing, every head path must give the same probabilities as the
//! scalar oracle on every record (the run stops otherwise). Timing: 2
//! warm-up passes, then `REPS` passes in a rotating path order; the report
//! gives the median and minimum ns per byte and the checksum of each path.
//! Research only, NOT_PROMOTED.

use sml_core::{
    elastic_ngram_paged_sha::{hex, sha256},
    gb11_p1_stream::{read_trace, TraceRecord},
    hyb_fused_head::{FusedHead, SquashTable},
    hyb_h1_residual::{replay_records, Master, DEPTHS, INPUTS},
    hyb_h1b_calibrated::{ScaledHead, LAMBDA_EIGHTHS},
    hyb_h1c_stable::FastHead,
};
use std::{error::Error, fs, fs::File, hint::black_box, time::Instant};

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;
const REPS: usize = 9;
const PATHS: [&str; 4] = ["p1_squash", "bitplane_alloc", "fast", "fused"];

fn parse_head(text: &str) -> Result<(ScaledHead, i32)> {
    let field = |name: &str| -> Result<&str> {
        text.lines()
            .find_map(|l| l.strip_prefix(name)?.strip_prefix('\t'))
            .ok_or_else(|| format!("missing {name}").into())
    };
    if field("inputs")?.parse::<usize>()? != INPUTS || field("depths")?.parse::<usize>()? != DEPTHS
    {
        return Err("head shape mismatch".into());
    }
    let ints = |name: &str| -> Result<[i32; DEPTHS]> {
        let v: Vec<i32> = field(name)?
            .split(',')
            .map(str::parse)
            .collect::<std::result::Result<_, _>>()?;
        v.try_into()
            .map_err(|_| format!("{name} needs {DEPTHS} values").into())
    };
    let weights: Vec<i8> = field("weights")?
        .split(',')
        .map(str::parse)
        .collect::<std::result::Result<_, _>>()?;
    if weights.len() != DEPTHS * INPUTS || weights.iter().any(|w| !(-1..=1).contains(w)) {
        return Err("weights must be DEPTHS*INPUTS ternary values".into());
    }
    let eighths: i32 = field("lambda_eighths")?.parse()?;
    if !LAMBDA_EIGHTHS.contains(&eighths) {
        return Err("lambda outside the registered grid".into());
    }
    Ok((
        ScaledHead::new(weights, ints("unit")?, ints("bias")?),
        eighths,
    ))
}

struct Paths {
    scaled: ScaledHead,
    fast: FastHead,
    fused: FusedHead,
    eighths: i32,
    squash: SquashTable,
}

impl Paths {
    /// Runs path `k` over `records`; returns a checksum of the outputs.
    fn run(&self, k: usize, records: &[TraceRecord]) -> u64 {
        let mut sum = 0u64;
        let mut out = [0u32; 8];
        for r in records {
            match k {
                0 => {
                    for (d, slot) in out.iter_mut().enumerate() {
                        *slot = self.squash.p1(i32::from(r.logits[d]));
                    }
                }
                1 => {
                    for (d, slot) in out.iter_mut().enumerate() {
                        let c = self
                            .scaled
                            .correction(self.scaled.raw(r, d), d, self.eighths);
                        *slot = self.squash.p1(i32::from(r.logits[d]).saturating_add(c));
                    }
                }
                2 => {
                    for (d, slot) in out.iter_mut().enumerate() {
                        let c = self.fast.correction(self.fast.raw(r, d), d, self.eighths);
                        *slot = self.squash.p1(i32::from(r.logits[d]).saturating_add(c));
                    }
                }
                _ => self.fused.probabilities(r, &self.squash, &mut out),
            }
            for &p in &out {
                sum = sum.wrapping_mul(0x100_0000_01b3).wrapping_add(u64::from(p));
            }
        }
        black_box(sum)
    }

    /// Oracle probabilities of every record, hashed.
    fn oracle(&self, records: &[TraceRecord]) -> u64 {
        let mut sum = 0u64;
        for r in records {
            for d in 0..DEPTHS {
                let raw = self.scaled.oracle_raw(r, d);
                let c = self.scaled.correction(raw, d, self.eighths);
                let p = self.squash.p1(i32::from(r.logits[d]).saturating_add(c));
                sum = sum.wrapping_mul(0x100_0000_01b3).wrapping_add(u64::from(p));
            }
        }
        sum
    }
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

fn bench(records: &[TraceRecord], scaled: ScaledHead, eighths: i32, source: &str) -> Result<()> {
    let paths = Paths {
        fast: FastHead::new(&scaled),
        fused: FusedHead::new(&scaled, eighths),
        scaled,
        eighths,
        squash: SquashTable::default(),
    };
    let oracle = paths.oracle(records);
    let mut checksums = [0u64; PATHS.len()];
    for (k, slot) in checksums.iter_mut().enumerate() {
        *slot = paths.run(k, records);
        if k > 0 && *slot != oracle {
            return Err(format!("path {} differs from the oracle", PATHS[k]).into());
        }
    }
    for _ in 0..2 {
        for k in 0..PATHS.len() {
            paths.run(k, records);
        }
    }
    let mut ns = vec![Vec::new(); PATHS.len()];
    for rep in 0..REPS {
        for i in 0..PATHS.len() {
            let k = (i + rep) % PATHS.len();
            let t = Instant::now();
            let sum = paths.run(k, records);
            let elapsed = t.elapsed().as_nanos() as f64;
            if sum != checksums[k] {
                return Err("checksum changed between passes".into());
            }
            ns[k].push(elapsed / records.len() as f64);
        }
    }
    let head_bytes = [0, DEPTHS * INPUTS + 320, 320, FusedHead::resident_bytes()];
    println!("schema\tSML_HYB_HEAD_BENCH_V1");
    println!("source\t{source}");
    println!("records\t{}", records.len());
    println!("lambda_eighths\t{eighths}");
    println!("oracle_equal\ttrue");
    println!("squash_table_bytes\t{}", paths.squash.bytes());
    println!(
        "path\tmedian_ns_per_byte\tmin_ns_per_byte\thead_ns_per_byte\thead_param_bytes\tchecksum"
    );
    let floor = median(ns[0].clone());
    for k in 0..PATHS.len() {
        let med = median(ns[k].clone());
        let min = ns[k].iter().copied().fold(f64::INFINITY, f64::min);
        println!(
            "{}\t{med:.2}\t{min:.2}\t{:.2}\t{}\t{:016x}",
            PATHS[k],
            (med - floor).max(0.0),
            head_bytes[k],
            checksums[k]
        );
    }
    Ok(())
}

fn cli(args: &[String]) -> Result<()> {
    match args.get(1).map(String::as_str) {
        Some("synthetic") if args.len() == 3 => {
            let n: usize = args[2].parse()?;
            let records = replay_records(n);
            let mut master = Master::new(7);
            for r in records.iter().take(50_000) {
                master.train(r);
            }
            bench(
                &records,
                ScaledHead::export(&master, 0.0625),
                4,
                "synthetic",
            )
        }
        Some("trace") if args.len() == 4 || args.len() == 5 => {
            let limit: usize = match args.get(4) {
                Some(n) => n.parse()?,
                None => 4_000_000,
            };
            let head_text = fs::read_to_string(&args[3])?;
            let (scaled, eighths) = parse_head(&head_text)?;
            let mut records = Vec::with_capacity(limit.min(1 << 24));
            read_trace(&mut File::open(&args[2])?, |r| {
                if records.len() < limit {
                    records.push(*r);
                }
            })?;
            let source = format!("trace head_sha256={}", hex(&sha256(head_text.as_bytes())));
            bench(&records, scaled, eighths, &source)
        }
        _ => Err("usage: synthetic RECORDS | trace TRACE.bin HEAD.tsv [LIMIT]".into()),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if let Err(error) = cli(&args) {
        eprintln!("sml-hyb-head-bench: {error}");
        std::process::exit(2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthetic_bench_runs_and_checks_the_oracle() {
        cli(&["bin".into(), "synthetic".into(), "2000".into()]).unwrap();
        assert!(cli(&["bin".into(), "trace".into()]).is_err());
    }

    #[test]
    fn head_parser_refuses_bad_heads() {
        let good = format!(
            "inputs\t{INPUTS}\ndepths\t{DEPTHS}\nlambda_eighths\t2\nunit\t1,1,1,1,1,1,1,1\nbias\t0,0,0,0,0,0,0,0\nweights\t{}\n",
            vec!["0"; DEPTHS * INPUTS].join(",")
        );
        assert!(parse_head(&good).is_ok());
        assert!(parse_head(&good.replace("lambda_eighths\t2", "lambda_eighths\t3")).is_err());
        assert!(parse_head(&good.replace("unit\t1,1,1,1,1,1,1,1", "unit\t1")).is_err());
    }
}
