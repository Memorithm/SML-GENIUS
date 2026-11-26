//! TUTOR-08 L1: supervised French controlled-language role acquisition.
//! Lexical annotations and role labels are external; templates are learned.
//! Held-out relations use known words in unseen triples, not unseen vocabulary.
//! This is not a general French parser or a language model.
use sml_core::semantic::SemanticBitField;
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fs::File;
use std::io::{Read, Write};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const LIMIT: u64 = 65_536;
const MARKER: &str = "SML_TUTOR_LINGUISTIC_ROLES_V1";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Word {
    Entity(usize),
    Verb(usize),
}

type Lexicon = BTreeMap<String, Word>;
type Templates = BTreeMap<String, (bool, bool)>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Relation {
    subject: usize,
    verb: usize,
    object: usize,
    negated: bool,
}

impl Relation {
    fn triple(self) -> (usize, usize, usize) {
        (self.subject, self.verb, self.object)
    }

    fn field(self) -> Result<SemanticBitField<64>> {
        // Disjoint role coordinates avoid order/negation collisions.
        let mut indices = vec![self.subject, 16 + self.verb, 32 + self.object];
        if self.negated {
            indices.push(48);
        }
        Ok(SemanticBitField::from_set_indices(&indices)?)
    }
}

fn read(path: &str) -> Result<String> {
    let mut raw = String::new();
    File::open(path)?.take(LIMIT + 1).read_to_string(&mut raw)?;
    if raw.len() as u64 > LIMIT {
        return Err("oversized input".into());
    }
    Ok(raw)
}

fn token(s: &str) -> bool {
    !s.is_empty() && s.len() <= 48 && s.chars().all(|c| c.is_alphabetic() || c == '\'')
}

fn index(s: &str) -> Result<usize> {
    let i: usize = s.parse()?;
    if i >= 16 || i.to_string() != s {
        return Err("invalid concept coordinate".into());
    }
    Ok(i)
}

fn lexicon(raw: &str) -> Result<Lexicon> {
    let mut words = BTreeMap::new();
    for row in raw.lines() {
        let parts: Vec<_> = row.split('|').collect();
        let [surface, kind, id] = parts.as_slice() else {
            return Err("lexicon row: surface|E/V|id".into());
        };
        if !token(surface) || surface.to_lowercase() != *surface {
            return Err("invalid lexical surface".into());
        }
        let word = match *kind {
            "E" => Word::Entity(index(id)?),
            "V" => Word::Verb(index(id)?),
            _ => return Err("invalid lexical kind".into()),
        };
        if words.insert((*surface).to_owned(), word).is_some() {
            return Err("duplicate lexical surface".into());
        }
    }
    if words.is_empty() || words.len() > 128 {
        return Err("invalid lexicon size".into());
    }
    Ok(words)
}

fn shape(text: &str, words: &Lexicon) -> Option<(String, [usize; 2], usize)> {
    let mut pattern = Vec::new();
    let mut entities = Vec::new();
    let mut verb = None;
    for surface in text.split(' ') {
        if !token(surface) || pattern.len() >= 12 {
            return None;
        }
        match words.get(surface) {
            Some(Word::Entity(i)) => {
                entities.push(*i);
                pattern.push("@E");
            }
            Some(Word::Verb(i)) => {
                if verb.replace(*i).is_some() {
                    return None;
                }
                pattern.push("@V");
            }
            None => pattern.push(surface),
        }
    }
    let entities: [usize; 2] = entities.try_into().ok()?;
    Some((pattern.join(" "), entities, verb?))
}

fn rows(raw: &str) -> Result<Vec<(&str, Relation)>> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for row in raw.lines() {
        let parts: Vec<_> = row.split('|').collect();
        let [text, subject, verb, object, negated] = parts.as_slice() else {
            return Err("relation row: text|subject|verb|object|negated".into());
        };
        let relation = Relation {
            subject: index(subject)?,
            verb: index(verb)?,
            object: index(object)?,
            negated: match *negated {
                "0" => false,
                "1" => true,
                _ => return Err("invalid polarity".into()),
            },
        };
        if !seen.insert(*text) {
            return Err("duplicate sentence".into());
        }
        out.push((*text, relation));
    }
    if out.is_empty() || out.len() > 1024 {
        return Err("invalid relation corpus size".into());
    }
    Ok(out)
}

