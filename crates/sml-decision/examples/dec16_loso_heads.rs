//! DEC-16 runner: leave-one-set-out over the spent reserved sets S12–S15.
//!
//! For each fold k and each router family (R12: DEC-12 features; R14: + DEC-14
//! anchors; R15: + DEC-15 anchors with antonym polarity, replicated block), a
//! router is trained on `dec16_loso::fold_training(_, k)` and TEST is asked
//! with the wordings of set k, with the DEC-2 content heads (BOOL and INT).
//! ORACLE and CONST are reported per fold.
//!
//! Usage: `dec16_loso_heads <input.tsv> <lexicon.tsv>`; rows `DEC16_*`.

use std::collections::{HashMap, HashSet};

use sml_decision::dec12_router::{
    class_of, intent_of, router_features, ROUTER_DIM, ROUTER_EPOCHS, ROUTER_LEVELS,
};
use sml_decision::dec14_anchor::{self, router14_features, ROUTER14_DIM};
use sml_decision::dec15_polarity::{self, router15_features, ROUTER15_DIM};
use sml_decision::dec16_loso::{fold_training, FOLDS};
use sml_decision::dec2_text::{
    pair_bits, predict, quantize, select_on_dev, text_bits, train_averaged_dim, Example, Head, Kind,
};
use sml_decision::dec4_instr::{answer, intents, wording, Intent};
use sml_decision::dec5_meaning::{QuestionCoder, WordCodes};
use sml_decision::dec6_lexicon::lexicon_codes;
use sml_decision::dec7_polarity::negators;

const T1: [Intent; 2] = [Intent::Continues, Intent::NotContinues];
const T3: [Intent; 3] = [Intent::Opening, Intent::Middle, Intent::End];

fn refuse(msg: &str) -> ! {
    eprintln!("DEC16_REFUSE {msg}");
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

#[derive(Clone, Copy)]
enum Fam<'a> {
    R12,
    R14(&'a dec14_anchor::Anchors),
    R15(&'a dec15_polarity::Anchors),
}

impl Fam<'_> {
    fn name(&self) -> &'static str {
        match self {
            Fam::R12 => "R12",
            Fam::R14(_) => "R14",
            Fam::R15(_) => "R15",
        }
    }
    fn dim(&self) -> usize {
        match self {
            Fam::R12 => ROUTER_DIM,
            Fam::R14(_) => ROUTER14_DIM,
            Fam::R15(_) => ROUTER15_DIM,
        }
    }
    fn feats(
        &self,
        lex: &WordCodes,
        c: &QuestionCoder,
        cues: &HashSet<String>,
        q: &str,
    ) -> Vec<u32> {
        match self {
            Fam::R12 => router_features(lex, c, cues, q),
            Fam::R14(a) => router14_features(lex, c, cues, a, q),
            Fam::R15(a) => router15_features(lex, c, cues, a, q),
        }
    }
}

struct Router<'a> {
    fam: Fam<'a>,
    coder: QuestionCoder,
    heads: HashMap<&'static str, (Head, Head)>,
}

impl<'a> Router<'a> {
    fn fit(fam: Fam<'a>, lex: &WordCodes, cues: &HashSet<String>, k: usize) -> Router<'a> {
        let all: Vec<String> = T1
            .iter()
            .chain(T3.iter())
            .flat_map(|&i| fold_training(i, k))
            .collect();
        let coder = QuestionCoder::fit(lex, all.iter().map(String::as_str));
        let mut heads = HashMap::new();
        for (task, set) in [("t1", &T1[..]), ("t3", &T3[..])] {
            let kind = kind_of(task);
            let ex: Vec<Example> = set
                .iter()
                .flat_map(|&i| fold_training(i, k).into_iter().map(move |w| (i, w)))
                .map(|(i, w)| Example {
                    x: vec![fam.feats(lex, &coder, cues, &w)],
                    y: class_of(i),
                })
                .collect();
            let (_, avg) = train_averaged_dim(kind, fam.dim(), &ex, &[ROUTER_EPOCHS])
                .pop()
                .unwrap_or_else(|| refuse("router training"));
            let q = quantize(kind, &avg, ROUTER_LEVELS, 1, &ex);
            println!(
                "DEC16_ROUTER {} {} {task} wordings={} nonzero_bool={}",
                fam.name(),
                FOLDS[k].0,
                ex.len(),
                q.nonzero()
            );
            heads.insert(task, (q, avg));
        }
        Router { fam, coder, heads }
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
            x: vec![self.fam.feats(lex, &self.coder, cues, q)],
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
        refuse("usage: dec16_loso_heads <input.tsv> <lexicon.tsv>");
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
    let a14 = dec14_anchor::Anchors::from_lexicon(&lex_text);
    let a15 = dec15_polarity::Anchors::from_lexicon(&lex_text);

    // DEC-2 content heads.
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
            println!("DEC16_CONTENT {name} {task} {c}/{}", tr.len());
        }
        heads.push((task, q.head, int.head));
    }
    let content = |r: &TestRow, int: bool| -> usize {
        let (_, hq, hi) = heads
            .iter()
            .find(|h| h.0 == r.task)
            .unwrap_or_else(|| refuse("head"));
        predict(kind_of(r.task), if int { hi } else { hq }, &r.content)
    };

    for (k, (fold, set)) in FOLDS.iter().enumerate() {
        // Reference arms.
        for (arm, mode) in [("ORACLE", 1u8), ("CONST", 2u8)] {
            let (mut n, mut c) = (0, 0);
            for r in &test {
                let p0 = content(r, false);
                for intent in intents(r.task, r.id, r.label) {
                    let asked = if mode == 1 {
                        intent
                    } else {
                        intent_of(r.task, 0)
                    };
                    n += 1;
                    c += usize::from(answer(asked, p0) == answer(intent, r.label));
                }
            }
            println!("DEC16_FOLD_SCORE {arm} BOOL {fold} {n} {c}");
        }
        for fam in [Fam::R12, Fam::R14(&a14), Fam::R15(&a15)] {
            let router = Router::fit(fam, &lex, &cues, k);
            for (variant, int) in [("BOOL", false), ("INT", true)] {
                let (mut ok, mut nw) = (0, 0);
                for (task, is) in [("t1", &T1[..]), ("t3", &T3[..])] {
                    for &i in is {
                        for w in set(i) {
                            let got = router.route(&lex, &cues, task, w, int);
                            nw += 1;
                            ok += usize::from(got == i);
                            if !int {
                                println!("DEC16_ROUTE {} {fold} {i:?} -> {got:?} {w}", fam.name());
                            }
                        }
                    }
                }
                println!("DEC16_FOLD_ROUTE {} {variant} {fold} {ok}/{nw}", fam.name());
                let (mut n, mut c) = (0, 0);
                let mut preds = Vec::new();
                for r in &test {
                    let p0 = content(r, int);
                    for intent in intents(r.task, r.id, r.label) {
                        let q = wording(set(intent), "test", r.id, intent);
                        let p = answer(router.route(&lex, &cues, r.task, q, int), p0);
                        preds.push(p);
                        n += 1;
                        c += usize::from(p == answer(intent, r.label));
                    }
                }
                println!("DEC16_FOLD_SCORE {} {variant} {fold} {n} {c}", fam.name());
                println!(
                    "DEC16_PRED {} {variant} {fold} digest={:016x}",
                    fam.name(),
                    digest(&preds)
                );
            }
        }
    }
}
