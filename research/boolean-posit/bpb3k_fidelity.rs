//! Exact-source diagnostic instrumentation and fail-closed full-logit comparator.
//! The inference kernel remains Rust. This adds observation to upstream's C++
//! debug example, outside every timed benchmark. No sampling/weight changes.
use std::{fs, path::Path, process::Command};
const SHA: &str = "f7591a6a01f9273de03331732bdc0b39721168be";
const OBSERVER: &str = r#"
    if (const char * destination = std::getenv("MEM_BPB3K_FIDELITY_DUMP")) {
        if (params.n_predict != 8) { return false; }
        std::ofstream dump(destination, std::ios::binary);
        const uint32_t nv = llama_vocab_n_tokens(vocab);
        const uint32_t np = tokens.size();
        const uint32_t steps = 8;
        dump.write("BPB3FID2", 8);
        dump.write(reinterpret_cast<const char *>(&nv), 4);
        dump.write(reinterpret_cast<const char *>(&np), 4);
        dump.write(reinterpret_cast<const char *>(tokens.data()), np * 4);
        dump.write(reinterpret_cast<const char *>(&steps), 4);
        for (uint32_t step = 0; step < steps; ++step) {
            const float * logits = llama_get_logits_ith(ctx, -1);
            if (!logits) { return false; }
            llama_token next = 0;
            for (uint32_t i = 0; i < nv; ++i) {
                if (!std::isfinite(logits[i])) { return false; }
                if (logits[i] > logits[next]) { next = i; }
            }
            dump.write(reinterpret_cast<const char *>(logits), nv * 4);
            dump.write(reinterpret_cast<const char *>(&next), 4);
            if (!dump) { return false; }
            if (step + 1 < steps && llama_decode(ctx, llama_batch_get_one(&next, 1)) != 0) { return false; }
        }
        dump.close();
        if (!dump) { return false; }
        fprintf(stderr, "PASS_BPB3K_FIDELITY_DUMP vocab=%u prompt_tokens=%u generated=%u\n", nv, np, steps);
        return true;
    }
