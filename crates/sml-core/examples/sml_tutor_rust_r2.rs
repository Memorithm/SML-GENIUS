//! R2: bounded compositional induction from input/output examples.
//! No teacher-supplied source is compiled. The grammar is closed.
use std::error::Error;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
type Example = (u8, u8, u8);
const TASKS: usize = 7;
const TRAIN: [(u8, u8); 16] = [
    (0, 0), (1, 1), (255, 1), (1, 255), (3, 5), (128, 128), (17, 0), (0, 17),
    (12, 37), (143, 62), (78, 219), (199, 130), (85, 170), (7, 15), (31, 93),
    (251, 249),
];
const DEV: [(u8, u8); 8] = [
    (2, 7), (7, 2), (15, 16), (16, 15), (127, 129), (129, 127), (254, 2), (2, 254),
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Op {
    Add,
    Sub,
    Mul,
    Xor,
    And,
    Or,
    Min,
    Max,
}
impl Op {
    const ALL: [Self; 8] = [
        Self::Add, Self::Sub, Self::Mul, Self::Xor, Self::And, Self::Or,
        Self::Min, Self::Max,
    ];
    fn eval(self, a: u8, b: u8) -> u8 {
        match self {
            Self::Add => a.wrapping_add(b),
            Self::Sub => a.wrapping_sub(b),
            Self::Mul => a.wrapping_mul(b),
            Self::Xor => a ^ b,
            Self::And => a & b,
            Self::Or => a | b,
            Self::Min => a.min(b),
            Self::Max => a.max(b),
        }
    }
    fn source(self, a: &str, b: &str) -> String {
        match self {
            Self::Add => format!("({a}).wrapping_add({b})"),
            Self::Sub => format!("({a}).wrapping_sub({b})"),
            Self::Mul => format!("({a}).wrapping_mul({b})"),
            Self::Xor => format!("(({a}) ^ ({b}))"),
            Self::And => format!("(({a}) & ({b}))"),
            Self::Or => format!("(({a}) | ({b}))"),
            Self::Min => format!("({a}).min({b})"),
            Self::Max => format!("({a}).max({b})"),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Expr {
    A,
    B,
    Apply(Op, Box<Expr>, Box<Expr>),
}
impl Expr {
    fn apply(op: Op, a: &Self, b: &Self) -> Self {
        Self::Apply(op, Box::new(a.clone()), Box::new(b.clone()))
    }
    fn eval(&self, a: u8, b: u8) -> u8 {
        match self {
            Self::A => a,
            Self::B => b,
            Self::Apply(op, l, r) => op.eval(l.eval(a, b), r.eval(a, b)),
        }
    }
    fn source(&self) -> String {
        match self {
            Self::A => "a".into(),
            Self::B => "b".into(),
            Self::Apply(op, l, r) => op.source(&l.source(), &r.source()),
        }
    }
    fn cost(&self) -> usize {
        match self {
            Self::A | Self::B => 0,
            Self::Apply(_, l, r) => 1 + l.cost() + r.cost(),
        }
    }
}

fn grammar() -> Vec<Expr> {
    let leaves = [Expr::A, Expr::B];
    let mut one = Vec::new();
    for op in Op::ALL {
        for a in &leaves {
            for b in &leaves {
                one.push(Expr::apply(op, a, b));
            }
        }
    }
    let mut all = leaves.to_vec();
    all.extend(one.iter().cloned());
    for op in Op::ALL {
        for inner in &one {
            for leaf in &leaves {
                all.push(Expr::apply(op, inner, leaf));
                all.push(Expr::apply(op, leaf, inner));
            }
        }
    }
    all
}

#[derive(Debug, PartialEq, Eq)]
enum Learning {
    Unique(Expr),
    Ambiguous,
    Inconsistent,
}

// Compare only candidate predictions, not hidden oracle labels. The entire
// finite input domain is known; it is used to collapse semantic duplicates.
fn equivalent(a: &Expr, b: &Expr) -> bool {
    (0..=255u8).all(|x| (0..=255u8).all(|y| a.eval(x, y) == b.eval(x, y)))
}
fn learn(grammar: &[Expr], examples: &[Example]) -> Learning {
    let mut found: Option<Expr> = None;
    for expr in grammar {
        if !examples.iter().all(|&(a, b, y)| expr.eval(a, b) == y) {
            continue;
        }
        if let Some(first) = &found {
            if !equivalent(first, expr) {
                return Learning::Ambiguous;
            }
            if (expr.cost(), expr.source()) < (first.cost(), first.source()) {
                found = Some(expr.clone());
            }
        } else {
            found = Some(expr.clone());
        }
    }
    found.map_or(Learning::Inconsistent, Learning::Unique)
}

// Independent mathematical oracle, using wider arithmetic and per-bit sums.
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
        0 => xor((a + b) % 256, a),
        1 => xor(a, b) + b,
        2 => intersection(a, b) * a,
        3 => a - intersection(a, b),
        4 => xor(if a < b { a } else { b }, b),
        5 => b.saturating_sub(a),
        6 => a * (b + 1),
        _ => unreachable!(),
    };
    (value % 256) as u8
}

fn fixture() -> String {
    let mut raw = String::from("SML_TUTOR_R2_TRAIN_V1\n");
    for task in 0..TASKS {
        for (a, b) in TRAIN {
            raw.push_str(&format!("{task}|{a}|{b}|{}\n", oracle(task, a, b)));
        }
    }
    raw
}
fn admit(raw: &str) -> Result<Vec<Vec<Example>>> {
    if raw.len() > 8192 || !raw.ends_with('\n') {
        return Err("invalid R2 corpus size or termination".into());
    }
    let mut lines = raw.lines();
    if lines.next() != Some("SML_TUTOR_R2_TRAIN_V1") {
        return Err("invalid R2 marker".into());
    }
    let mut batch = Vec::new();
    for task in 0..TASKS {
        let mut examples = Vec::new();
        for (a, b) in TRAIN {
            let row = lines.next().ok_or("missing R2 TRAIN row")?;
            let y = oracle(task, a, b);
            if row != format!("{task}|{a}|{b}|{y}") {
                return Err(format!("invalid teacher row task={task}, a={a}, b={b}").into());
            }
            // Read the admitted teacher label, not a replacement oracle label.
            let supplied: u8 = row.rsplit('|').next().ok_or("missing label")?.parse()?;
            examples.push((a, b, supplied));
        }
        batch.push(examples);
    }
    if lines.next().is_some() {
        return Err("extra R2 TRAIN rows".into());
    }
    Ok(batch)
}

fn checkpoint(ops: &[Expr]) -> String {
    let mut raw = String::from("SML_TUTOR_R2_BANK_V1\n");
    for op in ops {
        raw.push_str(&op.source());
        raw.push('\n');
    }
    raw
}
fn restore(raw: &str, grammar: &[Expr]) -> Result<Vec<Expr>> {
    let mut lines = raw.lines();
    if lines.next() != Some("SML_TUTOR_R2_BANK_V1") || !raw.ends_with('\n') {
        return Err("invalid R2 bank".into());
    }
    let ops: Vec<_> = lines
        .map(|s| {
            grammar.iter().find(|e| e.source() == s).cloned().ok_or("unknown expression")
        })
        .collect::<std::result::Result<_, _>>()?;
    if ops.len() != TASKS || ops.iter().any(|e| e.cost() != 2) {
        return Err("invalid R2 bank composition count".into());
    }
    Ok(ops)
}

struct Temp(PathBuf);
impl Temp {
    fn new() -> Result<Self> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "sml-r2-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path)?;
        Ok(Self(path))
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn compile_and_run(ops: &[Expr], temp: &Path) -> Result<Vec<u8>> {
    // All expression source originates from the grammar, never the corpus.
    let mut source = String::from("#![allow(unused_parens)]\nuse std::io::{self,Write};\n");
    for (i, op) in ops.iter().enumerate() {
        source.push_str(&format!(
            "fn learned_{i}(a:u8,b:u8)->u8{{ {} }}\n", op.source()
        ));
    }
    source.push_str("fn main()->io::Result<()> {let mut out=io::BufWriter::new(io::stdout().lock());for a in 0u8..=255{for b in 0u8..=255{out.write_all(&[");
    for i in 0..ops.len() {
        source.push_str(&format!("learned_{i}(a,b),"));
    }
    source.push_str("])?;}}out.flush()}\n");
    let file = temp.join("learned.rs");
    let executable = temp.join("learned");
    fs::write(&file, source)?;
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let status = Command::new(rustc)
        .args(["--edition=2021", "-Dwarnings", "-O", "--crate-name", "sml_tutor_r2_learned"])
        .arg(&file).arg("-o").arg(&executable).stdin(Stdio::null()).output()?;
    if !status.status.success() {
        return Err(String::from_utf8_lossy(&status.stderr).into_owned().into());
    }
    let out = Command::new(&executable).current_dir(temp).env_clear().stdin(Stdio::null()).output()?;
    if !out.status.success() || !out.stderr.is_empty() || out.stdout.len() != 65_536 * TASKS {
        return Err("compiled execution failed".into());
    }
    Ok(out.stdout)
}

fn qualify(raw: &str, external: bool, output: Option<&Path>) -> Result<()> {
    let grammar = grammar();
    if grammar.len() != 1058 || TRAIN.iter().any(|p| DEV.contains(p)) {
        return Err("R2 grammar or partition contract".into());
    }
    let baseline: Vec<_> = grammar.iter().filter(|e| e.cost() < 2).cloned().collect();
    let mut learned = Vec::new();
    for examples in admit(raw)? {
        if learn(&grammar, &[]) != Learning::Ambiguous
            || learn(&baseline, &examples) != Learning::Inconsistent
        {
            return Err("baseline failed to abstain".into());
        }
        match learn(&grammar, &examples) {
            Learning::Unique(expr) if expr.cost() == 2 => learned.push(expr),
            _ => return Err("R2 synthesis not uniquely identified".into()),
        }
    }
    let temp = Temp::new()?;
    let bank_path = temp.0.join("bank");
    let bank = checkpoint(&learned);
    fs::write(&bank_path, &bank)?;
    let restored = restore(&fs::read_to_string(bank_path)?, &grammar)?;
    if checkpoint(&restored) != bank {
        return Err("bank roundtrip mismatch".into());
    }
    let out = compile_and_run(&restored, &temp.0)?;
    let mut totals = [0usize; 4];
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
                    return Err(format!("R2 exhaustive mismatch task={task}, a={a}, b={b}").into());
                }
                totals[cohort] += 1;
            }
        }
    }
    if let Some(path) = output {
        let mut file = fs::OpenOptions::new().write(true).create_new(true).open(path)?;
        file.write_all(bank.as_bytes())?;
        file.sync_all()?;
    }
    for (name, total) in ["train", "dev", "test", "testtest"].into_iter().zip(totals) {
        println!("SML_TUTOR_R2_COHORT cohort={name} correct={total} total={total}");
    }
    let teacher = if external { "external_verified_labels" } else { "synthetic_oracle" };
    println!("SML_TUTOR_R2=PASS programs={TASKS} grammar=1058 primitive_baseline_abstains={TASKS} exact_cases={} reload={} teacher={teacher}", totals.iter().sum::<usize>(), totals.iter().sum::<usize>());
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    match args.as_slice() {
        [] => qualify(&fixture(), false, None),
        [flag, input, bank] if flag == "--teacher-train" => {
            let mut raw = String::new();
            fs::File::open(input)?.take(8193).read_to_string(&mut raw)?;
            qualify(&raw, true, Some(Path::new(bank)))
        }
        _ => Err("usage: sml_tutor_rust_r2 [--teacher-train corpus bank]".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_bad_teacher_and_ambiguous_evidence() {
        let raw = fixture();
        assert_eq!(admit(&raw).unwrap().len(), TASKS);
        assert!(admit(&raw.replacen("0|0|0|0", "0|0|0|1", 1)).is_err());
        assert!(admit(&format!("{raw}0|0|0|0\n")).is_err());
        assert!(admit(&raw[..raw.len() - 1]).is_err());
        let g = grammar();
        assert_eq!(learn(&g, &[(0, 0, 0)]), Learning::Ambiguous);
        assert_eq!(learn(&g, &[(0, 0, 0), (0, 0, 1)]), Learning::Inconsistent);
        assert!(restore("SML_TUTOR_R2_BANK_V1\nunsafe{}\n", &g).is_err());
    }
    #[test]
    fn compositional_fixture_compiles_and_exhaustively_reloads() {
        let temp = Temp::new().unwrap();
        let bank = temp.0.join("result.bank");
        // Fixture is synthetic; it is deliberately not called a real teacher run.
        qualify(&fixture(), false, Some(&bank)).unwrap();
        assert_eq!(restore(&fs::read_to_string(bank).unwrap(), &grammar()).unwrap().len(), TASKS);
    }
}
