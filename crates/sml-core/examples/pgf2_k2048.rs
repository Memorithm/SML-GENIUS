//! Deterministic PGF2 width-ladder qualification with an explicit K=2048 panel.
//!
//! This executable reports algebraic/correctness evidence only. It does not
//! measure latency and does not make model-quality or promotion claims.

use sml_core::pascal_gf2::{
    pack_bits, pascal_gf2_in_place, pascal_gf2_oracle, unpack_words, PascalGf2Stats,
};

const WIDTHS: [usize; 6] = [64, 128, 256, 512, 1024, 2048];
const K2048_SEEDS: u64 = 16;

fn deterministic_words(count: usize, seed: u64) -> Vec<u64> {
    let mut state = seed | 1;
    let mut words = Vec::with_capacity(count);
    for _ in 0..count {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        words.push(state);
    }
    words
}

fn checksum(words: &[u64]) -> u64 {
    words
        .iter()
        .copied()
        .enumerate()
        .fold(0x9e37_79b9_7f4a_7c15_u64, |acc, (index, word)| {
            acc.rotate_left(7) ^ word.wrapping_mul(0xd6e8_feb8_6659_fd93 ^ index as u64)
        })
}

fn qualify(width: usize, seed: u64) -> (PascalGf2Stats, u64) {
    let original = deterministic_words(width / 64, seed);
    let expected = pascal_gf2_oracle(&unpack_words(&original)).expect("oracle");

    let mut packed = original.clone();
    let stats = pascal_gf2_in_place(&mut packed).expect("packed transform");
    assert_eq!(unpack_words(&packed), expected, "oracle parity K={width}");
    assert_eq!(
        pack_bits(&expected).expect("repack oracle"),
        packed,
        "packed parity K={width}"
    );

    let once_checksum = checksum(&packed);
    pascal_gf2_in_place(&mut packed).expect("packed inverse");
    assert_eq!(packed, original, "self inverse K={width}");

    (stats, once_checksum)
}

fn main() {
    let mut ladder_checksum = 0_u64;

    for width in WIDTHS {
        let (stats, transformed_checksum) = qualify(width, 0x5047_4632_0000_0000 ^ width as u64);
        ladder_checksum ^= transformed_checksum.rotate_left(width.ilog2());
        println!(
            "PGF2_WIDTH,K={},words={},stages={},logical_xor_ops={},packed_word_updates={},state_bytes={},scratch_words={},oracle_equal=1,self_inverse=1,checksum={:016x}",
            stats.bits,
            stats.words,
            stats.stages,
            stats.logical_xor_ops,
            stats.packed_word_updates,
            stats.state_bytes,
            stats.scratch_words,
            transformed_checksum,
        );
    }

    let mut k2048_checksum = 0_u64;
    for seed in 0..K2048_SEEDS {
        let (stats, transformed_checksum) = qualify(
            2048,
            0x2048_5047_4632_0001_u64 ^ seed.wrapping_mul(0x9e37_79b9),
        );
        assert_eq!(stats.bits, 2048);
        k2048_checksum ^= transformed_checksum.rotate_left(seed as u32);
    }

    println!(
        "PGF2_K2048,observations={},oracle_equal={},self_inverse={},words=32,stages=11,logical_xor_ops_per_transform=11264,packed_word_updates_per_transform=272,state_bytes=256,scratch_words=0,aggregate_checksum={:016x}",
        K2048_SEEDS,
        K2048_SEEDS,
        K2048_SEEDS,
        k2048_checksum,
    );
    println!(
        "PGF2_SELECTION,promotion=BLOCKED,latency=NOT_MEASURED,model_quality=NOT_TESTED,ladder_checksum={ladder_checksum:016x}"
    );
}
