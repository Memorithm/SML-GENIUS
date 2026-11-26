//! C2 precursor: supervised contextual sense selection in French and English.
//! Sense identifiers are local to the supplied lexeme; unknown is not a negative fact.
//! Controlled fixtures do not establish open-domain understanding.
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fs::File;
use std::io::{Read, Write};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
type Lexeme = (String, String, String);
type RowKey = (Lexeme, String);
type Rows = BTreeMap<RowKey, Option<u32>>;
type Cues = BTreeMap<String, BTreeSet<u32>>;
const MARKER: &str = "SML_TUTOR_CONTEXT_SENSE_V1";
const LIMIT: u64 = 1024 * 1024;
const MAX_ROWS: usize = 4096;

fn read(path: &str) -> Result<String> {
    let mut raw = String::new();
    File::open(path)?.take(LIMIT + 1).read_to_string(&mut raw)?;
    if raw.len() as u64 > LIMIT {
        return Err("oversized corpus".into());
    }
    Ok(raw)
}

fn normalize(raw: &str) -> Option<String> {
    if raw.is_empty() || raw.len() > 512 || raw.chars().any(char::is_control) {
        return None;
    }
    let lower = raw.to_lowercase();
    let tokens: Vec<_> = lower
        .split(|c: char| !c.is_alphabetic())
        .filter(|s| !s.is_empty())
        .collect();
    if tokens.is_empty() || tokens.len() > 64 {
        return None;
    }
    Some(tokens.join(" "))
}

fn lexeme(lang: &str, lemma: &str, pos: &str) -> Result<Lexeme> {
    if !matches!(lang, "fr" | "en")
        || normalize(lemma).as_deref() != Some(lemma)
        || lemma.contains(' ')
        || !matches!(pos, "noun" | "verb" | "adj" | "adv")
    {
        return Err("invalid lexeme".into());
    }
    Ok((lang.to_owned(), lemma.to_owned(), pos.to_owned()))
}

fn context(raw: &str, key: &Lexeme) -> Result<String> {
    let normalized = normalize(raw).ok_or("invalid context")?;
    if normalized.split(' ').filter(|s| *s == key.1).count() != 1 {
        return Err("context must contain one target surface".into());
    }
    Ok(normalized)
}

fn rows(raw: &str, test: bool) -> Result<Rows> {
    let mut out = BTreeMap::new();
    for line in raw.lines() {
        let fields: Vec<_> = line.split('|').collect();
        let [lang, lemma, pos, sentence, target] = fields.as_slice() else {
            return Err("invalid context lesson".into());
        };
        let key = lexeme(lang, lemma, pos)?;
        let sentence = context(sentence, &key)?;
        let sense = if test && *target == "unknown" {
            None
        } else {
            let n: u32 = target.parse()?;
            if n == 0 || n.to_string() != *target {
                return Err("invalid local sense ID".into());
            }
            Some(n)
        };
        if out.insert((key, sentence), sense).is_some() || out.len() > MAX_ROWS {
            return Err("duplicate or oversized corpus".into());
        }
    }
    if out.is_empty() {
        return Err("empty corpus".into());
    }
    Ok(out)
}

struct Bank {
    taught: Rows,
    cues: BTreeMap<Lexeme, Cues>,
}

impl Bank {
    fn learn(taught: Rows) -> Result<Self> {
        let mut cues: BTreeMap<Lexeme, Cues> = BTreeMap::new();
        for ((key, sentence), sense) in &taught {
            let sense = sense.ok_or("unknown TRAIN label")?;
            let entry = cues.entry(key.clone()).or_default();
            for word in sentence.split(' ').filter(|s| *s != key.1) {
                entry.entry(word.to_owned()).or_default().insert(sense);
            }
        }
        Ok(Self { taught, cues })
    }

    fn predict(&self, key: &Lexeme, sentence: &str) -> Option<u32> {
        let sentence = context(sentence, key).ok()?;
        let cues = self.cues.get(key)?;
        let mut support: Option<BTreeSet<u32>> = None;
        for word in sentence.split(' ').filter(|s| *s != key.1) {
            if let Some(ids) = cues.get(word) {
                support = Some(match support {
                    None => ids.clone(),
                    Some(previous) => previous.intersection(ids).copied().collect(),
                });
            }
        }
        let support = support?;
        if support.len() == 1 {
            support.iter().next().copied()
        } else {
            None
        }
    }

