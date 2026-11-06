//! DEC-12 runner: question router + DEC-2 content heads.
//!
//! The router (`sml_decision::dec12_router`) maps each question to an intent;
//! the DEC-2 content heads answer the passage task; the yes/no answer follows
//! from both. Arms: ROUTER_BOOL (primary: quantized router and heads),
//! ROUTER_INT (integer router and heads), ORACLE_BOOL (true intent, quantized
//! heads: the ceiling) and CONST (router replaced by class 0: 0.5 by
//! construction). TEST is asked with the DEC-4 `seen` wordings and with the
//! DEC-12 `reserved` wordings.
//!
//! Usage: `dec12_router_heads <input.tsv> <lexicon.tsv>`; results go to
//! stdout as `DEC12_*` rows.

use std::collections::HashSet;
use std::time::Instant;

use sml_decision::dec12_router::{
    class_of, intent_of, reserved, router_features, training_wordings, ROUTER_DIM, ROUTER_EPOCHS,
    ROUTER_LEVELS,
};
use sml_decision::dec2_text::{
    pair_bits, predict, quantize, select_on_dev, text_bits, train_averaged_dim, Example, Head, Kind,
};
use sml_decision::dec4_instr::{answer, intents, seen, wording, Intent};
use sml_decision::dec5_meaning::QuestionCoder;
use sml_decision::dec6_lexicon::lexicon_codes;
use sml_decision::dec7_polarity::negators;

const T1: [Intent; 2] = [Intent::Continues, Intent::NotContinues];
const T3: [Intent; 3] = [Intent::Opening, Intent::Middle, Intent::End];

fn refuse(msg: &str) -> ! {
    eprintln!("DEC12_REFUSE {msg}");
    std::process::exit(2)
}

