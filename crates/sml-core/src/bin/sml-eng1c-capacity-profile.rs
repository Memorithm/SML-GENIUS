//! ENG-1c/CAP1: bounded TRAIN-only exact context cardinality scanner.
//!
//! Counts reverse-suffix context identities and distinct (context,next-byte)
//! pairs. It does NOT train an N-gram, calculate probabilities, read a heldout
//! scoring stream, or admit a model above its declared maximum event budget.
//! Corpus provenance is checked by the CI downloader, not this scanner.

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;

use sml_core::{
    anf_char::{ChunkLayout, LANES},
    elastic_ngram_trie::MAX_TRIE_EVENTS,
};

const SCAN_EVENT_CEILING: usize = 650_000;
const MAX_NODES: usize = 250_000;
const MAX_PAIRS: usize = 500_000;
const DEPTH_LADDER: [usize; 8] = [0, 2, 4, 7, 12, 16, 24, 32];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Caps {
    nodes: usize,
    pairs: usize,
}

impl Default for Caps {
    fn default() -> Self {
        Self {
            nodes: MAX_NODES,
            pairs: MAX_PAIRS,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScanStatus {
    Complete,
    NodeStop,
    PairStop,
}

impl ScanStatus {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Complete => "SCAN_COMPLETE",
            Self::NodeStop => "NODE_CAPACITY_STOP",
            Self::PairStop => "PAIR_CAPACITY_STOP",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ScanReport {
    status: ScanStatus,
    max_depth: usize,
    desired_events: usize,
    fully_processed_events: usize,
    contexts_stored: usize,
    follower_pairs_stored: usize,
    /// First incomplete event location, otherwise not applicable.
    interrupted_at: Option<(usize, usize)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScanError {
    InvalidCorpus,
    InvalidDepth,
    InvalidLimits,
    SourceTooLarge,
    CounterOverflow,
}

impl std::fmt::Display for ScanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl Error for ScanError {}

fn inspect(train: &[u8], max_depth: usize, caps: Caps) -> Result<ScanReport, ScanError> {
    if !DEPTH_LADDER.contains(&max_depth) {
        return Err(ScanError::InvalidDepth);
    }
    if caps.nodes == 0 || caps.nodes > MAX_NODES || caps.pairs == 0 || caps.pairs > MAX_PAIRS {
        return Err(ScanError::InvalidLimits);
    }
    let layout = ChunkLayout::new(train.len()).map_err(|_| ScanError::InvalidCorpus)?;
    let desired_events = LANES
        .checked_mul(layout.chunk - 1)
        .ok_or(ScanError::CounterOverflow)?;
    if desired_events > SCAN_EVENT_CEILING {
        return Err(ScanError::SourceTooLarge);
    }

    // IDs are deterministic and exact under ordered full-key equality.
    // This is not a lossy fingerprint and is not a Witten-Bell histogram.
    let mut edges = BTreeMap::<(u32, u8), u32>::new();
    let mut pairs = BTreeSet::<(u32, u8)>::new();
    let mut nodes = 1usize; // root, depth 0
    let mut fully_processed_events = 0usize;

    for lane in 0..LANES {
        let chunk = &train[lane * layout.chunk..(lane + 1) * layout.chunk];
        for t in 1..chunk.len() {
            let target = chunk[t];
            let mut parent = 0u32;
            let mut status = None;
            // The root's next-byte follower is recorded for every event.
            // Then traverse newest-to-oldest full context exactly as ENG-1b.
            for older in std::iter::once(None)
                .chain(chunk[..t].iter().rev().take(max_depth).map(|&b| Some(b)))
            {
                if let Some(byte) = older {
                    let child = match edges.get(&(parent, byte)) {
                        Some(&found) => found,
                        None => {
                            if nodes >= caps.nodes {
                                status = Some(ScanStatus::NodeStop);
                                break;
                            }
                            let next =
                                u32::try_from(nodes).map_err(|_| ScanError::CounterOverflow)?;
                            edges.insert((parent, byte), next);
                            nodes += 1;
                            next
                        }
                    };
                    parent = child;
                }
                let pair = (parent, target);
                if !pairs.contains(&pair) {
                    if pairs.len() >= caps.pairs {
                        status = Some(ScanStatus::PairStop);
                        break;
                    }
                    pairs.insert(pair);
                }
            }
            if let Some(status) = status {
                return Ok(ScanReport {
                    status,
                    max_depth,
                    desired_events,
                    fully_processed_events,
                    contexts_stored: nodes,
                    follower_pairs_stored: pairs.len(),
                    interrupted_at: Some((lane, t)),
                });
            }
            fully_processed_events += 1;
        }
    }

    Ok(ScanReport {
        status: ScanStatus::Complete,
        max_depth,
        desired_events,
        fully_processed_events,
        contexts_stored: nodes,
        follower_pairs_stored: pairs.len(),
        interrupted_at: None,
    })
}

#[derive(Clone, Copy)]
struct FrozenCorpus {
    label: &'static str,
    train_bytes: usize,
    train_events: usize,
}

fn frozen(label: &str) -> Option<FrozenCorpus> {
    match label {
        "FR" => Some(FrozenCorpus {
            label: "FR",
            train_bytes: 598_284,
            train_events: 598_208,
        }),
        "EN" => Some(FrozenCorpus {
            label: "EN",
            train_bytes: 590_360,
            train_events: 590_272,
        }),
        _ => None,
    }
}

fn vmhwm_kib() -> Result<u64, Box<dyn Error>> {
    let status = std::fs::read_to_string("/proc/self/status")?;
    let found = status
        .lines()
        .find(|line| line.starts_with("VmHWM:"))
        .ok_or("No Linux VmHWM field")?;
    let mut tokens = found.split_whitespace();
    if tokens.next() != Some("VmHWM:") {
        return Err("Malformed VmHWM field".into());
    }
    let value = tokens.next().ok_or("Missing VmHWM value")?.parse::<u64>()?;
    if value == 0 || tokens.next() != Some("kB") || tokens.next().is_some() {
        return Err("Invalid VmHWM sample".into());
    }
    Ok(value)
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = std::env::args().collect::<Vec<_>>();
    if args.len() != 4 {
        return Err("usage: sml-eng1c-capacity-profile FR|EN depth <frozen-TRAIN-file>".into());
    }
    let corpus = frozen(&args[1]).ok_or("Unknown frozen corpus label")?;
    let depth = args[2].parse::<usize>()?;
    if !DEPTH_LADDER.contains(&depth) {
        return Err("Depth is not preregistered".into());
    }
    let bytes = std::fs::read(&args[3])?;
    if bytes.len() != corpus.train_bytes {
        return Err("Frozen TRAIN source byte count mismatch".into());
    }
    let result = inspect(&bytes, depth, Caps::default())?;
    if result.desired_events != corpus.train_events {
        return Err("Frozen TRAIN target-event denominator mismatch".into());
    }
    let memory_peak = vmhwm_kib()?;
    let (lane, t) = result
        .interrupted_at
        .map(|(lane, t)| (lane.to_string(), t.to_string()))
        .unwrap_or_else(|| ("none".into(), "none".into()));

    // A complete scan is still NOT a model admission when the 500000-event
    // trained-model limit is breached. Never call .train() on that corpus.
    let model_event_gate = if result.desired_events <= MAX_TRIE_EVENTS {
        "PASS"
    } else {
        "STOP"
    };
    println!(
        "ENG1C_CAPACITY,language={},depth={},status={},source_train_bytes={},desired_events={},processed_complete_events={},nodes={},follower_pairs={},stopped_lane={},stopped_position={},model_event_gate={},model_event_max={},vmhwm_process_kib={},integrity=PASS",
        corpus.label,
        result.max_depth,
        result.status.as_str(),
        bytes.len(),
        result.desired_events,
        result.fully_processed_events,
        result.contexts_stored,
        result.follower_pairs_stored,
        lane,
        t,
        model_event_gate,
        MAX_TRIE_EVENTS,
        memory_peak
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sml_core::elastic_ngram_trie::{ElasticNGramTrie, TrieLimits};

    fn fixture() -> Vec<u8> {
        (0..LANES)
            .flat_map(|lane| {
                (0..40usize).map(move |t| ((lane * 17 + t * 7 + (t % 3) * 11) % 23) as u8)
            })
            .collect()
    }

    #[test]
    fn oracle_all_predeclared_depths_match_actual_immutable_eng1b_fits() {
        let corpus = fixture();
        for depth in DEPTH_LADDER {
            let candidate = inspect(&corpus, depth, Caps::default()).unwrap();
            let actual = ElasticNGramTrie::train(
                &corpus,
                TrieLimits {
                    max_depth: depth,
                    ..TrieLimits::default()
                },
            )
            .unwrap();
            assert_eq!(candidate.status, ScanStatus::Complete);
            assert_eq!(candidate.fully_processed_events, LANES * 39);
            assert_eq!(candidate.interrupted_at, None);
            assert_eq!(candidate.contexts_stored, actual.accounting().nodes);
            assert_eq!(
                candidate.follower_pairs_stored,
                actual.accounting().follower_pairs
            );
        }
    }

    #[test]
    fn bounded_stop_preserves_complete_event_count_and_first_interrupted_position() {
        let corpus = fixture();
        let nodes = inspect(
            &corpus,
            12,
            Caps {
                nodes: 1,
                pairs: 100,
            },
        )
        .unwrap();
        assert_eq!(nodes.status, ScanStatus::NodeStop);
        assert_eq!(nodes.fully_processed_events, 0);
        assert_eq!(nodes.interrupted_at, Some((0, 1)));
        assert_eq!(nodes.contexts_stored, 1);

        let pairs = inspect(&corpus, 0, Caps { nodes: 1, pairs: 1 }).unwrap();
        assert_eq!(pairs.status, ScanStatus::PairStop);
        assert!(pairs.fully_processed_events < LANES * 39);
        assert_eq!(pairs.follower_pairs_stored, 1);
        assert!(pairs.interrupted_at.is_some());
    }

    #[test]
    fn failures_are_typed_and_do_not_publish_partial_model() {
        let corpus = fixture();
        assert_eq!(
            inspect(&corpus, 33, Caps::default()),
            Err(ScanError::InvalidDepth)
        );
        assert_eq!(
            inspect(&corpus, 3, Caps::default()),
            Err(ScanError::InvalidDepth)
        );
        assert_eq!(
            inspect(&corpus, 12, Caps { nodes: 0, pairs: 3 }),
            Err(ScanError::InvalidLimits)
        );
        assert_eq!(
            inspect(&[0; LANES], 7, Caps::default()),
            Err(ScanError::InvalidCorpus)
        );
        assert_eq!(
            inspect(&vec![7u8; LANES * 10500], 0, Caps::default()),
            Err(ScanError::SourceTooLarge)
        );
        assert_eq!(frozen("fr").map(|x| x.label), None);
    }
}
