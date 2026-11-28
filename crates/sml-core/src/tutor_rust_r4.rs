//! R4: diagnose a wrong root operation and repair it using admitted TRAIN
//! examples and a retained R2 bank. No arbitrary teacher code is executed.
use crate::tutor_rust_r2::{self as r2, Expr, Learning, Op};
use crate::tutor_rust_r3 as r3;
use std::error::Error;
use std::fs;
use std::io::Read;
use std::path::Path;

type Result<T> = std::result::Result<T, Box<dyn Error>>;
type Example = (u8, u8, u8);

fn root_edits(expr: &Expr) -> Result<Vec<Expr>> {
    let Expr::Apply(_, left, right) = expr else {
        return Err("expected composed expression".into());
    };
    Ok(Op::ALL
        .into_iter()
        .map(|op| Expr::apply(op, left, right))
        .collect())
}

fn disagrees(expr: &Expr, examples: &[Example]) -> bool {
    examples.iter().any(|&(a, b, y)| expr.eval(a, b) != y)
}

fn repair_expr(expr: &Expr, examples: &[Example]) -> Result<Expr> {
    if !disagrees(expr, examples) {
        return Ok(expr.clone());
    }
    // Diagnosis and selection use admitted TRAIN labels only. The held-out
    // oracle does not guide selection, choose a patch, or provide labels.
    match r2::learn(&root_edits(expr)?, examples) {
        Learning::Unique(candidate) => Ok(candidate),
        _ => Err("root repair ambiguous or inconsistent".into()),
    }
}

/// Repair an existing closed-grammar bank. Refuse an unqualified parent,
/// unknown expressions or a result that fails independent examination.
pub fn repair(memory: &str, damaged: &str, train: &str) -> Result<String> {
    let parent = r3::verified_memory(memory, &r2::grammar())?;
    let candidates = r3::extend(&parent);
    let initial = r3::restore(damaged, &candidates)?;
    let admitted = r3::admit(train)?;
    let mut repaired = Vec::new();
    for (expr, examples) in initial.iter().zip(&admitted) {
        repaired.push(repair_expr(expr, examples)?);
    }
    let bank = r3::checkpoint(&repaired);
    r3::examine(memory, &bank)?;
    Ok(bank)
}

fn equivalent(a: &Expr, b: &Expr) -> bool {
    (0..=255u8).all(|x| (0..=255u8).all(|y| a.eval(x, y) == b.eval(x, y)))
}

#[derive(Debug, PartialEq, Eq)]
pub struct RepairReport {
    pub mutations: usize,
    pub equivalent: usize,
    pub harmful: usize,
    pub diagnosed: usize,
    pub repaired: usize,
    pub new_cases: u64,
    pub retained_cases: u64,
}

/// Independently benchmark all 49 single-root mutations of a qualified R3
/// bank. The clean bank is used only to manufacture and score the challenge;
/// the repair path receives the damaged expression and admitted TRAIN labels.
pub fn qualify(memory: &str, clean: &str, train: &str) -> Result<RepairReport> {
    let parent = r3::verified_memory(memory, &r2::grammar())?;
    let candidates = r3::extend(&parent);
    let initial = r3::restore(clean, &candidates)?;
    let admitted = r3::admit(train)?;
    let new_cases = r3::examine(memory, clean)?.iter().sum();
    let retained_cases = r2::examine(memory)?.iter().sum();
    let mut report = RepairReport {
        mutations: 0,
        equivalent: 0,
        harmful: 0,
        diagnosed: 0,
        repaired: 0,
        new_cases,
        retained_cases,
    };
    let mut simultaneous = Vec::new();
    for (original, examples) in initial.iter().zip(&admitted) {
        let mut first_bad = None;
        for mutant in root_edits(original)? {
            if mutant == *original {
                continue;
            }
            report.mutations += 1;
            if equivalent(original, &mutant) {
                report.equivalent += 1;
                continue;
            }
            report.harmful += 1;
            if disagrees(&mutant, examples) {
                report.diagnosed += 1;
            } else {
                return Err("harmful mutation invisible to TRAIN".into());
            }
            let fixed = repair_expr(&mutant, examples)?;
            if fixed.source() != original.source() {
                return Err("repair did not recover exact native bank expression".into());
            }
            report.repaired += 1;
            if first_bad.is_none() {
                first_bad = Some(mutant);
            }
        }
        simultaneous.push(first_bad.ok_or("task has no harmful root mutation")?);
    }
    let damaged = r3::checkpoint(&simultaneous);
    if report.mutations != 49
        || report.harmful == 0
        || report.harmful != report.diagnosed
        || report.harmful != report.repaired
        || report.equivalent + report.harmful != report.mutations
        || r3::examine(memory, &damaged).is_ok()
    {
        return Err("R4 mutation or baseline contract failed".into());
    }
    let fixed = repair(memory, &damaged, train)?;
    if fixed != clean || repair(memory, clean, train)? != clean {
        return Err("R4 exact recovery or clean replay failed".into());
    }
    Ok(report)
}

fn read(path: &Path) -> Result<String> {
    let mut raw = String::new();
    fs::File::open(path)?.take(8193).read_to_string(&mut raw)?;
    if raw.is_empty() || raw.len() > 8192 {
        return Err("R4 input size".into());
    }
    Ok(raw)
}

pub fn run_cli() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let (parent, child, train, source) = match args.as_slice() {
        [] => {
            let parent = r2::induce(&r2::fixture())?;
            let train = r3::fixture();
            let child = r3::induce(&parent, &train)?;
            (parent, child, train, "synthetic_fixture")
        }
        [flag, parent, child, train] if flag == "--qualify" => (
            read(Path::new(parent))?,
            read(Path::new(child))?,
            read(Path::new(train))?,
            "external_inputs",
        ),
        _ => return Err("usage: sml_tutor_rust_r4 [--qualify r2.bank r3.bank r3.train]".into()),
    };
    let report = qualify(&parent, &child, &train)?;
    println!(
        "SML_TUTOR_R4=PASS mutations={} equivalent={} harmful={} diagnosed={} repaired={} r3_cases={} retained_r2={} exact_recovery=true clean_replay=true source={source}",
        report.mutations,
        report.equivalent,
        report.harmful,
        report.diagnosed,
        report.repaired,
        report.new_cases,
        report.retained_cases
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_mutations_repaired_without_retraining() {
        let parent = r2::induce(&r2::fixture()).unwrap();
        let train = r3::fixture();
        let child = r3::induce(&parent, &train).unwrap();
        let result = qualify(&parent, &child, &train).unwrap();
        assert_eq!(result.mutations, 49);
        assert_eq!(result.repaired, result.harmful);
        assert_eq!(result.new_cases, 458_752);
        assert_eq!(result.retained_cases, 458_752);
        assert!(repair(&parent, "unknown program", &train).is_err());
        assert!(repair(&parent, &child, &train.replacen("0|0|0|0", "0|0|0|1", 1)).is_err());
        assert!(repair("invalid parent", &child, &train).is_err());
    }
}
