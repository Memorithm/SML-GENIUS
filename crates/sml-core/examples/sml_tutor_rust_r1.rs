//! R1: infer a typed Rust expression from input/output examples, then
//! independently compile and exhaustively examine the learned program.
//! The hypothesis grammar is closed and contains no untrusted Rust source.
use std::collections::BTreeSet;
use std::error::Error;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const TRAIN: [(u8, u8); 8] = [
    (0, 0),
    (1, 1),
    (255, 1),
    (1, 255),
    (3, 5),
    (128, 128),
    (17, 0),
    (0, 17),
];
const DEV: [(u8, u8); 8] = [
    (2, 7),
    (7, 2),
    (15, 16),
    (16, 15),
    (127, 129),
    (129, 127),
    (254, 2),
    (2, 254),
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Op {
    Add,
    Sub,
    ReverseSub,
    Mul,
    Xor,
    And,
    Or,
    Min,
    Max,
}

impl Op {
    const ALL: [Self; 9] = [
        Self::Add,
        Self::Sub,
        Self::ReverseSub,
        Self::Mul,
        Self::Xor,
        Self::And,
        Self::Or,
        Self::Min,
        Self::Max,
    ];

    fn expression(self) -> &'static str {
        match self {
            Self::Add => "a.wrapping_add(b)",
            Self::Sub => "a.wrapping_sub(b)",
            Self::ReverseSub => "b.wrapping_sub(a)",
            Self::Mul => "a.wrapping_mul(b)",
            Self::Xor => "a ^ b",
            Self::And => "a & b",
            Self::Or => "a | b",
            Self::Min => "a.min(b)",
            Self::Max => "a.max(b)",
        }
    }

    fn evaluate(self, a: u8, b: u8) -> u8 {
        match self {
            Self::Add => a.wrapping_add(b),
            Self::Sub => a.wrapping_sub(b),
            Self::ReverseSub => b.wrapping_sub(a),
            Self::Mul => a.wrapping_mul(b),
            Self::Xor => a ^ b,
            Self::And => a & b,
            Self::Or => a | b,
            Self::Min => a.min(b),
            Self::Max => a.max(b),
        }
    }

    fn parse(expression: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|op| op.expression() == expression)
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Learning {
    Unique(Op),
    Ambiguous,
    Inconsistent,
}

// The learner sees the examples only, never the task identifier or oracle.
fn learn(examples: &[(u8, u8, u8)]) -> Learning {
    let mut candidates = Op::ALL
        .into_iter()
        .filter(|op| examples.iter().all(|&(a, b, y)| op.evaluate(a, b) == y));
    match (candidates.next(), candidates.next()) {
        (None, _) => Learning::Inconsistent,
        (Some(op), None) => Learning::Unique(op),
        (Some(_), Some(_)) => Learning::Ambiguous,
    }
}

// Separate mathematical oracle: wider modular arithmetic and per-bit truth
// predicates, rather than evaluating the learner's Rust expressions.
fn oracle(task: usize, a: u8, b: u8) -> u8 {
    let x = u32::from(a);
    let y = u32::from(b);
    let value = match task {
        0 => (x + y) % 256,
        1 => (256 + x - y) % 256,
        2 => (256 + y - x) % 256,
        3 => (x * y) % 256,
        4..=6 => {
            let mut out = 0;
            for k in 0..8 {
                let left = (x / (1 << k)) % 2 == 1;
                let right = (y / (1 << k)) % 2 == 1;
                let selected = match task {
                    4 => left != right,
                    5 => left && right,
                    _ => left || right,
                };
                if selected {
                    out += 1 << k;
                }
            }
            out
        }
        7 => {
            if x <= y {
                x
            } else {
                y
            }
        }
        8 => {
            if x >= y {
                x
            } else {
                y
            }
        }
        _ => unreachable!("fixed curriculum task"),
    };
    u8::try_from(value).expect("bounded oracle output")
}

fn checkpoint(ops: &[Op]) -> String {
    let mut out = String::from("SML_TUTOR_R1_U8_V1\n");
    for (i, op) in ops.iter().enumerate() {
        out.push_str(&format!("{i}|{}\n", op.expression()));
    }
    out
}

fn restore(raw: &str) -> Result<Vec<Op>> {
    let mut lines = raw.lines();
    if lines.next() != Some("SML_TUTOR_R1_U8_V1") {
        return Err("invalid R1 checkpoint marker".into());
    }
    let mut ops = Vec::new();
    for (i, line) in lines.enumerate() {
        let (id, expr) = line.split_once('|').ok_or("invalid R1 row")?;
        if id != i.to_string() || i >= Op::ALL.len() {
            return Err("invalid R1 task identity".into());
        }
        ops.push(Op::parse(expr).ok_or("expression outside closed grammar")?);
    }
    if ops.len() != Op::ALL.len() || checkpoint(&ops) != raw {
        return Err("noncanonical or incomplete checkpoint".into());
    }
    Ok(ops)
}

fn program(ops: &[Op]) -> String {
    let mut source = String::from("#![forbid(unsafe_code)]\nuse std::io::{self, Write};\n");
    for (i, op) in ops.iter().enumerate() {
        source.push_str(&format!(
            "fn learned_{i}(a: u8, b: u8) -> u8 {{ {} }}\n",
            op.expression()
        ));
    }
    source.push_str(
        "fn main() -> io::Result<()> {\nlet mut out = io::BufWriter::new(io::stdout().lock());\nfor a in 0u8..=255 { for b in 0u8..=255 { out.write_all(&[",
    );
    for i in 0..ops.len() {
        source.push_str(&format!("learned_{i}(a,b),"));
    }
    source.push_str("])?; }}\nout.flush()\n}\n");
    source
}

struct Temp(PathBuf);
static NEXT: AtomicU64 = AtomicU64::new(0);

impl Temp {
    fn new() -> Result<Self> {
        let dir = std::env::temp_dir().join(format!(
            "sml-tutor-r1-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&dir)?;
        Ok(Self(dir))
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn qualify() -> Result<()> {
    let train: BTreeSet<_> = TRAIN.into_iter().collect();
    let dev: BTreeSet<_> = DEV.into_iter().collect();
    if train.len() != TRAIN.len() || dev.len() != DEV.len() || !train.is_disjoint(&dev) {
        return Err("partition overlap".into());
    }
    let mut learned = Vec::new();
    for task in 0..Op::ALL.len() {
        let examples: Vec<_> = TRAIN
            .iter()
            .map(|&(a, b)| (a, b, oracle(task, a, b)))
            .collect();
        match learn(&examples) {
            Learning::Unique(op) => learned.push(op),
            _ => return Err("R1 synthesis did not uniquely identify a program".into()),
        }
    }
    let raw = checkpoint(&learned);
    let reloaded = restore(&raw)?;
    if learned != reloaded {
        return Err("checkpoint changed after restore".into());
    }

    // Only the closed grammar above can emit code. No user/teacher source,
    // shell, dependencies, networking, unsafe operations or arbitrary calls.
    let temp = Temp::new()?;
    let source = temp.0.join("learned.rs");
    let executable = temp.0.join("learned-program");
    fs::write(&source, program(&reloaded))?;
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let compile = Command::new(rustc)
        .arg("--edition=2021")
        .arg("-Dwarnings")
        .arg("-O")
        .arg("--crate-name")
        .arg("sml_tutor_r1_learned")
        .arg(&source)
        .arg("-o")
        .arg(&executable)
        .stdin(Stdio::null())
        .output()?;
    if !compile.status.success() {
        return Err(format!(
            "compiler rejected synthesized program: {}",
            String::from_utf8_lossy(&compile.stderr)
        )
        .into());
    }
    let output = Command::new(&executable)
        .current_dir(&temp.0)
        .env_clear()
        .stdin(Stdio::null())
        .output()?;
    if !output.status.success()
        || !output.stderr.is_empty()
        || output.stdout.len() != 65_536 * learned.len()
    {
        return Err("compiled execution failed or incomplete".into());
    }

    let names = ["train_recall", "dev", "test", "testtest"];
    let mut correct = [0usize; 4];
    let mut total = [0usize; 4];
    let mut reload = 0usize;
    for a in 0u8..=255 {
        for b in 0u8..=255 {
            let i = usize::from(a) * 256 + usize::from(b);
            let cohort = if train.contains(&(a, b)) {
                0
            } else if dev.contains(&(a, b)) {
                1
            } else {
                2 + i % 2
            };
            for (task, op) in reloaded.iter().enumerate() {
                let want = oracle(task, a, b);
                let actual = output.stdout[i * reloaded.len() + task];
                total[cohort] += 1;
                correct[cohort] += usize::from(actual == want);
                reload += usize::from(op.evaluate(a, b) == want);
            }
        }
    }
    if correct != total || total.contains(&0) || reload != 65_536 * learned.len() {
        return Err("R1 compiled or reload mastery failed".into());
    }
    for i in 0..4 {
        println!(
            "SML_TUTOR_R1_COHORT cohort={} correct={} total={}",
            names[i], correct[i], total[i]
        );
    }
    println!("SML_TUTOR_R1=PASS programs={} compiler=rustc exact_cases={} reload={} teacher_inference=none", learned.len(), total.iter().sum::<usize>(), reload);
    Ok(())
}

fn main() -> Result<()> {
    qualify()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ambiguous_and_inconsistent_examples_never_emit_program() {
        assert_eq!(learn(&[]), Learning::Ambiguous);
        assert_eq!(learn(&[(0, 0, 0)]), Learning::Ambiguous);
        assert_eq!(learn(&[(0, 0, 1)]), Learning::Inconsistent);
        assert_eq!(learn(&[(3, 5, 8), (3, 5, 9)]), Learning::Inconsistent);
    }

    #[test]
    fn reject_expressions_outside_closed_grammar() {
        let bank = checkpoint(&Op::ALL);
        assert_eq!(restore(&bank).unwrap(), Op::ALL);
        assert!(restore(&bank.replace("a ^ b", "panic!()")).is_err());
        assert!(restore(&format!("{bank}9|a ^ b\n")).is_err());
    }

    #[test]
    fn synthesize_compile_and_exhaustively_qualify_nine_programs() {
        qualify().unwrap();
    }
}
