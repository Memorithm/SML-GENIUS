#![forbid(unsafe_code)]

use sml_core::{
    anf::{AnfMonomial, AnfPolynomial},
    atlas::{AtlasPageManifest, CircuitAtlas},
    audit::validate_non_attention_candidate,
    candidate::{SmlTaskCandidate, TaskCandidateConfig},
    circuit::{CircuitNodeSpec, CircuitSample, LatentBooleanCircuit, SignalRef},
    elastic_weights::{
        ElasticWeightPage, ElasticWeightPlanner, ElasticWeightPolicy, ResidencyTier,
        WeightPrecision,
    },
    forge_topology::ForgeTopologyCandidateV1,
    gf2::{Gf2LinearMap, Gf2Vector},
    memory::{BoundedAssociativeMemory, ReplacementPolicy},
    routing_train::{LatentDirectRouter, RouteLearningBudget, RouteTrainingConfig},
    semantic::{
        packed_word4_score, qualify_packed_word4, scalar_reference_score, SemanticBitField,
    },
    task_loss::{
        evaluate_task_panel, train_conjunction_from_tasks, train_state_update_from_tasks,
        ConjunctionTaskTrainingConfig, StateUpdateTaskTrainingConfig,
    },
    tasks::{development_suite, suite_from_case_id},
    trace::run_recurrent_traced,
    train::{GateTrainingConfig, LatentGate2, TruthTable2},
    BooleanWorkspace, ExecutionCounters, HaltingPolicy, PageId, ParameterBudget, ReasoningKernel,
    ReasoningStep, SML_25B_INITIAL_BUDGET, TARGET_RAM_BYTES, TARGET_TOKENS_PER_SECOND,
    TARGET_VRAM_BYTES,
};

fn main() {
    let command = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "contract".to_owned());
    match command.as_str() {
        "contract" => print_contract(),
        "sml01-smoke" => {
            if let Err(error) = sml01_smoke() {
                eprintln!("SML-0.1 smoke failed: {error}");
                std::process::exit(1);
            }
        }
        "sml02-smoke" => {
            if let Err(error) = sml02_smoke() {
                eprintln!("SML-0.2 smoke failed: {error}");
                std::process::exit(1);
            }
        }
        "sml03-smoke" => {
            if let Err(error) = sml03_smoke() {
                eprintln!("SML-0.3 smoke failed: {error}");
                std::process::exit(1);
            }
        }
        "sml04-smoke" => {
            if let Err(error) = sml04_smoke() {
                eprintln!("SML-0.4 smoke failed: {error}");
                std::process::exit(1);
            }
        }
        "sml05-smoke" => {
            if let Err(error) = sml05_smoke() {
                eprintln!("SML-0.5 smoke failed: {error}");
                std::process::exit(1);
            }
        }
        "sml06-smoke" => {
            if let Err(error) = sml06_smoke() {
                eprintln!("SML-0.6 smoke failed: {error}");
                std::process::exit(1);
            }
        }
        "sml07-smoke" => {
            if let Err(error) = sml07_smoke() {
                eprintln!("SML-0.7 elastic weights smoke failed: {error}");
                std::process::exit(1);
            }
        }
        "sml10-smoke" => {
            if let Err(error) = sml10_smoke() {
                eprintln!("SML-1.0 pilot smoke failed: {error}");
                std::process::exit(1);
            }
        }
        "sml11-smoke" => {
            if let Err(error) = sml11_smoke() {
                eprintln!("SML-1.1 task-loss smoke failed: {error}");
                std::process::exit(1);
            }
        }
        "sml12-smoke" => {
            if let Err(error) = sml12_smoke() {
                eprintln!("SML-1.2 state-update smoke failed: {error}");
                std::process::exit(1);
            }
        }
        "sbg012-bench" => {
            if let Err(error) = sbg012_bench() {
                eprintln!("SBG-0.12 benchmark failed: {error}");
                std::process::exit(1);
            }
        }
        other => {
            eprintln!("unknown command: {other}");
            eprintln!(
                "usage: sml-cli [contract|sml01-smoke|sml02-smoke|sml03-smoke|sml04-smoke|sml05-smoke|sml06-smoke|sml07-smoke|sml10-smoke|sml11-smoke|sml12-smoke|sbg012-bench]"
            );
            std::process::exit(2);
        }
    }
}

