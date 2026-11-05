//! DEC-4 runner: Boolean heads that must read a varying yes/no question.
//!
//! Reads the DEC-2 TSV, keeps T1 and T3, asks each example two questions with
//! opposite answers, trains the DEC-2 heads on TRAIN (seen wordings), selects
//! on DEV (seen wordings), then scores TEST once with seen and once with
//! held-out wordings. Arms: SML_BOOL_INSTR (primary), SML_INT_INSTR, and
//! BLIND (no question bits; 0.5 by construction).
//!
//! Usage: `dec4_instr_heads <input.tsv>`; results go to stdout as `DEC4_*` rows.

use std::time::Instant;

use sml_decision::dec2_text::{predict, select_on_dev_dim, Example, Head, Kind};
use sml_decision::dec4_instr::{
    answer, content_bits, features, held_out, intents, seen, wording, Intent, DIM4,
};

fn refuse(msg: &str) -> ! {
    eprintln!("DEC4_REFUSE {msg}");
    std::process::exit(2)
}

/// One asked row: features, answer, task, intent and the source example.
struct Row {
    e: Example,
    task: &'static str,
    intent: Intent,
    pair: usize,
}

#[derive(Default)]
struct Sets {
    train: Vec<Row>,
    dev: Vec<Row>,
    seen: Vec<Row>,
    new: Vec<Row>,
}

fn digest(preds: &[usize]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &p in preds {
        h ^= p as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

fn build(rows: &[Vec<&str>], blind: bool) -> Sets {
    let mut s = Sets::default();
    for (pair, r) in rows.iter().enumerate() {
        let task: &'static str = match r[1] {
            "t1" => "t1",
            "t3" => "t3",
            "t2" => continue,
            _ => refuse("bad task"),
        };
        let label: usize = r[3].parse().unwrap_or_else(|_| refuse("bad label"));
        if r[4..].len() != if task == "t1" { 2 } else { 1 } {
            refuse("bad field count");
        }
        let content = content_bits(task, &r[4..]);
        for intent in intents(task, r[2], label) {
            let ask = |set: &'static [&'static str]| {
                let q = wording(set, r[0], r[2], intent);
                let x = features(task, &content, (!blind).then_some(q));
                Row {
                    e: Example {
                        x: vec![x],
                        y: answer(intent, label),
                    },
                    task,
                    intent,
                    pair,
                }
            };
            match r[0] {
                "train" => s.train.push(ask(seen(intent))),
                "dev" => s.dev.push(ask(seen(intent))),
                "test" => {
                    s.seen.push(ask(seen(intent)));
                    s.new.push(ask(held_out(intent)));
                }
                _ => refuse("bad split"),
            }
        }
    }
    s
}

fn examples(rows: &[Row]) -> Vec<Example> {
    rows.iter().map(|r| r.e.clone()).collect()
}

fn score(arm: &str, variant: &str, head: &Head, rows: &[Row]) {
    let preds: Vec<usize> = rows
        .iter()
        .map(|r| predict(Kind::Binary, head, &r.e))
        .collect();
    let ok = |i: usize| preds[i] == rows[i].e.y;
    let n = rows.len();
    let c = (0..n).filter(|&i| ok(i)).count();
    println!(
        "DEC4_SCORE {arm} {variant} {n} {c} {:.4}",
        c as f64 / n.max(1) as f64
    );
    println!("DEC4_PRED {arm} {variant} digest={:016x}", digest(&preds));
    for task in ["t1", "t3"] {
        let idx: Vec<usize> = (0..n).filter(|&i| rows[i].task == task).collect();
        let c = idx.iter().filter(|&&i| ok(i)).count();
        // Both questions of one example answered right.
        let both = idx
            .windows(2)
            .filter(|w| rows[w[0]].pair == rows[w[1]].pair && ok(w[0]) && ok(w[1]))
            .count();
        println!(
            "DEC4_TASK {arm} {variant} {task} rows={} acc={:.4} both_right={:.4}",
            idx.len(),
            c as f64 / idx.len().max(1) as f64,
            both as f64 / (idx.len() / 2).max(1) as f64
        );
    }
    let mut intents: Vec<Intent> = rows.iter().map(|r| r.intent).collect();
    intents.dedup();
    let mut seen_i = Vec::new();
    for i in intents {
        if seen_i.contains(&i) {
            continue;
        }
        seen_i.push(i);
        let idx: Vec<usize> = (0..n).filter(|&k| rows[k].intent == i).collect();
        let c = idx.iter().filter(|&&k| ok(k)).count();
        println!(
            "DEC4_INTENT {arm} {variant} {i:?} rows={} acc={:.4}",
            idx.len(),
            c as f64 / idx.len().max(1) as f64
        );
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 {
        refuse("usage: dec4_instr_heads <input.tsv>");
    }
    let text = std::fs::read_to_string(&args[1]).unwrap_or_else(|_| refuse("cannot read input"));
    let rows: Vec<Vec<&str>> = text.lines().map(|l| l.split('\t').collect()).collect();
    if rows.iter().any(|r| r.len() < 5) {
        refuse("short row");
    }
    for (arm_q, arm_i, blind) in [
        ("SML_BOOL_INSTR", "SML_INT_INSTR", false),
        ("BLIND", "BLIND_INT", true),
    ] {
        let t = Instant::now();
        let s = build(&rows, blind);
        println!(
            "DEC4_ROWS {arm_q} train={} dev={} test_seen={} test_new={} feature_s={:.1}",
            s.train.len(),
            s.dev.len(),
            s.seen.len(),
            s.new.len(),
            t.elapsed().as_secs_f64()
        );
        let (train, dev) = (examples(&s.train), examples(&s.dev));
        let t = Instant::now();
        let (q, int, grid) = select_on_dev_dim(Kind::Binary, DIM4, &train, &dev);
        let fit_s = t.elapsed().as_secs_f64();
        for (epochs, levels, div, c, nz) in grid {
            println!(
                "DEC4_GRID {arm_q} epochs={epochs} levels={levels} keep_div={div} dev={c}/{} nonzero={nz}",
                dev.len()
            );
        }
        println!(
            "DEC4_SELECT {arm_q} quantized epochs={} levels={} keep_div={} dev={}/{} nonzero={} | integer epochs={} dev={}/{} fit_s={fit_s:.1}",
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
        for (arm, head) in [(arm_q, &q.head), (arm_i, &int.head)] {
            score(arm, "seen", head, &s.seen);
            score(arm, "new", head, &s.new);
        }
    }
}
