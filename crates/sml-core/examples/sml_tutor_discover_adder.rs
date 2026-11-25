//! Bounded expression synthesis from existing learned ANF primitives.
//! Search sees only supplied 8-row TRAIN targets. The integer checker is
//! independent and never participates in candidate enumeration or selection.
//! This is supervised circuit synthesis, not learned general-purpose routing.
use sml_core::anf_local::LocalAnfGate;
use std::{env, error::Error, fs::File, io::Read};

struct Skills {
    xor: LocalAnfGate,
    and: LocalAnfGate,
    parity: LocalAnfGate,
}

fn decode(lines: &[&str], marker: &str, width: usize) -> Result<LocalAnfGate, Box<dyn Error>> {
    if lines.len() != 3 || lines[0] != marker {
        return Err("invalid skill checkpoint schema".into());
    }
    let inputs = lines[1]
        .split(',')
        .map(str::parse::<usize>)
        .collect::<Result<Vec<_>, _>>()?;
    if inputs.is_empty() || inputs.len() > 4 || inputs.iter().any(|&i| !(1..width - 1).contains(&i))
    {
        return Err("invalid skill input domain".into());
    }
    Ok(LocalAnfGate::new(inputs, vec![lines[2].parse()?], width)?)
}

fn restore(bank: &str, parity: &str) -> Result<Skills, Box<dyn Error>> {
    if bank.len() > 256 || parity.len() > 128 {
        return Err("oversized checkpoint".into());
    }
    let lines: Vec<_> = bank.lines().collect();
    if lines.len() != 7 || lines[0] != "SML_TUTOR_BANK_V2" {
        return Err("invalid bank schema".into());
    }
    Ok(Skills {
        xor: decode(&lines[1..4], "SML_TUTOR_GATE_V1", 6)?,
        and: decode(&lines[4..7], "SML_TUTOR_GATE_V1", 6)?,
        parity: decode(
            &parity.lines().collect::<Vec<_>>(),
            "SML_TUTOR_PARITY3_V1",
            7,
        )?,
    })
}

// The original checkpoint's source positions are preserved, with unused
// nuisance and baseline inputs set to zero. 64 independent additions per word.
fn apply(gate: &LocalAnfGate, a: u64, b: u64, c: u64) -> u64 {
    let source = [0, a, b, c, 0, 0, 0];
    gate.evaluate_lanes(&source, &mut [0; 16])
}


#[derive(Clone, Debug, PartialEq, Eq)]
enum Expr {
    Input(usize),
    Xor(Box<Expr>, Box<Expr>),
    And(Box<Expr>, Box<Expr>),
    Parity(Box<Expr>, Box<Expr>, Box<Expr>),
}

impl Expr {
    fn eval(&self, skills: &Skills, inputs: &[u64; 3]) -> u64 {
        match self {
            Self::Input(i) => inputs[*i],
            Self::Xor(a, b) => apply(&skills.xor, a.eval(skills, inputs), b.eval(skills, inputs), 0),
            Self::And(a, b) => apply(&skills.and, a.eval(skills, inputs), b.eval(skills, inputs), 0),
            Self::Parity(a, b, c) => apply(
                &skills.parity,
                a.eval(skills, inputs),
                b.eval(skills, inputs),
                c.eval(skills, inputs),
            ),
        }
    }
}

fn train(raw: &str) -> Result<[u8; 2], Box<dyn Error>> {
    if raw.len() > 128 {
        return Err("oversized TRAIN".into());
    }
    let mut seen = 0_u8;
    let mut targets = [0_u8; 2];
    for line in raw.lines() {
        let fields = line
            .split('|')
            .map(|s| match s {
                "0" => Ok(0_u8),
                "1" => Ok(1_u8),
                _ => Err("invalid TRAIN bit"),
            })
            .collect::<Result<Vec<_>, _>>()?;
        let [a, b, c, sum, carry] = fields.as_slice() else {
            return Err("expected a|b|c|sum|carry".into());
        };
        let bit = 1_u8 << (a | (b << 1) | (c << 2));
        if seen & bit != 0 {
            return Err("duplicate TRAIN address".into());
        }
        seen |= bit;
        for (target, label) in targets.iter_mut().zip([sum, carry]) {
            if *label != 0 {
                *target |= bit;
            }
        }
    }
    if seen != u8::MAX {
        return Err("incomplete TRAIN".into());
    }
    Ok(targets)
}

