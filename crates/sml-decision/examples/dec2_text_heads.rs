//! DEC-2 runner: train Boolean text heads on DEC-1 TRAIN, select on DEV, then
//! score TEST once. Input is the TSV written by RemoteOps
//! `ops/sml/dec1_bench/export_dec2_tsv.py`:
//!
//! `split \t task \t id \t label \t field...` with task t1 (A, B), t2 (passage,
//! options A–D) or t3 (passage). Labels are integers.
//!
//! Usage: `dec2_text_heads <input.tsv>`; results go to stdout as `DEC2_*` rows.

use std::time::Instant;

use sml_decision::dec2_text::{
    accuracy, pair_bits, predict, select_on_dev, text_bits, Example, Kind,
};

struct Sets {
    train: Vec<Example>,
    dev: Vec<Example>,
    test: Vec<Example>,
}

fn kind(task: &str) -> Kind {
    match task {
        "t1" => Kind::Binary,
        "t2" => Kind::Choice,
        "t3" => Kind::Ordinal(3),
        _ => refuse("unknown task"),
    }
}

fn refuse(msg: &str) -> ! {
    eprintln!("DEC2_REFUSE {msg}");
    std::process::exit(2)
}

fn example(task: &str, label: usize, fields: &[&str]) -> Example {
    let x = match (task, fields.len()) {
        ("t1", 2) => vec![pair_bits(&text_bits(fields[0]), &text_bits(fields[1]))],
        ("t2", 5) => {
            let p = text_bits(fields[0]);
            fields[1..]
                .iter()
                .map(|o| pair_bits(&p, &text_bits(o)))
                .collect()
        }
        ("t3", 1) => vec![text_bits(fields[0])],
        _ => refuse("bad field count"),
    };
    Example { x, y: label }
}

/// FNV-1a over the TEST predictions, to compare repeated processes.
fn digest(preds: &[usize]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &p in preds {
        h ^= p as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 {
        refuse("usage: dec2_text_heads <input.tsv>");
    }
    let text = std::fs::read_to_string(&args[1]).unwrap_or_else(|_| refuse("cannot read input"));
    let mut by_task: Vec<(&str, Sets)> = ["t1", "t2", "t3"]
        .iter()
        .map(|&t| {
            (
                t,
                Sets {
                    train: Vec::new(),
                    dev: Vec::new(),
                    test: Vec::new(),
                },
            )
        })
        .collect();
    for line in text.lines() {
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() < 5 {
            refuse("short row");
        }
        let label: usize = f[3].parse().unwrap_or_else(|_| refuse("bad label"));
        let slot = by_task
            .iter_mut()
            .find(|(t, _)| *t == f[1])
            .unwrap_or_else(|| refuse("bad task"));
        let e = example(f[1], label, &f[4..]);
        match f[0] {
            "train" => slot.1.train.push(e),
            "dev" => slot.1.dev.push(e),
            "test" => slot.1.test.push(e),
            _ => refuse("bad split"),
        }
    }
    for (task, sets) in &by_task {
        let k = kind(task);
        let t0 = Instant::now();
        let (ter, int, grid) = select_on_dev(k, &sets.train, &sets.dev);
        let fit_s = t0.elapsed().as_secs_f64();
        for (epochs, levels, div, c, nz) in grid {
            println!(
                "DEC2_GRID {task} epochs={epochs} levels={levels} keep_div={div} dev={c}/{} nonzero={nz}",
                sets.dev.len()
            );
        }
        println!(
            "DEC2_SELECT {task} quantized epochs={} levels={} keep_div={} dev={}/{} nonzero={} | integer epochs={} dev={}/{} fit_s={fit_s:.1}",
            ter.epochs, ter.levels, ter.keep_div, ter.dev_correct, sets.dev.len(), ter.head.nonzero(),
            int.epochs, int.dev_correct, sets.dev.len()
        );
        for (arm, sel) in [("SML_BOOL", &ter), ("SML_INT", &int)] {
            let t = Instant::now();
            let preds: Vec<usize> = sets.test.iter().map(|e| predict(k, &sel.head, e)).collect();
            let us = t.elapsed().as_secs_f64() * 1e6 / sets.test.len().max(1) as f64;
            let (c, err) = accuracy(k, &sel.head, &sets.test);
            let n = sets.test.len();
            let mae = if matches!(k, Kind::Ordinal(_)) {
                format!("{:.4}", err as f64 / n as f64)
            } else {
                "-".into()
            };
            println!(
                "DEC2_SCORE {arm} {task} test {n} {c} {:.4} {mae}",
                c as f64 / n as f64
            );
            println!(
                "DEC2_PRED {arm} {task} digest={:016x} us_per_decision={us:.2}",
                digest(&preds)
            );
        }
    }
}
