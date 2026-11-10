//! DEC-13 runner: DEC-12 router vs DEC-13 router (scope grammar added), both
//! with the DEC-2 content heads, on a 50-wording reserved set.
//!
//! Arms: ROUTER13_BOOL (primary), ROUTER13_INT, ROUTER12_BOOL (DEC-12 router
//! unchanged), ORACLE_BOOL (true intent: the ceiling) and CONST (class 0:
//! 0.5 by construction). Wording sets: `seen` (DEC-4), `dec12res` (the spent
//! DEC-12 reserved set: reproduction of DEC-12 for ROUTER12_BOOL only; the
//! DEC-13 router trains on it) and `reserved` (DEC-13, read once).
//!
//! Usage: `dec13_scope_heads <input.tsv> <lexicon.tsv>`; results go to stdout
//! as `DEC13_*` rows.

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use sml_decision::dec12_router::{self, class_of, intent_of, router_features, ROUTER_DIM};
use sml_decision::dec12_router::{ROUTER_EPOCHS, ROUTER_LEVELS};
use sml_decision::dec13_scope;
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
    eprintln!("DEC13_REFUSE {msg}");
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
struct Router {
    coder: QuestionCoder,
    heads: HashMap<&'static str, (Head, Head)>,
}

impl Router {
    fn fit(name: &str, lex: &WordCodes, cues: &HashSet<String>, train: Wordings) -> Router {
        let all: Vec<String> = T1.iter().chain(T3.iter()).flat_map(|&i| train(i)).collect();
        let coder = QuestionCoder::fit(lex, all.iter().map(String::as_str));
        let mut heads = HashMap::new();
        for (task, set) in [("t1", &T1[..]), ("t3", &T3[..])] {
            let kind = kind_of(task);
            let ex: Vec<Example> = set
                .iter()
                .flat_map(|&i| train(i).into_iter().map(move |w| (i, w)))
                .map(|(i, w)| Example {
                    x: vec![router_features(lex, &coder, cues, &w)],
                    y: class_of(i),
                })
                .collect();
            let (_, avg) = train_averaged_dim(kind, ROUTER_DIM, &ex, &[ROUTER_EPOCHS])
                .pop()
                .unwrap_or_else(|| refuse("router training"));
            let q = quantize(kind, &avg, ROUTER_LEVELS, 1, &ex);
            println!(
                "DEC13_ROUTER {name} {task} wordings={} nonzero_bool={} nonzero_int={}",
                ex.len(),
                q.nonzero(),
                avg.nonzero()
            );
            heads.insert(task, (q, avg));
        }
        Router { coder, heads }
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
            x: vec![router_features(lex, &self.coder, cues, q)],
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
        refuse("usage: dec13_scope_heads <input.tsv> <lexicon.tsv>");
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
    let r12 = Router::fit("R12", &lex, &cues, dec12_router::training_wordings);
    let r13 = Router::fit("R13", &lex, &cues, dec13_scope::training_wordings);
    println!("DEC13_ROUTER_FIT_S {:.1}", t.elapsed().as_secs_f64());
    let routers = [("R12", &r12), ("R13", &r13)];
    for (rname, r) in routers {
        for int in [false, true] {
            let name = format!("{rname}_{}", if int { "INT" } else { "BOOL" });
            for (setname, set) in [
                ("seen", seen as Set),
                ("dec12res", dec12_router::reserved as Set),
                ("reserved", dec13_scope::reserved as Set),
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
                                println!("DEC13_ROUTE {name} {i:?} -> {got:?} {w}");
                            }
                        }
                    }
                }
                println!("DEC13_ROUTER_ACC {name} {setname} {ok}/{n} scope={sok}/{sn}");
            }
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
                "DEC13_CONTENT {name} {task} {c}/{} {:.4}",
                tr.len(),
                c as f64 / tr.len().max(1) as f64
            );
        }
        heads.push((task, q.head, int.head));
    }

    // 3. Asked TEST rows.
    // mode 0 = router, 1 = oracle, 2 = constant class 0.
    let arms: [(&str, Option<&Router>, bool, u8); 5] = [
        ("ROUTER13_BOOL", Some(&r13), false, 0),
        ("ROUTER13_INT", Some(&r13), true, 0),
        ("ROUTER12_BOOL", Some(&r12), false, 0),
        ("ORACLE_BOOL", None, false, 1),
        ("CONST", None, false, 2),
    ];
    for (arm, router, int, mode) in arms {
        for (setname, set_of) in [
            ("seen", seen as Set),
            ("dec12res", dec12_router::reserved as Set),
            ("reserved", dec13_scope::reserved as Set),
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
                "DEC13_SCORE {arm} {setname} {n} {c} {:.4}",
                c as f64 / n.max(1) as f64
            );
            println!("DEC13_PRED {arm} {setname} digest={:016x}", digest(&preds));
            let (sk, sok) = per
                .iter()
                .filter(|x| x.3)
                .fold((0, 0), |(k, o), x| (k + 1, o + usize::from(x.2)));
            println!(
                "DEC13_SCOPE {arm} {setname} rows={sk} acc={:.4}",
                sok as f64 / f64::from(sk.max(1) as u32)
            );
            for task in ["t1", "t3"] {
                let (k, ok) = per
                    .iter()
                    .filter(|x| x.0 == task)
                    .fold((0, 0), |(k, o), x| (k + 1, o + usize::from(x.2)));
                println!(
                    "DEC13_TASK {arm} {setname} {task} rows={k} acc={:.4}",
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
                        "DEC13_INTENT {arm} {setname} {i:?} rows={k} acc={:.4}",
                        ok as f64 / f64::from(k.max(1) as u32)
                    );
                }
            }
        }
    }
}
