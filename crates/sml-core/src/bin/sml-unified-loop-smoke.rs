//! Bounded, synthetic integration smoke executable; not SML-HARNESS or a benchmark.
use sml_core::{
    anf_large::LargeAnfGate,
    anf_large_bank::LargeAnfBank,
    anf_sbg_unified::{unified_anf_sbg_loop, UnifiedAnfSbgLoop},
    delta_kv::{DeltaKvCounters, DeltaKvMemory, DELTA_KV_SCALE},
    delta_kv_dynamic::DeltaKvDynamicRekeyKernel,
    delta_kv_recurrent::DeltaKvRecurrentPolicy,
    delta_kv_semantic::{
        DeltaKvSemanticProjectionPolicy, DeltaKvSemanticProjector, DeltaKvSourceProvenance,
    },
    mor::{MixtureOfRouters, MorPolicy, PreviousRoute, RouterId},
    mor_registry::{MorRegistryPolicy, MorRouterRegistry, RouterSpecialization},
    pascal_vector_projection::Pvp6PopulationProfile,
    pvp6_executor::{Pvp6Bank, Pvp6Gate},
    pvp6_sbg::{SbgAnfPlan, SbgCoordinate, SbgField},
    pvp6_sbg_atlas::{SbgExpertAtlas, SbgExpertWork},
    pvp6_sbg_experts::SbgAnfExpert,
    pvp6_sbg_routing::SbgRoutingContext,
    semantic::{
        run_semantic_recurrent, SemanticBitField, SemanticExpertSignature, SemanticMorBridge,
        SemanticMorTopK, SemanticRouterSignature, SemanticState,
    },
    HaltingPolicy, PageId,
};
use std::{fmt::Debug, fmt::Write, process::ExitCode};

const MAX_CYCLES: u32 = 64;
const USAGE: &str =
    "sml-unified-loop-smoke [cycles:1..64] [value:0..255] [routing:0..255] [identity|toggle]";
type Kernel = UnifiedAnfSbgLoop<8, 64, 8>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Config {
    cycles: u32,
    value: u8,
    routing: u8,
    toggle: bool,
}

fn checked<T, E: Debug>(result: Result<T, E>) -> Result<T, String> {
    result.map_err(|error| format!("{error:?}"))
}

fn parse(args: &[String]) -> Result<Config, String> {
    if args.len() > 4 {
        return Err(USAGE.to_owned());
    }
    let cycles = checked(args.first().map_or("4", String::as_str).parse::<u32>())?;
    if !(1..=MAX_CYCLES).contains(&cycles) {
        return Err(USAGE.to_owned());
    }
    let value = checked(args.get(1).map_or("0", String::as_str).parse::<u8>())?;
    let routing = checked(args.get(2).map_or("0", String::as_str).parse::<u8>())?;
    let toggle = match args.get(3).map_or("toggle", String::as_str) {
        "identity" => false,
        "toggle" => true,
        _ => return Err(USAGE.to_owned()),
    };
    Ok(Config {
        cycles,
        value,
        routing,
        toggle,
    })
}

fn initial(config: Config) -> Result<SemanticState<8>, String> {
    let mut state = checked(SemanticState::zeroed())?;
    for bit in 0..8 {
        checked(state.value.set(bit, config.value & (1 << bit) != 0))?;
        checked(state.routing.set(bit, config.routing & (1 << bit) != 0))?;
    }
    Ok(state)
}

fn byte(field: &SemanticBitField<8>) -> Result<u8, String> {
    let mut value = 0;
    for bit in 0..8 {
        value |= u8::from(checked(field.get(bit))?) << bit;
    }
    Ok(value)
}

