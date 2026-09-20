#![forbid(unsafe_code)]

use sml_core::{
    SML_25B_INITIAL_BUDGET, TARGET_RAM_BYTES, TARGET_TOKENS_PER_SECOND, TARGET_VRAM_BYTES,
};

fn main() {
    let command = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "contract".to_owned());
    match command.as_str() {
        "contract" => print_contract(),
        other => {
            eprintln!("unknown command: {other}");
            eprintln!("usage: sml-cli [contract]");
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
