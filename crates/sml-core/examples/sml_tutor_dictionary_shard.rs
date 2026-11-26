//! TUTOR-08: bounded exact lexical fact memory for dictionary-scale acquisition.
//! The dictionary is an external teacher; lookup alone is NOT contextual understanding.
//! Each immutable shard binds a language and a frozen source SHA-256.
//! Same-spelling senses remain distinct. No external dependencies.
use std::collections::BTreeMap;
use std::error::Error;
use std::fs::File;
use std::io::{Read, Write};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const MARKER: &str = "SML_TUTOR_DICTIONARY_SHARD_V1";
const MAX_BYTES: u64 = 16 * 1024 * 1024;
const MAX_RECORDS: usize = 4096;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Entry {
    lemma: String,
    pos: String,
    gloss: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Bank {
    lang: String,
    source: String,
    entries: BTreeMap<u64, Entry>,
}

fn read(path: &str) -> Result<String> {
    let mut raw = String::new();
    File::open(path)?
        .take(MAX_BYTES + 1)
        .read_to_string(&mut raw)?;
    if raw.len() as u64 > MAX_BYTES {
        return Err("shard exceeds byte budget".into());
    }
    Ok(raw)
}

fn hex_byte(b: u8) -> bool {
    b.is_ascii_digit() || (b'a'..=b'f').contains(&b)
}

fn decode(raw: &str, max: usize) -> Result<String> {
    if raw.is_empty()
        || raw.len() > max * 2
        || raw.len() % 2 != 0
        || !raw.bytes().all(hex_byte)
    {
        return Err("invalid canonical hex field".into());
    }
    let bytes = (0..raw.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&raw[i..i + 2], 16))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let text = String::from_utf8(bytes)?;
    if text.trim().is_empty() || text.contains('\0') {
        return Err("empty or invalid text".into());
    }
    Ok(text)
}

fn encode(text: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(text.len() * 2);
    for b in text.bytes() {
        out.push(char::from(HEX[usize::from(b >> 4)]));
        out.push(char::from(HEX[usize::from(b & 15)]));
    }
    out
}

fn id(raw: &str) -> Result<u64> {
    let value: u64 = raw.parse()?;
    if value == 0 || raw != value.to_string() {
        return Err("invalid sense record identity".into());
    }
    Ok(value)
}

impl Bank {
    fn parse(raw: &str) -> Result<Self> {
        if raw.len() as u64 > MAX_BYTES || !raw.ends_with('\n') {
            return Err("invalid shard length or termination".into());
        }
        let mut lines = raw.lines();
        if lines.next() != Some(MARKER) {
            return Err("unknown lexical schema".into());
        }
        let lang = lines
            .next()
            .and_then(|line| line.strip_prefix("lang="))
            .ok_or("missing language")?;
        if !matches!(lang, "fr" | "en") {
            return Err("unsupported language".into());
        }
        let source = lines
            .next()
            .and_then(|line| line.strip_prefix("source="))
            .ok_or("missing source")?;
        if source.len() != 64 || !source.bytes().all(hex_byte) {
            return Err("invalid source SHA-256".into());
        }
        let mut entries = BTreeMap::new();
        for row in lines {
            let fields: Vec<_> = row.split('\t').collect();
            let [record, lemma, pos, gloss] = fields.as_slice() else {
                return Err("invalid sense row".into());
            };
            let entry = Entry {
                lemma: decode(lemma, 512)?,
                pos: decode(pos, 64)?,
                gloss: decode(gloss, 8192)?,
            };
            if entries.insert(id(record)?, entry).is_some() || entries.len() > MAX_RECORDS {
                return Err("duplicate identity or shard capacity exceeded".into());
            }
        }
        if entries.is_empty() {
            return Err("empty shard".into());
        }
        Ok(Self {
            lang: lang.into(),
            source: source.into(),
            entries,
        })
    }

    fn serialize(&self) -> String {
        let mut out = format!("{MARKER}\nlang={}\nsource={}\n", self.lang, self.source);
        for (&id, entry) in &self.entries {
            out.push_str(&format!(
                "{id}\t{}\t{}\t{}\n",
                encode(&entry.lemma),
                encode(&entry.pos),
                encode(&entry.gloss)
            ));
        }
        out
    }

    fn restore(raw: &str) -> Result<Self> {
        let bank = Self::parse(raw)?;
        if bank.serialize() != raw {
            return Err("noncanonical checkpoint".into());
        }
        Ok(bank)
    }

    fn recall(&self, record: u64) -> Option<&Entry> {
        self.entries.get(&record)
    }

