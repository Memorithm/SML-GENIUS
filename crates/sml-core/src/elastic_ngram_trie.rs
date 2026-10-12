//! ENG-1b: lossless reverse-suffix trie for immutable Witten-Bell byte models.
//!
//! Child (parent, older_byte) references share entire suffix prefixes. Each
//! node keeps its own exact next-byte histogram. The full 256-way posterior
//! matches ENG-1a at the same explicitly requested depth, without hashes,
//! truncation, new model semantics or resource actuation.
//!
//! This is a standalone SML reference, not an ElasticXxx controller.

use std::collections::{btree_map::Entry, BTreeMap};

use crate::{
    anf_char::{ChunkLayout, LANES},
    char_baselines::ByteScore,
    elastic_ngram::DepthTransition,
    elastic_ngram_long::{
        MAX_LONG_CONTEXT_DEPTH, MAX_LONG_CONTEXT_EVENTS, MAX_LONG_CONTEXT_KEYS,
        MAX_LONG_FOLLOWER_PAIRS,
    },
};

pub const MAX_TRIE_DEPTH: usize = MAX_LONG_CONTEXT_DEPTH;
pub const MAX_TRIE_NODES: usize = MAX_LONG_CONTEXT_KEYS;
pub const MAX_TRIE_EDGES: usize = MAX_LONG_CONTEXT_KEYS;
pub const MAX_TRIE_FOLLOWER_PAIRS: usize = MAX_LONG_FOLLOWER_PAIRS;
pub const MAX_TRIE_EVENTS: usize = MAX_LONG_CONTEXT_EVENTS;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TrieLimits {
    pub max_depth: usize,
    pub max_events: usize,
    /// Includes the depth-0 root.
    pub max_nodes: usize,
    pub max_edges: usize,
    pub max_follower_pairs: usize,
}