#[derive(Clone)]
struct Candidate {
    signature: u8,
    expr: Expr,
}

fn retain(
    seen: &mut [bool; 256],
    layer: &mut Vec<Candidate>,
    signature: u8,
    make: impl FnOnce() -> Expr,
) {
    if !seen[usize::from(signature)] {
        seen[usize::from(signature)] = true;
        layer.push(Candidate {
            signature,
            expr: make(),
        });
    }
}

// Exact enumeration by expression-tree gate count, not DAG size. All
// possible truth signatures on the complete three-input domain are equivalent
// for subsequent pointwise compositions. Retaining the first minimal-cost
// expression per signature is therefore sound for this bounded search.
fn discover(skills: &Skills, targets: [u8; 2]) -> Result<([Expr; 2], [usize; 2]), Box<dyn Error>> {
    let mut seen = [false; 256];
    let mut layers = vec![Vec::new()];
    for (i, signature) in [0xaa, 0xcc, 0xf0].into_iter().enumerate() {
        retain(&mut seen, &mut layers[0], signature, || Expr::Input(i));
    }
    let mut work = 0_usize;
    for cost in 0..=4 {
        if cost > 0 {
            let mut next = Vec::new();
            for x in 0..cost {
                let y = cost - 1 - x;
                for a in &layers[x] {
                    for b in &layers[y] {
                        work += 2;
                        if work > 2_000_000 {
                            return Err("synthesis budget exhausted".into());
                        }
                        let av = u64::from(a.signature);
                        let bv = u64::from(b.signature);
                        retain(&mut seen, &mut next, apply(&skills.xor, av, bv, 0) as u8, || {
                            Expr::Xor(Box::new(a.expr.clone()), Box::new(b.expr.clone()))
                        });
                        retain(&mut seen, &mut next, apply(&skills.and, av, bv, 0) as u8, || {
                            Expr::And(Box::new(a.expr.clone()), Box::new(b.expr.clone()))
                        });
                    }
                }
                for y in 0..(cost - x) {
                    let z = cost - 1 - x - y;
                    for a in &layers[x] {
                        for b in &layers[y] {
                            for c in &layers[z] {
                                work += 1;
                                if work > 2_000_000 {
                                    return Err("synthesis budget exhausted".into());
                                }
                                let signature = apply(
                                    &skills.parity,
                                    u64::from(a.signature),
                                    u64::from(b.signature),
                                    u64::from(c.signature),
                                ) as u8;
                                retain(&mut seen, &mut next, signature, || {
                                    Expr::Parity(
                                        Box::new(a.expr.clone()),
                                        Box::new(b.expr.clone()),
                                        Box::new(c.expr.clone()),
                                    )
                                });
                            }
                        }
                    }
                }
            }
            layers.push(next);
        }
        if targets.iter().all(|&t| seen[usize::from(t)]) {
            let find = |target| {
                layers.iter().enumerate().find_map(|(cost, layer)| {
                    layer
                        .iter()
                        .find(|candidate| candidate.signature == target)
                        .map(|candidate| (candidate.expr.clone(), cost))
                })
            };
            let (sum, sc) = find(targets[0]).ok_or("missing sum")?;
            let (carry, cc) = find(targets[1]).ok_or("missing carry")?;
            println!("SML_TUTOR_DISCOVER work={work} sum_cost={sc} carry_cost={cc}");
            return Ok(([sum, carry], [sc, cc]));
        }
    }
    Err("no composition within bounded search".into())
}