fn print_contract() {
    let budget = SML_25B_INITIAL_BUDGET;
    println!("SML-GENIUS research contract");
    println!("parameters={}", budget.total_parameters());
    println!("boolean_ppm={}", budget.boolean_fraction_ppm());
    println!("packed_weight_bytes={}", budget.storage_bytes_ceil());
    println!("ram_limit_bytes={TARGET_RAM_BYTES}");
    println!("vram_limit_bytes={TARGET_VRAM_BYTES}");
    println!("target_tokens_per_second={TARGET_TOKENS_PER_SECOND}");
}

fn sml01_smoke() -> Result<(), Box<dyn std::error::Error>> {
    let mut counters = ExecutionCounters::default();

    let transform = Gf2LinearMap::new(
        2,
        vec![
            Gf2Vector::from_bits(&[true, true]),
            Gf2Vector::from_bits(&[false, true]),
        ],
    )?;
    let address = transform
        .apply(&Gf2Vector::from_bits(&[true, false]), &mut counters)?
        .to_u64_le()?;

    let relation = AnfPolynomial::new(
        2,
        false,
        vec![AnfMonomial::new(vec![0])?, AnfMonomial::new(vec![0, 1])?],
    )?
    .evaluate(&[true, false], &mut counters)?;

    let mut memory = BoundedAssociativeMemory::new(4, 2, ReplacementPolicy::ReplaceExisting)?;
    memory.write(
        address,
        BooleanWorkspace::from_bits(&[relation, true]),
        &mut counters,
    )?;

    let page = AtlasPageManifest::new(
        PageId(0),
        ParameterBudget {
            boolean: 64,
            ternary: 0,
            residual4: 0,
        },
        64,
        64,
    )?;
    let atlas = CircuitAtlas::new(vec![page])?;
    let activation = atlas.activate(PageId(0), &mut counters)?;

    validate_non_attention_candidate(&counters)?;

    println!("SML-0.1 smoke");
    println!("gf2_address={address}");
    println!("anf_relation={relation}");
    println!("memory_semantic_bits={}", memory.semantic_bits());
    println!("page_parameters={}", activation.learned_parameters);
    println!("page_physical_bytes={}", activation.physical_bytes);
    println!("pairwise_comparisons={}", counters.pairwise_comparisons);
    println!("attention_score_ops={}", counters.attention_score_ops);
    Ok(())
}

struct TraceSmokeKernel;

impl ReasoningKernel for TraceSmokeKernel {
    type Error = std::convert::Infallible;

    fn step(
        &mut self,
        workspace: &BooleanWorkspace,
        depth: u32,
    ) -> Result<ReasoningStep, Self::Error> {
        let next = if depth == 1 {
            BooleanWorkspace::from_bits(&[true, workspace.get(1).unwrap_or(false)])
        } else {
            BooleanWorkspace::from_bits(&[true, true])
        };
        Ok(ReasoningStep {
            workspace: next,
            contradiction: false,
            active_parameters: 32,
            active_storage_bits: 32,
            selected_pages: vec![PageId(depth - 1)],
            pairwise_comparisons: 0,
            attention_score_ops: 0,
        })
    }
}