fn context() -> Result<SbgRoutingContext<8>, String> {
    let registry = checked(MorRouterRegistry::new(
        vec![RouterSpecialization {
            router_id: RouterId(0),
            experts: vec![PageId(0), PageId(1)],
        }],
        MorRegistryPolicy {
            expert_count: 2,
            max_routers: 1,
            max_edges: 2,
            max_fanout: 2,
            require_full_coverage: true,
        },
    ))?;
    let bridge = checked(SemanticMorBridge::new(
        vec![SemanticRouterSignature {
            router_id: RouterId(0),
            signature: checked(SemanticBitField::from_set_indices(&[0, 1, 2, 3]))?,
        }],
        vec![
            SemanticExpertSignature {
                page_id: PageId(0),
                signature: checked(SemanticBitField::zeroed())?,
            },
            SemanticExpertSignature {
                page_id: PageId(1),
                signature: checked(SemanticBitField::ones())?,
            },
        ],
        SemanticMorTopK::One,
    ))?;
    Ok(SbgRoutingContext {
        bridge,
        registry,
        engine: checked(MixtureOfRouters::new(MorPolicy::default()))?,
        runtime_hints: vec![],
    })
}

fn page(id: u32, terms: &[u16]) -> Result<SbgAnfExpert<8, 64>, String> {
    let plan = checked(SbgAnfPlan::new(
        vec![(0..6)
            .map(|bit| SbgCoordinate {
                field: SbgField::Value,
                bit,
            })
            .collect()],
        vec![SbgCoordinate {
            field: SbgField::Value,
            bit: 0,
        }],
    ))?;
    let bank = checked(Pvp6Bank::new(vec![checked(Pvp6Gate::from_monomials(
        terms,
    ))?]))?;
    checked(SbgAnfExpert::new(
        PageId(id),
        bank,
        plan,
        Pvp6PopulationProfile::AffineUniform,
    ))
}

