//! L3: exact concept-equivalence and explicit opposition over sense IDs.
//! Equivalence is a supplied restricted relation, not unrestricted synonymy.
//! Unknown means not derivable from TRAIN, never a proof of unrelated meanings.
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fs::File;
use std::io::{Read, Write};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
type Pair = (String, String);
const MARKER: &str = "SML_TUTOR_SENSE_GRAPH_V1";
const MAX_ROWS: usize = 4096;
const MAX_BYTES: u64 = 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Relation {
    Equivalent,
    Opposite,
    Unknown,
}

impl Relation {
    fn parse(s: &str) -> Result<Self> {
        match s {
            "eq" => Ok(Self::Equivalent),
            "opp" => Ok(Self::Opposite),
            "unknown" => Ok(Self::Unknown),
            _ => Err("invalid relation".into()),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Equivalent => "eq",
            Self::Opposite => "opp",
            Self::Unknown => "unknown",
        }
    }
}

type Edges = BTreeMap<Pair, Relation>;

fn valid_id(id: &str) -> bool {
    let Some((lang, number)) = id.split_once(':') else {
        return false;
    };
    matches!(lang, "fr" | "en")
        && number
            .parse::<u32>()
            .is_ok_and(|n| n > 0 && n.to_string() == number)
}

fn pair(a: &str, b: &str) -> Result<Pair> {
    if !valid_id(a) || !valid_id(b) || a == b {
        return Err("invalid or reflexive sense pair".into());
    }
    Ok(if a < b {
        (a.into(), b.into())
    } else {
        (b.into(), a.into())
    })
}

fn read(path: &str) -> Result<String> {
    let mut raw = String::new();
    File::open(path)?
        .take(MAX_BYTES + 1)
        .read_to_string(&mut raw)?;
    if raw.len() as u64 > MAX_BYTES {
        return Err("oversized input".into());
    }
    Ok(raw)
}

fn rows(raw: &str, test: bool) -> Result<Edges> {
    let mut out = BTreeMap::new();
    for row in raw.lines() {
        let fields: Vec<_> = row.split('|').collect();
        let [a, b, label] = fields.as_slice() else {
            return Err("invalid edge row".into());
        };
        let r = Relation::parse(label)?;
        if (!test && r == Relation::Unknown) || out.insert(pair(a, b)?, r).is_some() {
            return Err("unknown TRAIN relation or duplicate pair".into());
        }
        if out.len() > MAX_ROWS {
            return Err("too many rows".into());
        }
    }
    if out.is_empty() {
        return Err("empty corpus".into());
    }
    Ok(out)
}

fn representative<'a>(parent: &'a BTreeMap<String, String>, id: &'a str) -> &'a str {
    let mut current = id;
    while let Some(next) = parent.get(current) {
        if next == current {
            break;
        }
        current = next;
    }
    current
}

struct Graph {
    parent: BTreeMap<String, String>,
    opposite: BTreeSet<Pair>,
    taught: Edges,
}

impl Graph {
    fn learn(taught: Edges) -> Result<Self> {
        let mut parent = BTreeMap::new();
        for (a, b) in taught.keys() {
            parent.insert(a.clone(), a.clone());
            parent.insert(b.clone(), b.clone());
        }
        // Canonical decreasing roots make cycles impossible.
        for ((a, b), relation) in &taught {
            if *relation == Relation::Equivalent {
                let x = representative(&parent, a).to_owned();
                let y = representative(&parent, b).to_owned();
                if x != y {
                    let (lo, hi) = if x < y { (x, y) } else { (y, x) };
                    parent.insert(hi, lo);
                }
            }
        }
        let mut opposite = BTreeSet::new();
        for ((a, b), relation) in &taught {
            if *relation == Relation::Opposite {
                let x = representative(&parent, a);
                let y = representative(&parent, b);
                if x == y {
                    return Err("opposition inside equivalence class".into());
                }
                opposite.insert(pair(x, y)?);
            }
        }
        Ok(Self {
            parent,
            opposite,
            taught,
        })
    }

