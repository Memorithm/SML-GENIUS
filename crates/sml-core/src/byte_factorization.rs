//! Binary decompositions of a next-byte distribution into eight decisions.
//!
//! `IndependentBits` predicts bit `k` from the context alone, so the byte
//! law is forced to a product of eight Bernoulli variables and every
//! dependence between bits of the same byte is lost (ASCII/UTF-8 case and
//! continuation bits, character classes). `MsbTree` predicts bits most
//! significant first, each conditioned on the bits already emitted: its
//! head is the node `1..=255` of the binary tree used by
//! `bool_coder::ByteBitTree`, so the eight decisions factor any byte
//! distribution exactly by the chain rule, at the same 8 decisions/byte.

/// Head indices are always below this bound.
pub const MAX_HEADS: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ByteFactorization {
    IndependentBits,
    MsbTree,
}

impl ByteFactorization {
    /// The `(head, target bit)` decisions that code `byte`, in coding order.
    #[must_use]
    pub fn decisions(self, byte: u8) -> [(usize, bool); 8] {
        let mut out = [(0, false); 8];
        match self {
            Self::IndependentBits => {
                for (bit, slot) in out.iter_mut().enumerate() {
                    *slot = (bit, (byte >> bit) & 1 != 0);
                }
            }
            Self::MsbTree => {
                let mut node = 1_usize;
                for (depth, slot) in out.iter_mut().enumerate() {
                    let bit = (byte >> (7 - depth)) & 1 != 0;
                    *slot = (node, bit);
                    node = 2 * node + usize::from(bit);
                }
            }
        }
        out
    }

    /// Whether `head` is a valid output head of this factorization.
    #[must_use]
    pub fn is_head(self, head: usize) -> bool {
        match self {
            Self::IndependentBits => head < 8,
            Self::MsbTree => (1..MAX_HEADS).contains(&head),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ByteFactorization, MAX_HEADS};
    use crate::bool_coder::ByteBitTree;

    #[test]
    fn independent_bits_are_the_legacy_lsb_first_heads() {
        let decisions = ByteFactorization::IndependentBits.decisions(0b1000_0101);
        let heads: Vec<_> = decisions.iter().map(|d| d.0).collect();
        assert_eq!(heads, (0..8).collect::<Vec<_>>());
        let bits: Vec<_> = decisions.iter().map(|d| d.1).collect();
        assert_eq!(bits, [true, false, true, false, false, false, false, true]);
    }

    #[test]
    fn msb_tree_heads_follow_the_bool_coder_node_path() {
        let mut probabilities = [0.0_f64; 256];
        probabilities[0b1011_0010] = 1.0;
        let tree = ByteBitTree::new(&probabilities).unwrap();
        for (head, bit) in ByteFactorization::MsbTree.decisions(0b1011_0010) {
            assert!(ByteFactorization::MsbTree.is_head(head));
            // A point mass makes every node on the path deterministic.
            assert_eq!(tree.p1(head), if bit { 1.0 } else { 0.0 });
        }
    }

    #[test]
    fn msb_tree_paths_are_distinct_and_cover_every_internal_node() {
        let mut seen = [false; MAX_HEADS];
        for byte in 0..=255_u8 {
            for (head, _) in ByteFactorization::MsbTree.decisions(byte) {
                seen[head] = true;
            }
        }
        assert!(!seen[0]);
        assert!(seen[1..].iter().all(|&s| s));
        assert!(!ByteFactorization::MsbTree.is_head(0));
        assert!(!ByteFactorization::IndependentBits.is_head(8));
    }
}
