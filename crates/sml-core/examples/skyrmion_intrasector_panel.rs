//! SKY-R2: writable intrasector Boolean payload vs topological charge.
//!
//! Two valid octahedral textures with the SAME lattice Q=-1 differ in one
//! serialized payload bit. Topological charge can admit both writes but
//! cannot detect that bit flipping. This is a software proof-of-limitation,
//! not a spintronic reliability experiment or a production model component.

#[allow(dead_code)]
#[path = "skyrmion_octahedron.rs"]
mod topology;

const SPINS: usize = 25;
const SERIALIZED_BITS: usize = SPINS * 3;
// Cell (1,1), Spin::Nx -> Spin::Ny, code 0b001 -> 0b011.
const DATA_INDEX: usize = (5 + 1) * 3 + 1;

fn encode(payload: u8) -> [u8; SERIALIZED_BITS] {
    assert!(payload <= 1);
    let mut coded = [0_u8; SERIALIZED_BITS];
    coded.copy_from_slice(&topology::encoded_reference_texture(false));
    coded[DATA_INDEX] = payload;
    assert_eq!(topology::decode_reference_texture(&coded), Some(0));
    coded
}

fn read(codeword: &[u8]) -> Option<u8> {
    if codeword.len() != SERIALIZED_BITS
        || codeword.iter().any(|bit| *bit > 1)
        || topology::decode_reference_texture(codeword) != Some(0)
    {
        return None;
    }
    // The chosen spin is Nx (0b001) or Ny (0b011).
    let base = 3 * (5 + 1);
    let local = codeword[base] | (codeword[base + 1] << 1) | (codeword[base + 2] << 2);
    match local {
        1 => Some(0),
        3 => Some(1),
        _ => None,
    }
}

fn guarded_write(state: &mut [u8; SERIALIZED_BITS], payload: u8) -> bool {
    if topology::decode_reference_texture(state) != Some(0) {
        return false;
    }
    let candidate = encode(payload);
    if topology::decode_reference_texture(&candidate) != Some(0) {
        return false;
    }
    *state = candidate;
    true
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Count {
    recovered: usize,
    erased: usize,
    wrong: usize,
}

impl Count {
    fn record(&mut self, expected: u8, observed: Option<u8>) {
        match observed {
            Some(value) if value == expected => self.recovered += 1,
            Some(_) => self.wrong += 1,
            None => self.erased += 1,
        }
    }

    const fn total(self) -> usize {
        self.recovered + self.erased + self.wrong
    }
}

fn exhaustive_microstate_flips() -> Count {
    let mut count = Count::default();
    for payload in 0..=1 {
        let state = encode(payload);
        for index in 0..SERIALIZED_BITS {
            let mut corrupted = state;
            corrupted[index] ^= 1;
            count.record(payload, read(&corrupted));
        }
    }
    count
}

fn majority75_flips() -> Count {
    let mut count = Count::default();
    for payload in 0..=1 {
        for index in 0..SERIALIZED_BITS {
            let mut repetition = [payload; SERIALIZED_BITS];
            repetition[index] ^= 1;
            let ones: usize = repetition.iter().map(|bit| usize::from(*bit)).sum();
            count.record(payload, Some(u8::from(ones >= 38)));
        }
    }
    count
}

fn hamming_distance(a: &[u8], b: &[u8]) -> usize {
    assert_eq!(a.len(), b.len());
    a.iter()
        .zip(b)
        .filter(|(left, right)| left != right)
        .count()
}

fn main() {
    let mut live = encode(0);
    let mut accepted = 0;
    let mut decoded = 0;
    for step in 0..512 {
        let payload = (step % 2) as u8;
        if guarded_write(&mut live, payload) {
            accepted += 1;
        }
        if read(&live) == Some(payload) {
            decoded += 1;
        }
    }

    let mut corrupted = encode(0);
    corrupted[DATA_INDEX] ^= 1;
    println!("SKY-R2: fixed Q=-1, variable microstate; no physical noise claims");
    println!("arm,logical_bits,dmin,single_flips,recovered,erasure,wrong");
    let sky = exhaustive_microstate_flips();
    println!(
        "INTRASECTOR,{SERIALIZED_BITS},{},{},{},{},{}",
        hamming_distance(&encode(0), &encode(1)),
        sky.total(),
        sky.recovered,
        sky.erased,
        sky.wrong
    );
    let repeated = majority75_flips();
    println!(
        "REPETITION75,{SERIALIZED_BITS},{SERIALIZED_BITS},{},{},{},{}",
        repeated.total(),
        repeated.recovered,
        repeated.erased,
        repeated.wrong
    );
    println!(
        "write_trial,attempts=512,accepted={accepted},read_correct={decoded},corrupted_charge_payload={:?},corrupted_read={:?}",
        topology::decode_reference_texture(&corrupted),
        read(&corrupted)
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_payloads_are_both_valid_and_have_hamming_distance_one() {
        assert_eq!(read(&encode(0)), Some(0));
        assert_eq!(read(&encode(1)), Some(1));
        assert_eq!(hamming_distance(&encode(0), &encode(1)), 1);
        assert_eq!(topology::decode_reference_texture(&encode(0)), Some(0));
        assert_eq!(topology::decode_reference_texture(&encode(1)), Some(0));
    }

    #[test]
    fn sector_preserving_writes_can_alternate_payloads() {
        let mut live = encode(0);
        for step in 0..512 {
            let payload = (step % 2) as u8;
            assert!(guarded_write(&mut live, payload));
            assert_eq!(topology::decode_reference_texture(&live), Some(0));
            assert_eq!(read(&live), Some(payload));
        }
    }

    #[test]
    fn the_encoded_payload_flip_is_silent_to_charge_invariant() {
        for payload in 0..=1 {
            let mut corrupted = encode(payload);
            corrupted[DATA_INDEX] ^= 1;
            assert_eq!(topology::decode_reference_texture(&corrupted), Some(0));
            assert_eq!(read(&corrupted), Some(payload ^ 1));
        }
    }

    #[test]
    fn exhaustive_flip_is_not_an_error_correcting_code() {
        let sample = exhaustive_microstate_flips();
        assert_eq!(sample.total(), 150);
        assert_eq!(sample.wrong, 2);
        let baseline = majority75_flips();
        assert_eq!(
            baseline,
            Count {
                recovered: 150,
                erased: 0,
                wrong: 0
            }
        );
    }

    #[test]
    fn malformed_and_nonsector_inputs_fail_closed() {
        assert_eq!(read(&[1, 0, 1]), None);
        let mut live = encode(0);
        live[0] = 2;
        assert_eq!(read(&live), None);
        assert!(!guarded_write(&mut live, 1));
    }
}