fn sml02_smoke() -> Result<(), Box<dyn std::error::Error>> {
    let suite = development_suite(8)?;
    for instance in &suite {
        if !instance.oracle_matches()? {
            return Err("SML-1 development oracle mismatch".into());
        }
    }

    let mut kernel = TraceSmokeKernel;
    let traced = run_recurrent_traced(
        BooleanWorkspace::from_bits(&[false, false]),
        &mut kernel,
        HaltingPolicy {
            min_depth: 2,
            max_depth: 8,
            stable_hamming_threshold: 0,
        },
        0,
    )
    .map_err(|error| format!("recurrent smoke failed: {error:?}"))?;
    traced.trace.validate_non_attention()?;

    println!("SML-0.2 smoke");
    println!("development_tasks={}", suite.len());
    println!("recurrent_depth={}", traced.outcome.depth);
    println!("trace_records={}", traced.trace.len());
    println!(
        "active_parameter_touches={}",
        traced.outcome.counters.active_parameters
    );
    println!(
        "pairwise_comparisons={}",
        traced.outcome.counters.pairwise_comparisons
    );
    Ok(())
}

fn sml03_smoke() -> Result<(), Box<dyn std::error::Error>> {
    let config = GateTrainingConfig::default();

    let mut xor_gate = LatentGate2::default();
    let xor_report = xor_gate.train_truth_table(TruthTable2::XOR, config)?;

    let mut and_gate = LatentGate2::default();
    let and_report = and_gate.train_truth_table(TruthTable2::AND, config)?;

    if xor_report.final_table != TruthTable2::XOR || xor_gate.hard_accuracy(TruthTable2::XOR)? != 4
    {
        return Err("XOR gate did not converge to the exact hard truth table".into());
    }
    if and_report.final_table != TruthTable2::AND || and_gate.hard_accuracy(TruthTable2::AND)? != 4
    {
        return Err("AND gate did not converge to the exact hard truth table".into());
    }

    println!("SML-0.3 smoke");
    println!("xor_final_code={}", xor_report.final_table.code());
    println!("xor_final_loss={:.8}", xor_report.final_loss);
    println!("and_final_code={}", and_report.final_table.code());
    println!("and_final_loss={:.8}", and_report.final_loss);
    println!("training_epochs={}", config.epochs);
    Ok(())
}

fn sml04_smoke() -> Result<(), Box<dyn std::error::Error>> {
    let mut samples = Vec::with_capacity(8);
    for mask in 0_u8..8 {
        let x0 = mask & 1 != 0;
        let x1 = mask & 2 != 0;
        let x2 = mask & 4 != 0;
        samples.push(CircuitSample {
            inputs: vec![x0, x1, x2],
            target: (x0 ^ x1) & x2,
        });
    }

    let mut circuit = LatentBooleanCircuit::new(
        3,
        vec![
            CircuitNodeSpec {
                left: SignalRef::Input(0),
                right: SignalRef::Input(1),
            },
            CircuitNodeSpec {
                left: SignalRef::Gate(0),
                right: SignalRef::Input(2),
            },
        ],
        SignalRef::Gate(1),
    )?;

    let report = circuit.train(
        &samples,
        GateTrainingConfig {
            epochs: 800,
            learning_rate: 0.2,
            epsilon: 1.0e-6,
        },
    )?;

    if report.final_tables != vec![TruthTable2::XOR, TruthTable2::AND] {
        return Err("two-gate circuit did not learn XOR -> AND identities".into());
    }
    if report.hard_correct != samples.len() as u64 {
        return Err("two-gate circuit hard forward is not exact".into());
    }

    let accounting = circuit.accounting();
    println!("SML-0.4 smoke");
    println!("hard_correct={}", report.hard_correct);
    println!("initial_loss={:.8}", report.initial_loss);
    println!("final_loss={:.8}", report.final_loss);
    println!("learned_boolean_bits={}", accounting.learned_boolean_bits);
    println!("wiring_metadata_bits={}", accounting.wiring_metadata_bits);
    println!(
        "latent_training_scalars={}",
        accounting.latent_training_scalars
    );
    Ok(())
}

