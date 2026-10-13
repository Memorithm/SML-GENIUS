//! Dependency-free SHA-256 for source/model artifact identity in ENG-1c/CAP2b.
//!
//! This implements FIPS 180-4 SHA-256 in safe Rust and is not an HMAC,
//! signature, hostile-host attestation or substitute for trusted source URLs.

const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
    0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
    0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
    0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
    0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
    0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
    0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
    0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
    0xc67178f2,
];

#[derive(Clone)]
pub struct Sha256 {
    state: [u32; 8],
    block: [u8; 64],
    used: usize,
    bytes: u64,
}

impl Default for Sha256 {
    fn default() -> Self {
        Self {
            state: [
                0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c,
                0x1f83d9ab, 0x5be0cd19,
            ],
            block: [0; 64],
            used: 0,
            bytes: 0,
        }
    }
}

impl Sha256 {
    fn compress(&mut self, block: &[u8; 64]) {
        let mut w = [0u32; 64];
        for (i, chunk) in block.chunks_exact(4).enumerate() {
            w[i] = u32::from_be_bytes(chunk.try_into().expect("four-byte chunk"));
        }
        for i in 16..64 {
            let a = w[i - 15];
            let b = w[i - 2];
            let s0 = a.rotate_right(7) ^ a.rotate_right(18) ^ (a >> 3);
            let s1 = b.rotate_right(17) ^ b.rotate_right(19) ^ (b >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = self.state;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let t1 = h
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        for (state, evolved) in self.state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
            *state = state.wrapping_add(evolved);
        }
    }

    pub fn update(&mut self, mut input: &[u8]) {
        self.bytes = self.bytes.checked_add(input.len() as u64).expect("SHA256 length bound");
        while !input.is_empty() {
            let size = (64 - self.used).min(input.len());
            self.block[self.used..self.used + size].copy_from_slice(&input[..size]);
            self.used += size;
            input = &input[size..];
            if self.used == 64 {
                let block = self.block;
                self.compress(&block);
                self.used = 0;
            }
        }
    }

    pub fn finish(mut self) -> [u8; 32] {
        let bit_len = self.bytes.checked_mul(8).expect("SHA256 bit length bound");
        self.update(&[0x80]);
        while self.used != 56 {
            self.update(&[0]);
        }
        self.update(&bit_len.to_be_bytes());
        let mut result = [0u8; 32];
        for (chunk, word) in result.chunks_exact_mut(4).zip(self.state) {
            chunk.copy_from_slice(&word.to_be_bytes());
        }
        result
    }
}

pub fn sha256(input: &[u8]) -> [u8; 32] {
    let mut state = Sha256::default();
    state.update(input);
    state.finish()
}

pub fn hex(hash: &[u8; 32]) -> String {
    let mut string = String::with_capacity(64);
    for &byte in hash {
        use std::fmt::Write as _;
        write!(&mut string, "{byte:02x}").expect("infallible String writer");
    }
    string
}

pub fn parse_hex(value: &str) -> Option<[u8; 32]> {
    if value.len() != 64 {
        return None;
    }
    let mut bytes = [0u8; 32];
    for (slot, part) in bytes.iter_mut().zip(value.as_bytes().chunks_exact(2)) {
        let str_slice = std::str::from_utf8(part).ok()?;
        *slot = u8::from_str_radix(str_slice, 16).ok()?;
    }
    Some(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eng2b_sha256_standard_known_vectors_and_incremental_streaming() {
        let empty = sha256(b"");
        assert_eq!(
            hex(&empty),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        let abc = sha256(b"abc");
        assert_eq!(
            hex(&abc),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let mut stream = Sha256::default();
        for piece in b"abc".chunks(1) {
            stream.update(piece);
        }
        assert_eq!(stream.finish(), abc);
        assert_eq!(parse_hex(&hex(&abc)), Some(abc));
        assert!(parse_hex("wrong").is_none());
    }
}