"#;
const TRACE_OBSERVER: &str = r#"
// Read-only graph checkpoint: only hashes of bounded FP16/FP32 tensors,
// never model weights or bulk activation dumps. Activated explicitly for
// diagnostic runs and not during timed inference.
static bool mem_bpb3k_layer_cb(ggml_tensor * t, bool ask, void *) {
    if (ask) { return true; }
    if (!t || !t->buffer || !ggml_is_contiguous(t) ||
        (t->type != GGML_TYPE_F32 && t->type != GGML_TYPE_F16)) { return true; }
    const size_t n = ggml_nbytes(t);
    if (n == 0 || n > 4 * 1024 * 1024) { return true; }
    static std::atomic<unsigned long long> sequence{0};
    const auto id = sequence.fetch_add(1, std::memory_order_relaxed);
    if (id >= 4096) { return true; }
    std::vector<uint8_t> bytes(n);
    ggml_backend_tensor_get(t, bytes.data(), 0, n);
    uint64_t hash = 14695981039346656037ULL;
    for (const uint8_t b : bytes) { hash = (hash ^ b) * 1099511628211ULL; }
    fprintf(stderr,
        "BPB3K_LAYER seq=%llu name=%s op=%s type=%s dims=%lld:%lld:%lld:%lld bytes=%zu fnv64=%016llx\n",
        id, t->name, ggml_op_name(t->op), ggml_type_name(t->type),
        (long long)t->ne[0], (long long)t->ne[1],
        (long long)t->ne[2], (long long)t->ne[3],
        n, (unsigned long long)hash);
    return true;
}
"#;
fn patch(root: &Path) -> Result<(), String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "HEAD"])
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() || String::from_utf8_lossy(&output.stdout).trim() != SHA {
        return Err("STOP_FIDELITY_SOURCE_SHA".into());
    }
    let path = root.join("examples/debug/debug.cpp");
    let source = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let marker = "    print_tokenized_prompt(ctx, tokens, params.prompt);";
    if source.matches(marker).count() != 1
        || source.matches("static bool run(llama_context * ctx, const common_params & params) {").count() != 1
        || source.matches("    auto llama_init = common_init_from_params(params);").count() != 1
        || source.matches("#include <cstdlib>").count() != 1
        || source.contains("MEM_BPB3K_FIDELITY_DUMP")
    {
        return Err("STOP_FIDELITY_SOURCE_DRIFT".into());
    }
    fs::write(
        path,
        source
            .replacen(marker, &format!("{marker}\n{OBSERVER}"), 1)
            .replacen(
                "static bool run(llama_context * ctx, const common_params & params) {",
                &format!("{TRACE_OBSERVER}\nstatic bool run(llama_context * ctx, const common_params & params) {{"),
                1,
            )
            .replacen(
                "    auto llama_init = common_init_from_params(params);",
                "    if (std::getenv(\"MEM_BPB3K_LAYER_TRACE\")) {\n        params.cb_eval = mem_bpb3k_layer_cb;\n        params.cb_eval_user_data = nullptr;\n    }\n    auto llama_init = common_init_from_params(params);",
                1,
            )
            .replacen(
                "#include <cstdlib>",
                "#include <cstdlib>\n#include <cmath>\n#include <atomic>\n#include <cstdint>\n#include <cstdio>\n#include \"ggml-backend.h\"",
                1,
            ),
    )
    .map_err(|e| e.to_string())
}
struct Dump {
    prompt: Vec<u32>,
    rows: Vec<Vec<f32>>,
    tokens: Vec<u32>,
}
fn parse(bytes: &[u8]) -> Result<Dump, String> {
    if bytes.get(..8) != Some(b"BPB3FID2") {
        return Err("STOP_FIDELITY_MAGIC".into());
    }
    let mut pos = 8usize;
    let mut word = || -> Result<u32, String> {
        let end = pos.checked_add(4).ok_or("STOP_FIDELITY_SIZE")?;
        let b = bytes.get(pos..end).ok_or("STOP_FIDELITY_TRUNCATED")?;
        pos = end;
        Ok(u32::from_le_bytes(b.try_into().unwrap()))
    };
    let nv = word()? as usize;
    let np = word()? as usize;
    if !(1..=1_000_000).contains(&nv) || !(1..=4096).contains(&np) {
        return Err("STOP_FIDELITY_SHAPE".into());
    }
    let prompt = (0..np).map(|_| word()).collect::<Result<Vec<_>, _>>()?;
    let steps = word()? as usize;
    if steps != 8 {
        return Err("STOP_FIDELITY_STEP_COUNT".into());
    }
    let mut rows = Vec::new();
    let mut tokens = Vec::new();
    for _ in 0..steps {
        let row = (0..nv)
            .map(|_| word().map(f32::from_bits))
            .collect::<Result<Vec<_>, _>>()?;
        if row.iter().any(|x| !x.is_finite()) {
            return Err("STOP_FIDELITY_NONFINITE".into());
        }
        let token = word()?;
        if token as usize >= nv {
            return Err("STOP_FIDELITY_TOKEN_RANGE".into());
        }
        let mut greedy = 0;
        for i in 1..nv {
            if row[i] > row[greedy] {
                greedy = i;
            }
        }
        if token as usize != greedy {
            return Err("STOP_FIDELITY_NOT_GREEDY".into());
        }
        rows.push(row);
        tokens.push(token);
    }
    if pos != bytes.len() {
        return Err("STOP_FIDELITY_TRAILING_BYTES".into());
    }
    Ok(Dump {
        prompt,
        rows,
        tokens,
    })
}
fn compare(a: &[u8], b: &[u8], exact: bool) -> Result<(), String> {
    let x = parse(a)?;
    let y = parse(b)?;
    if x.prompt != y.prompt || x.rows[0].len() != y.rows[0].len() {
        return Err("STOP_FIDELITY_INPUT_MISMATCH".into());
    }
    let mut max_abs = 0.0f64;
    let mut squares = 0.0f64;
    let mut violations = 0usize;
    let mut count = 0usize;
    for (xr, yr) in x.rows.iter().zip(&y.rows) {
        for (&v, &w) in xr.iter().zip(yr) {
            let delta = (f64::from(v) - f64::from(w)).abs();
            max_abs = max_abs.max(delta);
            squares += delta * delta;
            count += 1;
            // Frozen before physical measurement. Exact for v1/v2; stock has
            // distinct FP32 accumulation, abs 2e-4 + rel 2e-5 * |reference|.
            if if exact {
                v.to_bits() != w.to_bits()
            } else {
                delta > 2e-4 + 2e-5 * f64::from(v).abs()
            } {
                violations += 1;
            }
        }
    }
    println!("BPB3K_LOGITS exact={exact} values={count} max_abs={max_abs:.9e} rmse={:.9e} violations={violations} tokens_equal={}", (squares/count as f64).sqrt(),x.tokens==y.tokens);
    if violations != 0 || x.tokens != y.tokens {
        return Err("STOP_FIDELITY_COMPARISON".into());
    }
    Ok(())
}
fn run() -> Result<(), String> {
    let args = std::env::args().collect::<Vec<_>>();
    match args.as_slice() {
        [_, mode, root] if mode == "patch-debug" => patch(Path::new(root)),
        [_, mode, a, b, policy]
            if mode == "compare" && (policy == "exact" || policy == "stock") =>
        {
            compare(
                &fs::read(a).map_err(|e| e.to_string())?,
                &fs::read(b).map_err(|e| e.to_string())?,
                policy == "exact",
            )
        }
        _ => Err("USAGE patch-debug ROOT | compare A B exact|stock".into()),
    }
}
fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Vec<u8> {
        let mut b = b"BPB3FID2".to_vec();
        for v in [2u32, 1, 1, 8] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        for _ in 0..8 {
            for v in [0f32.to_bits(), 1f32.to_bits(), 1] {
                b.extend_from_slice(&v.to_le_bytes());
            }
        }
        b
    }
    #[test]
    fn reads_and_compares_actual_logits() {
        let b = fixture();
        assert!(compare(&b, &b, true).is_ok());
    }
    #[test]
    fn rejects_mismatch_and_truncation() {
        let a = fixture();
        let mut b = a.clone();
        b[24..28].copy_from_slice(&0.01f32.to_le_bytes());
        assert!(compare(&a, &b, true).is_err());
        assert!(compare(&a, &b, false).is_err());
        assert!(parse(&a[..a.len() - 1]).is_err());
    }
    #[test]
    fn rejects_nonfinite_and_forged_token() {
        let mut b = fixture();
        b[24..28].copy_from_slice(&f32::NAN.to_le_bytes());
        assert!(parse(&b).is_err());
        let mut b = fixture();
        b[32..36].copy_from_slice(&0u32.to_le_bytes());
        assert!(parse(&b).is_err());
    }
    #[test]
    fn rejects_unpinned_debug_source() {
        assert!(patch(Path::new("/nonexistent-bpb3k-fixture")).is_err());
    }
}
