//! Executable SBG-REA: native R mutation -> packed dirty words ->
//! reconstructed native R -> real MOR-selected pages -> TRAIN/holdout replay.
//!
//! This is an illustrative deterministic SBG/MOR experiment with synthetic
//! signatures, not a workload benchmark or measured KV/DRAM traffic. MOR
//! executes on EVERY cycle, including a stable R field with zero dirty words.

use sml_core::{
    mor::{MixtureOfRouters, MorPolicy, PreviousRoute, RouterId},
    mor_registry::{MorRegistryPolicy, MorRouterRegistry, RouterSpecialization},
    semantic::{
        rae_dirty_journal::{SbgDirtyRoutingWord, SbgRoutingDirtyJournal},
        rae_learned_route::{
            RaeReplayPolicy, RaeTrainPolicy, RaeTrainedEventRouter, RAE_LEARNING_PPM,
        },
        rae_native_trace::SbgRaeNativeTrace,
        rising_edge::SbgNativeRouteContext,
        SemanticBitField, SemanticExpertSignature, SemanticMorBridge, SemanticMorTopK,
        SemanticRouterSignature, SemanticState,
    },
    PageId,
};

const BITS: usize = 4_096;

fn execute() -> Result<(), Box<dyn std::error::Error>> {
    let zero = SemanticBitField::<BITS>::zeroed()?;
    let single = SemanticBitField::<BITS>::from_set_indices(&[0])?;
    let registry = MorRouterRegistry::new(
        vec![
            RouterSpecialization {
                router_id: RouterId(0),
                experts: vec![PageId(0)],
            },
            RouterSpecialization {
                router_id: RouterId(1),
                experts: vec![PageId(1)],
            },
        ],
        MorRegistryPolicy {
            expert_count: 2,
            max_routers: 2,
            max_edges: 2,
            max_fanout: 1,
            require_full_coverage: true,
        },
    )?;
    let bridge = SemanticMorBridge::new(
        vec![
            SemanticRouterSignature {
                router_id: RouterId(0),
                signature: zero.clone(),
            },
            SemanticRouterSignature {
                router_id: RouterId(1),
                signature: single.clone(),
            },
        ],
        vec![
            SemanticExpertSignature {
                page_id: PageId(0),
                signature: zero.clone(),
            },
            SemanticExpertSignature {
                page_id: PageId(1),
                signature: single,
            },
        ],
        SemanticMorTopK::One,
    )?;
    let engine = MixtureOfRouters::new(MorPolicy::default())?;
    let baseline = SemanticState::<BITS>::zeroed()?;
    let mut trace = SbgRaeNativeTrace::new(&baseline, 0, 2)?;
    let mut journal = SbgRoutingDirtyJournal::new(zero, 0);
    let mut reconstructed = vec![0_u64; BITS.div_ceil(64)];
    let mut total_visited_dirty_words = 0_usize;
    let mut actual_mor_routes = 0_usize;

    println!("tick,dirty_words,rising_bits,falling_bits,mor_selected_pages");
    let pattern = [true, false, true, false, true, false, true, true];
    for (index, high) in pattern.into_iter().enumerate() {
        let tick = (index + 1) as u64;
        // One point mutation in a 64-word native R field. No need to
        // compare the other 63 packed words to create the dirty notice.
        journal.set(0, high)?;
        let mut notices = Vec::<SbgDirtyRoutingWord>::new();
        let evidence = journal.flush(tick, |dirty| {
            for word in dirty {
                reconstructed[word.index] = word.next;
            }
            notices.extend_from_slice(dirty);
            Ok::<(), std::convert::Infallible>(())
        })?;
        total_visited_dirty_words += evidence.dirty_words;

        // Full authoritative reconciliation is intentionally performed in
        // this qualification harness. It is NOT free and not part of a
        // production zero-scan or physical speedup claim.
        assert_eq!(
            reconstructed,
            journal.routing().words(),
            "dirty producer failed to reconstruct exact native R state"
        );

        let mut state = baseline.clone();
        state.routing = journal.routing().clone();
        let frame = trace.capture(
            tick,
            &state,
            SbgNativeRouteContext {
                bridge: &bridge,
                registry: &registry,
                engine: &engine,
                runtime_hints: &[],
                previous_route: &PreviousRoute::default(),
            },
            false,
        )?;
        actual_mor_routes += 1;

        let expected = if high { vec![PageId(1)] } else { vec![PageId(0)] };
        assert_eq!(frame.selected_experts, expected);
        assert_eq!(frame.rising_routing_bits, u64::from(high && tick != 8));
        assert_eq!(evidence.dirty_words, usize::from(tick != 8));
        let selected = frame
            .selected_experts
            .iter()
            .map(|id| id.0.to_string())
            .collect::<Vec<_>>()
            .join("|");
        println!(
            "{},{},{},{},{}",
            tick,
            notices.len(),
            frame.rising_routing_bits,
            frame.falling_routing_bits,
            selected,
        );
    }

    assert_eq!(actual_mor_routes, pattern.len());
    assert_eq!(total_visited_dirty_words, pattern.len() - 1);
    assert_eq!(trace.frames()[7].rising_routing_bits, 0);
    assert_eq!(trace.frames()[7].selected_experts, vec![PageId(1)]);

    // Only the first six frames are TRAIN. Both holdout frames are never
    // consumed by the learning step; the model remains frozen throughout
    // the comparison and native MOR was evaluated on all eight cycles.
    let learned = RaeTrainedEventRouter::train::<BITS>(
        "native-delta-synthetic-TRAIN",
        2,
        RaeTrainPolicy {
            minimum_exposures: 3,
            minimum_precision_ppm: RAE_LEARNING_PPM,
            max_associations: 1,
        },
        &trace.frames()[..6],
    )?;
    assert_eq!(learned.learned_feature_count(), 1);
    let holdout = &trace.frames()[6..];
    let rising = learned.evaluate_with_policy(
        "native-delta-synthetic-HOLDOUT",
        holdout,
        RaeReplayPolicy::RisingOnly,
    )?;
    let persistent = learned.evaluate_with_policy(
        "native-delta-synthetic-HOLDOUT",
        holdout,
        RaeReplayPolicy::PersistentActive,
    )?;
    assert_eq!(rising.totals.missed_required_pages, 1);
    assert_eq!(persistent.totals.missed_required_pages, 0);

    println!(
        "summary,source_total_words={},visited_dirty_words={},native_mor_routes={},rising_only_missed={},persistent_missed={},persistent_selected={}",
        BITS.div_ceil(64),
        total_visited_dirty_words,
        actual_mor_routes,
        rising.totals.missed_required_pages,
        persistent.totals.missed_required_pages,
        persistent.totals.selected_pages,
    );
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    execute()
}

#[cfg(test)]
mod tests {
    #[test]
    fn real_native_mor_runs_with_journaled_r_and_holdout_evidence() {
        super::execute().unwrap();
    }
}
