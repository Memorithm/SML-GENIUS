//! R3: extend an admitted R2 memory by one primitive, rather than enumerate all trees.
//! The bank is parsed against the closed grammar and verified before reuse.
use crate::tutor_rust_r2 as r2;

use r2::{Expr, Learning, Op, Temp};
use std::error::Error;
use std::fs;
use std::io::{Read, Write};
use std::path::Path;

type Result<T> = std::result::Result<T, Box<dyn Error>>;
type Example = (u8, u8, u8);
const TASKS: usize = 7;
pub const TRAIN: [(u8, u8); 24] = [
    (0, 0),
    (1, 1),
    (255, 1),
    (1, 255),
    (3, 5),
    (128, 128),
    (17, 0),
    (0, 17),
    (12, 37),
    (143, 62),
    (78, 219),
    (199, 130),
    (85, 170),
    (7, 15),
    (31, 93),
    (251, 249),
    (18, 57),
    (91, 14),
    (220, 173),
    (61, 117),
    (213, 86),
    (46, 201),
    (97, 103),
    (233, 19),
];
pub const DEV: [(u8, u8); 8] = [
    (2, 7),
    (7, 2),
    (15, 16),
    (16, 15),
    (127, 129),
    (129, 127),
    (254, 2),
    (2, 254),
];

fn verified_memory(raw: &str, baseline: &[Expr]) -> Result<Vec<Expr>> {
    if raw.len() > 8192 {
        return Err("R2 bank exceeds bound".into());
    }
    let bank = r2::restore(raw, baseline)?;
    for (task, expr) in bank.iter().enumerate() {
        for a in 0..=255u8 {
            for b in 0..=255u8 {
                if expr.eval(a, b) != r2::oracle(task, a, b) {
                    return Err("R2 retention failed before R3 learning".into());
                }
            }
        }
    }
    Ok(bank)
}

fn extend(bank: &[Expr]) -> Vec<Expr> {
    let mut candidates = Vec::new();
    for op in Op::ALL {
        for saved in bank {
            for leaf in [Expr::A, Expr::B] {
                candidates.push(Expr::apply(op, saved, &leaf));
                candidates.push(Expr::apply(op, &leaf, saved));
            }
        }
    }
    candidates
}

// Separate mathematical oracle: never evaluates a candidate or the saved bank.
fn intersection(a: u32, b: u32) -> u32 {
    (0..8)
        .map(|k| {
            let p = 1 << k;
            if (a / p) % 2 == 1 && (b / p) % 2 == 1 {
                p
            } else {
                0
            }
        })
        .sum()
}

fn oracle(task: usize, a: u8, b: u8) -> u8 {
    let (a, b) = (u32::from(a), u32::from(b));
    let xor = |x, y| x + y - 2 * intersection(x, y);
    let value = match task {
        0 => xor((a + b) % 256, a) * b,
        1 => xor((xor(a, b) + b) % 256, a),
        2 => intersection(a, b) * a + b,
        3 => (a - intersection(a, b)) * b,
        4 => xor(a.min(b), b) + a,
        5 => xor(b.saturating_sub(a), a),
        6 => xor((a * (b + 1)) % 256, b),
        _ => unreachable!(),
    };
    (value % 256) as u8
}

pub fn fixture() -> String {
    let mut raw = String::from("SML_TUTOR_R3_TRAIN_V1\n");
    for task in 0..TASKS {
        for (a, b) in TRAIN {
            raw.push_str(&format!("{task}|{a}|{b}|{}\n", oracle(task, a, b)));
        }
    }
    raw
}

fn admit(raw: &str) -> Result<Vec<Vec<Example>>> {
    if raw.len() > 8192 || !raw.ends_with('\n') {
        return Err("invalid R3 corpus size or termination".into());
    }
    let mut lines = raw.lines();
    if lines.next() != Some("SML_TUTOR_R3_TRAIN_V1") {
        return Err("invalid R3 marker".into());
    }
    let mut batch = Vec::new();
    for task in 0..TASKS {
        let mut examples = Vec::new();
        for (a, b) in TRAIN {
            let row = lines.next().ok_or("missing R3 TRAIN row")?;
            if row != format!("{task}|{a}|{b}|{}", oracle(task, a, b)) {
                return Err(format!("invalid R3 teacher row task={task}, a={a}, b={b}").into());
            }
            let supplied: u8 = row.rsplit('|').next().ok_or("missing label")?.parse()?;
            examples.push((a, b, supplied));
        }
        batch.push(examples);
    }
    if lines.next().is_some() {
        return Err("extra R3 TRAIN rows".into());
    }
    Ok(batch)
}

fn checkpoint(ops: &[Expr]) -> String {
    let mut raw = String::from("SML_TUTOR_R3_BANK_V1\n");
    for op in ops {
        raw.push_str(&op.source());
        raw.push('\n');
    }
    raw
}

fn restore(raw: &str, candidates: &[Expr]) -> Result<Vec<Expr>> {
    if raw.len() > 8192 || !raw.ends_with('\n') {
        return Err("invalid R3 bank size or termination".into());
    }
    let mut lines = raw.lines();
    if lines.next() != Some("SML_TUTOR_R3_BANK_V1") {
        return Err("invalid R3 bank marker".into());
    }
    let ops: Vec<_> = lines
        .map(|s| {
            candidates
                .iter()
                .find(|e| e.source() == s)
                .cloned()
                .ok_or("unknown R3 expression")
        })
        .collect::<std::result::Result<_, _>>()?;
    if ops.len() != TASKS || ops.iter().any(|e| e.cost() != 3) {
        return Err("invalid R3 bank composition count".into());
    }
    Ok(ops)
}

