//! DEC-14 runner: anchor-based routing (OEWN anchors + scope rule) vs the
//! DEC-12 router, both with the DEC-2 content heads, on a new 50-wording
//! reserved set.
//!
//! Arms: ROUTER14_BOOL (primary: DEC-12 features + anchor block, trained on
//! DEC-13 training + spent DEC-13 reserved), ROUTER14_INT, ANCHOR_RULE (anchor
//! rule, ROUTER14_BOOL on ties), ROUTER12_BOOL (DEC-12 unchanged),
//! ORACLE_BOOL and CONST. Sets: `seen`, `dec13res` (spent; reproduction of
//! ROUTER12_BOOL in DEC-13) and `reserved` (DEC-14, read once).
//!
//! Usage: `dec14_anchor_heads <input.tsv> <lexicon.tsv>`.

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use sml_decision::dec12_router::{self, class_of, intent_of, router_features, ROUTER_DIM};
use sml_decision::dec12_router::{ROUTER_EPOCHS, ROUTER_LEVELS};
use sml_decision::dec13_scope;
use sml_decision::dec14_anchor::{self, anchor_rule, router14_features, Anchors, ROUTER14_DIM};
use sml_decision::dec2_text::{
    pair_bits, predict, quantize, select_on_dev, text_bits, train_averaged_dim, Example, Head, Kind,
};
use sml_decision::dec4_instr::{answer, intents, seen, wording, Intent};
use sml_decision::dec5_meaning::{QuestionCoder, WordCodes};
use sml_decision::dec6_lexicon::lexicon_codes;
use sml_decision::dec7_polarity::negators;

const T1: [Intent; 2] = [Intent::Continues, Intent::NotContinues];
const T3: [Intent; 3] = [Intent::Opening, Intent::Middle, Intent::End];
type Wordings = fn(Intent) -> Vec<String>;
type Set = fn(Intent) -> &'static [&'static str];

fn refuse(msg: &str) -> ! {
    eprintln!("DEC14_REFUSE {msg}");
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

fn is_scope(q: &str) -> bool {
    [" rather than ", " instead of ", " as opposed to "]
        .iter()
        .any(|l| q.contains(l))
}

/// A trained question router: lexicon question coder plus one head per task.
struct Router<'a> {
    anchors: Option<&'a Anchors>,
    coder: QuestionCoder,
    heads: HashMap<&'static str, (Head, Head)>,
}

fn features(
    anchors: Option<&Anchors>,
    lex: &WordCodes,
    coder: &QuestionCoder,
    cues: &HashSet<String>,
    q: &str,
) -> Vec<u32> {
    match anchors {
        Some(a) => router14_features(lex, coder, cues, a, q),
        None => router_features(lex, coder, cues, q),
    }
}

impl<'a> Router<'a> {
    fn fit(
        name: &str,
        lex: &WordCodes,
        cues: &HashSet<String>,
        anchors: Option<&'a Anchors>,
        train: Wordings,
    ) -> Router<'a> {
        let dim = if anchors.is_some() {
            ROUTER14_DIM
        } else {
            ROUTER_DIM
        };
        let all: Vec<String> = T1.iter().chain(T3.iter()).flat_map(|&i| train(i)).collect();
        let coder = QuestionCoder::fit(lex, all.iter().map(String::as_str));
        let mut heads = HashMap::new();
        for (task, set) in [("t1", &T1[..]), ("t3", &T3[..])] {
            let kind = kind_of(task);
            let ex: Vec<Example> = set
                .iter()
                .flat_map(|&i| train(i).into_iter().map(move |w| (i, w)))
                .map(|(i, w)| Example {
                    x: vec![features(anchors, lex, &coder, cues, &w)],
                    y: class_of(i),
                })
                .collect();
            let (_, avg) = train_averaged_dim(kind, dim, &ex, &[ROUTER_EPOCHS])
                .pop()
                .unwrap_or_else(|| refuse("router training"));
            let q = quantize(kind, &avg, ROUTER_LEVELS, 1, &ex);
            println!(
                "DEC14_ROUTER {name} {task} wordings={} nonzero_bool={} nonzero_int={}",
                ex.len(),
                q.nonzero(),
                avg.nonzero()
            );
            heads.insert(task, (q, avg));
        }
        Router {
            anchors,
            coder,
            heads,
        }
    }

    fn route(
        &self,
        lex: &WordCodes,
        cues: &HashSet<String>,
        task: &str,
        q: &str,
        int: bool,
    ) -> Intent {
        let (hq, hi) = self.heads.get(task).unwrap_or_else(|| refuse("router"));
        let e = Example {
            x: vec![features(self.anchors, lex, &self.coder, cues, q)],
            y: 0,
        };
        intent_of(task, predict(kind_of(task), if int { hi } else { hq }, &e))
    }
}

