//! CONTEXT-3 G1: pre-registered text -> 64-bit search-key encoder v1.
//!
//! Maps the token lists produced by [`crate::g1_realtext_eval`] (segments and
//! queries) to the single 64-bit word the G1 bench indexes and searches.
//! Frozen before any real-text number is seen; changing anything is v2. See
//! `docs/G1-CONTEXT-3-REALTEXT-CORPUS-PROTOCOL.md` ("Search-key encoder v1").
//!
//! The encoder is a plain lexical SimHash. No claim is made that Hamming
//! distance between keys tracks relevance; that is what a pinned run measures.

use crate::raw_context::sha256_raw_bytes;

/// Identifier printed in bench meta (`key=`).
pub const KEY_ENCODER_V1: &str = "simhash64-sha256-v1";
/// Domain-separation prefix for per-token hashes.
pub const TOKEN_HASH_PREFIX_V1: &str = "g1k-tok|";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyError {
    /// Empty token list: no key can be derived (refusal, never a zero key).
    Empty,
    Hash,
}

/// `u64_be(sha256("g1k-tok|" + token)[0..8])`.
pub fn token_hash(token: &str) -> Result<u64, KeyError> {
    let h = sha256_raw_bytes(format!("{TOKEN_HASH_PREFIX_V1}{token}").as_bytes())
        .map_err(|_| KeyError::Hash)?;
    let mut b = [0_u8; 8];
    b.copy_from_slice(&h.bytes()[..8]);
    Ok(u64::from_be_bytes(b))
}

/// SimHash v1 over the token multiset (repeats count, order ignored): for each
/// bit `b` (0 = LSB), `c_b = #{tokens with bit b set} - #{tokens with bit b
/// clear}`; key bit `b` is 1 iff `c_b > 0` (ties -> 0).
pub fn simhash_v1<S: AsRef<str>>(tokens: &[S]) -> Result<u64, KeyError> {
    if tokens.is_empty() {
        return Err(KeyError::Empty);
    }
    let mut c = [0_i64; 64];
    for t in tokens {
        let h = token_hash(t.as_ref())?;
        for (b, cb) in c.iter_mut().enumerate() {
            *cb += if (h >> b) & 1 == 1 { 1 } else { -1 };
        }
    }
    Ok(c.iter()
        .enumerate()
        .filter(|(_, &cb)| cb > 0)
        .fold(0_u64, |k, (b, _)| k | (1 << b)))
}
