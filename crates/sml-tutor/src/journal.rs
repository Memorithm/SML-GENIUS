//! Append-only, crash-tolerant transition journal for automatic resume.
//!
//! The journal records knowledge-state transitions so that an interrupted
//! session can find the lessons that were in flight. It is a plain text file:
//!
//! ```text
//! SML_TUTOR_JOURNAL_V1
//! T <lesson_hex> <from_state> <to_state> <seq>
//! ```
//!
//! Sequence numbers start at 1 and increase by exactly one per record. A final
//! line without a trailing newline is a torn write from a crash: it is ignored
//! and truncated away on open. Every other irregularity (wrong header, a bad or
//! out-of-order record, a transition the state machine forbids, or a `from`
//! state that does not match the replayed state) fails closed with
//! [`JournalError::Corrupt`]. Nothing is repaired silently.
//!
//! [`Journal::append`] writes a record; [`Journal::sync`] makes the records
//! written so far durable. The journal assumes a single writer.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;

use crate::domain::KnowledgeState;
use crate::hash::Digest;

const HEADER: &str = "SML_TUTOR_JOURNAL_V1";

/// Errors from opening, replaying or appending to a journal.
#[derive(Debug)]
pub enum JournalError {
    Io(io::Error),
    Corrupt {
        line: usize,
        reason: String,
    },
    IllegalTransition {
        from: KnowledgeState,
        to: KnowledgeState,
    },
}

impl fmt::Display for JournalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JournalError::Io(e) => write!(f, "journal I/O error: {e}"),
            JournalError::Corrupt { line, reason } => {
                write!(f, "journal corrupt at line {line}: {reason}")
            }
            JournalError::IllegalTransition { from, to } => write!(
                f,
                "illegal journal transition {} -> {}",
                from.name(),
                to.name()
            ),
        }
    }
}

impl Error for JournalError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            JournalError::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for JournalError {
    fn from(e: io::Error) -> Self {
        JournalError::Io(e)
    }
}

fn corrupt(line: usize, reason: impl Into<String>) -> JournalError {
    JournalError::Corrupt {
        line,
        reason: reason.into(),
    }
}

/// The state of every lesson as reconstructed from the journal.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Replay {
    pub states: BTreeMap<Digest, KnowledgeState>,
    pub records: u64,
    pub torn_tail: bool,
}

impl Replay {
    /// Lessons whose replayed state is in flight: work lost if the process crashed.
    pub fn interrupted(&self) -> Vec<Digest> {
        self.states
            .iter()
            .filter(|(_, state)| state.is_in_flight())
            .map(|(lesson, _)| *lesson)
            .collect()
    }
}

/// An open journal positioned for appending.
#[derive(Debug)]
pub struct Journal {
    file: File,
    next_seq: u64,
    states: BTreeMap<Digest, KnowledgeState>,
    /// Set after a failed write, which may have left a partial line behind.
    /// Appending further would bury that fragment mid-file, so the journal
    /// refuses writes until it is reopened (which truncates the fragment).
    poisoned: bool,
}