fn train(words: &Lexicon, training: &[(&str, Relation)]) -> Result<Templates> {
    let mut templates = BTreeMap::new();
    for &(text, target) in training {
        let (pattern, entities, verb) = shape(text, words).ok_or("unparseable TRAIN")?;
        if entities[0] == entities[1] || verb != target.verb {
            return Err("ambiguous or inconsistent TRAIN".into());
        }
        let swap = if entities == [target.subject, target.object] {
            false
        } else if entities == [target.object, target.subject] {
            true
        } else {
            return Err("TRAIN roles incompatible with lexicon".into());
        };
        let value = (swap, target.negated);
        if templates.insert(pattern, value).is_some_and(|old| old != value) {
            return Err("contradictory TRAIN template".into());
        }
    }
    Ok(templates)
}

fn predict(text: &str, words: &Lexicon, templates: &Templates) -> Option<Relation> {
    let (pattern, entities, verb) = shape(text, words)?;
    let &(swap, negated) = templates.get(&pattern)?;
    let subject = usize::from(swap);
    Some(Relation {
        subject: entities[subject],
        verb,
        object: entities[1 - subject],
        negated,
    })
}

fn evaluate(
    words: &Lexicon,
    templates: &Templates,
    training: &[(&str, Relation)],
    test: &[(&str, Relation)],
) -> Result<usize> {
    let seen: BTreeSet<_> = training.iter().map(|(_, r)| r.triple()).collect();
    let sentences: BTreeSet<_> = training.iter().map(|(text, _)| *text).collect();
    let mut errors = 0;
    for &(text, expected) in test {
        if seen.contains(&expected.triple()) || sentences.contains(text) {
            return Err("TRAIN/TEST leakage".into());
        }
        match predict(text, words, templates) {
            Some(actual) if actual.field()? == expected.field()? => {}
            _ => errors += 1,
        }
    }
    Ok(errors)
}

fn checkpoint(words: &Lexicon, templates: &Templates) -> String {
    let mut out = format!("{MARKER}\n");
    for (surface, word) in words {
        let (kind, id) = match word {
            Word::Entity(i) => ("E", i),
            Word::Verb(i) => ("V", i),
        };
        out.push_str(&format!("L|{surface}|{kind}|{id}\n"));
    }
    for (pattern, (swap, negated)) in templates {
        out.push_str(&format!(
            "T|{pattern}|{}|{}\n",
            u8::from(*swap),
            u8::from(*negated)
        ));
    }
    out
}