impl Default for TrieLimits {
    fn default() -> Self {
        Self {
            max_depth: 12,
            max_events: MAX_TRIE_EVENTS,
            max_nodes: MAX_TRIE_NODES,
            max_edges: MAX_TRIE_EDGES,
            max_follower_pairs: MAX_TRIE_FOLLOWER_PAIRS,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrieError {
    InvalidLimits,
    InvalidCorpus,
    TooManyEvents,
    TooManyNodes,
    TooManyEdges,
    TooManyFollowerPairs,
    NodeIdOverflow,
    CounterOverflow,
    InvalidDepth,
    BrokenTrie,
}

impl std::fmt::Display for TrieError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for TrieError {}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct TrieNode {
    depth: u8,
    total: u32,
    /// Ordered iteration is essential for bit-exact Witten-Bell parity.
    followers: BTreeMap<u8, u32>,
}

impl TrieNode {
    fn observe(
        &mut self,
        byte: u8,
        follower_pairs: &mut usize,
        max_pairs: usize,
    ) -> Result<(), TrieError> {
        let next_total = self
            .total
            .checked_add(1)
            .ok_or(TrieError::CounterOverflow)?;
        match self.followers.entry(byte) {
            Entry::Occupied(mut entry) => {
                let next_count = entry
                    .get()
                    .checked_add(1)
                    .ok_or(TrieError::CounterOverflow)?;
                *entry.get_mut() = next_count;
            }
            Entry::Vacant(entry) => {
                if *follower_pairs >= max_pairs {
                    return Err(TrieError::TooManyFollowerPairs);
                }
                *follower_pairs += 1;
                entry.insert(1);
            }
        }
        self.total = next_total;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TrieAccounting {
    pub training_events: usize,
    pub nodes: usize,
    pub edges: usize,
    pub follower_pairs: usize,
    /// u32 total per node + u32 next-byte counter per observed pair.
    pub logical_counter_bytes: usize,
    /// Edge parent u32 + child u32, excluding BTreeMap internals.
    pub logical_edge_ref_bytes: usize,
    /// Edge labels plus distinct follower byte labels.
    pub logical_label_bytes: usize,
    /// One logical u8 depth per trie node.
    pub logical_node_depth_bytes: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ElasticNGramTrie {
    max_depth: usize,
    nodes: Vec<TrieNode>,
    /// Parent is an index in nodes; the label is the next OLDER byte.
    edges: BTreeMap<(u32, u8), u32>,
    accounting: TrieAccounting,
}

impl ElasticNGramTrie {
    /// Build an entire private TRAIN snapshot, or return an error with no model.
    pub fn train(bytes: &[u8], limits: TrieLimits) -> Result<Self, TrieError> {
        if limits.max_depth > MAX_TRIE_DEPTH
            || limits.max_events == 0
            || limits.max_events > MAX_TRIE_EVENTS
            || limits.max_nodes == 0
            || limits.max_nodes > MAX_TRIE_NODES
            || limits.max_edges == 0
            || limits.max_edges > MAX_TRIE_EDGES
            || limits.max_follower_pairs == 0
            || limits.max_follower_pairs > MAX_TRIE_FOLLOWER_PAIRS
        {
            return Err(TrieError::InvalidLimits);
        }
        let layout = ChunkLayout::new(bytes.len()).map_err(|_| TrieError::InvalidCorpus)?;
        let events = LANES
            .checked_mul(layout.chunk - 1)
            .ok_or(TrieError::TooManyEvents)?;
        if events > limits.max_events {
            return Err(TrieError::TooManyEvents);
        }

        let mut nodes = vec![TrieNode::default()];
        let mut edges = BTreeMap::<(u32, u8), u32>::new();
        let mut follower_pairs = 0usize;
        let mut edge_count = 0usize;

        for lane in 0..LANES {
            let chunk = &bytes[lane * layout.chunk..(lane + 1) * layout.chunk];
            for t in 1..chunk.len() {
                let target = chunk[t];
                nodes[0].observe(target, &mut follower_pairs, limits.max_follower_pairs)?;
                let mut parent = 0u32;
                for &older_byte in chunk[..t].iter().rev().take(limits.max_depth) {
                    let child = match edges.entry((parent, older_byte)) {
                        Entry::Occupied(entry) => *entry.get(),
                        Entry::Vacant(entry) => {
                            if nodes.len() >= limits.max_nodes {
                                return Err(TrieError::TooManyNodes);
                            }
                            if edge_count >= limits.max_edges {
                                return Err(TrieError::TooManyEdges);
                            }
                            let parent_depth = nodes
                                .get(parent as usize)
                                .ok_or(TrieError::BrokenTrie)?
                                .depth;
                            let depth = parent_depth
                                .checked_add(1)
                                .ok_or(TrieError::NodeIdOverflow)?;
                            if usize::from(depth) > limits.max_depth {
                                return Err(TrieError::BrokenTrie);
                            }
                            let index = u32::try_from(nodes.len())
                                .map_err(|_| TrieError::NodeIdOverflow)?;
                            nodes.push(TrieNode {
                                depth,
                                ..TrieNode::default()
                            });
                            edge_count += 1;
                            entry.insert(index);
                            index
                        }
                    };
                    let child_node = nodes
                        .get_mut(child as usize)
                        .ok_or(TrieError::BrokenTrie)?;
                    child_node.observe(target, &mut follower_pairs, limits.max_follower_pairs)?;
                    parent = child;
                }
            }
        }

        if edge_count.checked_add(1) != Some(nodes.len()) {
            return Err(TrieError::BrokenTrie);
        }
        let counter_items = nodes
            .len()
            .checked_add(follower_pairs)
            .ok_or(TrieError::CounterOverflow)?;
        let logical_counter_bytes = counter_items
            .checked_mul(std::mem::size_of::<u32>())
            .ok_or(TrieError::CounterOverflow)?;
        let logical_edge_ref_bytes = edge_count
            .checked_mul(2 * std::mem::size_of::<u32>())
            .ok_or(TrieError::CounterOverflow)?;
        let logical_label_bytes = edge_count
            .checked_add(follower_pairs)
            .ok_or(TrieError::CounterOverflow)?;
        let accounting = TrieAccounting {
            training_events: events,
            nodes: nodes.len(),
            edges: edge_count,
            follower_pairs,
            logical_counter_bytes,
            logical_edge_ref_bytes,
            logical_label_bytes,
            logical_node_depth_bytes: nodes.len(),
        };
        Ok(Self {
            max_depth: limits.max_depth,
            nodes,
            edges,
            accounting,
        })
    }

    #[must_use]
    pub fn max_depth(&self) -> usize {
        self.max_depth
    }

    #[must_use]
    pub fn accounting(&self) -> TrieAccounting {
        self.accounting
    }

    /// Returns the count of retained contexts for depths 0..=max_depth.
    #[must_use]
    pub fn contexts_by_depth(&self) -> Vec<usize> {
        let mut counts = vec![0usize; self.max_depth + 1];
        for node in &self.nodes {
            counts[usize::from(node.depth)] += 1;
        }
        counts
    }

    /// A depth change is never authorized as an exact resource transition.
    pub fn classify_depth_transition(
        &self,
        current: usize,
        target: usize,
    ) -> Result<DepthTransition, TrieError> {
        if current > self.max_depth || target > self.max_depth {
            return Err(TrieError::InvalidDepth);
        }
        Ok(if current == target {
            DepthTransition::Unchanged
        } else {
            DepthTransition::ModelSemanticsChanged
        })
    }

    /// Exact 256-class probability vector at an explicit model-owned depth.
    /// No allocation on the normal lookup path; caller-supplied depth is
    /// clamped only to available HISTORY, never silently lowered by policy.
    pub fn distribution(&self, history: &[u8], query_depth: usize) -> Result<[f64; 256], TrieError> {
        if query_depth > self.max_depth {
            return Err(TrieError::InvalidDepth);
        }
        let mut path = [0usize; MAX_TRIE_DEPTH + 1];
        let mut available = 1usize;
        let mut parent = 0u32;
        for &byte in history.iter().rev().take(query_depth) {
            let Some(&child) = self.edges.get(&(parent, byte)) else {
                break;
            };
            let node_id = child as usize;
            let node = self.nodes.get(node_id).ok_or(TrieError::BrokenTrie)?;
            if usize::from(node.depth) != available {
                return Err(TrieError::BrokenTrie);
            }
            path[available] = node_id;
            available += 1;
            parent = child;
        }

        let mut probs = [1.0 / 256.0; 256];
        for &node_id in path.iter().take(available) {
            let hist = &self.nodes[node_id];
            if hist.total == 0 || hist.followers.is_empty() {
                return Err(TrieError::BrokenTrie);
            }
            let n = f64::from(hist.total);
            let distinct = hist.followers.len() as f64;
            let scale = distinct / (n + distinct);
            for p in &mut probs {
                *p *= scale;
            }
            for (&byte, &count) in &hist.followers {
                probs[usize::from(byte)] += f64::from(count) / (n + distinct);
            }
        }
        Ok(probs)
    }

    /// Identical lane/denominator contract as ENG-1a and ENG-0.
    pub fn score(&self, bytes: &[u8], query_depth: usize) -> Result<ByteScore, TrieError> {
        if query_depth > self.max_depth {
            return Err(TrieError::InvalidDepth);
        }
        let layout = ChunkLayout::new(bytes.len()).map_err(|_| TrieError::InvalidCorpus)?;
        let (mut bits, mut correct, mut tokens) = (0.0f64, 0u64, 0u64);
        for lane in 0..LANES {
            let chunk = &bytes[lane * layout.chunk..(lane + 1) * layout.chunk];
            for t in 1..chunk.len() {
                let probs = self.distribution(&chunk[..t], query_depth)?;
                let target = usize::from(chunk[t]);
                bits -= probs[target].log2();
                let best = (0..256).fold(0, |prev, candidate| {
                    if probs[candidate] > probs[prev] {
                        candidate
                    } else {
                        prev
                    }
                });
                correct += u64::from(best == target);
                tokens += 1;
            }
        }
        Ok(ByteScore {
            tokens,
            bits_per_byte: bits / tokens as f64,
            accuracy: correct as f64 / tokens as f64,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        char_baselines::NGramModel,
        elastic_ngram::{ElasticNGram, ElasticNGramLimits},
        elastic_ngram_long::{ElasticNGramLong, LongNGramLimits},
    };

    fn fixture(heldout: bool) -> Vec<u8> {
        (0..LANES)
            .flat_map(|lane| {
                (0..40usize).map(move |t| {
                    if heldout {
                        ((lane * 19 + t * 3 + (t % 5) * 23 + 1) % 29) as u8
                    } else {
                        ((lane * 17 + t * 7 + (t % 3) * 11) % 23) as u8
                    }
                })
            })
            .collect()
    }

    fn trie_limits(depth: usize) -> TrieLimits {
        TrieLimits {
            max_depth: depth,
            ..TrieLimits::default()
        }
    }

    #[test]
    fn exact_0_to_32_parity_with_full_suffix_and_legacy_oracles() {
        let train = fixture(false);
        let heldout = fixture(true);
        let trie = ElasticNGramTrie::train(&train, trie_limits(32)).unwrap();
        let long = ElasticNGramLong::train(
            &train,
            LongNGramLimits {
                max_depth: 32,
                ..LongNGramLimits::default()
            },
        )
        .unwrap();
        let old = ElasticNGram::train(
            &train,
            ElasticNGramLimits {
                max_order: 7,
                ..ElasticNGramLimits::default()
            },
        )
        .unwrap();
        assert_eq!(trie.contexts_by_depth(), long.contexts_by_depth());
        assert_eq!(trie.accounting().nodes, long.accounting().contexts);
        assert_eq!(trie.accounting().follower_pairs, long.accounting().follower_pairs);

        let histories = [
            &train[0..24],
            &train[40..60],
            &heldout[0..32],
            &heldout[72..96],
            &[255u8; 32][..],
            &[][..],
        ];
        for depth in 0..=32 {
            for history in histories {
                let actual = trie.distribution(history, depth).unwrap();
                let expected = long.distribution(history, depth).unwrap();
                for y in 0..256 {
                    assert_eq!(
                        actual[y].to_bits(),
                        expected[y].to_bits(),
                        "depth={depth}, label={y}"
                    );
                }
                if depth <= 7 {
                    let eng0 = old.distribution(history, depth).unwrap();
                    let ngram = NGramModel::train(&train, depth);
                    let mut reference = [0.0f64; 256];
                    ngram.distribution(history, &mut reference);
                    for y in 0..256 {
                        assert_eq!(actual[y].to_bits(), eng0[y].to_bits());
                        assert_eq!(actual[y].to_bits(), reference[y].to_bits());
                    }
                }
                assert!(actual.iter().all(|&p| p.is_finite() && p > 0.0));
                assert!((actual.iter().sum::<f64>() - 1.0).abs() < 1.0e-10);
            }
        }
        for depth in [0, 4, 7, 12, 16, 24, 32] {
            let a = trie.score(&heldout, depth).unwrap();
            let b = long.score(&heldout, depth).unwrap();
            assert_eq!(a.tokens, b.tokens);
            assert_eq!(a.bits_per_byte.to_bits(), b.bits_per_byte.to_bits());
            assert_eq!(a.accuracy.to_bits(), b.accuracy.to_bits());
        }
    }

    #[test]
    fn twelve_byte_context_differentiates_same_last_seven_bytes() {
        let train = (0..LANES)
            .flat_map(|lane| {
                let tag = (lane % 2) as u8;
                std::iter::once(tag)
                    .chain(std::iter::repeat_n(0, 11))
                    .chain(std::iter::once(200 + tag))
                    .chain(std::iter::repeat_n(77, 7))
            })
            .collect::<Vec<_>>();
        let trie = ElasticNGramTrie::train(&train, trie_limits(12)).unwrap();
        let a = std::iter::once(0u8)
            .chain(std::iter::repeat_n(0, 11))
            .collect::<Vec<_>>();
        let b = std::iter::once(1u8)
            .chain(std::iter::repeat_n(0, 11))
            .collect::<Vec<_>>();
        let short_a = trie.distribution(&a, 7).unwrap();
        let short_b = trie.distribution(&b, 7).unwrap();
        for label in 0..256 {
            assert_eq!(short_a[label].to_bits(), short_b[label].to_bits());
        }
        let long_a = trie.distribution(&a, 12).unwrap();
        let long_b = trie.distribution(&b, 12).unwrap();
        assert!(long_a[200] > 0.9);
        assert!(long_b[201] > 0.9);
        assert_ne!(long_a[200].to_bits(), long_b[200].to_bits());
        assert_eq!(
            trie.classify_depth_transition(7, 12),
            Ok(DepthTransition::ModelSemanticsChanged)
        );
    }

    #[test]
    fn permutation_of_train_lanes_preserves_every_distribution() {
        let train = fixture(false);
        let chunk = train.len() / LANES;
        let reversed = (0..LANES)
            .rev()
            .flat_map(|lane| train[lane * chunk..(lane + 1) * chunk].iter().copied())
            .collect::<Vec<_>>();
        let first = ElasticNGramTrie::train(&train, trie_limits(24)).unwrap();
        let second = ElasticNGramTrie::train(&reversed, trie_limits(24)).unwrap();
        // Trie IDs depend on insertion order, so equality is on predictions
        // and counts rather than serialized node order.
        assert_eq!(first.accounting(), second.accounting());
        let histories = [&train[0..40], &train[40..80], &[0, 1, 2, 3, 4][..]];
        for depth in [0, 4, 7, 12, 24] {
            for history in histories {
                let left = first.distribution(history, depth).unwrap();
                let right = second.distribution(history, depth).unwrap();
                for y in 0..256 {
                    assert_eq!(left[y].to_bits(), right[y].to_bits());
                }
            }
        }
        assert_eq!(first.accounting().edges + 1, first.accounting().nodes);
    }

    #[test]
    fn budgets_unseen_and_immutable_inference_are_fail_closed() {
        let train = fixture(false);
        let invalid = [
            (
                TrieLimits {
                    max_depth: 33,
                    ..TrieLimits::default()
                },
                TrieError::InvalidLimits,
            ),
            (
                TrieLimits {
                    max_events: 1,
                    ..TrieLimits::default()
                },
                TrieError::TooManyEvents,
            ),
            (
                TrieLimits {
                    max_nodes: 1,
                    ..TrieLimits::default()
                },
                TrieError::TooManyNodes,
            ),
            (
                TrieLimits {
                    max_edges: 1,
                    ..TrieLimits::default()
                },
                TrieError::TooManyEdges,
            ),
            (
                TrieLimits {
                    max_follower_pairs: 1,
                    ..TrieLimits::default()
                },
                TrieError::TooManyFollowerPairs,
            ),
            (
                TrieLimits {
                    max_nodes: 0,
                    ..TrieLimits::default()
                },
                TrieError::InvalidLimits,
            ),
        ];
        for (limits, expected) in invalid {
            assert_eq!(ElasticNGramTrie::train(&train, limits), Err(expected));
        }
        assert_eq!(
            ElasticNGramTrie::train(&[0u8; LANES], TrieLimits::default()),
            Err(TrieError::InvalidCorpus)
        );
        let no_history = ElasticNGramTrie::train(&train, trie_limits(0)).unwrap();
        assert_eq!(no_history.accounting().nodes, 1);
        assert_eq!(no_history.accounting().edges, 0);
        let trie = ElasticNGramTrie::train(&train, trie_limits(32)).unwrap();
        assert_eq!(trie.distribution(&[0u8; 34], 33), Err(TrieError::InvalidDepth));
        assert_eq!(
            trie.classify_depth_transition(32, 33),
            Err(TrieError::InvalidDepth)
        );
        let before = trie.clone();
        for depth in 0..=32 {
            let probs = trie.distribution(&[255u8; 40], depth).unwrap();
            assert!(probs.iter().all(|&p| p.is_finite() && p > 0.0));
            assert!((probs.iter().sum::<f64>() - 1.0).abs() < 1.0e-10);
        }
        assert!(trie.score(&fixture(true), 32).unwrap().bits_per_byte.is_finite());
        assert_eq!(trie, before);
        assert_eq!(trie, ElasticNGramTrie::train(&train, trie_limits(32)).unwrap());
    }
}