fn sml05_smoke() -> Result<(), Box<dyn std::error::Error>> {
    const CANDIDATE: &str = "sml-topology/v1|g0=0,1,6|g1=3,2,8|g2=0,0,0|out=4";
    let target: Vec<bool> = (0_usize..8)
        .map(|row| {
            let x0 = row & 1 != 0;
            let x1 = row & 2 != 0;
            let x2 = row & 4 != 0;
            (x0 ^ x1) & x2
        })
        .collect();

    let candidate = ForgeTopologyCandidateV1::parse(CANDIDATE)?;
    let report = candidate.requalify(3, 3, &target)?;
    if !report.exact() {
        return Err("Forge candidate failed destination-side full-oracle requalification".into());
    }

    println!("SML-0.5 smoke");
    println!("rows={}", report.rows);
    println!("incorrect_rows={}", report.incorrect_rows);
    println!("reachable_gates={}", report.reachable_gates);
    println!("learned_boolean_bits={}", report.learned_boolean_bits);
    println!("wiring_metadata_bits={}", report.wiring_metadata_bits);
    Ok(())
}

fn sml06_smoke() -> Result<(), Box<dyn std::error::Error>> {
    let mut router = LatentDirectRouter::new(
        2,
        3,
        RouteLearningBudget {
            max_address_width: 2,
            max_pages: 3,
            max_learned_bits: 8,
        },
    )?;
    let targets = [PageId(0), PageId(2), PageId(1), PageId(2)];
    let report = router.train_complete(&targets, RouteTrainingConfig::default())?;
    if report.hard_correct != targets.len() as u64 {
        return Err("latent direct router did not learn the complete route oracle".into());
    }

    let accounting = router.accounting();
    let frozen = router.freeze()?;
    let mut counters = ExecutionCounters::default();
    for (address, expected) in targets.iter().copied().enumerate() {
        let workspace = BooleanWorkspace::from_bits(&[address & 1 != 0, address & 2 != 0]);
        let page = frozen.route(&workspace, &mut counters)?;
        if page != expected {
            return Err(format!("frozen route mismatch at address {address}").into());
        }
    }
    validate_non_attention_candidate(&counters)?;
    if counters.route_lookups != targets.len() as u64 {
        return Err("frozen router did not execute exactly one lookup per address".into());
    }

    println!("SML-0.6 smoke");
    println!("addresses={}", report.address_count);
    println!("hard_correct={}", report.hard_correct);
    println!("initial_loss={:.8}", report.initial_loss);
    println!("final_loss={:.8}", report.final_loss);
    println!("learned_boolean_bits={}", accounting.learned_boolean_bits);
    println!(
        "latent_training_scalars={}",
        accounting.latent_training_scalars
    );
    println!("route_lookups={}", counters.route_lookups);
    Ok(())
}

fn sml07_smoke() -> Result<(), Box<dyn std::error::Error>> {
    let pages = vec![
        ElasticWeightPage::new(
            PageId(0),
            128,
            WeightPrecision::Boolean,
            ResidencyTier::Disk,
            900_000,
        )?,
        ElasticWeightPage::new(
            PageId(1),
            256,
            WeightPrecision::Boolean,
            ResidencyTier::Disk,
            600_000,
        )?,
        ElasticWeightPage::new(
            PageId(2),
            512,
            WeightPrecision::Boolean,
            ResidencyTier::Disk,
            100_000,
        )?,
    ];
    let policy = ElasticWeightPolicy {
        ram_limit_bytes: 64,
        vram_limit_bytes: 64,
        max_active_parameters: 384,
        ternary_threshold_ppm: 500_000,
        residual4_threshold_ppm: 800_000,
        vram_threshold_ppm: 750_000,
    };
    let mut planner = ElasticWeightPlanner::new(pages, policy)?;
    let plan = planner.plan_cycle(&[PageId(0), PageId(1)], 1)?;

    if plan.active_parameters != 384
        || plan.active_storage_bits != 1_024
        || plan.ram_bytes != 64
        || plan.vram_bytes != 64
    {
        return Err("elastic weight plan accounting mismatch".into());
    }

    planner.commit(&plan)?;
    let accounting = planner.accounting()?;
    if accounting.boolean_parameters != 512
        || accounting.ternary_parameters != 256
        || accounting.residual4_parameters != 128
        || accounting.disk_bytes != 64
        || accounting.ram_bytes != 64
        || accounting.vram_bytes != 64
    {
        return Err("elastic weight committed accounting mismatch".into());
    }

    println!("SML-0.7 elastic weights smoke");
    println!("generation={}", planner.generation());
    println!("active_parameters={}", accounting.active_parameters);
    println!("active_storage_bits={}", accounting.active_storage_bits);
    println!("disk_bytes={}", accounting.disk_bytes);
    println!("ram_bytes={}", accounting.ram_bytes);
    println!("vram_bytes={}", accounting.vram_bytes);
    println!("precision_changes={}", plan.precision_changes);
    println!("bytes_moved_to_ram={}", plan.bytes_moved_to_ram);
    println!("bytes_moved_to_vram={}", plan.bytes_moved_to_vram);
    Ok(())
}

