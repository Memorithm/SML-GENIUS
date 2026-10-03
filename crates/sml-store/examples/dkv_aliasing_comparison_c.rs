//! Adapted C attribution diagnostic. Never emits source-answer acceptance.
//! Original generator/constants; current bounded storage and real fixed-page MOR.
#![allow(dead_code)]
#[path = "support/counterpart_c.rs"]
mod counterpart_c;
#[path = "support/counterpart_c_identity.rs"]
mod identity;
#[path = "support/counterpart_b.rs"]
mod native;
#[path = "support/counterpart_c_store.rs"]
mod store;
use counterpart_c::*;
use identity::StateIndex;
use native::{project, RouteMode, Work};
use sml_store::delta_kv_context::{ContextError, RecordBindingLimits, RecordRequest};
use std::{collections::BTreeMap, fmt::Write as _};
use store::World;
type R<T> = Result<T, Box<dyn std::error::Error>>;
const PROTOCOL: &str = "a86fd5483cf6b90b135247df5f255247a43fd485";
fn bindings() -> RecordBindingLimits {
    RecordBindingLimits {
        max_write_records: 16,
        max_sources: 16,
    }
}
fn group<const K: usize>(
    seed: u64,
    contrast: bool,
    text: &mut String,
    summary: &mut BTreeMap<(usize, bool, String), [u64; 10]>,
) -> R<()> {
    let states: Vec<_> = (0..12).map(|j| random_state(seed, j)).collect();
    let ids: Vec<_> = states.iter().map(legacy_record_id).collect();
    let index = StateIndex::new(
        &states
            .iter()
            .cloned()
            .zip(ids.iter().copied())
            .collect::<Vec<_>>(),
        12,
    )?;
    let mut work = Work::default();
    let mut keys = Vec::new();
    for s in &states {
        keys.push(
            project::<K>(s, seed, RouteMode::NativeFixed, contrast, &mut work)?.map(|p| p.key_q12),
        );
    }
    let w = World::new(&ids, &keys, K, 1);
    let rejected_writes = keys.iter().filter(|k| k.is_none()).count();
    let before = work;
    let mut reads = 0_u64;
    let mut bytes = 0_u64;
    let mut index_work = 0_usize;
    for fact in 0..12 {
        for (kind, state) in queries(seed, fact) {
            let rid = legacy_record_id(&state);
            let (exact, index_comparisons) = index.lookup(&state);
            index_work += index_comparisons;
            // Label checks occur after the observable-index query; no label enters projection/binding.
            if (kind == "written") != exact.is_some() {
                return Err("supplied unwritten fixture aliases a full admitted state".into());
            }
            let key = project::<K>(&state, seed, RouteMode::NativeFixed, contrast, &mut work)?
                .map(|p| p.key_q12);
            let mut legacy_key = false;
            let mut legacy_record = false;
            let mut full_key = false;
            if let Some(k) = &key {
                for (id, stored) in ids.iter().zip(&keys) {
                    if let Some(stored) = stored {
                        let hash_match = key_fingerprint(k) == key_fingerprint(stored);
                        legacy_key |= hash_match;
                        legacy_record |= hash_match && *id == rid;
                        full_key |= k == stored;
                    }
                }
            }
            let mut receipt_present = false;
            let mut record_bound = false;
            let mut exact_bound = false;
            let mut source_reads = 0_usize;
            let mut source_bytes = 0_u64;
            if let Some(k) = &key {
                match w.receipt(k) {
                    Ok(r) => {
                        receipt_present = true;
                        source_reads = r.sources().len();
                        source_bytes = r.raw_bytes();
                        match r
                            .bind_record(RecordRequest::new(w.ledger.stream_id(), rid), bindings())
                        {
                            Ok(view) => {
                                for source in view.sources() {
                                    let raw = std::str::from_utf8(source.bytes())?;
                                    let value =
                                        raw.strip_prefix("fact ").ok_or("source grammar")?;
                                    let parsed: u64 = value.parse()?;
                                    if parsed != rid || parsed.to_string() != value {
                                        return Err("record raw identity mismatch".into());
                                    }
                                }
                                record_bound = true;
                            }
                            Err(
                                sml_store::delta_kv_context::RecordBindingError::NoMatchingWrite,
                            ) => {}
                            Err(e) => return Err(e.into()),
                        }
                        if let Some(id) = exact {
                            exact_bound = r
                                .bind_record(
                                    RecordRequest::new(w.ledger.stream_id(), id),
                                    bindings(),
                                )
                                .is_ok();
                        }
                    }
                    Err(ContextError::NoWriteEvidence) => {}
                    Err(e) => return Err(e.into()),
                }
            }
            reads += source_reads as u64;
            bytes += source_bytes;
            let values = [
                1,
                u64::from(legacy_key),
                u64::from(legacy_record),
                u64::from(receipt_present),
                u64::from(record_bound),
                u64::from(exact_bound),
                u64::from(exact.is_some()),
                u64::from(key.is_none()),
                u64::from(legacy_key != full_key),
                u64::from(receipt_present && exact.is_none()),
            ];
            let row = summary.entry((K, contrast, kind.clone())).or_default();
            for (a, b) in row.iter_mut().zip(values) {
                *a += b;
            }
            let vector = key
                .as_ref()
                .map(|k| k.iter().map(i16::to_string).collect::<Vec<_>>().join(";"))
                .unwrap_or_else(|| "rejected".into());
            writeln!(text,"ROW,{K},{seed},{fact},{kind},{contrast},{},{},{},{},{},{},{},{},{},{source_reads},{source_bytes},{index_comparisons},{vector}",
            values[1],values[2],values[3],values[4],values[5],values[6],values[7],values[8],values[9])?;
        }
    }
    let c = w.memory.context_counters();
    if c.query_prediction_macs != 0 {
        return Err("attribution experiment read the matrix".into());
    }
    writeln!(text,"SETUP,{K},{seed},{contrast},rejected_writes={rejected_writes},ingest_macs={},matrix_query_macs=0,setup_mor={},setup_projections={},total_mor={},total_projections={},projection_coordinates={},contrast_coordinates={},raw_reads={reads},raw_bytes={bytes},index_elements={},index_comparisons={index_work},retained_elements={},staging_elements={}",
        c.ingest_prediction_macs,before.mor_calls,before.projector_calls,work.mor_calls,work.projector_calls,work.coordinates,work.contrast_coordinates,index.element_bytes(),w.memory.element_plan().max_retained_element_bytes,w.memory.element_plan().max_write_staging_element_bytes)?;
    Ok(())
}
pub fn report() -> R<String> {
    let mut text =
        format!("PROTOCOL,{PROTOCOL}\nSTATUS,ADAPTED_DEVELOPMENT_NOT_ORIGINAL_RUST_REPLAY\n");
    text.push_str("ROW_HEADER,K,seed,fact,kind,contrast,legacy_key,legacy_record,verified_key_candidates,record_bound,exact_index_bound,index_only,rejected_query,hash_vector_disagreement,wrong_subject_candidates,raw_reads,raw_bytes,index_comparisons,key\n");
    let mut summaries = BTreeMap::new();
    for seed in 0..8 {
        for contrast in [false, true] {
            group::<16>(seed, contrast, &mut text, &mut summaries)?;
            group::<64>(seed, contrast, &mut text, &mut summaries)?;
        }
    }
    text.push_str("SUMMARY_HEADER,K,contrast,kind,queries,legacy_key,legacy_record,verified_key_candidates,record_bound,exact_index_bound,index_only,rejected_query,hash_vector_disagreement,wrong_subject_candidates\n");
    for ((k, c, kind), counts) in &summaries {
        writeln!(
            text,
            "SUMMARY,{k},{c},{kind},{}",
            counts
                .iter()
                .map(u64::to_string)
                .collect::<Vec<_>>()
                .join(",")
        )?;
    }
    let n: u64 = summaries.values().map(|c| c[0]).sum();
    if n != 3072 {
        return Err("incomplete C paired grid".into());
    }
    text.push_str("PROMOTION,BLOCKED,source_attribution_is_not_model_quality\n");
    Ok(text)
}
fn main() -> R<()> {
    print!("{}", report()?);
    Ok(())
}