/// Extend the actual retained R2 memory; no teacher source is compiled.
pub fn induce(memory: &str, raw: &str) -> Result<String> {
    let baseline = r2::grammar();
    let bank = verified_memory(memory, &baseline)?;
    let candidates = extend(&bank);
    if baseline.len() != 1058 || candidates.len() != 224 || TRAIN.iter().any(|p| DEV.contains(p)) {
        return Err("R3 grammar or partition contract".into());
    }
    let mut learned = Vec::new();
    for examples in admit(raw)? {
        if r2::learn(&baseline, &examples) != Learning::Inconsistent
            || r2::learn(&candidates, &[]) != Learning::Ambiguous
        {
            return Err("R3 baseline failed to abstain".into());
        }
        match r2::learn(&candidates, &examples) {
            Learning::Unique(expr) if expr.cost() == 3 => learned.push(expr),
            _ => return Err("R3 synthesis not uniquely identified".into()),
        }
    }
    Ok(checkpoint(&learned))
}

/// Verify a persisted R3 bank against its retained R2 parent, without learning.
pub fn examine(memory: &str, serialized: &str) -> Result<[u64; 4]> {
    let baseline = r2::grammar();
    let candidates = extend(&verified_memory(memory, &baseline)?);
    let temp = Temp::new()?;
    let bank_path = temp.0.join("r3.bank");
    fs::write(&bank_path, serialized)?;
    let restored = restore(&fs::read_to_string(bank_path)?, &candidates)?;
    if checkpoint(&restored) != serialized {
        return Err("R3 bank roundtrip mismatch".into());
    }
    let out = r2::compile_and_run(&restored, &temp.0)?;
    let mut totals = [0u64; 4];
    for a in 0..=255u8 {
        for b in 0..=255u8 {
            let i = usize::from(a) * 256 + usize::from(b);
            let cohort = if TRAIN.contains(&(a, b)) {
                0
            } else if DEV.contains(&(a, b)) {
                1
            } else {
                2 + i % 2
            };
            for (task, expr) in restored.iter().enumerate() {
                let want = oracle(task, a, b);
                if expr.eval(a, b) != want || out[i * TASKS + task] != want {
                    return Err(format!("R3 exhaustive mismatch task={task}, a={a}, b={b}").into());
                }
                totals[cohort] += 1;
            }
        }
    }
    // Revalidate the unchanged input memory, independently of R3 synthesis.
    verified_memory(memory, &baseline)?;
    Ok(totals)
}

fn qualify(memory: &str, raw: &str, source: &str, output: Option<&Path>) -> Result<()> {
    let serialized = induce(memory, raw)?;
    let totals = examine(memory, &serialized)?;
    if let Some(path) = output {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?;
        file.write_all(serialized.as_bytes())?;
        file.sync_all()?;
    }
    for (name, total) in ["train", "dev", "test", "testtest"].into_iter().zip(totals) {
        println!("SML_TUTOR_R3_COHORT cohort={name} correct={total} total={total}");
    }
    println!("SML_TUTOR_R3=PASS programs=7 memory_macros=7 extension_candidates=224 depth2_baseline_abstains=7 exact_cases=458752 reload=458752 retained_r2=458752 source={source}");
    Ok(())
}

fn read_bounded(path: &Path) -> Result<String> {
    let mut raw = String::new();
    fs::File::open(path)?.take(8193).read_to_string(&mut raw)?;
    if raw.len() > 8192 {
        return Err("input exceeds bound".into());
    }
    Ok(raw)
}

/// Original CLI, kept compatible with RemoteOps qualification receipts.
pub fn run_cli() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    match args.as_slice() {
        [] => {
            let temp = Temp::new()?;
            let bank = temp.0.join("r2.bank");
            r2::qualify(&r2::fixture(), false, Some(&bank))?;
            qualify(&read_bounded(&bank)?, &fixture(), "synthetic_fixture", None)
        }
        [flag, bank, input, output] if flag == "--teacher-train" => qualify(
            &read_bounded(Path::new(bank))?,
            &read_bounded(Path::new(input))?,
            "external_inputs",
            Some(Path::new(output)),
        ),
        _ => Err("usage: sml_tutor_rust_r3 [--teacher-train r2.bank corpus r3.bank]".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_reuse_compiles_and_retains_all_r2_cases() {
        let temp = Temp::new().unwrap();
        let prior = temp.0.join("r2.bank");
        let next = temp.0.join("r3.bank");
        r2::qualify(&r2::fixture(), false, Some(&prior)).unwrap();
        let raw = read_bounded(&prior).unwrap();
        qualify(&raw, &fixture(), "synthetic_fixture", Some(&next)).unwrap();
        assert_eq!(read_bounded(&prior).unwrap(), raw);
        let candidates = extend(&verified_memory(&raw, &r2::grammar()).unwrap());
        assert_eq!(
            restore(&read_bounded(&next).unwrap(), &candidates)
                .unwrap()
                .len(),
            TASKS
        );
        let mut swapped: Vec<_> = raw.lines().collect();
        swapped.swap(1, 2);
        assert!(verified_memory(&(swapped.join("\n") + "\n"), &r2::grammar()).is_err());
    }

    #[test]
    fn reject_invalid_labels_and_bank() {
        let raw = fixture();
        assert_eq!(admit(&raw).unwrap().len(), TASKS);
        assert!(admit(&raw.replacen("0|0|0|0", "0|0|0|1", 1)).is_err());
        assert!(admit(&raw[..raw.len() - 1]).is_err());
        assert!(verified_memory("SML_TUTOR_R2_BANK_V1\nunsafe{}\n", &r2::grammar()).is_err());
    }
}
