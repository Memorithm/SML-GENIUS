//! DEC-10 runner: question-reading heads trained on the richer DEC-10 grammar.
//!
//! Arms: SML_BOOL_GRAMMAR10 (primary: DEC-7 features, TRAIN and DEV asked
//! with `dec10_grammar::grammar`), SML_BOOL_GRAMMAR9 (DEC-9 primary exactly),
//! their integer heads, and BLIND. TEST is asked with the DEC-4 `seen`
//! wordings and with `dec10_grammar::reserved`.
//!
//! Usage: `dec10_grammar_heads <input.tsv> <lexicon.tsv>`; results go to
//! stdout as `DEC10_*` rows.

use std::collections::HashSet;
use std::time::Instant;

use sml_decision::dec10_grammar::{self, reserved};
use sml_decision::dec2_text::{predict, select_on_dev_dim, Example, Head, Kind};
use sml_decision::dec4_instr::{answer, content_bits, intents, seen, wording, Intent, DIM4};
use sml_decision::dec5_meaning::{QuestionCoder, WordCodes};
use sml_decision::dec6_lexicon::{features, lexicon_codes};
use sml_decision::dec7_polarity::{cues_in, negated, negators};
use sml_decision::dec9_grammar;

const ALL: [Intent; 5] = [
    Intent::Continues,
    Intent::NotContinues,
    Intent::Opening,
    Intent::Middle,
    Intent::End,
];

/// Training wordings of one arm.
#[derive(Clone, Copy, PartialEq)]
enum Arm {
    Grammar10,
    Grammar9,
    Blind,
}

fn refuse(msg: &str) -> ! {
    eprintln!("DEC10_REFUSE {msg}");
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
    reserved: Vec<Row>,
}

