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
                assert_eq!(p_left, p_right, "prediction drift at byte {byte} bit {shift}");
                assert!((1..=4095).contains(&p_left), "invalid probability {p_left}");
                let bit = byte & (1 << shift) != 0;
                left.update(bit);
                right.update(bit);
            }
        }
    }
}
