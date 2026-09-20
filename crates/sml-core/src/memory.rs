//! Bounded deterministic associative memory with explicit collision semantics.

use core::fmt;

use crate::{BooleanWorkspace, ExecutionCounters};

/// Collision behavior for a directly addressed associative slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplacementPolicy {
    /// Preserve the resident entry and reject the colliding write.
    RejectCollision,
    /// Replace the resident entry deterministically.
    ReplaceExisting,
}

/// Result of a memory read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadOutcome {
    Hit {
        slot: usize,
        value: BooleanWorkspace,
        write_epoch: u64,
    },
    EmptyMiss {
        slot: usize,
    },
    KeyMismatch {
        slot: usize,
        resident_key: u64,
    },
}

/// Result of a memory write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteOutcome {
    Inserted { slot: usize },
    Updated { slot: usize },
    CollisionRejected { slot: usize, resident_key: u64 },
    Replaced { slot: usize, evicted_key: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MemorySlot {
    key: u64,
    value: BooleanWorkspace,
    write_epoch: u64,
}

/// Fixed-capacity associative memory with O(1) direct addressing.
///
/// The reference design deliberately uses one resident entry per address.
/// Collisions are therefore visible and reproducible instead of being hidden
/// behind a history scan or approximate nearest-neighbour search.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundedAssociativeMemory {
    slots: Vec<Option<MemorySlot>>,
    value_bits: usize,
    replacement: ReplacementPolicy,
    next_epoch: u64,
}

impl BoundedAssociativeMemory {
    /// Constructs a fixed-capacity memory.
    pub fn new(
        capacity: usize,
        value_bits: usize,
        replacement: ReplacementPolicy,
    ) -> Result<Self, MemoryError> {
        if capacity == 0 {
            return Err(MemoryError::ZeroCapacity);
        }
        if value_bits == 0 {
            return Err(MemoryError::ZeroValueWidth);
        }
        Ok(Self {
            slots: vec![None; capacity],
            value_bits,
            replacement,
            next_epoch: 0,
        })
    }

    /// Number of directly addressable slots.
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.slots.len()
    }

    /// Fixed logical value width.
    #[must_use]
    pub const fn value_bits(&self) -> usize {
        self.value_bits
    }

    /// Number of currently occupied slots.
    #[must_use]
    pub fn occupied_slots(&self) -> usize {
        self.slots.iter().filter(|slot| slot.is_some()).count()
    }

    /// Canonical SML semantic-memory size, including required metadata.
    ///
    /// This is not Rust heap allocation size. Each slot accounts for:
    /// occupancy (1), key (64), write epoch (64), and the fixed-width value.
    /// Global metadata accounts for capacity (64), value width (64),
    /// replacement policy (1), and next write epoch (64).
    #[must_use]
    pub fn semantic_bits(&self) -> u128 {
        const GLOBAL_METADATA_BITS: u128 = 64 + 64 + 1 + 64;
        let slot_bits = 1_u128 + 64 + 64 + self.value_bits as u128;
        GLOBAL_METADATA_BITS + self.capacity() as u128 * slot_bits
    }

    /// Returns the direct slot selected for a key.
    #[must_use]
    pub fn slot_for_key(&self, key: u64) -> usize {
        (mix64(key) % self.capacity() as u64) as usize
    }

    /// Reads one directly addressed slot and distinguishes all miss classes.
    pub fn read(&self, key: u64, counters: &mut ExecutionCounters) -> ReadOutcome {
        counters.memory_reads = counters.memory_reads.saturating_add(1);
        let slot = self.slot_for_key(key);

        match &self.slots[slot] {
            None => {
                counters.memory_misses = counters.memory_misses.saturating_add(1);
                ReadOutcome::EmptyMiss { slot }
            }
            Some(resident) => {
                counters.memory_key_comparisons = counters.memory_key_comparisons.saturating_add(1);
                if resident.key == key {
                    counters.memory_hits = counters.memory_hits.saturating_add(1);
                    ReadOutcome::Hit {
                        slot,
                        value: resident.value.clone(),
                        write_epoch: resident.write_epoch,
                    }
                } else {
                    counters.memory_misses = counters.memory_misses.saturating_add(1);
                    ReadOutcome::KeyMismatch {
                        slot,
                        resident_key: resident.key,
                    }
                }
            }
        }
    }

    /// Writes one entry with explicit deterministic collision behavior.
    pub fn write(
        &mut self,
        key: u64,
        value: BooleanWorkspace,
        counters: &mut ExecutionCounters,
    ) -> Result<WriteOutcome, MemoryError> {
        if value.len() != self.value_bits {
            return Err(MemoryError::ValueWidth {
                expected: self.value_bits,
                actual: value.len(),
            });
        }

        counters.memory_writes = counters.memory_writes.saturating_add(1);
        let slot = self.slot_for_key(key);
        let resident_key = self.slots[slot].as_ref().map(|resident| resident.key);

        match resident_key {
            None => {
                let write_epoch = self.take_epoch()?;
                self.slots[slot] = Some(MemorySlot {
                    key,
                    value,
                    write_epoch,
                });
                Ok(WriteOutcome::Inserted { slot })
            }
            Some(existing) if existing == key => {
                counters.memory_key_comparisons = counters.memory_key_comparisons.saturating_add(1);
                let write_epoch = self.take_epoch()?;
                self.slots[slot] = Some(MemorySlot {
                    key,
                    value,
                    write_epoch,
                });
                Ok(WriteOutcome::Updated { slot })
            }
            Some(existing) => {
                counters.memory_key_comparisons = counters.memory_key_comparisons.saturating_add(1);
                counters.memory_collisions = counters.memory_collisions.saturating_add(1);
                match self.replacement {
                    ReplacementPolicy::RejectCollision => Ok(WriteOutcome::CollisionRejected {
                        slot,
                        resident_key: existing,
                    }),
                    ReplacementPolicy::ReplaceExisting => {
                        let write_epoch = self.take_epoch()?;
                        self.slots[slot] = Some(MemorySlot {
                            key,
                            value,
                            write_epoch,
                        });
                        counters.memory_replacements =
                            counters.memory_replacements.saturating_add(1);
                        Ok(WriteOutcome::Replaced {
                            slot,
                            evicted_key: existing,
                        })
                    }
                }
            }
        }
    }

    fn take_epoch(&mut self) -> Result<u64, MemoryError> {
        let current = self.next_epoch;
        self.next_epoch = self
            .next_epoch
            .checked_add(1)
            .ok_or(MemoryError::EpochOverflow)?;
        Ok(current)
    }
}

/// Memory construction or write errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryError {
    ZeroCapacity,
    ZeroValueWidth,
    ValueWidth { expected: usize, actual: usize },
    EpochOverflow,
}

impl fmt::Display for MemoryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for MemoryError {}

fn mix64(mut value: u64) -> u64 {
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}
