//! SKY-M1: exhaustive single-bit software corruption controls for one payload bit.
//! The 75 SKY bits are hypothetical serialized bits, not physical-device area.
//! Returning an erasure is detection, not correction of the lattice state.

#[allow(dead_code)]
#[path = "skyrmion_octahedron.rs"]
mod reference;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Arm {
    Raw,
    Parity,
    Crc8,
    Repetition3,
    Repetition75,
    Sky,
}

impl Arm {
    const ALL: [Self; 6] = [
        Self::Raw,
        Self::Parity,
        Self::Crc8,
        Self::Repetition3,
        Self::Repetition75,
        Self::Sky,
    ];

    const fn name(self) -> &'static str {
        match self {
            Self::Raw => "RAW",
            Self::Parity => "PARITY",
            Self::Crc8 => "CRC8",
            Self::Repetition3 => "REPETITION3",
            Self::Repetition75 => "REPETITION75",
            Self::Sky => "SKY_OCTAHEDRAL",
        }
    }

    fn encode(self, payload: u8) -> Vec<u8> {
        assert!(payload <= 1);
        match self {
            Self::Raw => vec![payload],
            Self::Parity => vec![payload, payload],
            Self::Crc8 => {
                let crc = crc8(payload);
                let mut bits = vec![payload];
                bits.extend((0..8).map(|bit| (crc >> bit) & 1));
                bits
            }
            Self::Repetition3 => vec![payload; 3],
            Self::Repetition75 => vec![payload; 75],
            Self::Sky => reference::encoded_reference_texture(payload == 1),
        }
    }

    fn decode(self, bits: &[u8]) -> Option<u8> {
        if bits.iter().any(|bit| *bit > 1) {
            return None;
        }
        match self {
            Self::Raw if bits.len() == 1 => Some(bits[0]),
            Self::Parity if bits.len() == 2 => (bits[0] == bits[1]).then_some(bits[0]),
            Self::Crc8 if bits.len() == 9 => {
                let observed = bits[1..]
                    .iter()
                    .enumerate()
                    .fold(0_u8, |crc, (position, bit)| crc | (*bit << position));
                (observed == crc8(bits[0])).then_some(bits[0])
            }
            Self::Repetition3 if bits.len() == 3 => Some(u8::from(bits.iter().sum::<u8>() >= 2)),
            Self::Repetition75 if bits.len() == 75 => Some(u8::from(bits.iter().sum::<u8>() >= 38)),
            Self::Sky if bits.len() == 75 => reference::decode_reference_texture(bits),
            _ => None,
        }
    }
}

// CRC-8/ATM: polynomial 0x07, initial state and final xor both zero.
fn crc8(byte: u8) -> u8 {
    let mut crc = byte;
    for _ in 0..8 {
        crc = if crc & 0x80 != 0 {
            (crc << 1) ^ 0x07
        } else {
            crc << 1
        };
    }
    crc
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Outcome {
    payload_recovered: u64,
    detected_erasure: u64,
    wrong_payload: u64,
}

impl Outcome {
    fn observe(&mut self, expected: u8, decoded: Option<u8>) {
        match decoded {
            Some(actual) if actual == expected => self.payload_recovered += 1,
            Some(_) => self.wrong_payload += 1,
            None => self.detected_erasure += 1,
        }
    }

    const fn total(self) -> u64 {
        self.payload_recovered + self.detected_erasure + self.wrong_payload
    }
}

fn exhaustive_single_faults(arm: Arm) -> (usize, Outcome) {
    let mut result = Outcome::default();
    let size = arm.encode(0).len();
    for payload in 0..=1 {
        let clean = arm.encode(payload);
        assert_eq!(clean.len(), size);
        assert_eq!(arm.decode(&clean), Some(payload));
        for position in 0..size {
            let mut corrupted = clean.clone();
            corrupted[position] ^= 1;
            result.observe(payload, arm.decode(&corrupted));
        }
    }
    (size, result)
}

fn codeword_distance(arm: Arm) -> usize {
    arm.encode(0)
        .iter()
        .zip(arm.encode(1).iter())
        .filter(|(a, b)| a != b)
        .count()
}

fn main() {
    println!("SKY-M1: single software-bit corruption; payload=1 bit; no device claims");
    println!("arm,bits,hamming_distance,flips,payload_recovered,erasure,wrong_payload");
    for arm in Arm::ALL {
        let (bits, count) = exhaustive_single_faults(arm);
        println!(
            "{},{bits},{},{},{},{},{}",
            arm.name(),
            codeword_distance(arm),
            count.total(),
            count.payload_recovered,
            count.detected_erasure,
            count.wrong_payload
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc8_known_vectors() {
        assert_eq!(crc8(0), 0);
        assert_eq!(crc8(1), 7);
    }

    #[test]
    fn round_trip_both_payloads() {
        for arm in Arm::ALL {
            for payload in 0..=1 {
                assert_eq!(arm.decode(&arm.encode(payload)), Some(payload));
            }
        }
    }

    #[test]
    fn exhaustive_frozen_single_fault_counts() {
        let frozen = [
            (Arm::Raw, 1, 0, 0, 2),
            (Arm::Parity, 2, 0, 4, 0),
            (Arm::Crc8, 9, 0, 18, 0),
            (Arm::Repetition3, 3, 6, 0, 0),
            (Arm::Repetition75, 75, 150, 0, 0),
            (Arm::Sky, 75, 8, 142, 0),
        ];
        for (arm, bits, recovered, detected, wrong) in frozen {
            let (length, result) = exhaustive_single_faults(arm);
            assert_eq!(length, bits, "{arm:?}");
            assert_eq!(result.payload_recovered, recovered, "{arm:?}");
            assert_eq!(result.detected_erasure, detected, "{arm:?}");
            assert_eq!(result.wrong_payload, wrong, "{arm:?}");
        }
    }

    #[test]
    fn codeword_distances_and_malformed_input() {
        assert_eq!(codeword_distance(Arm::Sky), 2);
        assert_eq!(codeword_distance(Arm::Repetition3), 3);
        assert_eq!(codeword_distance(Arm::Repetition75), 75);
        assert_eq!(Arm::Sky.decode(&[0, 1, 0]), None);
        let mut malformed = Arm::Sky.encode(0);
        malformed[0] = 2;
        assert_eq!(Arm::Sky.decode(&malformed), None);
    }
}
