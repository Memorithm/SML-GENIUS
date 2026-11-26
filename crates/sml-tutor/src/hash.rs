//! Content identity helpers built on the dependency-free SHA-256 of `sml-core`.

use sml_core::elastic_ngram_paged_sha::{hex, parse_hex, Sha256};

/// A 32-byte SHA-256 digest.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Digest(pub [u8; 32]);

impl Digest {
    pub const ZERO: Digest = Digest([0; 32]);

    pub fn of(bytes: &[u8]) -> Self {
        let mut h = Sha256::default();
        h.update(bytes);
        Digest(h.finish())
    }

    pub fn to_hex(&self) -> String {
        hex(&self.0)
    }

    pub fn from_hex(text: &str) -> Option<Self> {
        parse_hex(text).map(Digest)
    }

    /// Short prefix for logs only; never use it as an identity.
    pub fn short(&self) -> String {
        self.to_hex()[..12].to_string()
    }
}

impl core::fmt::Debug for Digest {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Digest({})", self.short())
    }
}

/// Canonical, length-prefixed encoder so that distinct field sequences can
/// never collide by concatenation.
#[derive(Default)]
pub struct Canonical {
    inner: Sha256,
}

impl Canonical {
    pub fn new(schema: &str) -> Self {
        let mut c = Self::default();
        c.str(schema);
        c
    }

    pub fn bytes(&mut self, bytes: &[u8]) -> &mut Self {
        self.inner.update(&(bytes.len() as u64).to_le_bytes());
        self.inner.update(bytes);
        self
    }

    pub fn str(&mut self, s: &str) -> &mut Self {
        self.bytes(s.as_bytes())
    }

    pub fn u64(&mut self, v: u64) -> &mut Self {
        self.bytes(&v.to_le_bytes())
    }

    pub fn digest(&mut self, d: &Digest) -> &mut Self {
        self.bytes(&d.0)
    }

    pub fn finish(self) -> Digest {
        Digest(self.inner.finish())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_vector_and_roundtrip() {
        let d = Digest::of(b"abc");
        assert_eq!(
            d.to_hex(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(Digest::from_hex(&d.to_hex()), Some(d));
    }

    #[test]
    fn length_prefix_prevents_concatenation_collisions() {
        let mut a = Canonical::new("t");
        a.str("ab").str("c");
        let mut b = Canonical::new("t");
        b.str("a").str("bc");
        assert_ne!(a.finish(), b.finish());
    }
}
