//! DEC-3 runner: DEC-2 Boolean heads plus SML P1 language-model features.
//!
//! Reads the same TSV as `dec2_text_heads`. Trains the P1 byte model once on
//! the unique TRAIN texts (file order), freezes it, adds the `bpc` and
//! `boundary gain` thermometers, then selects on DEV and scores TEST once.
//!
//! Usage: `dec3_lm_heads <input.tsv>`; results go to stdout as `DEC3_*` rows.

use std::collections::HashSet;
use std::time::Instant;

use sml_decision::dec2_text::{
    accuracy, pair_bits, predict, select_on_dev, text_bits, Example, Kind,
};
use sml_decision::dec3_lm::{bpc_bits, gain_bits, with_extra, Lm};

fn refuse(msg: &str) -> ! {
    eprintln!("DEC3_REFUSE {msg}");
    std::process::exit(2)
}

fn kind(task: &str) -> Kind {
    match task {
        "t1" => Kind::Binary,
        "t2" => Kind::Choice,
        "t3" => Kind::Ordinal(3),
        _ => refuse("unknown task"),
    }
}

fn pair(lm: &mut Lm, a: &str, b: &str) -> Vec<u32> {
    let base = pair_bits(&text_bits(a), &text_bits(b));
    let mut extra = bpc_bits(lm.bpc(b));
    extra.extend(gain_bits(lm.boundary_gain(a, b)));
    with_extra(base, &extra)
}

fn example(lm: &mut Lm, task: &str, label: usize, f: &[&str]) -> Example {
    let x = match (task, f.len()) {
        ("t1", 2) => vec![pair(lm, f[0], f[1])],
        ("t2", 5) => f[1..].iter().map(|o| pair(lm, f[0], o)).collect(),
        ("t3", 1) => vec![with_extra(text_bits(f[0]), &bpc_bits(lm.bpc(f[0])))],
        _ => refuse("bad field count"),
    };
    Example { x, y: label }
}

fn digest(preds: &[usize]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &p in preds {
        h ^= p as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 {
        refuse("usage: dec3_lm_heads <input.tsv>");
    }
    let text = std::fs::read_to_string(&args[1]).unwrap_or_else(|_| refuse("cannot read input"));
    let rows: Vec<Vec<&str>> = text.lines().map(|l| l.split('\t').collect()).collect();
    if rows.iter().any(|r| r.len() < 5) {
        refuse("short row");
    }
    // 1. The SML P1 model learns the unique TRAIN texts, in file order.
    let mut lm = Lm::new();
    let mut seen = HashSet::new();
    let train_texts: Vec<&str> = rows
        .iter()
        .filter(|r| r[0] == "train")
        .flat_map(|r| r[4..].iter().copied())
        .filter(|t| seen.insert(*t))
        .collect();
    let t0 = Instant::now();
    let (bits, n) = lm.train(train_texts.iter().copied());
    println!(
        "DEC3_LM texts={} bytes={n} train_bpc={:.4} weights_sha256={} train_s={:.1}",
        train_texts.len(),
        bits / n.max(1) as f64,
        hex(&lm.weights_sha()),
        t0.elapsed().as_secs_f64()
    );
    // 2. Features with the frozen model.
    let t1 = Instant::now();
    let mut sets: Vec<(&str, [Vec<Example>; 3])> = ["t1", "t2", "t3"]
        .iter()
        .map(|&t| (t, [Vec::new(), Vec::new(), Vec::new()]))
        .collect();
    for r in &rows {
        let label: usize = r[3].parse().unwrap_or_else(|_| refuse("bad label"));
        let split = match r[0] {
            "train" => 0,
            "dev" => 1,
            "test" => 2,
            _ => refuse("bad split"),
        };
        let e = example(&mut lm, r[1], label, &r[4..]);
        sets.iter_mut()
            .find(|(t, _)| *t == r[1])
            .unwrap_or_else(|| refuse("bad task"))
            .1[split]
            .push(e);
    }
    println!(
        "DEC3_FEATURES rows={} feature_s={:.1}",
        rows.len(),
        t1.elapsed().as_secs_f64()
    );
    // 3. DEC-2 selection and scoring, unchanged.
    for (task, [train, dev, test]) in &sets {
        let k = kind(task);
        let t = Instant::now();
        let (q, int, grid) = select_on_dev(k, train, dev);
        let fit_s = t.elapsed().as_secs_f64();
        for (epochs, levels, div, c, nz) in grid {
            println!("DEC3_GRID {task} epochs={epochs} levels={levels} keep_div={div} dev={c}/{} nonzero={nz}", dev.len());
        }
        println!(
            "DEC3_SELECT {task} quantized epochs={} levels={} keep_div={} dev={}/{} nonzero={} | integer epochs={} dev={}/{} fit_s={fit_s:.1}",
            q.epochs,
            q.levels,
            q.keep_div,
            q.dev_correct,
            dev.len(),
            q.head.nonzero(),
            int.epochs,
            int.dev_correct,
            dev.len()
        );
        for (arm, sel) in [("SML_BOOL_LM", &q), ("SML_INT_LM", &int)] {
            let preds: Vec<usize> = test.iter().map(|e| predict(k, &sel.head, e)).collect();
            let (c, err) = accuracy(k, &sel.head, test);
            let n = test.len();
            let mae = if matches!(k, Kind::Ordinal(_)) {
                format!("{:.4}", err as f64 / n as f64)
            } else {
                "-".into()
            };
            println!(
                "DEC3_SCORE {arm} {task} test {n} {c} {:.4} {mae}",
                c as f64 / n as f64
            );
            println!("DEC3_PRED {arm} {task} digest={:016x}", digest(&preds));
        }
    }
}