    fn checkpoint(&self) -> String {
        let mut out = format!("{MARKER}\n");
        for ((key, sentence), sense) in &self.taught {
            if let Some(sense) = sense {
                out.push_str(&format!(
                    "{}|{}|{}|{sentence}|{sense}\n",
                    key.0, key.1, key.2
                ));
            }
        }
        out
    }

    fn restore(raw: &str) -> Result<Self> {
        let body = raw
            .strip_prefix(&format!("{MARKER}\n"))
            .ok_or("wrong context schema")?;
        let bank = Self::learn(rows(body, false)?)?;
        if bank.checkpoint() != raw {
            return Err("noncanonical checkpoint".into());
        }
        Ok(bank)
    }

    fn verify(&self, test: &Rows) -> Result<[[usize; 2]; 2]> {
        let mut totals = [[0; 2]; 2];
        let mut correct = [[0; 2]; 2];
        for ((key, sentence), expected) in test {
            if self.taught.contains_key(&(key.clone(), sentence.clone())) {
                return Err("TRAIN/TEST leakage".into());
            }
            let l = usize::from(key.0 == "en");
            let c = usize::from(expected.is_none());
            totals[l][c] += 1;
            correct[l][c] += usize::from(self.predict(key, sentence) == *expected);
        }
        if totals.iter().flatten().any(|&n| n == 0) || totals != correct {
            return Err(format!("context mastery mismatch: {correct:?}/{totals:?}").into());
        }
        Ok(correct)
    }
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [mode, train, output] if mode == "--learn" => {
            let bank = Bank::learn(rows(&read(train)?, false)?)?;
            let raw = bank.checkpoint();
            let restored = Bank::restore(&raw)?;
            if restored.taught != bank.taught {
                return Err("context checkpoint mismatch".into());
            }
            let mut file = File::options().write(true).create_new(true).open(output)?;
            file.write_all(raw.as_bytes())?;
            file.sync_all()?;
            println!("SML_TUTOR_CONTEXT_LEARN=PASS lessons={}", bank.taught.len());
        }
        [mode, bank, test] if mode == "--verify" => {
            let bank = Bank::restore(&read(bank)?)?;
            let counts = bank.verify(&rows(&read(test)?, true)?)?;
            for (lang, c) in ["fr", "en"].iter().zip(counts) {
                println!(
                    "SML_TUTOR_CONTEXT=PASS lang={lang} resolved={} unknown={} errors=0",
                    c[0], c[1]
                );
            }
        }
        [mode, bank, lang, lemma, pos, sentence] if mode == "--predict" => {
            let bank = Bank::restore(&read(bank)?)?;
            match bank.predict(&lexeme(lang, lemma, pos)?, sentence) {
                Some(id) => println!("{id}"),
                None => println!("ABSTAIN"),
            }
        }
        _ => return Err(
            "use --learn TRAIN BANK, --verify BANK TEST or --predict BANK LANG LEMMA POS CONTEXT"
                .into(),
        ),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    const TRAIN: &str = include_str!("../fixtures/tutor-c2-context/train.txt");
    const TEST: &str = include_str!("../fixtures/tutor-c2-context/test.txt");

    #[test]
    fn exact_bilingual_context_and_checkpoint() {
        let bank = Bank::learn(rows(TRAIN, false).unwrap()).unwrap();
        let expected = [[16, 8], [16, 8]];
        let test = rows(TEST, true).unwrap();
        assert_eq!(bank.taught.len(), 16);
        assert_eq!(bank.verify(&test).unwrap(), expected);
        let restored = Bank::restore(&bank.checkpoint()).unwrap();
        assert_eq!(restored.verify(&test).unwrap(), expected);
        assert!(restored.verify(&restored.taught).is_err());
    }

    #[test]
    fn abstain_on_conflict_and_unknown_without_language_leakage() {
        let bank = Bank::learn(rows(TRAIN, false).unwrap()).unwrap();
        let key = lexeme("fr", "avocat", "noun").unwrap();
        assert_eq!(bank.predict(&key, "avocat tribunal fruit"), None);
        assert_eq!(bank.predict(&key, "avocat"), None);
        let en = lexeme("en", "avocat", "noun").unwrap();
        assert_eq!(bank.predict(&en, "avocat tribunal"), None);
        assert!(
            Bank::restore(&(bank.checkpoint() + "fr|avocat|noun|avocat tribunal|unknown\n"))
                .is_err()
        );
        assert!(rows(
            "fr|avocat|noun|avocat tribunal|1\nfr|avocat|noun|Avocat tribunal!|2\n",
            false
        )
        .is_err());
    }
}
