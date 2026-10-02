use core::fmt;

use sml_core::delta_kv::{DeltaKvCounters, DeltaKvError, DeltaKvMemory};

/// One immutable matrix/key episode. Cached numbers are never source receipts.
///
/// The payload budget covers cached i32 elements, not the borrowed matrix/key,
/// this object's metadata, allocator overhead or process RSS. Failed numeric
/// reads retain their attempted-work counters but never install a cached value.
///
/// A matrix update cannot coexist with a later use of its borrowed cache:
/// ```compile_fail
/// use sml_core::delta_kv::{DeltaKvCounters, DeltaKvMemory};
/// use sml_store::observed_history::FrozenDeltaKvRead;
/// let mut memory = DeltaKvMemory::new(1, 1).unwrap();
/// let key = [4096];
/// let mut cache = FrozenDeltaKvRead::new(&memory, &key, 4).unwrap();
/// memory.update(&key, &[4096], 4096, &mut DeltaKvCounters::default()).unwrap();
/// cache.read().unwrap();
/// ```
/// The borrowed key cannot be changed either:
/// ```compile_fail
/// use sml_core::delta_kv::DeltaKvMemory;
/// use sml_store::observed_history::FrozenDeltaKvRead;
/// let memory = DeltaKvMemory::new(1, 1).unwrap();
/// let mut key = [4096];
/// let mut cache = FrozenDeltaKvRead::new(&memory, &key, 4).unwrap();
/// key[0] = 0;
/// cache.read().unwrap();
/// ```
#[derive(Debug)]
pub struct FrozenDeltaKvRead<'a> {
    memory: &'a DeltaKvMemory,
    key: &'a [i16],
    value: Option<Vec<i32>>,
    numeric: DeltaKvCounters,
    hits: u64,
    payload_bytes: usize,
}

impl<'a> FrozenDeltaKvRead<'a> {
    pub fn new(
        memory: &'a DeltaKvMemory,
        key: &'a [i16],
        max_payload_bytes: usize,
    ) -> Result<Self, FrozenReadError> {
        if key.len() != memory.key_dim() {
            return Err(FrozenReadError::Shape);
        }
        let payload_bytes = memory
            .value_dim()
            .checked_mul(core::mem::size_of::<i32>())
            .ok_or(FrozenReadError::Overflow)?;
        if payload_bytes > max_payload_bytes {
            return Err(FrozenReadError::Budget);
        }
        Ok(Self {
            memory,
            key,
            value: None,
            numeric: DeltaKvCounters::default(),
            hits: 0,
            payload_bytes,
        })
    }

    /// The first successful call reads numerically; subsequent calls reuse it.
    /// An error does not trigger an automatic retry or imply a memory miss.
    pub fn read(&mut self) -> Result<&[i32], FrozenReadError> {
        if self.value.is_none() {
            let terms = self
                .key
                .len()
                .checked_mul(self.memory.value_dim())
                .and_then(|n| u64::try_from(n).ok())
                .ok_or(FrozenReadError::Overflow)?;
            self.numeric
                .reads
                .checked_add(1)
                .ok_or(FrozenReadError::Overflow)?;
            self.numeric
                .read_macs
                .checked_add(terms)
                .ok_or(FrozenReadError::Overflow)?;
            let value = self
                .memory
                .read(self.key, &mut self.numeric)
                .map_err(FrozenReadError::Numeric)?;
            self.value = Some(value);
        } else {
            self.hits = self.hits.checked_add(1).ok_or(FrozenReadError::Overflow)?;
        }
        self.value.as_deref().ok_or(FrozenReadError::Invariant)
    }

    pub const fn numeric_counters(&self) -> DeltaKvCounters {
        self.numeric
    }

    pub const fn cache_hits(&self) -> u64 {
        self.hits
    }

    pub fn cached_value_bytes(&self) -> usize {
        if self.value.is_some() {
            self.payload_bytes
        } else {
            0
        }
    }
}

#[derive(Debug)]
pub enum FrozenReadError {
    Shape,
    Budget,
    Overflow,
    Invariant,
    Numeric(DeltaKvError),
}

