//! Independent BMX online-update determinism regression (no quality claim).
use sml_core::bit_mixer::{Architecture, BitMixer, MaskSet, MixerConfig};

fn small_config(architecture: Architecture) -> MixerConfig {
    MixerConfig {
        table_bits: 10,
        match_bits: 10,
        counter_limit: 31,
        mixer_rate: 6,
        mask_set: MaskSet::V3,
        architecture,
    }
}

#[test]
fn online_updates_are_deterministic_across_replayed_streams() {
    let stream: Vec<u8> = (0..128)
        .map(|i| ((i * 73 + (i / 7) * 11) % 256) as u8)
        .collect();
    for architecture in [Architecture::Classic, Architecture::V4, Architecture::V5] {
        let config = small_config(architecture);
        let mut left = BitMixer::new(config);
        let mut right = BitMixer::new(config);
        left.prime(&stream[..16]);
        right.prime(&stream[..16]);
        assert_eq!(left.identity(), right.identity());
        for &byte in &stream[16..] {
            for shift in (0..8).rev() {
                let p_left = left.p12();
                let p_right = right.p12();
                assert_eq!(
                    p_left, p_right,
                    "prediction drift at byte {byte} bit {shift}"
                );
                assert!((1..=4095).contains(&p_left), "invalid probability {p_left}");
                let bit = byte & (1 << shift) != 0;
                left.update(bit);
                right.update(bit);
            }
        }
    }
}

#[test]
fn online_replay_handles_adversarial_bit_patterns_and_repeated_priming() {
    // Exercise saturated counters, alternating updates, and a non-byte-aligned
    // suffix. This is a deterministic correctness regression, not a quality score.
    for architecture in [Architecture::Classic, Architecture::V4, Architecture::V5] {
        for pattern in [0_u8, 255, 0xAA, 0x55] {
            let config = small_config(architecture);
            let prefix = [pattern; 32];
            let mut first = BitMixer::new(config);
            let mut replay = BitMixer::new(config);
            first.prime(&prefix);
            replay.prime(&prefix);
            assert_eq!(first.identity(), replay.identity());
            for step in 0..1027 {
                let expected = first.p12();
                assert_eq!(
                    expected,
                    replay.p12(),
                    "architecture {architecture:?}, pattern {pattern}, step {step}"
                );
                assert!((1..=4095).contains(&expected));
                let bit = (pattern >> (7 - (step % 8))) & 1 != 0;
                first.update(bit);
                replay.update(bit);
            }
            assert_eq!(first.p12(), replay.p12());
        }
    }
}