impl Journal {
    /// Opens or creates the journal at `path` and replays it.
    ///
    /// A missing or empty file gets the header. A torn final line is truncated.
    /// Any corruption returns an error and leaves the file unmodified.
    pub fn open(path: &Path) -> Result<(Journal, Replay), JournalError> {
        let mut file = OpenOptions::new()
            .read(true)
            .append(true)
            .create(true)
            .open(path)?;

        let mut bytes = Vec::new();
        file.seek(SeekFrom::Start(0))?;
        file.read_to_end(&mut bytes)?;

        if bytes.is_empty() {
            write_header(&mut file)?;
            return Ok((Journal::new(file, 1, BTreeMap::new()), Replay::default()));
        }

        let Some(last_newline) = bytes.iter().rposition(|&b| b == b'\n') else {
            // No complete line at all. Only a torn header prefix is acceptable.
            if !HEADER.as_bytes().starts_with(&bytes) {
                return Err(corrupt(1, "wrong header"));
            }
            file.set_len(0)?;
            write_header(&mut file)?;
            let replay = Replay {
                torn_tail: true,
                ..Replay::default()
            };
            return Ok((Journal::new(file, 1, BTreeMap::new()), replay));
        };

        let body = &bytes[..last_newline];
        let mut states: BTreeMap<Digest, KnowledgeState> = BTreeMap::new();
        let mut records = 0u64;
        let mut next_seq = 1u64;
        for (index, raw) in body.split(|&b| b == b'\n').enumerate() {
            let line_no = index + 1;
            let line = std::str::from_utf8(raw).map_err(|_| corrupt(line_no, "invalid UTF-8"))?;
            if line_no == 1 {
                if line != HEADER {
                    return Err(corrupt(1, "wrong header"));
                }
                continue;
            }
            let (lesson, from, to) =
                parse_record(line, next_seq).map_err(|reason| corrupt(line_no, reason))?;
            let current = states
                .get(&lesson)
                .copied()
                .unwrap_or(KnowledgeState::Unknown);
            if from != current {
                return Err(corrupt(
                    line_no,
                    format!(
                        "from state {} but replayed state is {}",
                        from.name(),
                        current.name()
                    ),
                ));
            }
            if !from.can_transition(to) {
                return Err(corrupt(
                    line_no,
                    format!("illegal transition {} -> {}", from.name(), to.name()),
                ));
            }
            states.insert(lesson, to);
            records += 1;
            next_seq += 1;
        }

        let torn_tail = last_newline + 1 < bytes.len();
        if torn_tail {
            file.set_len((last_newline + 1) as u64)?;
            file.sync_data()?;
        }

        let replay = Replay {
            states: states.clone(),
            records,
            torn_tail,
        };
        Ok((Journal::new(file, next_seq, states), replay))
    }

    fn new(file: File, next_seq: u64, states: BTreeMap<Digest, KnowledgeState>) -> Journal {
        Journal {
            file,
            next_seq,
            states,
            poisoned: false,
        }
    }

    /// Appends one transition and returns its sequence number.
    ///
    /// The transition must be allowed by [`KnowledgeState::can_transition`], and
    /// `from` must be the lesson's current state (Unknown for a new lesson).
    /// Otherwise nothing is written. The record is written with a single
    /// `write_all`; call [`Journal::sync`] to make it durable.
    pub fn append(
        &mut self,
        lesson: &Digest,
        from: KnowledgeState,
        to: KnowledgeState,
    ) -> Result<u64, JournalError> {
        if self.poisoned {
            return Err(JournalError::Io(io::Error::other(
                "journal refuses writes after a failed write; reopen it",
            )));
        }
        if !from.can_transition(to) {
            return Err(JournalError::IllegalTransition { from, to });
        }
        let current = self
            .states
            .get(lesson)
            .copied()
            .unwrap_or(KnowledgeState::Unknown);
        if from != current {
            return Err(JournalError::IllegalTransition { from, to });
        }

        let seq = self.next_seq;
        let line = format!(
            "T {} {} {} {}\n",
            lesson.to_hex(),
            from.name(),
            to.name(),
            seq
        );
        if let Err(e) = self.file.write_all(line.as_bytes()) {
            self.poisoned = true;
            return Err(JournalError::Io(e));
        }
        self.states.insert(*lesson, to);
        self.next_seq += 1;
        Ok(seq)
    }

    /// Flushes written records to stable storage (`sync_data`).
    pub fn sync(&mut self) -> Result<(), JournalError> {
        self.file.sync_data()?;
        Ok(())
    }
}

fn write_header(file: &mut File) -> Result<(), JournalError> {
    file.write_all(format!("{HEADER}\n").as_bytes())?;
    file.sync_data()?;
    Ok(())
}

