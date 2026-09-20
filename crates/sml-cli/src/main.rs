#![forbid(unsafe_code)]

use sml_core::{
    anf::{AnfMonomial, AnfPolynomial},
    atlas::{AtlasPageManifest, CircuitAtlas},
    audit::validate_non_attention_candidate,
    gf2::{Gf2LinearMap, Gf2Vector},
    memory::{BoundedAssociativeMemory, ReplacementPolicy},
    BooleanWorkspace, ExecutionCounters, PageId, ParameterBudget, SML_25B_INITIAL_BUDGET,
    TARGET_RAM_BYTES, TARGET_TOKENS_PER_SECOND, TARGET_VRAM_BYTES,
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
        other => {
            eprintln!("unknown command: {other}");
            eprintln!("usage: sml-cli [contract|sml01-smoke]");
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