    fn predict(&self, a: &str, b: &str) -> Relation {
        if !self.parent.contains_key(a) || !self.parent.contains_key(b) {
            return Relation::Unknown;
        }
        let x = representative(&self.parent, a);
        let y = representative(&self.parent, b);
        if x == y {
            Relation::Equivalent
        } else if pair(x, y).is_ok_and(|p| self.opposite.contains(&p)) {
            Relation::Opposite
        } else {
            Relation::Unknown
        }
    }

    fn checkpoint(&self) -> String {
        let mut raw = format!("{MARKER}\n");
        for ((a, b), r) in &self.taught {
            raw.push_str(&format!("{a}|{b}|{}\n", r.label()));
        }
        raw
    }

    fn restore(raw: &str) -> Result<Self> {
        let body = raw
            .strip_prefix(&format!("{MARKER}\n"))
            .ok_or("invalid graph marker")?;
        let graph = Self::learn(rows(body, false)?)?;
        if graph.checkpoint() != raw {
            return Err("noncanonical graph".into());
        }
        Ok(graph)
    }

    fn verify(&self, test: &Edges) -> Result<()> {
        let mut totals = [0; 3];
        let mut correct = [0; 3];
        for ((a, b), expected) in test {
            if self.taught.contains_key(&(a.clone(), b.clone())) {
                return Err("TRAIN/TEST pair leakage".into());
            }
            let i = match expected {
                Relation::Equivalent => 0,
                Relation::Opposite => 1,
                Relation::Unknown => 2,
            };
            totals[i] += 1;
            correct[i] += usize::from(self.predict(a, b) == *expected);
        }
        if totals.contains(&0) || totals != correct {
            return Err("incomplete or imperfect semantic cohort".into());
        }
        println!(
            "SML_TUTOR_SENSE_GRAPH=PASS eq={}/{} opp={}/{} unknown={}/{}",
            correct[0], totals[0], correct[1], totals[1], correct[2], totals[2]
        );
        Ok(())
    }
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [mode, train, output] if mode == "--learn" => {
            let graph = Graph::learn(rows(&read(train)?, false)?)?;
            let raw = graph.checkpoint();
            let reloaded = Graph::restore(&raw)?;
            if reloaded.taught != graph.taught {
                return Err("semantic retention mismatch".into());
            }
            let mut file = File::options().write(true).create_new(true).open(output)?;
            file.write_all(raw.as_bytes())?;
            file.sync_all()?;
        }
        [mode, bank, test] if mode == "--verify" => {
            Graph::restore(&read(bank)?)?.verify(&rows(&read(test)?, true)?)?;
        }
        [mode, bank, a, b] if mode == "--predict" => {
            pair(a, b)?;
            println!("{}", Graph::restore(&read(bank)?)?.predict(a, b).label());
        }
        _ => {
            return Err(
                "usage: --learn TRAIN BANK | --verify BANK TEST | --predict BANK A B".into(),
            )
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    const TRAIN: &str = include_str!("../fixtures/tutor-l3/train.txt");
    const TEST: &str = include_str!("../fixtures/tutor-l3/test.txt");

    #[test]
    fn held_out_bilingual_relation_closure_and_reload() {
        let graph = Graph::learn(rows(TRAIN, false).unwrap()).unwrap();
        let test = rows(TEST, true).unwrap();
        assert_eq!(graph.taught.len(), 28);
        assert_eq!(test.len(), 468);
        graph.verify(&test).unwrap();
        Graph::restore(&graph.checkpoint())
            .unwrap()
            .verify(&test)
            .unwrap();
        assert_eq!(graph.predict("fr:1", "fr:999"), Relation::Unknown);
    }

    #[test]
    fn reject_leakage_contradiction_and_false_antonym_transitivity() {
        let bad = "fr:1|en:1|eq\nen:1|fr:2|eq\nfr:1|fr:2|opp\n";
        assert!(Graph::learn(rows(bad, false).unwrap()).is_err());
        let chain = "fr:1|en:1|opp\nen:1|fr:2|opp\n";
        let graph = Graph::learn(rows(chain, false).unwrap()).unwrap();
        assert_eq!(graph.predict("fr:1", "fr:2"), Relation::Unknown);
        assert!(graph.verify(&graph.taught).is_err());
        assert!(rows("fr:1|en:1|eq\nen:1|fr:1|eq\n", false).is_err());
    }
}
