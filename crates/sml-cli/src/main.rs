#![forbid(unsafe_code)]

use sml_core::{
    anf::{AnfMonomial, AnfPolynomial},
    atlas::{AtlasPageManifest, CircuitAtlas},
    audit::validate_non_attention_candidate,
    circuit::{CircuitNodeSpec, CircuitSample, LatentBooleanCircuit, SignalRef},
    forge_topology::ForgeTopologyCandidateV1,
    gf2::{Gf2LinearMap, Gf2Vector},
    memory::{BoundedAssociativeMemory, ReplacementPolicy},
    routing_train::{LatentDirectRouter, RouteLearningBudget, RouteTrainingConfig},
    tasks::development_suite,
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
        other => {
            eprintln!("unknown command: {other}");
            eprintln!(
                "usage: sml-cli [contract|sml01-smoke|sml02-smoke|sml03-smoke|sml04-smoke|sml05-smoke|sml06-smoke]"
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