// Held-out *compositions*, not unseen local truth-table rows. Every predicted
// carry flows into the next bit, with no oracle carry feedback.
fn qualify(skills: &Skills, exprs: &[Expr; 2], width: usize) -> (usize, usize) {
    let cases = 1_usize << (2 * width + 1);
    let mask = (1_usize << width) - 1;
    let mut errors = 0;
    for start in (0..cases).step_by(64) {
        let lanes = (cases - start).min(64);
        let mut carry = 0_u64;
        let mut output = [0_u64; 9];
        for lane in 0..lanes {
            carry |= (((start + lane) >> (2 * width)) as u64) << lane;
        }
        for (bit, word) in output.iter_mut().take(width).enumerate() {
            let mut inputs = [0, 0, carry];
            for lane in 0..lanes {
                let address = start + lane;
                inputs[0] |= (((address >> bit) & 1) as u64) << lane;
                inputs[1] |= (((address >> (width + bit)) & 1) as u64) << lane;
            }
            *word = exprs[0].eval(skills, &inputs);
            carry = exprs[1].eval(skills, &inputs);
        }
        output[width] = carry;
        for lane in 0..lanes {
            let address = start + lane;
            let expected =
                (address & mask) + ((address >> width) & mask) + (address >> (2 * width));
            let predicted = output[..=width]
                .iter()
                .enumerate()
                .fold(0, |n, (i, word)| n | ((((word >> lane) & 1) as usize) << i));
            errors += usize::from(predicted != expected);
        }
    }
    (errors, cases)
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = env::args().skip(1).collect();
    if args.len() != 3 {
        return Err("usage: STUDENT_BANK PARITY3_GATE TRAIN".into());
    }
    let skills = restore(&read_bounded(&args[0], 256)?, &read_bounded(&args[1], 128)?)?;
    let targets = train(&read_bounded(&args[2], 128)?)?;
    let (exprs, costs) = discover(&skills, targets)?;
    println!("SML_TUTOR_DISCOVER_EXPRS={exprs:?} costs={costs:?}");
    for width in [2, 4, 8] {
        let (errors, cases) = qualify(&skills, &exprs, width);
        println!("SML_TUTOR_DISCOVER_TEST width={width} errors={errors} cases={cases}");
        if errors != 0 {
            return Err("composition TEST veto".into());
        }
    }
    println!("SML_TUTOR_DISCOVER_ADDER=PASS search=bounded supervised=true");
    Ok(())
}

fn read_bounded(path: &str, limit: usize) -> Result<String, Box<dyn Error>> {
    let mut raw = String::new();
    File::open(path)?
        .take((limit + 1) as u64)
        .read_to_string(&mut raw)?;
    if raw.len() > limit {
        return Err("oversized checkpoint".into());
    }
    Ok(raw)
}


#[cfg(test)]
mod tests {
    use super::*;

    const BANK: &str = "SML_TUTOR_BANK_V2\nSML_TUTOR_GATE_V1\n1,2\n6\nSML_TUTOR_GATE_V1\n1,2\n8\n";
    const PARITY: &str = "SML_TUTOR_PARITY3_V1\n1,2,3\n22\n";
    const TRAIN: &str = "0|0|0|0|0\n1|0|0|1|0\n0|1|0|1|0\n1|1|0|0|1\n0|0|1|1|0\n1|0|1|0|1\n0|1|1|0|1\n1|1|1|1|1\n";

    #[test]
    fn discovers_and_transfers_to_unseen_word_widths() {
        let skills = restore(BANK, PARITY).unwrap();
        let (exprs, costs) = discover(&skills, train(TRAIN).unwrap()).unwrap();
        assert_eq!(costs[0], 1);
        assert!(costs[1] <= 4);
        for width in [2, 4, 8] {
            assert_eq!(qualify(&skills, &exprs, width), (0, 1 << (2 * width + 1)));
        }
    }

    #[test]
    fn wrong_supervision_cannot_pass_independent_test() {
        let skills = restore(BANK, PARITY).unwrap();
        let wrong = TRAIN.replace("1|1|1|1|1", "1|1|1|0|1");
        if let Ok((exprs, _)) = discover(&skills, train(&wrong).unwrap()) {
            assert!(qualify(&skills, &exprs, 4).0 > 0);
        }
        assert!(train(&format!("{TRAIN}0|0|0|0|0\n")).is_err());
        assert!(train("0|0|0|0|0\n").is_err());
    }
}