fn sml10_smoke() -> Result<(), Box<dyn std::error::Error>> {
    let (candidate, training) = SmlTaskCandidate::train(TaskCandidateConfig::default())?;
    let holdout = suite_from_case_id(10_000, 8)?;

    let mut exact = 0_u64;
    let mut trace_records = 0_u64;
    let mut route_lookups = 0_u64;
    let mut memory_reads = 0_u64;
    let mut memory_writes = 0_u64;
    let mut semantic_route_bits_touched = 0_u64;

    for instance in &holdout {
        let evidence = candidate.execute(instance)?;
        if evidence.exact {
            exact = exact.saturating_add(1);
        }
        trace_records = trace_records.saturating_add(evidence.trace.len() as u64);
        route_lookups = route_lookups.saturating_add(evidence.counters.route_lookups);
        memory_reads = memory_reads.saturating_add(evidence.counters.memory_reads);
        memory_writes = memory_writes.saturating_add(evidence.counters.memory_writes);
        semantic_route_bits_touched =
            semantic_route_bits_touched.saturating_add(evidence.semantic_route_bits_touched);
    }

    if exact != holdout.len() as u64 {
        return Err(format!("SML-1.0 holdout mismatch: {exact}/{} exact", holdout.len()).into());
    }

    println!("SML-1.0 pilot smoke");
    println!(
        "route_hard_correct={}/{}",
        training.route.hard_correct, training.route.address_count
    );
    println!(
        "learned_route_bits={}",
        training.route_accounting.learned_boolean_bits
    );
    println!("holdout_tasks={exact}/{}", holdout.len());
    println!("trace_records={trace_records}");
    println!("route_lookups={route_lookups}");
    println!("memory_reads={memory_reads}");
    println!("memory_writes={memory_writes}");
    println!("semantic_route_bits_touched={semantic_route_bits_touched}");
    Ok(())
}

