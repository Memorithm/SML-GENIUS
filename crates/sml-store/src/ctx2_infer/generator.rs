//! CTX2-FACTS-v1 generator (prereg section 3). Deterministic, model-free.

use std::collections::BTreeSet;

use sml_core::elastic_ngram_paged_sha::sha256;

use super::{first_u64_le, Ctx2InferError, SplitMix64};

/// Turns per session.
pub const TURNS: u32 = 48;
/// Queries per session.
pub const QUERIES: usize = 6;
/// Sessions per seed (dev and test).
pub const SESSIONS_PER_SEED: u32 = 64;
/// One-based distance buckets `[lo, hi]` (prereg section 3).
pub const BUCKETS: [(u32, u32); 5] = [(1, 4), (5, 8), (9, 16), (17, 32), (33, 48)];
/// Domain prefix for per-session generator seeds.
pub const SESSION_SEED_DOMAIN: &[u8] = b"CTX2-FACTS-v1/session";

/// Dev or test vocabulary (disjoint lists).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Split {
    Dev,
    Test,
}

impl Split {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Dev => "dev",
            Self::Test => "test",
        }
    }

    #[must_use]
    pub const fn entities(self) -> &'static [&'static str] {
        match self {
            Self::Dev => &DEV_ENTITIES,
            Self::Test => &TEST_ENTITIES,
        }
    }

    #[must_use]
    pub const fn attributes(self) -> &'static [&'static str] {
        match self {
            Self::Dev => &DEV_ATTRIBUTES,
            Self::Test => &TEST_ATTRIBUTES,
        }
    }
}

pub const DEV_ENTITIES: [&str; 12] = [
    "Alder", "Birch", "Cedar", "Dahlia", "Elm", "Fern", "Gorse", "Hazel", "Iris", "Juniper",
    "Kale", "Laurel",
];
pub const TEST_ENTITIES: [&str; 12] = [
    "Maple", "Nettle", "Oak", "Poppy", "Quince", "Rowan", "Sage", "Tansy", "Umber", "Violet",
    "Willow", "Yarrow",
];
pub const DEV_ATTRIBUTES: [&str; 8] = [
    "code", "badge", "locker", "room", "pin", "desk", "shelf", "gate",
];
pub const TEST_ATTRIBUTES: [&str; 8] = [
    "ticket", "vault", "cabin", "berth", "token", "slot", "bay", "dock",
];

/// Kind of a history turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnKind {
    /// First assignment of a queried key.
    Set,
    /// Overwrite of a queried key (its latest assignment).
    Update,
    /// Fact about a key that is never queried in this session.
    Distractor,
}

impl TurnKind {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Set => "set",
            Self::Update => "update",
            Self::Distractor => "distractor",
        }
    }
}

/// One history turn; `bytes` is the exact CONTEXT-2 segment payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Turn {
    /// One-based turn number.
    pub number: u32,
    pub kind: TurnKind,
    pub entity: &'static str,
    pub attribute: &'static str,
    pub value: String,
    pub bytes: Vec<u8>,
}

/// One query with its gold and one-based distance `d = T − j + 1`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Query {
    pub index: u8,
    pub entity: &'static str,
    pub attribute: &'static str,
    pub gold: String,
    /// Turn `j` of the latest assignment of the queried key.
    pub answer_turn: u32,
    pub distance: u32,
    /// Index into [`BUCKETS`].
    pub bucket: usize,
    pub text: String,
}

impl Query {
    /// Answer retained by a `k`-turn window iff `d ≤ k`.
    #[must_use]
    pub const fn retained(&self, k: u32) -> bool {
        self.distance <= k
    }
}

/// One generated session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub seed: u64,
    pub split: Split,
    pub index: u32,
    pub turns: Vec<Turn>,
    pub queries: Vec<Query>,
}

/// Exact bytes of one turn. The trailing `\n` is part of the segment so that
/// in-order concatenation is unambiguous.
#[must_use]
pub fn turn_text(number: u32, attribute: &str, entity: &str, value: &str) -> String {
    format!("Note {number}: The {attribute} of {entity} is {value}.\n")
}

/// Exact query text (prereg section 3).
#[must_use]
pub fn query_text(attribute: &str, entity: &str) -> String {
    format!("What is the {attribute} of {entity}? Answer with the number only.")
}

/// One-based distance of answer turn `j` (prereg section 3).
#[must_use]
pub const fn distance(answer_turn: u32) -> u32 {
    TURNS - answer_turn + 1
}

/// Per-session generator seed: first 8 bytes LE of
/// `SHA-256(SESSION_SEED_DOMAIN || split || seed_le || index_le)`.
#[must_use]
pub fn session_seed(seed: u64, split: Split, index: u32) -> u64 {
    let mut input = SESSION_SEED_DOMAIN.to_vec();
    input.push(b'/');
    input.extend_from_slice(split.name().as_bytes());
    input.extend_from_slice(&seed.to_le_bytes());
    input.extend_from_slice(&index.to_le_bytes());
    first_u64_le(&sha256(&input))
}

