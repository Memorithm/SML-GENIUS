//! Development diagnostic; preserves B's fixed output coordinates, not A's expert circuit.
#[path = "support/counterpart_b.rs"]
mod fixture;
use fixture::*;
use sml_core::{
    delta_kv::{DeltaKvCounters, DeltaKvMemory},
    delta_kv_diagnostics::KeyGeometry,
    delta_kv_recurrent::{DeltaKvRecurrentKernel, DeltaKvRecurrentPolicy},
    delta_kv_resolution::Resolution2,
    semantic::{run_semantic_recurrent, SemanticState},
    HaltingPolicy,
};
use std::{collections::BTreeMap, fmt::Write as _};
fn boxed(e: impl std::fmt::Debug) -> Box<dyn std::error::Error> {
    format!("{e:?}").into()
}
fn key_text(k: &[i16]) -> String {
    k.iter().map(i16::to_string).collect::<Vec<_>>().join(";")
}
fn bit_text(v: Option<[bool; 2]>) -> String {
    v.map_or_else(
        || "unknown".into(),
        |v| format!("{}{}", u8::from(v[0]), u8::from(v[1])),
    )
}
#[derive(Clone, Copy, Debug)]
pub enum Order {
    Old,
    Recent,
    Missing,
}
impl Order {
    fn name(self) -> &'static str {
        match self {
            Self::Old => "old",
            Self::Recent => "recent",
            Self::Missing => "missing",
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultRow {
    pub correct: bool,
    pub partial: bool,
    pub resolved_missing: bool,
    pub rejected: bool,
    pub pair: Option<[bool; 2]>,
    pub depth: u32,
    pub reads: u64,
    pub macs: u64,
}
#[derive(Default)]
struct Total {
    rows: u64,
    correct: u64,
    partial: u64,
    resolved_missing: u64,
    rejected: u64,
    reads: u64,
    macs: u64,
}
fn projection_name(contrast: bool) -> &'static str {
    if contrast {
        "contrast"
    } else {
        "standard"
    }
}

pub fn cell<const K: usize>(
    mode: RouteMode,
    contrast: bool,
    h: usize,
    seed: u64,
    order: Order,
    text: &mut String,
) -> R<ResultRow> {
    let initial = target_state(seed);
    let mut work = Work::default();
    let query = project::<K>(&initial, seed, mode, contrast, &mut work)?;
    let states = (0..5)
        .map(|d| distractor_state(seed, d, h))
        .collect::<Vec<_>>();
    let mut keys = Vec::new();
    for state in &states {
        keys.push(project::<K>(state, seed, mode, contrast, &mut work)?);
    }
    let label = format!(
        "{},{},{K},{h},{seed},{}",
        mode.name(),
        projection_name(contrast),
        order.name()
    );
    let mut duplicates = 0;
    for i in 0..5 {
        for j in 0..i {
            duplicates += usize::from(states[i] == states[j]);
        }
    }
    let Some(query) = query else {
        writeln!(
            text,
            "ROW,{label},rejected,unknown,0,0,0,0,0,0,0,0,0,0,{},{}",
            work.mor_calls, work.projector_calls
        )?;
        return Ok(ResultRow {
            correct: false,
            partial: false,
            resolved_missing: false,
            rejected: true,
            pair: None,
            depth: 0,
            reads: 0,
            macs: 0,
        });
    };
    writeln!(
        text,
        "KEY,{label},query,{},{}",
        query.selected_experts[0].0,
        key_text(&query.key_q12)
    )?;
    let mut aliases = 0;
    for (i, key) in keys.iter().enumerate() {
        if let Some(key) = key {
            let g = KeyGeometry::measure(&query.key_q12, &key.key_q12, 128)?;
            writeln!(
                text,
                "GRAM,{label},{i},{},{},{},{}",
                g.dot, g.left_norm_sq, g.right_norm_sq, key.selected_experts[0].0
            )?;
            writeln!(
                text,
                "KEY,{label},{i},{},{}",
                key.selected_experts[0].0,
                key_text(&key.key_q12)
            )?;
            aliases += usize::from(key.key_q12 == query.key_q12);
        } else {
            writeln!(text, "KEY,{label},{i},rejected")?;
        }
    }
    let mut memory = DeltaKvMemory::new(K, 2)?;
    let mut numeric = DeltaKvCounters::default();
    if matches!(order, Order::Old) {
        memory.update(&query.key_q12, &[4096; 2], 4096, &mut numeric)?;
    }
    for key in keys.iter().flatten() {
        memory.update(&key.key_q12, &[-4096; 2], 4096, &mut numeric)?;
    }
    if matches!(order, Order::Recent) {
        memory.update(&query.key_q12, &[4096; 2], 4096, &mut numeric)?;
    }
    let ingest = numeric.read_macs;
    let saturations = numeric.saturations;
    let rejected_writes = keys.iter().filter(|k| k.is_none()).count();
    let page = query.selected_experts[0].0;
    let frozen = query.clone();
    let mut kernel = DeltaKvRecurrentKernel::<BITS>::new(
        memory,
        query,
        vec![0, 1],
        DeltaKvRecurrentPolicy::default(),
    )
    .map_err(boxed)?;
    let outcome = run_semantic_recurrent(
        initial.clone(),
        &mut kernel,
        HaltingPolicy {
            min_depth: 1,
            max_depth: 2,
            stable_hamming_threshold: 0,
        },
    )
    .map_err(boxed)?;
    assert_eq!(kernel.key_evidence(), &frozen);
    assert_eq!(initial.meaning_mask, outcome.state.meaning_mask);
    assert_eq!(initial.routing, outcome.state.routing);
    assert!(outcome.depth <= 2);
    let read_pair = |s: &SemanticState<BITS>, ambiguous: bool| -> R<[bool; 2]> {
        let f = if ambiguous { &s.ambiguity } else { &s.value };
        Ok([f.get(0).map_err(boxed)?, f.get(1).map_err(boxed)?])
    };
    let r = Resolution2::from_bits(
        read_pair(&outcome.state, false)?,
        read_pair(&outcome.state, true)?,
    );
    let missing = matches!(order, Order::Missing);
    let correct = if missing {
        r == Resolution2::Unknown
    } else {
        r.exact_pair() == Some([true; 2])
    };
    let partial = matches!(r, Resolution2::Partial { .. });
    let resolved_missing = missing && r.resolved_count() != 0;
    let class = match r {
        Resolution2::Unknown => "unknown",
        Resolution2::Partial { .. } => "partial",
        Resolution2::Complete(_) => "complete",
    };
    let counts = kernel.counters();
    assert_eq!(counts.read_macs, counts.reads * K as u64 * 2);
    let row = ResultRow {
        correct,
        partial,
        resolved_missing,
        rejected: false,
        pair: r.exact_pair(),
        depth: outcome.depth,
        reads: counts.reads,
        macs: counts.read_macs,
    };
    writeln!(
        text,
        "ROW,{label},{class},{},{},{},{},{},{},{},{},{},{},{},{},{}",
        bit_text(row.pair),
        u8::from(correct),
        u8::from(partial),
        u8::from(resolved_missing),
        outcome.depth,
        counts.reads,
        counts.read_macs,
        ingest,
        saturations,
        aliases,
        duplicates,
        work.mor_calls,
        work.projector_calls
    )?;
    writeln!(text,"WORK,{label},page={page},projection_coordinates={},contrast_coordinates={},rejected_writes={rejected_writes},matrix_instances=1,matrix_coefficients_bytes={}",work.coordinates,work.contrast_coordinates,K*2*2)?;
    for cycle in kernel.cycle_evidence() {
        writeln!(
            text,
            "READ,{label},{},{},{},{}",
            cycle.depth, cycle.read_values_q12[0], cycle.read_values_q12[1], cycle.ambiguous_values
        )?;
    }
    for trace in outcome.trace {
        writeln!(
            text,
            "TRACE,{label},{},{},{},{},{},{}",
            trace.depth,
            trace.delta.value,
            trace.delta.meaning_mask,
            trace.delta.ambiguity,
            trace.delta.routing,
            u8::from(trace.stability_blocked)
        )?;
    }
    Ok(row)
}
pub fn report() -> R<String> {
    let mut text=String::from("PROTOCOL,472d2f6805873ef8078f0fbe1f566a06b53a09ac\nSTATUS,DEVELOPMENT_DIAGNOSTIC_ONLY\nROW_HEADER,route,projection,K,h,seed,order,class,pair,correct,partial,missing_resolved,depth,reads,query_macs,ingest_macs,saturations,aliases,input_duplicates,mor_calls,projector_calls\n");
    let mut totals: BTreeMap<(String, String, String), Total> = BTreeMap::new();
    for mode in RouteMode::ALL {
        for contrast in [false, true] {
            for h in [1, 2, 4, 8, 16, 32] {
                for seed in 0..8 {
                    for order in [Order::Old, Order::Recent, Order::Missing] {
                        for dimension in [16, 64] {
                            let row = if dimension == 16 {
                                cell::<16>(mode, contrast, h, seed, order, &mut text)?
                            } else {
                                cell::<64>(mode, contrast, h, seed, order, &mut text)?
                            };
                            let total = totals
                                .entry((
                                    mode.name().into(),
                                    projection_name(contrast).into(),
                                    order.name().into(),
                                ))
                                .or_default();
                            total.rows += 1;
                            total.correct += u64::from(row.correct);
                            total.partial += u64::from(row.partial);
                            total.resolved_missing += u64::from(row.resolved_missing);
                            total.rejected += u64::from(row.rejected);
                            total.reads += row.reads;
                            total.macs += row.macs;
                        }
                    }
                }
            }
        }
    }
    assert_eq!(totals.values().map(|t| t.rows).sum::<u64>(), 1728);
    for ((mode, projection, order), t) in totals {
        writeln!(text,"SUMMARY,{mode},{projection},{order},rows={},correct={},partial={},missing_resolved={},rejected={},reads={},query_macs={}",t.rows,t.correct,t.partial,t.resolved_missing,t.rejected,t.reads,t.macs)?;
    }
    text.push_str("PROMOTION,BLOCKED,no_source_or_model_qualification\n");
    Ok(text)
}
fn main() -> R<()> {
    print!("{}", report()?);
    Ok(())
}