    fn candidates(&self, lemma: &str, pos: &str) -> Vec<u64> {
        // Bounded shard scan only. A future global address index routes shards.
        self.entries
            .iter()
            .filter_map(|(&record, entry)| {
                (entry.lemma == lemma && entry.pos == pos).then_some(record)
            })
            .collect()
    }

    fn score(&self, oracle: &Self) -> Result<(usize, usize)> {
        if self.lang != oracle.lang
            || self.source != oracle.source
            || self.entries.len() != oracle.entries.len()
        {
            return Err("source, language or cardinality mismatch".into());
        }
        let correct = oracle
            .entries
            .iter()
            .filter(|(record, entry)| self.recall(**record) == Some(*entry))
            .count();
        Ok((correct, oracle.entries.len()))
    }
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [mode, train, output] if mode == "--learn" => {
            let oracle = Bank::parse(&read(train)?)?;
            let serialized = oracle.serialize();
            let bank = Bank::restore(&serialized)?;
            let (correct, total) = bank.score(&oracle)?;
            if correct != total {
                return Err("lexical recall not exact".into());
            }
            let mut file = File::options().write(true).create_new(true).open(output)?;
            file.write_all(serialized.as_bytes())?;
            file.sync_all()?;
            println!(
                "SML_TUTOR_DICTIONARY_LEARNED=PASS lang={} records={total} correct={correct}",
                bank.lang
            );
        }
        [mode, checkpoint, reference] if mode == "--verify" => {
            let bank = Bank::restore(&read(checkpoint)?)?;
            let oracle = Bank::parse(&read(reference)?)?;
            let (correct, total) = bank.score(&oracle)?;
            if correct != total {
                return Err("dictionary recall below 100 percent".into());
            }
            println!(
                "SML_TUTOR_DICTIONARY_RECALL=PASS lang={} records={total} correct={correct} errors=0",
                bank.lang
            );
        }
        [mode, checkpoint, record] if mode == "--recall" => {
            let bank = Bank::restore(&read(checkpoint)?)?;
            match bank.recall(id(record)?) {
                Some(entry) => println!(
                    "{}\t{}\t{}",
                    encode(&entry.lemma),
                    encode(&entry.pos),
                    encode(&entry.gloss)
                ),
                None => println!("ABSTAIN"),
            }
        }
        [mode, checkpoint, lemma, pos] if mode == "--candidates" => {
            let bank = Bank::restore(&read(checkpoint)?)?;
            let matches = bank.candidates(lemma, pos);
            if matches.is_empty() {
                println!("ABSTAIN");
            } else {
                for record in matches {
                    println!("{record}");
                }
            }
        }
        _ => {
            return Err(
                "usage: --learn TRAIN BANK | --verify BANK REFERENCE | --recall BANK ID | --candidates BANK LEMMA POS".into(),
            )
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(lang: &str) -> String {
        let source = "a".repeat(64);
        let mut out = format!("{MARKER}\nlang={lang}\nsource={source}\n");
        for (id, word, pos, gloss) in [
            (3, "avocat", "noun", "fruit comestible"),
            (1, "avocat", "noun", "profession juridique"),
            (2, "forêt", "noun", "ensemble d’arbres"),
        ] {
            out.push_str(&format!(
                "{id}\t{}\t{}\t{}\n",
                encode(word),
                encode(pos),
                encode(gloss)
            ));
        }
        out
    }

    #[test]
    fn exact_recall_preserves_all_senses_and_utf8() {
        let oracle = Bank::parse(&fixture("fr")).unwrap();
        let bank = Bank::restore(&oracle.serialize()).unwrap();
        assert_eq!(bank.score(&oracle).unwrap(), (3, 3));
        assert_eq!(bank.candidates("avocat", "noun"), vec![1, 3]);
        assert_eq!(bank.recall(2).unwrap().lemma, "forêt");
        assert!(bank.recall(4).is_none());
        let mut bad = bank.clone();
        bad.entries.get_mut(&1).unwrap().gloss = "autre".into();
        assert_eq!(bad.score(&oracle).unwrap(), (2, 3));
        assert!(bank.score(&Bank::parse(&fixture("en")).unwrap()).is_err());
    }

    #[test]
    fn rejects_corruption_duplicate_and_noncanonical_bank() {
        let raw = fixture("fr");
        assert!(Bank::restore(&raw).is_err());
        let row = raw.lines().last().unwrap();
        assert!(Bank::parse(&format!("{raw}{row}\n")).is_err());
        assert!(Bank::parse(&raw.replace("lang=fr", "lang=es")).is_err());
        assert!(decode("ff", 16).is_err());
        assert!(decode("41AB", 16).is_err());
        assert!(decode("00", 16).is_err());
    }
}
