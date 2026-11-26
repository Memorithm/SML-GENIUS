//! L2: acquire previously unknown lexical surfaces from aligned relations.
//! L1 role templates and semantic IDs are external anchors, not discovered meanings.
//! No TEST labels are read during induction; output remains L1-compatible.
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fs::File;
use std::io::{Read, Write};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
type Word = (char, usize);
type Lexicon = BTreeMap<String, Word>;
type Templates = BTreeMap<String, (bool, bool)>;
const MARKER: &str = "SML_TUTOR_LINGUISTIC_ROLES_V1";

fn read(path: &str) -> Result<String> {
    let mut raw = String::new();
    File::open(path)?.take(65_537).read_to_string(&mut raw)?;
    if raw.len() > 65_536 {
        return Err("oversized input".into());
    }
    Ok(raw)
}

fn token(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 48
        && s.to_lowercase() == s
        && s.chars().all(|c| c.is_alphabetic() || c == '\'')
}

fn index(s: &str) -> Result<usize> {
    let i: usize = s.parse()?;
    if i >= 16 || i.to_string() != s {
        return Err("invalid concept coordinate".into());
    }
    Ok(i)
}

fn flag(s: &str) -> Result<bool> {
    match s {
        "0" => Ok(false),
        "1" => Ok(true),
        _ => Err("invalid flag".into()),
    }
}

fn checkpoint(words: &Lexicon, templates: &Templates) -> String {
    let mut out = format!("{MARKER}\n");
    for (surface, (kind, id)) in words {
        out.push_str(&format!("L|{surface}|{kind}|{id}\n"));
    }
    for (pattern, (swap, neg)) in templates {
        out.push_str(&format!(
            "T|{pattern}|{}|{}\n",
            u8::from(*swap),
            u8::from(*neg)
        ));
    }
    out
}

fn restore(raw: &str) -> Result<(Lexicon, Templates)> {
    let mut lines = raw.lines();
    if lines.next() != Some(MARKER) {
        return Err("invalid L1 marker".into());
    }
    let mut words = Lexicon::new();
    let mut templates = Templates::new();
    for row in lines {
        let fields: Vec<_> = row.split('|').collect();
        match fields.as_slice() {
            ["L", surface, kind, id] if token(surface) => {
                let kind = match *kind {
                    "E" => 'E',
                    "V" => 'V',
                    _ => return Err("invalid kind".into()),
                };
                if words
                    .insert((*surface).into(), (kind, index(id)?))
                    .is_some()
                {
                    return Err("duplicate lexical surface".into());
                }
            }
            ["T", pattern, swap, neg] => {
                let parts: Vec<_> = pattern.split(' ').collect();
                if parts.len() > 12
                    || parts.iter().filter(|&&p| p == "@E").count() != 2
                    || parts.iter().filter(|&&p| p == "@V").count() != 1
                    || parts
                        .iter()
                        .any(|p| !matches!(*p, "@E" | "@V") && !token(p))
                    || templates
                        .insert((*pattern).into(), (flag(swap)?, flag(neg)?))
                        .is_some()
                {
                    return Err("invalid template".into());
                }
            }
            _ => return Err("invalid record".into()),
        }
    }
    if words.is_empty()
        || words.len() > 128
        || templates.is_empty()
        || templates.len() > 1024
        || checkpoint(&words, &templates) != raw
    {
        return Err("noncanonical L1 bank".into());
    }
    Ok((words, templates))
}