fn pipeline(config: Config) -> Result<Kernel, String> {
    if !(1..=MAX_CYCLES).contains(&config.cycles) {
        return Err(USAGE.to_owned());
    }
    let gates = (0..8)
        .map(|bit| {
            let mut terms = vec![1_u32 << bit];
            if config.toggle && bit == 7 {
                terms.push(0);
            }
            checked(LargeAnfGate::new(20, terms))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let bank = checked(LargeAnfBank::new(gates))?;
    let routing = context()?;
    let seed = checked(SemanticState::zeroed())?;
    let route = checked(routing.bridge.route(
        &seed,
        &routing.registry,
        &routing.engine,
        &[],
        &PreviousRoute::default(),
    ))?
    .decision;
    let projector = checked(DeltaKvSemanticProjector::new(
        DeltaKvSemanticProjectionPolicy {
            semantic_weight: 1,
            mor_weight: 32,
        },
        17,
    ))?;
    let key = checked(projector.project(&seed, &route, None))?;
    let mut memory = checked(DeltaKvMemory::new(8, 1))?;
    checked(memory.update(
        &key.key_q12,
        &[2000],
        DELTA_KV_SCALE as u16,
        &mut DeltaKvCounters::default(),
    ))?;
    let memory = checked(DeltaKvDynamicRekeyKernel::new(
        memory,
        projector,
        route,
        Some(DeltaKvSourceProvenance {
            semantic_record_id: 7,
            raw_segment_ids: vec![11, 13],
        }),
        vec![1],
        DeltaKvRecurrentPolicy::default(),
    ))?;
    let atlas = checked(SbgExpertAtlas::new(vec![page(0, &[0])?, page(1, &[1])?], 2))?;
    let cycles = u64::from(config.cycles);
    checked(unified_anf_sbg_loop(
        bank,
        routing,
        atlas,
        memory,
        config.cycles,
        SbgExpertWork {
            pages: cycles,
            gate_queries: cycles,
            retained_words: 2 * cycles,
        },
    ))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Run {
    kernel: Kernel,
    final_state: SemanticState<8>,
}

fn execute(config: Config) -> Result<Run, String> {
    let mut kernel = pipeline(config)?;
    let output = checked(run_semantic_recurrent(
        initial(config)?,
        &mut kernel,
        HaltingPolicy {
            min_depth: config.cycles,
            max_depth: config.cycles,
            stable_hamming_threshold: 0,
        },
    ))?;
    let expected = config.cycles as usize;
    if output.depth != config.cycles
        || kernel.cycles().len() != expected
        || kernel.experts().cycles().len() != expected
        || kernel.experts().routed().cycles().len() != expected
        || kernel.experts().routed().inner().history().len() != expected
        || kernel.memory().cycle_evidence().len() != expected
    {
        return Err("incomplete or misaligned committed trajectory".to_owned());
    }
    Ok(Run {
        kernel,
        final_state: output.state,
    })
}

fn report(config: Config, run: &Run) -> Result<String, String> {
    let mut text = String::new();
    checked(writeln!(
        text,
        "{{\"schema\":\"sml.unified-loop-smoke/v1\",\"kind\":\"header\",\"synthetic\":true,\"cycles_requested\":{},\"initial_value\":{},\"initial_routing\":{},\"toggle_anf\":{},\"semantic_bits\":8,\"anf_address_bits\":20,\"anf_domain_size\":1048576,\"anf_gates\":8,\"atlas_pages\":2,\"expert_backend\":\"pvp6\",\"large_anf_selected_experts\":false,\"performance_claim\":false}}",
        config.cycles, config.value, config.routing, config.toggle
    ))?;
    let mut input = config.value;
    for (index, cycle) in run.kernel.cycles().iter().enumerate() {
        let anf = &run.kernel.experts().routed().inner().history()[index];
        let expert = &run.kernel.experts().cycles()[index];
        let selected: Vec<u32> = cycle.selected.iter().map(|page| page.0).collect();
        checked(writeln!(
            text,
            "{{\"kind\":\"cycle\",\"depth\":{},\"input_value\":{},\"anf_words\":{:?},\"selected_pages\":{:?},\"executed_pages\":{},\"gate_queries\":{},\"post_expert_value\":{},\"memory_key_q12\":{:?},\"memory_read_macs\":{},\"output_value\":{}}}",
            cycle.depth,
            input,
            anf,
            selected,
            expert.pages.len(),
            expert.work.gate_queries,
            byte(&cycle.post_expert.value)?,
            cycle.memory.key_q12,
            cycle.memory.read_macs_delta,
            byte(&cycle.output.value)?
        ))?;
        input = byte(&cycle.output.value)?;
    }
    checked(writeln!(
        text,
        "{{\"kind\":\"summary\",\"cycles_committed\":{},\"anf_cycles\":{},\"routing_cycles\":{},\"expert_cycles\":{},\"memory_cycles\":{},\"final_value\":{},\"final_routing\":{}}}",
        run.kernel.cycles().len(),
        run.kernel.experts().routed().inner().history().len(),
        run.kernel.experts().routed().cycles().len(),
        run.kernel.experts().cycles().len(),
        run.kernel.memory().cycle_evidence().len(),
        byte(&run.final_state.value)?,
        byte(&run.final_state.routing)?
    ))?;
    Ok(text)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() == 1 && matches!(args[0].as_str(), "-h" | "--help") {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    let result = parse(&args).and_then(|config| {
        let run = execute(config)?;
        report(config, &run)
    });
    match result {
        Ok(text) => {
            print!("{text}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("unified-loop smoke rejected: {error}");
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sml_core::{
        pvp6_sbg_coupled::{CoupledSbgError, SbgMemoryWork},
        semantic::SemanticRewriter,
    };

    fn config() -> Config {
        Config {
            cycles: 4,
            value: 165,
            routing: 0,
            toggle: true,
        }
    }

    #[test]
    fn input_bounds_fail_before_pipeline_construction() {
        assert_eq!(parse(&[]).unwrap().cycles, 4);
        for args in [
            vec!["0"],
            vec!["65"],
            vec!["4", "256"],
            vec!["4", "0", "256"],
            vec!["4", "0", "0", "unknown"],
            vec!["4", "0", "0", "toggle", "extra"],
        ] {
            let args: Vec<String> = args.into_iter().map(str::to_owned).collect();
            assert!(parse(&args).is_err());
        }
    }

    #[test]
    fn native_runner_matches_explicit_stage_execution_and_literal_anf_oracle() {
        for toggle in [false, true] {
            for routing in [0, 255] {
                for value in [0, 165, 255] {
                    let config = Config {
                        toggle,
                        routing,
                        value,
                        ..config()
                    };
                    let actual = execute(config).unwrap();
                    let reference = pipeline(config).unwrap();
                    let mut routed = reference.experts().routed().clone();
                    let mut memory = reference.memory().clone();
                    let atlas = reference.experts().atlas();
                    let mut state = initial(config).unwrap();
                    for (index, cycle) in actual.kernel.cycles().iter().enumerate() {
                        let expected_anf =
                            byte(&state.value).unwrap() ^ if toggle { 128 } else { 0 };
                        let rewritten = routed.rewrite(&state, cycle.depth).unwrap();
                        assert_eq!(byte(&rewritten.state.value).unwrap(), expected_anf);
                        assert_eq!(
                            actual.kernel.experts().routed().inner().history()[index],
                            vec![u64::from(expected_anf)]
                        );
                        let decision = &routed.cycles().last().unwrap().route.decision;
                        let batch = atlas.execute(&rewritten.state, decision).unwrap();
                        assert_eq!(cycle.selected, decision.selected_experts);
                        assert_eq!(cycle.post_expert, batch.state);
                        assert_eq!(actual.kernel.experts().cycles()[index].pages, batch.pages);
                        let output = memory
                            .rewrite_with_mor(&batch.state, cycle.depth, decision)
                            .unwrap();
                        assert_eq!(cycle.output, output.state);
                        assert_eq!(&cycle.memory, memory.cycle_evidence().last().unwrap());
                        state = output.state;
                    }
                    assert_eq!(actual.final_state, state);
                    assert_eq!(actual.kernel.memory(), &memory);
                    assert_eq!(actual.kernel.experts().routed(), &routed);
                }
            }
        }
    }

    #[test]
    fn late_memory_rejection_preserves_every_committed_stage() {
        let config = config();
        let kernel = pipeline(config).unwrap();
        let total = kernel.memory_budget();
        let count = u64::from(config.cycles);
        let mut kernel = kernel
            .with_memory_budget(SbgMemoryWork {
                cycles: 1,
                read_macs: total.read_macs / count,
                projected_coordinates: total.projected_coordinates / count,
                retained_units: total.retained_units / count,
            })
            .unwrap();
        let step = kernel.rewrite(&initial(config).unwrap(), 1).unwrap();
        let before = kernel.clone();
        assert!(matches!(
            kernel.rewrite(&step.state, 2),
            Err(CoupledSbgError::Budget)
        ));
        assert_eq!(kernel, before);
        assert!(matches!(
            kernel.rewrite(&step.state, 1),
            Err(CoupledSbgError::Depth)
        ));
        assert_eq!(kernel, before);
        assert_eq!(kernel.experts().routed().inner().history().len(), 1);
        assert_eq!(kernel.memory().cycle_evidence().len(), 1);
    }

    #[test]
    fn maximum_depth_replay_has_identical_full_state_and_report() {
        let config = Config {
            cycles: MAX_CYCLES,
            ..config()
        };
        let first = execute(config).unwrap();
        let second = execute(config).unwrap();
        assert_eq!(first, second);
        let text = report(config, &first).unwrap();
        assert_eq!(text, report(config, &second).unwrap());
        assert_eq!(text.lines().count(), MAX_CYCLES as usize + 2);
        assert!(text.contains("\"large_anf_selected_experts\":false"));
        assert!(text.contains("\"performance_claim\":false"));
    }
}
