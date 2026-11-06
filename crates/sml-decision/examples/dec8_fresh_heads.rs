//! DEC-8 runner: the DEC-7 heads, unchanged, asked a fresh reserved set of
//! wordings (`sml_decision::dec8_fresh`).
//!
//! TRAIN, DEV, features, arms and grid are exactly DEC-7's, so the `seen` and
//! `new` predictions must reproduce the DEC-7 digests. TEST is asked a third
//! time with the fresh wordings (`fresh`), never read before this run.
//!
//! Usage: `dec8_fresh_heads <input.tsv> <lexicon.tsv>`; results go to
//! stdout as `DEC8_*` rows.

use std::time::Instant;

use sml_decision::dec2_text::{predict, select_on_dev_dim, Example, Head, Kind};
use std::collections::HashSet;

use sml_decision::dec4_instr::{
    answer, content_bits, held_out, intents, seen, wording, Intent, DIM4,
};
use sml_decision::dec5_meaning::{hamming, QuestionCoder, WordCodes};
use sml_decision::dec6_lexicon::{features, lexicon_codes};
use sml_decision::dec7_polarity::{cues_in, negated, negators};
use sml_decision::dec8_fresh::fresh;

/// Question features of one arm.
#[derive(Clone, Copy, PartialEq)]
enum Arm {
    Polarity,
    PolarityCtx,
    Blind,
}

/// The two word codes and their question coders.
struct Coders {
    lex: WordCodes,
    lex_q: QuestionCoder,
    ctx: WordCodes,
    ctx_q: QuestionCoder,
    cues: HashSet<String>,
}

/// Words whose code distances are logged (descriptive).
const PROBE: [&str; 16] = [
    "start",
    "beginning",
    "opening",
    "end",
    "final",
    "closing",
    "middle",
    "central",
    "not",
    "false",
    "wrong",
    "continue",
    "continues",
    "follow",
    "next",
    "after",
];

fn refuse(msg: &str) -> ! {
    eprintln!("DEC8_REFUSE {msg}");
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
    fresh: Vec<Row>,
}

