//! Complete-state fixture index; NOT a general semantic identity service.
#![allow(dead_code)]
use sml_core::semantic::SemanticState;
pub fn canonical(s: &SemanticState<64>) -> [u64; 4] {
    let mut words = [0; 4];
    for (j, f) in [&s.value, &s.meaning_mask, &s.ambiguity, &s.routing]
        .into_iter()
        .enumerate()
    {
        for bit in 0..64 {
            if f.get(bit).unwrap() {
                words[j] |= 1_u64 << bit;
            }
        }
    }
    words
}
#[derive(Debug)]
pub struct StateIndex {
    entries: Vec<([u64; 4], u64)>,
}
impl StateIndex {
    pub fn new(
        states: &[(SemanticState<64>, u64)],
        max_entries: usize,
    ) -> Result<Self, &'static str> {
        if max_entries == 0 || states.len() > max_entries {
            return Err("index limit");
        }
        let mut entries: Vec<_> = states.iter().map(|(s, id)| (canonical(s), *id)).collect();
        entries.sort_unstable();
        for (i, (state, id)) in entries.iter().enumerate() {
            if entries[..i]
                .iter()
                .any(|(other, oid)| other == state || oid == id)
            {
                return Err("duplicate state or surrogate identity");
            }
        }
        Ok(Self { entries })
    }
    pub fn lookup(&self, s: &SemanticState<64>) -> (Option<u64>, usize) {
        let key = canonical(s);
        let mut comparisons = 0;
        let result = self.entries.binary_search_by(|(k, _)| {
            comparisons += 1;
            k.cmp(&key)
        });
        (result.ok().map(|i| self.entries[i].1), comparisons)
    }
    pub fn element_bytes(&self) -> usize {
        self.entries.len() * core::mem::size_of::<([u64; 4], u64)>()
    }
}