fn restore(raw: &str) -> Result<(Lexicon, Templates)> {
    let mut lines = raw.lines();
    if lines.next() != Some(MARKER) {
        return Err("invalid checkpoint marker".into());
    }
    let mut lexical = String::new();
    let mut templates = BTreeMap::new();
    for line in lines {
        if let Some(row) = line.strip_prefix("L|") {
            lexical.push_str(row);
            lexical.push('\n');
        } else if let Some(row) = line.strip_prefix("T|") {
            let parts: Vec<_> = row.split('|').collect();
            let [pattern, swap, negated] = parts.as_slice() else {
                return Err("invalid template record".into());
            };
            let flag = |s: &str| match s {
                "0" => Ok(false),
                "1" => Ok(true),
                _ => Err("invalid template flag"),
            };
            let fields: Vec<_> = pattern.split(' ').collect();
            if fields.len() > 12
                || fields.iter().filter(|&&s| s == "@E").count() != 2
                || fields.iter().filter(|&&s| s == "@V").count() != 1
                || fields.iter().any(|s| !matches!(*s, "@E" | "@V") && !token(s))
                || templates
                    .insert((*pattern).to_owned(), (flag(swap)?, flag(negated)?))
                    .is_some()
            {
                return Err("invalid or duplicate template".into());
            }
        } else {
            return Err("unknown checkpoint record".into());
        }
    }
    let words = lexicon(&lexical)?;
    if templates.is_empty() || templates.len() > 1024 || checkpoint(&words, &templates) != raw {
        return Err("noncanonical checkpoint".into());
    }
    Ok((words, templates))
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [lex, train_path, test_path, output] => {
            let words = lexicon(&read(lex)?)?;
            let train_raw = read(train_path)?;
            let test_raw = read(test_path)?;
            let training = rows(&train_raw)?;
            let test = rows(&test_raw)?;
            let templates = train(&words, &training)?;
            let errors = evaluate(&words, &templates, &training, &test)?;
            println!(
                "SML_TUTOR_LINGUISTIC train={} test={} templates={} errors={errors}",
                training.len(),
                test.len(),
                templates.len()
            );
            if errors != 0 {
                return Err("held-out relation mismatch".into());
            }
            let raw = checkpoint(&words, &templates);
            let (restored, rules) = restore(&raw)?;
            if evaluate(&restored, &rules, &training, &test)? != 0 {
                return Err("checkpoint replay failed".into());
            }
            let mut file = File::options().write(true).create_new(true).open(output)?;
            file.write_all(raw.as_bytes())?;
            file.sync_all()?;
        }
        [mode, input, text] if mode == "--predict" => {
            let (words, templates) = restore(&read(input)?)?;
            match predict(text, &words, &templates) {
                Some(r) => println!(
                    "{}|{}|{}|{}",
                    r.subject,
                    r.verb,
                    r.object,
                    u8::from(r.negated)
                ),
                None => println!("ABSTAIN"),
            }
        }
        _ => return Err("usage: LEXICON TRAIN TEST NEW_CHECKPOINT | --predict CHECKPOINT TEXT".into()),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    const LEX: &str = "alice|E|0\nbob|E|1\nclaire|E|2\ndavid|E|3\nvoit|V|0\nvu|V|0\nsuit|V|1\nsuivi|V|1\n";
    const TRAIN: &str = "alice voit bob|0|0|1|0\nalice ne voit pas bob|0|0|1|1\nbob est vu par alice|0|0|1|0\nbob n'est pas vu par alice|0|0|1|1\n";
    const TEST: &str = "claire suit david|2|1|3|0\nclaire ne aide pas david|2|1|3|1\ndavid est suivi par claire|2|1|3|0\ndavid n'est pas suivi par claire|2|1|3|1\ndavid suit claire|3|1|2|0\n";

    #[test]
    fn held_out_role_reversal_and_polarity() {
        let words = lexicon(LEX).unwrap();
        let training = rows(TRAIN).unwrap();
        let test = rows(TEST).unwrap();
        let templates = train(&words, &training).unwrap();
        assert_eq!(templates.len(), 4);
        assert_eq!(evaluate(&words, &templates, &training, &test).unwrap(), 0);
        let (new_words, new_templates) = restore(&checkpoint(&words, &templates)).unwrap();
        assert_eq!(
            evaluate(&new_words, &new_templates, &training, &test).unwrap(),
            0
        );
        assert!(predict("eve suit david", &words, &templates).is_none());
        assert!(predict("david suit vite claire", &words, &templates).is_none());
        assert!(evaluate(&words, &templates, &training, &training).is_err());
    }

    #[test]
    fn conflicting_labels_and_corrupt_checkpoints_fail_closed() {
        let words = lexicon(LEX).unwrap();
        let bad = rows("alice voit bob|0|0|1|0\nclaire suit david|3|1|2|0\n").unwrap();
        assert!(train(&words, &bad).is_err());
        let templates = train(&words, &rows(TRAIN).unwrap()).unwrap();
        let raw = checkpoint(&words, &templates);
        assert!(restore(&(raw + "T|@E @V @E|0|0\n")).is_err());
    }
}