fn induce(original: &Lexicon, templates: &Templates, raw: &str) -> Result<Lexicon> {
    let mut words = original.clone();
    let mut seen = BTreeSet::new();
    // Grammar literals are never silently reclassified as lexical content.
    let literals: BTreeSet<_> = templates
        .keys()
        .flat_map(|p| p.split(' '))
        .filter(|p| !matches!(*p, "@E" | "@V"))
        .collect();
    for row in raw.lines() {
        let fields: Vec<_> = row.split('|').collect();
        let [sentence, subject, verb, object, neg] = fields.as_slice() else {
            return Err("invalid aligned TRAIN row".into());
        };
        if !seen.insert(*sentence) || seen.len() > 1024 {
            return Err("duplicate or excessive TRAIN".into());
        }
        let (s, v, o, neg) = (index(subject)?, index(verb)?, index(object)?, flag(neg)?);
        if s == o {
            return Err("ambiguous entity alignment".into());
        }
        let surfaces: Vec<_> = sentence.split(' ').collect();
        if surfaces.len() > 12 || surfaces.iter().any(|s| !token(s)) {
            return Err("invalid surface sentence".into());
        }
        let mut candidates = BTreeSet::new();
        for (pattern, &(swap, polarity)) in templates {
            let parts: Vec<_> = pattern.split(' ').collect();
            if parts.len() != surfaces.len() || polarity != neg {
                continue;
            }
            let entities = if swap { [o, s] } else { [s, o] };
            let mut entity = 0;
            let mut proposed = Lexicon::new();
            let mut valid = true;
            for (&p, &surface) in parts.iter().zip(&surfaces) {
                let word = match p {
                    "@E" => {
                        let id = entities[entity];
                        entity += 1;
                        ('E', id)
                    }
                    "@V" => ('V', v),
                    _ if p == surface && !words.contains_key(surface) => continue,
                    _ => {
                        valid = false;
                        break;
                    }
                };
                if literals.contains(surface)
                    || words.get(surface).is_some_and(|old| *old != word)
                    || proposed
                        .insert(surface.into(), word)
                        .is_some_and(|old| old != word)
                {
                    valid = false;
                    break;
                }
            }
            if valid {
                candidates.insert(proposed);
            }
        }
        if candidates.len() != 1 {
            return Err("ambiguous or contradictory lexical alignment".into());
        }
        words.extend(candidates.into_iter().next().expect("unique alignment"));
        if words.len() > 128 {
            return Err("lexical capacity exceeded".into());
        }
    }
    if seen.is_empty() || words.len() == original.len() {
        return Err("no new lexical information".into());
    }
    Ok(words)
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let [parent, train, output] = args.as_slice() else {
        return Err("usage: L1_CHECKPOINT ALIGNED_TRAIN NEW_CHECKPOINT".into());
    };
    let (old, templates) = restore(&read(parent)?)?;
    let learned = induce(&old, &templates, &read(train)?)?;
    let raw = checkpoint(&learned, &templates);
    let (restored, rules) = restore(&raw)?;
    if rules != templates || old.iter().any(|(s, w)| restored.get(s) != Some(w)) {
        return Err("retention mismatch".into());
    }
    let mut file = File::options().write(true).create_new(true).open(output)?;
    file.write_all(raw.as_bytes())?;
    file.sync_all()?;
    println!(
        "SML_TUTOR_LEXICON=PASS old={} new={} acquired={} templates={}",
        old.len(),
        restored.len(),
        restored.len() - old.len(),
        templates.len()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    const BANK: &str = "SML_TUTOR_LINGUISTIC_ROLES_V1\nL|alice|E|0\nL|bob|E|1\nL|voit|V|0\nT|@E @V @E|0|0\nT|@E est @V par @E|1|0\nT|@E ne @V pas @E|0|1\n";

    #[test]
    fn acquires_unknown_surfaces_and_preserves_old_templates() {
        let (old, templates) = restore(BANK).unwrap();
        let training = "claire suit david|2|1|3|0\ndavid est suivi par claire|2|1|3|0\n";
        let learned = induce(&old, &templates, training).unwrap();
        assert_eq!(learned.len(), 7);
        assert_eq!(learned.get("claire"), Some(&('E', 2)));
        assert_eq!(learned.get("david"), Some(&('E', 3)));
        assert_eq!(learned.get("suit"), Some(&('V', 1)));
        assert_eq!(learned.get("suivi"), Some(&('V', 1)));
        let (reloaded, rules) = restore(&checkpoint(&learned, &templates)).unwrap();
        assert_eq!(reloaded, learned);
        assert_eq!(rules, templates);
        assert!(old.iter().all(|(s, w)| reloaded.get(s) == Some(w)));
    }

    #[test]
    fn refuses_contradictions_and_literal_reclassification() {
        let (old, templates) = restore(BANK).unwrap();
        assert!(induce(&old, &templates, "alice suit david|2|1|3|0\n").is_err());
        assert!(induce(&old, &templates, "claire ne david|2|1|3|0\n").is_err());
        let bad = "claire suit david|2|1|3|0\ndavid ne suit pas claire|2|1|3|1\n";
        assert!(induce(&old, &templates, bad).is_err());
        assert_eq!(checkpoint(&old, &templates), BANK);
    }
}