/// Generate one session deterministically.
pub fn generate_session(seed: u64, split: Split, index: u32) -> Result<Session, Ctx2InferError> {
    let mut rng = SplitMix64::new(session_seed(seed, split, index));
    let entities = split.entities();
    let attributes = split.attributes();

    // 1. Bucket per query: all five buckets plus one drawn twice, then shuffled.
    let mut buckets: Vec<usize> = (0..BUCKETS.len()).collect();
    buckets.push(rng.below(BUCKETS.len() as u64) as usize);
    shuffle(&mut rng, &mut buckets);

    // 2. Distinct answer turns, distance uniform inside each bucket.
    let mut used_turns = BTreeSet::new();
    let mut answer_turns = Vec::with_capacity(QUERIES);
    for &b in &buckets {
        let (lo, hi) = BUCKETS[b];
        let j = loop {
            let d = lo + rng.below(u64::from(hi - lo + 1)) as u32;
            let j = TURNS - d + 1;
            if used_turns.insert(j) {
                break j;
            }
        };
        answer_turns.push(j);
    }

    // 3. Distinct queried keys; the rest is the distractor pool.
    let mut keys: Vec<(usize, usize)> = (0..entities.len())
        .flat_map(|e| (0..attributes.len()).map(move |a| (e, a)))
        .collect();
    shuffle(&mut rng, &mut keys);
    let (queried, distractors) = keys.split_at(QUERIES);
    if distractors.is_empty() {
        return Err(Ctx2InferError::Generator("empty distractor pool"));
    }

    // 4. Optional prior Set for each queried key, strictly before its answer turn.
    let mut slots: Vec<Option<(TurnKind, (usize, usize))>> = vec![None; TURNS as usize];
    for (q, &j) in answer_turns.iter().enumerate() {
        slots[(j - 1) as usize] = Some((TurnKind::Update, queried[q]));
    }
    for (q, &j) in answer_turns.iter().enumerate() {
        let free: Vec<u32> = (1..j)
            .filter(|t| slots[(*t - 1) as usize].is_none())
            .collect();
        if rng.below(2) == 1 && !free.is_empty() {
            let p = free[rng.below(free.len() as u64) as usize];
            slots[(p - 1) as usize] = Some((TurnKind::Set, queried[q]));
        } else {
            slots[(j - 1) as usize] = Some((TurnKind::Set, queried[q]));
        }
    }

    // 5. Distractors fill the remaining turns.
    for slot in &mut slots {
        if slot.is_none() {
            let key = distractors[rng.below(distractors.len() as u64) as usize];
            *slot = Some((TurnKind::Distractor, key));
        }
    }

    // 6. Values: 4-digit strings, unique within the session.
    let mut seen_values = BTreeSet::new();
    let mut turns = Vec::with_capacity(TURNS as usize);
    for (i, slot) in slots.iter().enumerate() {
        let (kind, (e, a)) = slot.ok_or(Ctx2InferError::Generator("unfilled slot"))?;
        let value = loop {
            let v = rng.below(10_000);
            if seen_values.insert(v) {
                break format!("{v:04}");
            }
        };
        let number = i as u32 + 1;
        let bytes = turn_text(number, attributes[a], entities[e], &value).into_bytes();
        turns.push(Turn {
            number,
            kind,
            entity: entities[e],
            attribute: attributes[a],
            value,
            bytes,
        });
    }

    let queries = answer_turns
        .iter()
        .zip(&buckets)
        .enumerate()
        .map(|(q, (&j, &b))| {
            let turn = &turns[(j - 1) as usize];
            Query {
                index: q as u8,
                entity: turn.entity,
                attribute: turn.attribute,
                gold: turn.value.clone(),
                answer_turn: j,
                distance: distance(j),
                bucket: b,
                text: query_text(turn.attribute, turn.entity),
            }
        })
        .collect();

    Ok(Session {
        seed,
        split,
        index,
        turns,
        queries,
    })
}

/// All sessions of one seed.
pub fn generate_seed(seed: u64, split: Split) -> Result<Vec<Session>, Ctx2InferError> {
    (0..SESSIONS_PER_SEED)
        .map(|i| generate_session(seed, split, i))
        .collect()
}

/// Canonical session file (one line per turn and query), the pinned artifact.
#[must_use]
pub fn session_file_bytes(sessions: &[Session]) -> Vec<u8> {
    let mut out = String::from("CTX2-FACTS-v1\n");
    for s in sessions {
        out.push_str(&format!(
            "session\t{}\t{}\t{:#018x}\n",
            s.split.name(),
            s.index,
            s.seed
        ));
        for t in &s.turns {
            out.push_str(&format!(
                "turn\t{}\t{}\t{}",
                t.number,
                t.kind.name(),
                String::from_utf8_lossy(&t.bytes)
            ));
        }
        for q in &s.queries {
            out.push_str(&format!(
                "query\t{}\t{}\t{}\t{}\t{}\t{}\n",
                q.index, q.answer_turn, q.distance, q.bucket, q.gold, q.text
            ));
        }
    }
    out.into_bytes()
}

/// SHA-256 of [`session_file_bytes`].
#[must_use]
pub fn session_file_sha256(sessions: &[Session]) -> [u8; 32] {
    sha256(&session_file_bytes(sessions))
}

fn shuffle<T>(rng: &mut SplitMix64, items: &mut [T]) {
    for i in (1..items.len()).rev() {
        let j = rng.below(i as u64 + 1) as usize;
        items.swap(i, j);
    }
}
