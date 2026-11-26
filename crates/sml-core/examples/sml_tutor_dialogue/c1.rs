//! C1 bounded conversation over acquired L1 roles. Question templates are
//! supervised; response rendering is explicit. No teacher at inference.
use super::{predict, restore, Lexicon, Result, Templates, Word};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, BufRead, Read, Write};

const MAX_TURNS: usize = 256;
const MAX_TEMPLATES: usize = 64;
type Triple = (usize, usize, usize);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Intent {
    Truth,
    Agent,
    Object,
    FocusObject,
    Revise,
}

impl Intent {
    fn parse(s: &str) -> Result<Self> {
        match s {
            "truth" => Ok(Self::Truth),
            "agent" => Ok(Self::Agent),
            "object" => Ok(Self::Object),
            "focus_object" => Ok(Self::FocusObject),
            "revise" => Ok(Self::Revise),
            _ => Err("unknown intent".into()),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Truth => "truth",
            Self::Agent => "agent",
            Self::Object => "object",
            Self::FocusObject => "focus_object",
            Self::Revise => "revise",
        }
    }

    fn counts(self) -> (usize, usize) {
        match self {
            Self::Truth => (2, 1),
            Self::Agent | Self::Object => (1, 1),
            Self::FocusObject => (0, 1),
            Self::Revise => (0, 0),
        }
    }
}

#[derive(Clone, Debug)]
struct Form {
    key: String,
    entities: Vec<usize>,
    verbs: Vec<usize>,
}

fn normalized(text: &str) -> Option<String> {
    if text.len() > 512 || text.contains('|') || text.contains('\0') {
        return None;
    }
    let lower = text.trim().to_lowercase();
    let result = lower.trim_end_matches(['?', '.', '!']).trim();
    if result.is_empty() || result.split_whitespace().count() > 16 {
        return None;
    }
    Some(result.split_whitespace().collect::<Vec<_>>().join(" "))
}