fn digest(preds: &[usize]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &p in preds {
        h ^= p as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

fn build(rows: &[Vec<&str>], arm: Arm, c: &Coders) -> Sets {
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
                let x = match arm {
                    Arm::Blind => features(task, &content, &[]),
                    Arm::Polarity => features(
                        task,
                        &content,
                        &[
                            (b"lex", c.lex_q.bits(&c.lex, q)),
                            (b"neg", u64::from(negated(&c.cues, q))),
                        ],
                    ),
                    Arm::PolarityCtx => features(
                        task,
                        &content,
                        &[
                            (b"lex", c.lex_q.bits(&c.lex, q)),
                            (b"ctx", c.ctx_q.bits(&c.ctx, q)),
                            (b"neg", u64::from(negated(&c.cues, q))),
                        ],
                    ),
                };
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
                    s.fresh.push(ask(fresh(intent)));
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
        "DEC8_SCORE {arm} {variant} {n} {c} {:.4}",
        c as f64 / n.max(1) as f64
    );
    println!("DEC8_PRED {arm} {variant} digest={:016x}", digest(&preds));
    for task in ["t1", "t3"] {
        let idx: Vec<usize> = (0..n).filter(|&i| rows[i].task == task).collect();
        let c = idx.iter().filter(|&&i| ok(i)).count();
        // Both questions of one example answered right.
        let both = idx
            .windows(2)
            .filter(|w| rows[w[0]].pair == rows[w[1]].pair && ok(w[0]) && ok(w[1]))
            .count();
        println!(
            "DEC8_TASK {arm} {variant} {task} rows={} acc={:.4} both_right={:.4}",
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
            "DEC8_INTENT {arm} {variant} {i:?} rows={} acc={:.4}",
            idx.len(),
            c as f64 / idx.len().max(1) as f64
        );
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        refuse("usage: dec7_polarity_heads <input.tsv> <lexicon.tsv>");
    }
    let text = std::fs::read_to_string(&args[1]).unwrap_or_else(|_| refuse("cannot read input"));
    let rows: Vec<Vec<&str>> = text.lines().map(|l| l.split('\t').collect()).collect();
    if rows.iter().any(|r| r.len() < 5) {
        refuse("short row");
    }
    // 1. Word codes from the unique TRAIN texts, then the question coder.
    let t = Instant::now();
    let mut uniq = HashSet::new();
    let texts: Vec<&str> = rows
        .iter()
        .filter(|r| r[0] == "train")
        .flat_map(|r| r[4..].iter().copied())
        .filter(|x| uniq.insert(*x))
        .collect();
    let ctx = WordCodes::learn(texts.iter().copied());
    let lex_text =
        std::fs::read_to_string(&args[2]).unwrap_or_else(|_| refuse("cannot read lexicon"));
    let lex = lexicon_codes(&lex_text).unwrap_or_else(|e| refuse(&e));
    let mut train_q = Vec::new();
    for r in rows.iter().filter(|r| r[0] == "train" && r[1] != "t2") {
        let label: usize = r[3].parse().unwrap_or_else(|_| refuse("bad label"));
        for i in intents(r[1], r[2], label) {
            train_q.push(wording(seen(i), r[0], r[2], i));
        }
    }
    let c = Coders {
        lex_q: QuestionCoder::fit(&lex, train_q.iter().copied()),
        ctx_q: QuestionCoder::fit(&ctx, train_q.iter().copied()),
        lex,
        ctx,
        cues: negators(&lex_text),
    };
    let mut cue_list: Vec<&str> = c.cues.iter().map(String::as_str).collect();
    cue_list.sort_unstable();
    println!("DEC8_CUES n={} {}", cue_list.len(), cue_list.join(","));
    println!(
        "DEC8_CODES texts={} ctx_vocab={} ctx_fingerprint={:016x} lex_vocab={} lex_fingerprint={:016x} questions={} codes_s={:.1}",
        texts.len(),
        c.ctx.len(),
        c.ctx.fingerprint(),
        c.lex.len(),
        c.lex.fingerprint(),
        train_q.len(),
        t.elapsed().as_secs_f64()
    );
    for (name, codes) in [("lex", &c.lex), ("ctx", &c.ctx)] {
        for (i, a) in PROBE.iter().enumerate() {
            for b in &PROBE[i + 1..] {
                let d = hamming(codes, a, b).map_or("-".into(), |d| d.to_string());
                println!("DEC8_HAMMING {name} {a} {b} {d}");
            }
        }
    }
    for i in [
        Intent::Continues,
        Intent::NotContinues,
        Intent::Opening,
        Intent::Middle,
        Intent::End,
    ] {
        for (set, name) in [(seen(i), "seen"), (held_out(i), "new"), (fresh(i), "fresh")] {
            for q in set {
                println!(
                    "DEC8_QCODE {i:?} {name} lex={:016x} ctx={:016x} neg={} cues={} {q}",
                    c.lex_q.bits(&c.lex, q),
                    c.ctx_q.bits(&c.ctx, q),
                    u8::from(negated(&c.cues, q)),
                    cues_in(&c.cues, q).join("+")
                );
            }
        }
    }
    for (arm_q, arm_i, arm) in [
        ("SML_BOOL_POLARITY", "SML_INT_POLARITY", Arm::Polarity),
        (
            "SML_BOOL_POLARITY_CTX",
            "SML_INT_POLARITY_CTX",
            Arm::PolarityCtx,
        ),
        ("BLIND", "BLIND_INT", Arm::Blind),
    ] {
        let t = Instant::now();
        let s = build(&rows, arm, &c);
        println!(
            "DEC8_ROWS {arm_q} train={} dev={} test_seen={} test_new={} test_fresh={} feature_s={:.1}",
            s.train.len(),
            s.dev.len(),
            s.seen.len(),
            s.new.len(),
            s.fresh.len(),
            t.elapsed().as_secs_f64()
        );
        let (train, dev) = (examples(&s.train), examples(&s.dev));
        let t = Instant::now();
        let (q, int, grid) = select_on_dev_dim(Kind::Binary, DIM4, &train, &dev);
        let fit_s = t.elapsed().as_secs_f64();
        for (epochs, levels, div, c, nz) in grid {
            println!(
                "DEC8_GRID {arm_q} epochs={epochs} levels={levels} keep_div={div} dev={c}/{} nonzero={nz}",
                dev.len()
            );
        }
        println!(
            "DEC8_SELECT {arm_q} quantized epochs={} levels={} keep_div={} dev={}/{} nonzero={} | integer epochs={} dev={}/{} fit_s={fit_s:.1}",
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
            score(arm, "fresh", head, &s.fresh);
        }
    }
}
