//! TUTOR-06: test transfer from persisted Boolean skills to binary addition.
//! No fitting, oracle-generated coefficients or arithmetic in the prediction path.
//! The composition is an explicitly supplied architecture, not learned routing.
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

impl Skills {
    fn full_adder(&self, a: u64, b: u64, carry: u64) -> (u64, u64) {
        let sum = apply(&self.parity, a, b, carry);
        let propagate = apply(&self.xor, a, b, 0);
        let generate = apply(&self.and, a, b, 0);
        let propagated = apply(&self.and, propagate, carry, 0);
        // The two carry terms are disjoint for correct XOR/AND skills.
        let next_carry = apply(&self.xor, generate, propagated, 0);
        (sum, next_carry)
    }

    fn verify_primitives(&self) -> Result<(), Box<dyn Error>> {
        for a in 0..=1 {
            for b in 0..=1 {
                if apply(&self.xor, a, b, 0) & 1 != a ^ b || apply(&self.and, a, b, 0) & 1 != a & b
                {
                    return Err("prior XOR/AND skill failed".into());
                }
                for c in 0..=1 {
                    if apply(&self.parity, a, b, c) & 1 != a ^ b ^ c {
                        return Err("prior PARITY3 skill failed".into());
                    }
                }
            }
        }
        Ok(())
    }
}

// Enumerate input pairs and both external carry-in values. Only the checker
// computes ordinary integer addition. Each next carry is the model prediction.
fn qualify(skills: &Skills, width: usize) -> (usize, usize) {
    assert!((1..=8).contains(&width));
    let cases = 1_usize << (2 * width + 1);
    let mask = (1_usize << width) - 1;
    let mut errors = 0;
    for start in (0..cases).step_by(64) {
        let lanes = (cases - start).min(64);
        let mut carry = 0_u64;
        let mut words = [0_u64; 9];
        for lane in 0..lanes {
            carry |= (((start + lane) >> (2 * width)) as u64) << lane;
        }
        for (bit, word) in words.iter_mut().take(width).enumerate() {
            let mut a = 0_u64;
            let mut b = 0_u64;
            for lane in 0..lanes {
                let address = start + lane;
                a |= (((address >> bit) & 1) as u64) << lane;
                b |= (((address >> (width + bit)) & 1) as u64) << lane;
            }
            (*word, carry) = skills.full_adder(a, b, carry);
        }
        words[width] = carry;
        for lane in 0..lanes {
            let address = start + lane;
            let expected =
                (address & mask) + ((address >> width) & mask) + (address >> (2 * width));
            let prediction = words[..=width]
                .iter()
                .enumerate()
                .fold(0_usize, |value, (bit, word)| {
                    value | ((((word >> lane) & 1) as usize) << bit)
                });
            errors += usize::from(prediction != expected);
        }
    }
    (errors, cases)
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

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = env::args().skip(1).collect();
    if args.len() != 2 {
        return Err("usage: STUDENT_BANK PARITY3_GATE".into());
    }
    let skills = restore(&read_bounded(&args[0], 256)?, &read_bounded(&args[1], 128)?)?;
    skills.verify_primitives()?;
    for width in [1, 2, 4, 8] {
        let (errors, cases) = qualify(&skills, width);
        println!("SML_TUTOR_COMPOSED_ADDITION width={width} errors={errors} cases={cases}");
        if errors != 0 {
            return Err("composed addition qualification failed".into());
        }
    }
    println!("SML_TUTOR_COMPOSED_ADDITION=PASS fitting=none routing=supplied");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const BANK: &str = "SML_TUTOR_BANK_V2\nSML_TUTOR_GATE_V1\n1,2\n6\nSML_TUTOR_GATE_V1\n1,2\n8\n";
    const PARITY: &str = "SML_TUTOR_PARITY3_V1\n1,2,3\n22\n";

    #[test]
    fn composed_addition_exhaustive_fixture() {
        let skills = restore(BANK, PARITY).unwrap();
        skills.verify_primitives().unwrap();
        for width in [1, 2, 4, 8] {
            assert_eq!(qualify(&skills, width), (0, 1 << (2 * width + 1)));
        }
    }

    #[test]
    fn corrupted_skills_cannot_pass() {
        let mut skills = restore(BANK, PARITY).unwrap();
        skills.and.flip(0);
        assert!(skills.verify_primitives().is_err());
        assert!(qualify(&skills, 4).0 > 0);
        let mut skills = restore(BANK, PARITY).unwrap();
        skills.parity.flip(0);
        assert!(skills.verify_primitives().is_err());
        assert!(qualify(&skills, 4).0 > 0);
        assert!(restore(BANK, "SML_TUTOR_PARITY3_V1\n0\n1\n").is_err());
    }
}