fn sml11_smoke() -> Result<(), Box<dyn std::error::Error>> {
    let (learned, training) =
        train_conjunction_from_tasks(ConjunctionTaskTrainingConfig::default())?;
    let holdout = suite_from_case_id(10_000, 16)?;
    let learned_eval = evaluate_task_panel(&learned, &holdout)?;

    let (wrong_xor, _) =
        SmlTaskCandidate::train_with_conjunction(TaskCandidateConfig::default(), TruthTable2::XOR)?;
    let wrong_eval = evaluate_task_panel(&wrong_xor, &holdout)?;

    let (fixed_and, _) = SmlTaskCandidate::train(TaskCandidateConfig::default())?;
    let fixed_eval = evaluate_task_panel(&fixed_and, &holdout)?;

    if training.gate.final_table != TruthTable2::AND {
        return Err(format!(
            "task-loss gate selected table code {} instead of AND",
            training.gate.final_table.code()
        )
        .into());
    }
    if learned_eval.exact_tasks != learned_eval.tasks {
        return Err(format!(
            "learned task-loss candidate mismatch: {}/{} exact",
            learned_eval.exact_tasks, learned_eval.tasks
        )
        .into());
    }
    if wrong_eval.exact_conjunction_tasks >= learned_eval.exact_conjunction_tasks {
        return Err("wrong-gate ablation did not reduce conjunction accuracy".into());
    }
    if fixed_eval.exact_tasks != learned_eval.exact_tasks {
        return Err("learned gate did not recover the fixed AND oracle baseline".into());
    }

    println!("SML-1.1 task-loss smoke");
    println!("task_training_samples={}", training.task_samples);
    println!("input_pattern_mask={:#06b}", training.input_pattern_mask);
    println!("initial_gate_code={}", training.gate.initial_table.code());
    println!("learned_gate_code={}", training.gate.final_table.code());
    println!("learned_gate_bits={}", training.learned_boolean_bits);
    println!(
        "learned_holdout_exact={}/{}",
        learned_eval.exact_tasks, learned_eval.tasks
    );
    println!(
        "learned_conjunction_exact={}/{}",
        learned_eval.exact_conjunction_tasks, learned_eval.conjunction_tasks
    );
    println!(
        "wrong_xor_conjunction_exact={}/{}",
        wrong_eval.exact_conjunction_tasks, wrong_eval.conjunction_tasks
    );
    println!(
        "fixed_and_holdout_exact={}/{}",
        fixed_eval.exact_tasks, fixed_eval.tasks
    );
    Ok(())
}

fn sml12_smoke() -> Result<(), Box<dyn std::error::Error>> {
    let (learned, training) =
        train_state_update_from_tasks(StateUpdateTaskTrainingConfig::default())?;
    let holdout = suite_from_case_id(10_000, 16)?;
    let learned_eval = evaluate_task_panel(&learned, &holdout)?;

    let (wrong_xor, _) = SmlTaskCandidate::train_with_micro_ops(
        TaskCandidateConfig::default(),
        TruthTable2::AND,
        TruthTable2::XOR,
    )?;
    let wrong_eval = evaluate_task_panel(&wrong_xor, &holdout)?;

    let (fixed, _) = SmlTaskCandidate::train(TaskCandidateConfig::default())?;
    let fixed_eval = evaluate_task_panel(&fixed, &holdout)?;

    if training.gate.final_table != TruthTable2::RIGHT {
        return Err(format!(
            "task-loss state update selected table code {} instead of RIGHT projection",
            training.gate.final_table.code()
        )
        .into());
    }
    if learned_eval.exact_tasks != learned_eval.tasks {
        return Err(format!(
            "learned state-update candidate mismatch: {}/{} exact",
            learned_eval.exact_tasks, learned_eval.tasks
        )
        .into());
    }
    if wrong_eval.exact_overwrite_tasks >= learned_eval.exact_overwrite_tasks {
        return Err("wrong XOR state update did not reduce overwrite accuracy".into());
    }
    if fixed_eval.exact_tasks != learned_eval.exact_tasks {
        return Err("learned state update did not recover the fixed overwrite baseline".into());
    }

    println!("SML-1.2 state-update smoke");
    println!("task_training_samples={}", training.task_samples);
    println!("input_pattern_mask={:#06b}", training.input_pattern_mask);
    println!("initial_gate_code={}", training.gate.initial_table.code());
    println!("learned_gate_code={}", training.gate.final_table.code());
    println!("learned_gate_bits={}", training.learned_boolean_bits);
    println!(
        "learned_holdout_exact={}/{}",
        learned_eval.exact_tasks, learned_eval.tasks
    );
    println!(
        "learned_overwrite_exact={}/{}",
        learned_eval.exact_overwrite_tasks, learned_eval.overwrite_tasks
    );
    println!(
        "wrong_xor_overwrite_exact={}/{}",
        wrong_eval.exact_overwrite_tasks, wrong_eval.overwrite_tasks
    );
    println!(
        "fixed_holdout_exact={}/{}",
        fixed_eval.exact_tasks, fixed_eval.tasks
    );
    Ok(())
}