fn digest(preds: &[usize]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &p in preds {
        h ^= p as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

fn kind_of(task: &str) -> Kind {
    if task == "t1" {
        Kind::Binary
    } else {
        Kind::Ordinal(3)
    }
}

/// One TEST row: task, id, label and content features.
struct TestRow<'a> {
    task: &'static str,
    id: &'a str,
    label: usize,
    content: Example,
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        refuse("usage: dec12_router_heads <input.tsv> <lexicon.tsv>");
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

    // 1. Router, trained on question wordings only.
    let t = Instant::now();
    let all_train: Vec<String> = T1
        .iter()
        .chain(T3.iter())
        .flat_map(|&i| training_wordings(i))
        .collect();
    let coder = QuestionCoder::fit(&lex, all_train.iter().map(String::as_str));
    let feat = |q: &str| router_features(&lex, &coder, &cues, q);
    let mut routers: Vec<(&'static str, Head, Head)> = Vec::new();
    for (task, set) in [("t1", &T1[..]), ("t3", &T3[..])] {
        let kind = kind_of(task);
        let ex: Vec<Example> = set
            .iter()
            .flat_map(|&i| training_wordings(i).into_iter().map(move |w| (i, w)))
            .map(|(i, w)| Example {
                x: vec![feat(&w)],
                y: class_of(i),
            })
            .collect();
        let (_, avg) = train_averaged_dim(kind, ROUTER_DIM, &ex, &[ROUTER_EPOCHS])
            .pop()
            .unwrap_or_else(|| refuse("router training"));
        let q = quantize(kind, &avg, ROUTER_LEVELS, 1, &ex);
        println!(
            "DEC12_ROUTER {task} wordings={} nonzero_bool={} nonzero_int={}",
            ex.len(),
            q.nonzero(),
            avg.nonzero()
        );
        routers.push((task, q, avg));
    }
    let route = |task: &str, q: &str, int: bool| -> Intent {
        let (_, hq, hi) = routers
            .iter()
            .find(|r| r.0 == task)
            .unwrap_or_else(|| refuse("router"));
        let e = Example {
            x: vec![feat(q)],
            y: 0,
        };
        intent_of(task, predict(kind_of(task), if int { hi } else { hq }, &e))
    };
    println!("DEC12_ROUTER_FIT_S {:.1}", t.elapsed().as_secs_f64());
    // Router accuracy per wording (question only), per set.
    for (name, int) in [("ROUTER_BOOL", false), ("ROUTER_INT", true)] {
        for (setname, pick) in [("train", 0u8), ("seen", 1u8), ("reserved", 2u8)] {
            let mut ok = 0;
            let mut n = 0;
            for (task, set) in [("t1", &T1[..]), ("t3", &T3[..])] {
                for &i in set {
                    let ws: Vec<String> = match pick {
                        0 => training_wordings(i),
                        1 => seen(i).iter().map(|s| s.to_string()).collect(),
                        _ => reserved(i).iter().map(|s| s.to_string()).collect(),
                    };
                    for w in &ws {
                        let got = route(task, w, int);
                        n += 1;
                        ok += usize::from(got == i);
                        if pick == 2 {
                            println!("DEC12_ROUTE {name} {i:?} -> {got:?} {w}");
                        }
                    }
                }
            }
            println!("DEC12_ROUTER_ACC {name} {setname} {ok}/{n}");
        }
    }

    // 2. DEC-2 content heads (never see the question).
    let mut heads: Vec<(&'static str, Head, Head)> = Vec::new();
    let mut test: Vec<TestRow> = Vec::new();
    for task in ["t1", "t3"] {
        let kind = kind_of(task);
        let mut train = Vec::new();
        let mut dev = Vec::new();
        for r in rows.iter().filter(|r| r[1] == task) {
            let label: usize = r[3].parse().unwrap_or_else(|_| refuse("bad label"));
            let bits = match (task, &r[4..]) {
                ("t1", [a, b]) => pair_bits(&text_bits(a), &text_bits(b)),
                ("t3", [p]) => text_bits(p),
                _ => refuse("bad field count"),
            };
            let e = Example {
                x: vec![bits],
                y: label,
            };
            match r[0] {
                "train" => train.push(e),
                "dev" => dev.push(e),
                "test" => test.push(TestRow {
                    task,
                    id: r[2],
                    label,
                    content: e,
                }),
                _ => refuse("bad split"),
            }
        }
        let (q, int, _) = select_on_dev(kind, &train, &dev);
        let tr: Vec<&TestRow> = test.iter().filter(|r| r.task == task).collect();
        for (name, h) in [("BOOL", &q.head), ("INT", &int.head)] {
            let c = tr
                .iter()
                .filter(|r| predict(kind, h, &r.content) == r.label)
                .count();
            println!(
                "DEC12_CONTENT {name} {task} {c}/{} {:.4}",
                tr.len(),
                c as f64 / tr.len().max(1) as f64
            );
        }
        println!(
            "DEC12_CONTENT_SELECT {task} quantized epochs={} levels={} keep_div={} | integer epochs={}",
            q.epochs, q.levels, q.keep_div, int.epochs
        );
        heads.push((task, q.head, int.head));
    }

    // 3. Asked TEST rows.
    for (arm, int, mode) in [
        ("ROUTER_BOOL", false, 0u8),
        ("ROUTER_INT", true, 0u8),
        ("ORACLE_BOOL", false, 1u8),
        ("CONST", false, 2u8),
    ] {
        for (setname, set_of) in [
            ("seen", seen as fn(Intent) -> &'static [&'static str]),
            (
                "reserved",
                reserved as fn(Intent) -> &'static [&'static str],
            ),
        ] {
            let mut preds = Vec::new();
            let mut per: Vec<(&str, Intent, bool)> = Vec::new();
            for r in &test {
                let (_, hq, hi) = heads
                    .iter()
                    .find(|h| h.0 == r.task)
                    .unwrap_or_else(|| refuse("head"));
                let content = predict(kind_of(r.task), if int { hi } else { hq }, &r.content);
                for intent in intents(r.task, r.id, r.label) {
                    let q = wording(set_of(intent), "test", r.id, intent);
                    let asked = match mode {
                        0 => route(r.task, q, int),
                        1 => intent,
                        _ => intent_of(r.task, 0),
                    };
                    let p = answer(asked, content);
                    preds.push(p);
                    per.push((r.task, intent, p == answer(intent, r.label)));
                }
            }
            let n = per.len();
            let c = per.iter().filter(|x| x.2).count();
            println!(
                "DEC12_SCORE {arm} {setname} {n} {c} {:.4}",
                c as f64 / n.max(1) as f64
            );
            println!("DEC12_PRED {arm} {setname} digest={:016x}", digest(&preds));
            for task in ["t1", "t3"] {
                let (k, ok) = per
                    .iter()
                    .filter(|x| x.0 == task)
                    .fold((0, 0), |(k, o), x| (k + 1, o + usize::from(x.2)));
                println!(
                    "DEC12_TASK {arm} {setname} {task} rows={k} acc={:.4}",
                    ok as f64 / f64::from(k.max(1) as u32)
                );
            }
            let mut done: HashSet<String> = HashSet::new();
            for &(_, i, _) in &per {
                if done.insert(format!("{i:?}")) {
                    let (k, ok) = per
                        .iter()
                        .filter(|x| x.1 == i)
                        .fold((0, 0), |(k, o), x| (k + 1, o + usize::from(x.2)));
                    println!(
                        "DEC12_INTENT {arm} {setname} {i:?} rows={k} acc={:.4}",
                        ok as f64 / f64::from(k.max(1) as u32)
                    );
                }
            }
        }
    }
}