impl fmt::Display for FrozenReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for FrozenReadError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cached_and_fresh_values_are_exact_even_when_q12_is_rounded() {
        let key = [4096; 3];
        let mut memory = DeltaKvMemory::new(3, 2).unwrap();
        let mut setup = DeltaKvCounters::default();
        memory
            .update(&key, &[4096, -4096], 4096, &mut setup)
            .unwrap();
        let mut cache = FrozenDeltaKvRead::new(&memory, &key, 8).unwrap();
        assert_eq!(cache.cached_value_bytes(), 0);
        let mut fresh = DeltaKvCounters::default();
        for _ in 0..4 {
            let expected = memory.read(&key, &mut fresh).unwrap();
            assert_eq!(cache.read().unwrap(), expected);
            assert_eq!(expected, [4095, -4095]);
        }
        assert_eq!(cache.numeric_counters().reads, 1);
        assert_eq!(cache.numeric_counters().read_macs, 6);
        assert_eq!(fresh.reads, 4);
        assert_eq!(fresh.read_macs, 24);
        assert_eq!(cache.cache_hits(), 3);
        assert_eq!(cache.cached_value_bytes(), 8);
    }

    #[test]
    fn key_shape_and_payload_budgets_reject_before_first_read() {
        let memory = DeltaKvMemory::new(2, 2).unwrap();
        assert!(matches!(
            FrozenDeltaKvRead::new(&memory, &[4096], 8),
            Err(FrozenReadError::Shape)
        ));
        assert!(matches!(
            FrozenDeltaKvRead::new(&memory, &[4096, 0], 7),
            Err(FrozenReadError::Budget)
        ));
    }

    #[test]
    fn zero_key_is_a_numeric_value_not_a_source_existence_claim() {
        let memory = DeltaKvMemory::new(2, 1).unwrap();
        let key = [0, 0];
        let mut cache = FrozenDeltaKvRead::new(&memory, &key, 4).unwrap();
        assert_eq!(cache.read().unwrap(), [0]);
        assert_eq!(cache.read().unwrap(), [0]);
        assert_eq!(cache.numeric_counters().reads, 1);
        assert_eq!(cache.cache_hits(), 1);
    }

    #[test]
    fn a_new_episode_after_update_reads_the_new_matrix() {
        let key = [4096];
        let mut memory = DeltaKvMemory::new(1, 1).unwrap();
        let mut setup = DeltaKvCounters::default();
        memory.update(&key, &[4096], 4096, &mut setup).unwrap();
        {
            let mut cache = FrozenDeltaKvRead::new(&memory, &key, 4).unwrap();
            assert_eq!(cache.read().unwrap(), [4096]);
        }
        memory.update(&key, &[-4096], 4096, &mut setup).unwrap();
        let mut cache = FrozenDeltaKvRead::new(&memory, &key, 4).unwrap();
        assert_eq!(cache.read().unwrap(), [-4096]);
        assert_eq!(cache.numeric_counters().reads, 1);
    }

    #[test]
    fn caching_preserves_a_wrong_old_value_instead_of_claiming_retention_repair() {
        let key = [4096, 0];
        let mut memory = DeltaKvMemory::new(2, 1).unwrap();
        let mut setup = DeltaKvCounters::default();
        memory.update(&key, &[4096], 4096, &mut setup).unwrap();
        memory
            .update(&[4096, 2048], &[-4096], 4096, &mut setup)
            .unwrap();
        let expected = memory.read(&key, &mut DeltaKvCounters::default()).unwrap();
        assert!(expected[0] < 0);
        let mut cache = FrozenDeltaKvRead::new(&memory, &key, 4).unwrap();
        for _ in 0..4 {
            assert_eq!(cache.read().unwrap(), expected);
        }
        assert_eq!(cache.numeric_counters().reads, 1);
        assert_eq!(cache.cache_hits(), 3);
    }

    #[test]
    fn exhausted_hit_counter_does_not_wrap_or_change_the_cached_value() {
        let memory = DeltaKvMemory::new(1, 1).unwrap();
        let key = [4096];
        let mut cache = FrozenDeltaKvRead::new(&memory, &key, 4).unwrap();
        assert_eq!(cache.read().unwrap(), [0]);
        cache.hits = u64::MAX;
        assert!(matches!(cache.read(), Err(FrozenReadError::Overflow)));
        assert_eq!(cache.hits, u64::MAX);
        assert_eq!(cache.value.as_deref(), Some([0].as_slice()));
        assert_eq!(cache.numeric_counters().reads, 1);
    }
}