fn sbg012_bench() -> Result<(), Box<dyn std::error::Error>> {
    const BITS: usize = 16_384;
    const WARMUP_ITERATIONS: u64 = 32;
    const MEASURED_ITERATIONS: u64 = 512;

    let left_indices: Vec<usize> = (0..BITS).filter(|index| index % 3 == 0).collect();
    let right_indices: Vec<usize> = (0..BITS).filter(|index| index % 5 == 1).collect();
    let mask_indices: Vec<usize> = (0..BITS).filter(|index| index % 7 == 2).collect();

    let left = SemanticBitField::<BITS>::from_set_indices(&left_indices)?;
    let right = SemanticBitField::<BITS>::from_set_indices(&right_indices)?;
    let mask = SemanticBitField::<BITS>::from_set_indices(&mask_indices)?;

    let qualification = qualify_packed_word4(&left, &right, &mask)?;

    for _ in 0..WARMUP_ITERATIONS {
        std::hint::black_box(scalar_reference_score(
            std::hint::black_box(&left),
            std::hint::black_box(&right),
            std::hint::black_box(&mask),
        )?);
        std::hint::black_box(packed_word4_score(
            std::hint::black_box(&left),
            std::hint::black_box(&right),
            std::hint::black_box(&mask),
        )?);
    }

    let scalar_start = std::time::Instant::now();
    let mut scalar_checksum = 0_u64;
    for iteration in 0..MEASURED_ITERATIONS {
        let evidence = scalar_reference_score(
            std::hint::black_box(&left),
            std::hint::black_box(&right),
            std::hint::black_box(&mask),
        )?;
        scalar_checksum = scalar_checksum.wrapping_add(
            evidence.full_differing_bits ^ evidence.masked_differing_bits ^ iteration,
        );
        std::hint::black_box(scalar_checksum);
    }
    let scalar_ns = scalar_start.elapsed().as_nanos();

    let packed_start = std::time::Instant::now();
    let mut packed_checksum = 0_u64;
    for iteration in 0..MEASURED_ITERATIONS {
        let evidence = packed_word4_score(
            std::hint::black_box(&left),
            std::hint::black_box(&right),
            std::hint::black_box(&mask),
        )?;
        packed_checksum = packed_checksum.wrapping_add(
            evidence.full_differing_bits ^ evidence.masked_differing_bits ^ iteration,
        );
        std::hint::black_box(packed_checksum);
    }
    let packed_ns = packed_start.elapsed().as_nanos();

    if scalar_checksum != packed_checksum {
        return Err("benchmark checksum mismatch".into());
    }

    let scalar_ns_per_iteration = scalar_ns / u128::from(MEASURED_ITERATIONS);
    let packed_ns_per_iteration = packed_ns / u128::from(MEASURED_ITERATIONS);
    let relative_throughput_ppm = scalar_ns
        .saturating_mul(1_000_000)
        .checked_div(packed_ns.max(1))
        .unwrap_or(0);

    println!("SBG-0.12 packed semantic benchmark");
    println!("arch={}", std::env::consts::ARCH);
    println!("logical_bits={BITS}");
    println!("packed_words={}", qualification.packed.packed_words);
    println!("warmup_iterations={WARMUP_ITERATIONS}");
    println!("measured_iterations={MEASURED_ITERATIONS}");
    println!("scalar_ns_total={scalar_ns}");
    println!("packed_ns_total={packed_ns}");
    println!("scalar_ns_per_iteration={scalar_ns_per_iteration}");
    println!("packed_ns_per_iteration={packed_ns_per_iteration}");
    println!("relative_throughput_ppm={relative_throughput_ppm}");
    println!("checksum={scalar_checksum}");
    Ok(())
}