fn form(text: &str, words: &Lexicon) -> Option<Form> {
    let text = normalized(text)?;
    let mut parts = Vec::new();
    let mut entities = Vec::new();
    let mut verbs = Vec::new();
    for token in text.split(' ') {
        match words.get(token) {
            Some(Word::Entity(i)) => {
                entities.push(*i);
                parts.push("@E");
            }
            Some(Word::Verb(i)) => {
                verbs.push(*i);
                parts.push("@V");
            }
            None => {
                if !token.chars().all(|c| c.is_alphabetic() || c == '\'') {
                    return None;
                }
                parts.push(token);
            }
        }
    }
    Some(Form {
        key: parts.join(" "),
        entities,
        verbs,
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Questions(BTreeMap<String, Intent>);

impl Questions {
    fn learn(raw: &str, lex: &Lexicon) -> Result<Self> {
        let mut rules = BTreeMap::new();
        for row in raw.lines() {
            let (sentence, tag) = row.split_once('|').ok_or("invalid question lesson")?;
            let intent = Intent::parse(tag)?;
            let f = form(sentence, lex).ok_or("invalid question form")?;
            if (f.entities.len(), f.verbs.len()) != intent.counts()
                || (intent == Intent::Revise && f.key.split(' ').count() != 1)
            {
                return Err("wrong argument count".into());
            }
            if rules.insert(f.key, intent).is_some_and(|old| old != intent) {
                return Err("conflicting question labels".into());
            }
        }
        if rules.len() > MAX_TEMPLATES
            || ["truth", "agent", "object", "focus_object", "revise"]
                .iter()
                .any(|tag| !rules.values().any(|i| i.label() == *tag))
        {
            return Err("incomplete or oversized question curriculum".into());
        }
        Ok(Self(rules))
    }

    fn checkpoint(&self) -> String {
        let mut out = String::from("SML_TUTOR_C1_QUESTIONS_V1\n");
        for (k, intent) in &self.0 {
            out.push_str(&format!("{k}|{}\n", intent.label()));
        }
        out
    }

    fn restore(raw: &str) -> Result<Self> {
        let mut lines = raw.lines();
        if lines.next() != Some("SML_TUTOR_C1_QUESTIONS_V1") {
            return Err("question checkpoint marker".into());
        }
        let mut rules = BTreeMap::new();
        for row in lines {
            let (key, tag) = row.split_once('|').ok_or("invalid rule")?;
            let intent = Intent::parse(tag)?;
            let parts: Vec<_> = key.split(' ').collect();
            let counts = (
                parts.iter().filter(|&&s| s == "@E").count(),
                parts.iter().filter(|&&s| s == "@V").count(),
            );
            if counts != intent.counts()
                || parts.len() > 16
                || parts.iter().any(|s| {
                    s.is_empty()
                        || (!matches!(*s, "@E" | "@V")
                            && !s.chars().all(|c| c.is_alphabetic() || c == '\''))
                })
                || (intent == Intent::Revise && parts.len() != 1)
                || rules.insert(key.to_owned(), intent).is_some()
            {
                return Err("invalid or duplicate restored rule".into());
            }
        }
        let bank = Self(rules);
        if bank.0.len() > MAX_TEMPLATES
            || ["truth", "agent", "object", "focus_object", "revise"]
                .iter()
                .any(|tag| !bank.0.values().any(|i| i.label() == *tag))
            || bank.checkpoint() != raw
        {
            return Err("noncanonical or incomplete question bank".into());
        }
        Ok(bank)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Answer {
    Yes,
    No,
    Unknown,
    Ack,
    Conflict,
    Ambiguous,
    Entity(usize),
}

struct Session<'a> {
    lex: &'a Lexicon,
    roles: &'a Templates,
    questions: &'a Questions,
    facts: BTreeMap<Triple, bool>,
    focus: Option<usize>,
    turns: usize,
}

impl<'a> Session<'a> {
    fn new(lex: &'a Lexicon, roles: &'a Templates, questions: &'a Questions) -> Self {
        Self {
            lex,
            roles,
            questions,
            facts: BTreeMap::new(),
            focus: None,
            turns: 0,
        }
    }

    fn query(&mut self, f: &Form, intent: Intent) -> Answer {
        match intent {
            Intent::Truth => {
                let key = (f.entities[0], f.verbs[0], f.entities[1]);
                match self.facts.get(&key) {
                    Some(true) => {
                        self.focus = Some(key.0);
                        Answer::Yes
                    }
                    Some(false) => {
                        self.focus = Some(key.0);
                        Answer::No
                    }
                    None => Answer::Unknown,
                }
            }
            Intent::Agent | Intent::Object | Intent::FocusObject => {
                let agent = intent == Intent::Agent;
                let anchor = if intent == Intent::FocusObject {
                    match self.focus {
                        Some(i) => i,
                        None => return Answer::Unknown,
                    }
                } else {
                    f.entities[0]
                };
                let candidates: BTreeSet<_> = self
                    .facts
                    .iter()
                    .filter(|&(&(s, v, o), &positive)| {
                        positive && v == f.verbs[0] && if agent { o == anchor } else { s == anchor }
                    })
                    .map(|(&(s, _, o), _)| if agent { s } else { o })
                    .collect();
                match candidates.len() {
                    0 => Answer::Unknown,
                    1 => {
                        let entity = *candidates.iter().next().expect("one candidate");
                        self.focus = Some(if agent { entity } else { anchor });
                        Answer::Entity(entity)
                    }
                    _ => Answer::Ambiguous,
                }
            }
            Intent::Revise => Answer::Unknown,
        }
    }

    fn respond(&mut self, input: &str) -> Answer {
        if self.turns >= MAX_TURNS {
            return Answer::Unknown;
        }
        self.turns += 1;
        let Some(text) = normalized(input) else {
            return Answer::Unknown;
        };
        let (revision, sentence) = if let Some((prefix, tail)) = text.split_once(": ") {
            let revision = self.questions.0.get(prefix) == Some(&Intent::Revise);
            if !revision {
                return Answer::Unknown;
            }
            (true, tail)
        } else {
            (false, text.as_str())
        };
        if !revision {
            if let Some(f) = form(sentence, self.lex) {
                if let Some(&intent) = self.questions.0.get(&f.key) {
                    if intent != Intent::Revise {
                        return self.query(&f, intent);
                    }
                }
            }
        }
        // Do not admit unsupported questions as assertions.
        if input.trim_end().ends_with('?') {
            return Answer::Unknown;
        }
        let Some(r) = predict(sentence, self.lex, self.roles) else {
            return Answer::Unknown;
        };
        let key = r.triple();
        let value = !r.negated;
        match self.facts.get(&key) {
            Some(old) if *old != value && !revision => return Answer::Conflict,
            None if revision => return Answer::Unknown,
            _ => {}
        }
        self.facts.insert(key, value);
        self.focus = Some(r.subject);
        Answer::Ack
    }

    fn render(&self, answer: Answer, lang: &str) -> Result<String> {
        let text = match (lang, answer) {
            ("fr", Answer::Yes) => "oui".into(),
            ("fr", Answer::No) => "non".into(),
            ("fr", Answer::Unknown) => "je ne sais pas".into(),
            ("fr", Answer::Ack) => "compris".into(),
            ("fr", Answer::Conflict) => "information contradictoire".into(),
            ("fr", Answer::Ambiguous) => "plusieurs réponses possibles".into(),
            ("en", Answer::Yes) => "yes".into(),
            ("en", Answer::No) => "no".into(),
            ("en", Answer::Unknown) => "i do not know".into(),
            ("en", Answer::Ack) => "understood".into(),
            ("en", Answer::Conflict) => "conflicting information".into(),
            ("en", Answer::Ambiguous) => "multiple possible answers".into(),
            ("fr" | "en", Answer::Entity(i)) => self
                .lex
                .iter()
                .find_map(|(name, word)| (*word == Word::Entity(i)).then(|| name.clone()))
                .ok_or("unrenderable entity")?,
            _ => return Err("invalid language".into()),
        };
        Ok(text)
    }
}

fn evaluate(
    lex: &Lexicon,
    roles: &Templates,
    questions: &Questions,
    lang: &str,
    raw: &str,
) -> Result<(usize, usize)> {
    let mut sessions = 0;
    let mut turns = 0;
    for block in raw.trim_end().split("\n\n") {
        let mut session = Session::new(lex, roles, questions);
        let mut count = 0;
        for line in block.lines() {
            let (input, expected) = line.split_once('|').ok_or("invalid dialogue test")?;
            let actual = session.respond(input);
            if session.render(actual, lang)? != expected {
                return Err(format!("dialogue mismatch session={sessions} turn={count}").into());
            }
            count += 1;
        }
        if count != 10 {
            return Err("dialogue must have exactly ten turns".into());
        }
        turns += count;
        sessions += 1;
    }
    if sessions == 0 || sessions > 64 {
        return Err("invalid dialogue count".into());
    }
    Ok((sessions, turns))
}

pub(super) fn dispatch(mode: &str, args: &[String]) -> Result<()> {
    match (mode, args) {
        ("--dialogue-learn", [l1, lessons, output]) => {
            let (lex, _) = restore(&super::read(l1)?)?;
            let q = Questions::learn(&super::read(lessons)?, &lex)?;
            let raw = q.checkpoint();
            if Questions::restore(&raw)? != q {
                return Err("question checkpoint mismatch".into());
            }
            let mut file = std::fs::File::options()
                .write(true)
                .create_new(true)
                .open(output)?;
            file.write_all(raw.as_bytes())?;
            file.sync_all()?;
            println!("SML_TUTOR_C1_LEARN=PASS templates={}", q.0.len());
        }
        ("--dialogue-verify", [l1, question_bank, lang, test]) => {
            if !matches!(lang.as_str(), "fr" | "en") {
                return Err("invalid language".into());
            }
            let (lex, roles) = restore(&super::read(l1)?)?;
            let q = Questions::restore(&super::read(question_bank)?)?;
            let (sessions, turns) = evaluate(&lex, &roles, &q, lang, &super::read(test)?)?;
            println!("SML_TUTOR_C1=PASS lang={lang} sessions={sessions} turns={turns} errors=0");
        }
        ("--dialogue-run", [l1, question_bank, lang]) => {
            if !matches!(lang.as_str(), "fr" | "en") {
                return Err("invalid language".into());
            }
            let (lex, roles) = restore(&super::read(l1)?)?;
            let q = Questions::restore(&super::read(question_bank)?)?;
            let mut session = Session::new(&lex, &roles, &q);
            let mut input = io::stdin().lock();
            let mut output = io::stdout().lock();
            // Bound input memory even for malformed or untrusted long lines.
            for _ in 0..MAX_TURNS {
                let mut line = Vec::new();
                let n = input
                    .by_ref()
                    .take(514)
                    .read_until(b'\n', &mut line)?;
                if n == 0 {
                    break;
                }
                if line.len() > 513 || !line.ends_with(b"\n") {
                    return Err("oversized or unterminated dialogue input".into());
                }
                let line = std::str::from_utf8(&line)?;
                let answer = session.respond(line);
                writeln!(output, "{}", session.render(answer, lang)?)?;
                output.flush()?;
            }
        }
        _ => return Err("invalid C1 command or arguments".into()),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{checkpoint, lexicon, rows, train};

    fn fixture(lang: &str) -> (Lexicon, Templates, Questions, &'static str) {
        let (l, t, q, d) = if lang == "fr" {
            (
                include_str!("../../fixtures/tutor-l1/lexicon.txt"),
                include_str!("../../fixtures/tutor-l1/train.txt"),
                include_str!("../../fixtures/tutor-c1/fr-questions.txt"),
                include_str!("../../fixtures/tutor-c1/fr-dialogues.txt"),
            )
        } else {
            (
                include_str!("../../fixtures/tutor-en-l1/lexicon.txt"),
                include_str!("../../fixtures/tutor-en-l1/train.txt"),
                include_str!("../../fixtures/tutor-c1/en-questions.txt"),
                include_str!("../../fixtures/tutor-c1/en-dialogues.txt"),
            )
        };
        let lex = lexicon(l).unwrap();
        let roles = train(&lex, &rows(t).unwrap()).unwrap();
        let (lex, roles) = restore(&checkpoint(&lex, &roles)).unwrap();
        let q = Questions::learn(q, &lex).unwrap();
        let q = Questions::restore(&q.checkpoint()).unwrap();
        (lex, roles, q, d)
    }

    #[test]
    fn ten_turn_bilingual_dialogues_without_teacher() {
        for lang in ["fr", "en"] {
            let (lex, roles, q, d) = fixture(lang);
            assert_eq!(evaluate(&lex, &roles, &q, lang, d).unwrap(), (16, 160));
        }
    }

    #[test]
    fn session_isolation_and_ambiguity() {
        let (lex, roles, q, _) = fixture("en");
        let mut s = Session::new(&lex, &roles, &q);
        assert_eq!(s.respond("alice sees beth"), Answer::Ack);
        assert_eq!(s.respond("alice sees claire"), Answer::Ack);
        assert_eq!(s.respond("who does alice see?"), Answer::Ambiguous);
        assert_eq!(s.respond("who does she see?"), Answer::Ambiguous);
        let mut fresh = Session::new(&lex, &roles, &q);
        assert_eq!(fresh.respond("does alice see beth?"), Answer::Unknown);
        assert_eq!(fresh.respond("who does she see?"), Answer::Unknown);
        assert_eq!(fresh.respond("correction: alice sees beth"), Answer::Unknown);
        assert_eq!(fresh.respond("alice sees beth?"), Answer::Unknown);
        assert!(Questions::restore(&(q.checkpoint() + "who|truth\n")).is_err());
    }
}