struct TestRow<'a> {
    task: &'static str,
    id: &'a str,
    label: usize,
    content: Example,
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        refuse("usage: dec14_anchor_heads <input.tsv> <lexicon.tsv>");
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

    // 1. Routers, trained on question wordings only.
    let t = Instant::now();
    let anchors = Anchors::from_lexicon(&lex_text);
    println!("DEC14_ANCHOR_WORDS {}", anchors.len());
    let r12 = Router::fit("R12", &lex, &cues, None, dec12_router::training_wordings);
    let r14 = Router::fit(
        "R14",
        &lex,
        &cues,
        Some(&anchors),
        dec14_anchor::training_wordings,
    );
    println!("DEC14_ROUTER_FIT_S {:.1}", t.elapsed().as_secs_f64());
    let routers = [("R12", &r12), ("R14", &r14)];
    for (rname, r) in routers {
        for int in [false, true] {
            let name = format!("{rname}_{}", if int { "INT" } else { "BOOL" });
            for (setname, set) in [
                ("seen", seen as Set),
                ("dec13res", dec13_scope::reserved as Set),
                ("reserved", dec14_anchor::reserved as Set),
            ] {
                let (mut ok, mut n, mut sok, mut sn) = (0, 0, 0, 0);
                for (task, is) in [("t1", &T1[..]), ("t3", &T3[..])] {
                    for &i in is {
                        for w in set(i) {
                            let got = r.route(&lex, &cues, task, w, int);
                            n += 1;
                            ok += usize::from(got == i);
                            if is_scope(w) {
                                sn += 1;
                                sok += usize::from(got == i);
                            }
                            if setname == "reserved" {
                                println!("DEC14_ROUTE {name} {i:?} -> {got:?} {w}");
                            }
                        }
                    }
                }
                println!("DEC14_ROUTER_ACC {name} {setname} {ok}/{n} scope={sok}/{sn}");
            }
        }
    }

    // Anchor rule per wording (decided / correct), backoff R14_BOOL.
    for (setname, set) in [
        ("seen", seen as Set),
        ("dec13res", dec13_scope::reserved as Set),
        ("reserved", dec14_anchor::reserved as Set),
    ] {
        let (mut ok, mut n, mut dec, mut dok) = (0, 0, 0, 0);
        for (task, is) in [("t1", &T1[..]), ("t3", &T3[..])] {
            for &i in is {
                for w in set(i) {
                    let rule = anchor_rule(&anchors, &cues, task, w);
                    let got = rule.unwrap_or_else(|| r14.route(&lex, &cues, task, w, false));
                    n += 1;
                    ok += usize::from(got == i);
                    if let Some(r) = rule {
                        dec += 1;
                        dok += usize::from(r == i);
                    }
                    if setname == "reserved" {
                        println!(
                            "DEC14_ROUTE ANCHOR_RULE {i:?} -> {got:?} decided={} counts={:?} {w}",
                            rule.is_some(),
                            anchors.counts(w)
                        );
                    }
                }
            }
        }
        println!("DEC14_ROUTER_ACC ANCHOR_RULE {setname} {ok}/{n} decided={dok}/{dec}");
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
                "DEC14_CONTENT {name} {task} {c}/{} {:.4}",
                tr.len(),
                c as f64 / tr.len().max(1) as f64
            );
        }
        heads.push((task, q.head, int.head));
    }

    // 3. Asked TEST rows.
    // mode 0 = router, 1 = oracle, 2 = constant class 0, 3 = anchor rule.
    let arms: [(&str, Option<&Router>, bool, u8); 6] = [
        ("ROUTER14_BOOL", Some(&r14), false, 0),
        ("ROUTER14_INT", Some(&r14), true, 0),
        ("ANCHOR_RULE", Some(&r14), false, 3),
        ("ROUTER12_BOOL", Some(&r12), false, 0),
        ("ORACLE_BOOL", None, false, 1),
        ("CONST", None, false, 2),
    ];
    for (arm, router, int, mode) in arms {
        for (setname, set_of) in [
            ("seen", seen as Set),
            ("dec13res", dec13_scope::reserved as Set),
            ("reserved", dec14_anchor::reserved as Set),
        ] {
            let mut preds = Vec::new();
            let mut per: Vec<(&str, Intent, bool, bool)> = Vec::new();
            for r in &test {
                let (_, hq, hi) = heads
                    .iter()
                    .find(|h| h.0 == r.task)
                    .unwrap_or_else(|| refuse("head"));
                let content = predict(kind_of(r.task), if int { hi } else { hq }, &r.content);
                for intent in intents(r.task, r.id, r.label) {
                    let q = wording(set_of(intent), "test", r.id, intent);
                    let asked = match (mode, router) {
                        (0, Some(rt)) => rt.route(&lex, &cues, r.task, q, int),
                        (3, Some(rt)) => anchor_rule(&anchors, &cues, r.task, q)
                            .unwrap_or_else(|| rt.route(&lex, &cues, r.task, q, int)),
                        (1, _) => intent,
                        _ => intent_of(r.task, 0),
                    };
                    let p = answer(asked, content);
                    preds.push(p);
                    per.push((r.task, intent, p == answer(intent, r.label), is_scope(q)));
                }
            }
            let n = per.len();
            let c = per.iter().filter(|x| x.2).count();
            println!(
                "DEC14_SCORE {arm} {setname} {n} {c} {:.4}",
                c as f64 / n.max(1) as f64
            );
            println!("DEC14_PRED {arm} {setname} digest={:016x}", digest(&preds));
            let (sk, sok) = per
                .iter()
                .filter(|x| x.3)
                .fold((0, 0), |(k, o), x| (k + 1, o + usize::from(x.2)));
            println!(
                "DEC14_SCOPE {arm} {setname} rows={sk} acc={:.4}",
                sok as f64 / f64::from(sk.max(1) as u32)
            );
            for task in ["t1", "t3"] {
                let (k, ok) = per
                    .iter()
                    .filter(|x| x.0 == task)
                    .fold((0, 0), |(k, o), x| (k + 1, o + usize::from(x.2)));
                println!(
                    "DEC14_TASK {arm} {setname} {task} rows={k} acc={:.4}",
                    ok as f64 / f64::from(k.max(1) as u32)
                );
            }
            let mut done: HashSet<String> = HashSet::new();
            for &(_, i, _, _) in &per {
                if done.insert(format!("{i:?}")) {
                    let (k, ok) = per
                        .iter()
                        .filter(|x| x.1 == i)
                        .fold((0, 0), |(k, o), x| (k + 1, o + usize::from(x.2)));
                    println!(
                        "DEC14_INTENT {arm} {setname} {i:?} rows={k} acc={:.4}",
                        ok as f64 / f64::from(k.max(1) as u32)
                    );
                }
            }
        }
    }
}