/// Parses one record line. `expected_seq` is the sequence number it must carry.
fn parse_record(
    line: &str,
    expected_seq: u64,
) -> Result<(Digest, KnowledgeState, KnowledgeState), String> {
    let fields: Vec<&str> = line.split(' ').collect();
    let (lesson_text, from_text, to_text, seq_text) = match fields.as_slice() {
        ["T", lesson, from, to, seq] => (*lesson, *from, *to, *seq),
        _ => return Err("expected `T <lesson> <from> <to> <seq>`".into()),
    };
    let lesson = Digest::from_hex(lesson_text)
        .filter(|d| d.to_hex() == lesson_text)
        .ok_or_else(|| format!("bad lesson digest {lesson_text:?}"))?;
    let from =
        KnowledgeState::parse(from_text).ok_or_else(|| format!("unknown state {from_text:?}"))?;
    let to = KnowledgeState::parse(to_text).ok_or_else(|| format!("unknown state {to_text:?}"))?;
    if seq_text != expected_seq.to_string() {
        return Err(format!(
            "sequence {seq_text} where {expected_seq} was expected"
        ));
    }
    Ok((lesson, from, to))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    /// A unique scratch directory under the system temp dir, removed on drop.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(test: &str) -> Self {
            let dir = std::env::temp_dir()
                .join(format!("sml-tutor-journal-{test}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).expect("create scratch dir");
            Scratch(dir)
        }

        fn journal(&self) -> PathBuf {
            self.0.join("journal.txt")
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn lesson(i: u32) -> Digest {
        Digest::of(format!("lesson-{i}").as_bytes())
    }

    fn raw_append(path: &Path, text: &str) {
        let mut f = OpenOptions::new().append(true).open(path).unwrap();
        f.write_all(text.as_bytes()).unwrap();
    }

    #[test]
    fn roundtrip_replays_many_interleaved_transitions() {
        let scratch = Scratch::new("roundtrip");
        let path = scratch.journal();
        let path_states = [
            KnowledgeState::Identified,
            KnowledgeState::Acquiring,
            KnowledgeState::Learned,
            KnowledgeState::Validating,
            KnowledgeState::Certified,
            KnowledgeState::Consolidated,
            KnowledgeState::NeedsRevision,
            KnowledgeState::Validating,
            KnowledgeState::Certified,
        ];
        let lessons: Vec<Digest> = (0..5).map(lesson).collect();

        {
            let (mut j, replay) = Journal::open(&path).unwrap();
            assert_eq!(replay, Replay::default());
            let mut seq = 0;
            for (step, to) in path_states.iter().enumerate() {
                for l in &lessons {
                    let from = if step == 0 {
                        KnowledgeState::Unknown
                    } else {
                        path_states[step - 1]
                    };
                    seq = j.append(l, from, *to).unwrap();
                }
            }
            assert_eq!(seq, (path_states.len() * lessons.len()) as u64);
            j.sync().unwrap();
        }

        let text = fs::read_to_string(&path).unwrap();
        assert!(text.starts_with("SML_TUTOR_JOURNAL_V1\n"));
        assert_eq!(text.lines().count(), 1 + 9 * 5);

        let (mut j, replay) = Journal::open(&path).unwrap();
        assert!(!replay.torn_tail);
        assert_eq!(replay.records, 45);
        assert_eq!(replay.states.len(), 5);
        for l in &lessons {
            assert_eq!(replay.states[l], KnowledgeState::Certified);
        }
        // Continues the sequence after reopening.
        assert_eq!(
            j.append(
                &lessons[0],
                KnowledgeState::Certified,
                KnowledgeState::Consolidated
            )
            .unwrap(),
            46
        );
    }

    #[test]
    fn torn_tail_is_ignored_truncated_and_appending_continues() {
        let scratch = Scratch::new("torn");
        let path = scratch.journal();
        let l = lesson(1);
        {
            let (mut j, _) = Journal::open(&path).unwrap();
            j.append(&l, KnowledgeState::Unknown, KnowledgeState::Identified)
                .unwrap();
            j.append(&l, KnowledgeState::Identified, KnowledgeState::Acquiring)
                .unwrap();
            j.append(&l, KnowledgeState::Acquiring, KnowledgeState::Learned)
                .unwrap();
        }
        let complete_len = fs::metadata(&path).unwrap().len();
        raw_append(&path, &format!("T {} learned val", l.to_hex()));
        assert!(fs::metadata(&path).unwrap().len() > complete_len);

        let (mut j, replay) = Journal::open(&path).unwrap();
        assert!(replay.torn_tail);
        assert_eq!(replay.records, 3);
        assert_eq!(fs::metadata(&path).unwrap().len(), complete_len);
        assert_eq!(replay.states[&l], KnowledgeState::Learned);

        let seq = j
            .append(&l, KnowledgeState::Learned, KnowledgeState::Validating)
            .unwrap();
        assert_eq!(seq, 4);
        j.sync().unwrap();
        drop(j);

        let (_, replay) = Journal::open(&path).unwrap();
        assert!(!replay.torn_tail);
        assert_eq!(replay.records, 4);
        assert_eq!(replay.states[&l], KnowledgeState::Validating);
    }

    #[test]
    fn torn_header_prefix_is_rewritten() {
        let scratch = Scratch::new("torn_header");
        let path = scratch.journal();
        fs::write(&path, "SML_TUT").unwrap();
        let (_, replay) = Journal::open(&path).unwrap();
        assert!(replay.torn_tail);
        assert_eq!(replay.records, 0);
        assert_eq!(fs::read_to_string(&path).unwrap(), "SML_TUTOR_JOURNAL_V1\n");
    }

    #[test]
    fn corrupt_middle_line_is_rejected_with_its_line_number() {
        let scratch = Scratch::new("corrupt_middle");
        let path = scratch.journal();
        let l = lesson(2);
        let content = format!(
            "SML_TUTOR_JOURNAL_V1\nT {} unknown identified 1\nT zz unknown identified 2\nT {} identified acquiring 3\n",
            l.to_hex(),
            l.to_hex()
        );
        fs::write(&path, &content).unwrap();

        match Journal::open(&path) {
            Err(JournalError::Corrupt { line, .. }) => assert_eq!(line, 3),
            other => panic!("expected Corrupt at line 3, got {other:?}"),
        }
        // Fail closed: the file is left exactly as it was.
        assert_eq!(fs::read_to_string(&path).unwrap(), content);
    }

    #[test]
    fn wrong_header_is_rejected() {
        let scratch = Scratch::new("wrong_header");
        let path = scratch.journal();
        fs::write(&path, "NOT_A_JOURNAL\n").unwrap();
        match Journal::open(&path) {
            Err(JournalError::Corrupt { line: 1, .. }) => {}
            other => panic!("expected Corrupt at line 1, got {other:?}"),
        }
    }

    #[test]
    fn non_increasing_sequence_is_rejected() {
        let scratch = Scratch::new("seq");
        let path = scratch.journal();
        let l = lesson(3);
        let content = format!(
            "SML_TUTOR_JOURNAL_V1\nT {0} unknown identified 1\nT {0} identified acquiring 1\n",
            l.to_hex()
        );
        fs::write(&path, content).unwrap();
        match Journal::open(&path) {
            Err(JournalError::Corrupt { line: 3, .. }) => {}
            other => panic!("expected Corrupt at line 3, got {other:?}"),
        }
    }

    #[test]
    fn illegal_transition_is_rejected_on_append() {
        let scratch = Scratch::new("illegal_append");
        let path = scratch.journal();
        let l = lesson(4);
        let (mut j, _) = Journal::open(&path).unwrap();

        // Unknown -> Learned skips the state machine.
        match j.append(&l, KnowledgeState::Unknown, KnowledgeState::Learned) {
            Err(JournalError::IllegalTransition { from, to }) => {
                assert_eq!(from, KnowledgeState::Unknown);
                assert_eq!(to, KnowledgeState::Learned);
            }
            other => panic!("expected IllegalTransition, got {other:?}"),
        }
        // Wrong `from` for the lesson's current state is refused too.
        j.append(&l, KnowledgeState::Unknown, KnowledgeState::Identified)
            .unwrap();
        assert!(matches!(
            j.append(&l, KnowledgeState::Unknown, KnowledgeState::Identified),
            Err(JournalError::IllegalTransition { .. })
        ));
        drop(j);

        // Nothing illegal reached the file.
        let (_, replay) = Journal::open(&path).unwrap();
        assert_eq!(replay.records, 1);
        assert_eq!(replay.states[&l], KnowledgeState::Identified);
    }

    #[test]
    fn illegal_transition_is_rejected_on_replay() {
        let scratch = Scratch::new("illegal_replay");
        let path = scratch.journal();
        let l = lesson(5);
        let illegal = format!("SML_TUTOR_JOURNAL_V1\nT {} unknown learned 1\n", l.to_hex());
        fs::write(&path, illegal).unwrap();
        match Journal::open(&path) {
            Err(JournalError::Corrupt { line: 2, reason }) => {
                assert!(reason.contains("illegal transition"), "{reason}");
            }
            other => panic!("expected Corrupt at line 2, got {other:?}"),
        }

        // A `from` that does not match the replayed state is also refused.
        let mismatch = format!(
            "SML_TUTOR_JOURNAL_V1\nT {0} unknown identified 1\nT {0} unknown identified 2\n",
            l.to_hex()
        );
        fs::write(&path, mismatch).unwrap();
        match Journal::open(&path) {
            Err(JournalError::Corrupt { line: 3, .. }) => {}
            other => panic!("expected Corrupt at line 3, got {other:?}"),
        }
    }

    #[test]
    fn interrupted_lists_in_flight_lessons_only() {
        let scratch = Scratch::new("interrupted");
        let path = scratch.journal();
        let acquiring = lesson(10);
        let validating = lesson(11);
        let certified = lesson(12);
        {
            let (mut j, _) = Journal::open(&path).unwrap();
            j.append(
                &acquiring,
                KnowledgeState::Unknown,
                KnowledgeState::Identified,
            )
            .unwrap();
            j.append(
                &acquiring,
                KnowledgeState::Identified,
                KnowledgeState::Acquiring,
            )
            .unwrap();
            j.append(
                &validating,
                KnowledgeState::Unknown,
                KnowledgeState::Identified,
            )
            .unwrap();
            j.append(
                &validating,
                KnowledgeState::Identified,
                KnowledgeState::Acquiring,
            )
            .unwrap();
            j.append(
                &validating,
                KnowledgeState::Acquiring,
                KnowledgeState::Learned,
            )
            .unwrap();
            j.append(
                &validating,
                KnowledgeState::Learned,
                KnowledgeState::Validating,
            )
            .unwrap();
            j.append(
                &certified,
                KnowledgeState::Unknown,
                KnowledgeState::Identified,
            )
            .unwrap();
            j.append(
                &certified,
                KnowledgeState::Identified,
                KnowledgeState::Acquiring,
            )
            .unwrap();
            j.append(
                &certified,
                KnowledgeState::Acquiring,
                KnowledgeState::Learned,
            )
            .unwrap();
            j.append(
                &certified,
                KnowledgeState::Learned,
                KnowledgeState::Validating,
            )
            .unwrap();
            j.append(
                &certified,
                KnowledgeState::Validating,
                KnowledgeState::Certified,
            )
            .unwrap();
        }
        let (_, replay) = Journal::open(&path).unwrap();
        let mut expected = vec![acquiring, validating];
        expected.sort();
        assert_eq!(replay.interrupted(), expected.clone());
        // Validating is in flight; the certified lesson is not.
        assert!(!replay.interrupted().contains(&certified));
        assert_eq!(replay.states[&certified], KnowledgeState::Certified);
    }
}
