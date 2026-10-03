//! Bounded test/example source world, not a deployed storage service.
#![allow(dead_code)]
use sml_core::context_ledger::{
    ContextSegmentId, ContextSegmentRef, ContextStreamId, RawContextLedger,
};
use sml_store::delta_kv_context::{
    ContextError, ContextLimits, DeltaKvContextMemory, VerificationLimits, VerifiedContext,
};
use sml_store::raw_context::sha256_raw_bytes;
pub struct World {
    pub ledger: RawContextLedger,
    pub memory: DeltaKvContextMemory,
    pub raw: Vec<Vec<u8>>,
}
impl World {
    pub fn new(ids: &[u64], keys: &[Option<Vec<i16>>], k: usize, stream: u64) -> Self {
        Self::with_raw(
            ids,
            ids.iter()
                .map(|id| format!("fact {id}").into_bytes())
                .collect(),
            keys,
            k,
            stream,
        )
    }
    pub fn with_raw(
        ids: &[u64],
        raw: Vec<Vec<u8>>,
        keys: &[Option<Vec<i16>>],
        k: usize,
        stream: u64,
    ) -> Self {
        assert_eq!(ids.len(), raw.len());
        assert_eq!(ids.len(), keys.len());
        assert!(ids.len() <= 16);
        let stream = ContextStreamId::new(stream).unwrap();
        let mut ledger = RawContextLedger::new(stream);
        for (i, (&id, bytes)) in ids.iter().zip(&raw).enumerate() {
            let r = ContextSegmentRef::new(
                ContextSegmentId::new(stream, i as u64 + 1).unwrap(),
                ledger.stats().raw_bytes,
                bytes.len() as u64,
                0,
                0,
                i as u64,
                sha256_raw_bytes(bytes).unwrap(),
                vec![id],
            )
            .unwrap();
            ledger.append(r).unwrap();
        }
        let mut memory = DeltaKvContextMemory::new(
            stream,
            k,
            2,
            ContextLimits {
                max_writes: 16,
                max_sources_per_write: 16,
                max_source_links: 256,
                max_ledger_segments: 16,
                max_ledger_semantic_links: 256,
                max_element_bytes: 1 << 20,
                max_staging_bytes: 1 << 20,
            },
        )
        .unwrap();
        for (id, key) in ids.iter().zip(keys) {
            if let Some(key) = key {
                let sources = ledger
                    .segments()
                    .iter()
                    .filter(|r| r.semantic_record_ids().contains(id))
                    .map(|r| r.identity)
                    .collect::<Vec<_>>();
                memory
                    .write(&ledger, *id, key, &[4096, 4096], 4096, &sources)
                    .unwrap();
            }
        }
        Self {
            ledger,
            memory,
            raw,
        }
    }
    pub fn receipt(&self, key: &[i16]) -> Result<VerifiedContext<'_>, ContextError> {
        self.memory.query(key)?.verify(
            &self.ledger,
            VerificationLimits {
                max_sources: 16,
                max_raw_bytes: 65536,
            },
            |r| Ok(self.raw[(r.identity.segment_id - 1) as usize].clone()),
        )
    }
}