fn digest(preds: &[usize]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &p in preds {
        h ^= p as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// Grammar wordings as static slices (leaked once; the runner is short-lived).
fn grammar_sets(grammar: fn(Intent) -> Vec<String>) -> Vec<&'static [&'static str]> {
    ALL.iter()
        .map(|&i| {
            let v: Vec<&'static str> = grammar(i)
                .into_iter()
                .map(|s| &*Box::leak(s.into_boxed_str()))
                .collect();
            &*Box::leak(v.into_boxed_slice())
        })
        .collect()
}

fn pos(i: Intent) -> usize {
    ALL.iter().position(|&j| j == i).unwrap_or(0)
}

fn build(
    rows: &[Vec<&str>],
    arm: Arm,
    lex: &WordCodes,
    q: &QuestionCoder,
    cues: &HashSet<String>,
    gram: &[&'static [&'static str]],
) -> Sets {
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
                let text = wording(set, r[0], r[2], intent);
                let x = match arm {
                    Arm::Blind => features(task, &content, &[]),
                    _ => features(
                        task,
                        &content,
                        &[
                            (b"lex", q.bits(lex, text)),
                            (b"neg", u64::from(negated(cues, text))),
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
            let fit_set = gram[pos(intent)];
            match r[0] {
                "train" => s.train.push(ask(fit_set)),
                "dev" => s.dev.push(ask(fit_set)),
                "test" => {
                    s.seen.push(ask(seen(intent)));
                    s.reserved.push(ask(reserved(intent)));
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
        "DEC10_SCORE {arm} {variant} {n} {c} {:.4}",
        c as f64 / n.max(1) as f64
    );
    println!("DEC10_PRED {arm} {variant} digest={:016x}", digest(&preds));
    for task in ["t1", "t3"] {
        let idx: Vec<usize> = (0..n).filter(|&i| rows[i].task == task).collect();
        let c = idx.iter().filter(|&&i| ok(i)).count();
        // Both questions of one example answered right.
        let both = idx
            .windows(2)
            .filter(|w| rows[w[0]].pair == rows[w[1]].pair && ok(w[0]) && ok(w[1]))
            .count();
        println!(
            "DEC10_TASK {arm} {variant} {task} rows={} acc={:.4} both_right={:.4}",
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
            "DEC10_INTENT {arm} {variant} {i:?} rows={} acc={:.4}",
            idx.len(),
            c as f64 / idx.len().max(1) as f64
        );
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        refuse("usage: dec9_grammar_heads <input.tsv> <lexicon.tsv>");
    }
    let text = std::fs::read_to_string(&args[1]).unwrap_or_else(|_| refuse("cannot read input"));
    let rows: Vec<Vec<&str>> = text.lines().map(|l| l.split('\t').collect()).collect();
    if rows.iter().any(|r| r.len() < 5) {
        refuse("short row");
    }
    let lex_text =
        std::fs::read_to_string(&args[2]).unwrap_or_else(|_| refuse("cannot read lexicon"));
    let lex = lexicon_codes(&lex_text).unwrap_or_else(|e| refuse(&e));
    let cues = negators(&lex_text);
    let gram = grammar_sets(dec10_grammar::grammar);
    let gram9 = grammar_sets(dec9_grammar::grammar);
    // Question coders, each fitted on its own arm's TRAIN questions.
    let mut base_q = Vec::new();
    let mut gram_q = Vec::new();
    for r in rows.iter().filter(|r| r[0] == "train" && r[1] != "t2") {
        let label: usize = r[3].parse().unwrap_or_else(|_| refuse("bad label"));
        for i in intents(r[1], r[2], label) {
            base_q.push(wording(gram9[pos(i)], r[0], r[2], i));
            gram_q.push(wording(gram[pos(i)], r[0], r[2], i));
        }
    }
    let base_c = QuestionCoder::fit(&lex, base_q.iter().copied());
    let gram_c = QuestionCoder::fit(&lex, gram_q.iter().copied());
    let used: HashSet<&str> = gram_q.iter().copied().collect();
    println!(
        "DEC10_CODES lex_vocab={} lex_fingerprint={:016x} cues={} train_questions={}",
        lex.len(),
        lex.fingerprint(),
        cues.len(),
        base_q.len()
    );
    for i in ALL {
        println!(
            "DEC10_GRAMMAR {i:?} wordings={} used_in_train={}",
            gram[pos(i)].len(),
            gram[pos(i)].iter().filter(|w| used.contains(*w)).count()
        );
        for q in reserved(i) {
            println!(
                "DEC10_QCODE {i:?} reserved neg={} cues={} {q}",
                u8::from(negated(&cues, q)),
                cues_in(&cues, q).join("+")
            );
        }
    }
    for (arm_q, arm_i, arm, coder, sets) in [
        (
            "SML_BOOL_GRAMMAR10",
            "SML_INT_GRAMMAR10",
            Arm::Grammar10,
            &gram_c,
            &gram,
        ),
        (
            "SML_BOOL_GRAMMAR9",
            "SML_INT_GRAMMAR9",
            Arm::Grammar9,
            &base_c,
            &gram9,
        ),
        ("BLIND", "BLIND_INT", Arm::Blind, &base_c, &gram9),
    ] {
        let t = Instant::now();
        let s = build(&rows, arm, &lex, coder, &cues, sets);
        println!(
            "DEC10_ROWS {arm_q} train={} dev={} test_seen={} test_reserved={} feature_s={:.1}",
            s.train.len(),
            s.dev.len(),
            s.seen.len(),
            s.reserved.len(),
            t.elapsed().as_secs_f64()
        );
        let (train, dev) = (examples(&s.train), examples(&s.dev));
        let t = Instant::now();
        let (q, int, grid) = select_on_dev_dim(Kind::Binary, DIM4, &train, &dev);
        let fit_s = t.elapsed().as_secs_f64();
        for (epochs, levels, div, c, nz) in grid {
            println!(
                "DEC10_GRID {arm_q} epochs={epochs} levels={levels} keep_div={div} dev={c}/{} nonzero={nz}",
                dev.len()
            );
        }
        println!(
            "DEC10_SELECT {arm_q} quantized epochs={} levels={} keep_div={} dev={}/{} nonzero={} | integer epochs={} dev={}/{} fit_s={fit_s:.1}",
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
            score(arm, "reserved", head, &s.reserved);
        }
    }
}
