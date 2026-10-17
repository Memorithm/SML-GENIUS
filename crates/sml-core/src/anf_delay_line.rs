//! Bit-packed causal delay line for an already observed Boolean ANF feature.
//! Reference oracle, not learned recurrence: suitable for controlled comparisons.
#[derive(Clone, Debug)]
pub struct AnfDelayLine {
    words: Vec<u64>,
    capacity: usize,
    next: usize,
    observed: usize,
}

impl AnfDelayLine {
    /// Capacity is a **number of past ANF observations**, not raw bytes.
    /// This reference implementation deliberately bounds one allocation.
    pub fn new(capacity: usize) -> Result<Self, &'static str> {
        if !(1..=1_048_576).contains(&capacity) {
            return Err("ANF delay-line capacity must be in 1..=1_048_576 bits");
        }
        Ok(Self {
            words: vec![0_u64; capacity.div_ceil(64)],
            capacity,
            next: 0,
            observed: 0,
        })
    }

    #[must_use]
    pub const fn capacity(&self) -> usize {
        self.capacity
    }

    #[must_use]
    pub fn storage_bytes(&self) -> usize {
        self.words.len() * size_of::<u64>()
    }

    /// Forget volatile history, but retain allocated packed storage.
    pub fn reset(&mut self) {
        self.words.fill(0);
        self.next = 0;
        self.observed = 0;
    }

    /// Observe exactly the pair the causal recurrent model also receives:
    /// (source[t-2], source[t-1]). No label or future byte is read.
    pub fn observe_pair(&mut self, older: u8, newer: u8) {
        let feature = ((older & newer).count_ones() & 1) != 0;
        let index = self.next;
        let mask = 1_u64 << (index % 64);
        let slot = &mut self.words[index / 64];
        if feature {
            *slot |= mask;
        } else {
            *slot &= !mask;
        }
        self.next = (self.next + 1) % self.capacity;
        self.observed = self.observed.saturating_add(1).min(self.capacity);
    }

    /// `lag=1` means the most recently observed ANF pair. A query
    /// cannot read a future, unobserved or evicted feature.
    #[must_use]
    pub fn read_lag(&self, lag: usize) -> Option<bool> {
        if lag == 0 || lag > self.observed {
            return None;
        }
        let index = (self.next + self.capacity - lag) % self.capacity;
        Some((self.words[index / 64] & (1_u64 << (index % 64))) != 0)
    }
}

#[cfg(test)]
mod tests {
    use super::AnfDelayLine;

    #[test]
    fn capacity_and_strict_observation_boundary() {
        assert!(AnfDelayLine::new(0).is_err());
        assert!(AnfDelayLine::new(1_048_577).is_err());
        let mut line = AnfDelayLine::new(1024).unwrap();
        assert_eq!(line.capacity(), 1024);
        assert_eq!(line.storage_bytes(), 128);
        assert_eq!(line.read_lag(0), None);
        assert_eq!(line.read_lag(1), None);
        line.observe_pair(0xff, 0x01);
        assert_eq!(line.read_lag(1), Some(true));
        assert_eq!(line.read_lag(2), None);
        line.reset();
        assert_eq!(line.read_lag(1), None);
        assert_eq!(line.storage_bytes(), 128);
    }

    #[test]
    fn bit_packed_ring_wrap_and_eviction() {
        let mut line = AnfDelayLine::new(129).unwrap();
        let mut reference = Vec::new();
        for i in 0..2048 {
            let older = (i as u8).wrapping_mul(59);
            let newer = ((i / 3) as u8).wrapping_mul(17);
            let expected = ((older & newer).count_ones() & 1) != 0;
            line.observe_pair(older, newer);
            reference.push(expected);
            for lag in [1, 2, 16, 64, 65, 128, 129] {
                let target = if lag <= reference.len() {
                    Some(reference[reference.len() - lag])
                } else {
                    None
                };
                assert_eq!(line.read_lag(lag), target);
            }
            assert_eq!(line.read_lag(130), None);
        }
        assert_eq!(line.storage_bytes(), 24);
    }

    #[test]
    fn exact_causal_anf_label_for_delays_up_to_1023() {
        let data: Vec<u8> = (0..=4096)
            .map(|i| (i as u8).wrapping_mul(37) ^ ((i / 19) as u8))
            .collect();
        let mut line = AnfDelayLine::new(1024).unwrap();
        for t in 2..1024 {
            line.observe_pair(data[t - 2], data[t - 1]);
        }
        for i in 1024..4096 {
            line.observe_pair(data[i - 2], data[i - 1]);
            for lag in [1_usize, 16, 128, 1023] {
                let expected = ((data[i - lag - 1] & data[i - lag]).count_ones() & 1) != 0;
                assert_eq!(line.read_lag(lag), Some(expected));
            }
        }
    }
}
