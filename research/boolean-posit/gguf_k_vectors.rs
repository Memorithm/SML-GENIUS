//! Deterministic GGUF K-format scalar parity vector emitter, not a benchmark.
#[path = "gguf_k.rs"]
mod gguf_k;

fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if arguments.len() != 1 || arguments[0] != "--fixture-vectors" {
        eprintln!("Usage: gguf_k_vectors --fixture-vectors");
        std::process::exit(2);
    }
    gguf_k::emit_reference_vectors();
}
