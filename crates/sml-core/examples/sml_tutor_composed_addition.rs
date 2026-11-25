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

// All result bits below are produced by the restored gates. Integer
// arithmetic is confined to indexing, input packing and independent checks.
type Word = [u64; 16];

impl Skills {
    fn xor_word(&self, a: u64, b: u64) -> u64 {
        apply(&self.xor, a, b, 0)
    }

    fn and_word(&self, a: u64, b: u64) -> u64 {
        apply(&self.and, a, b, 0)
    }

    fn add_words(&self, a: &Word, b: &Word, width: usize, mut carry: u64) -> (Word, u64) {
        let mut out = [0; 16];
        for i in 0..width {
            (out[i], carry) = self.full_adder(a[i], b[i], carry);
        }
        (out, carry)
    }

    fn subtract(&self, a: &Word, b: &Word, width: usize) -> (Word, u64) {
        let mut negated = [0; 16];
        for i in 0..width {
            negated[i] = self.xor_word(b[i], u64::MAX);
        }
        let (difference, carry) = self.add_words(a, &negated, width, u64::MAX);
        (difference, self.xor_word(carry, u64::MAX))
    }

    fn multiply(&self, a: &Word, b: &Word, width: usize) -> Word {
        let mut acc = [0; 16];
        for (j, &bit) in b.iter().take(width).enumerate() {
            let mut partial = [0; 16];
            for i in 0..width {
                partial[i + j] = self.and_word(a[i], bit);
            }
            acc = self.add_words(&acc, &partial, 2 * width, 0).0;
        }
        acc
    }

    fn mux(&self, old: u64, new: u64, select: u64) -> u64 {
        self.xor_word(old, self.and_word(self.xor_word(old, new), select))
    }

    // Restoring unsigned division, with one extra remainder bit. No host
    // comparisons or integer division in this prediction path.
    fn divide(&self, a: &Word, b: &Word, width: usize) -> (Word, Word, u64) {
        let mut quotient = [0; 16];
        let mut remainder = [0; 16];
        let mut zero = u64::MAX;
        for &bit in b.iter().take(width) {
            zero = self.and_word(zero, self.xor_word(bit, u64::MAX));
        }
        let valid = self.xor_word(zero, u64::MAX);
        for i in (0..width).rev() {
            for j in (1..=width).rev() {
                remainder[j] = remainder[j - 1];
            }
            remainder[0] = a[i];
            let (candidate, borrow) = self.subtract(&remainder, b, width + 1);
            let take = self.and_word(self.xor_word(borrow, u64::MAX), valid);
            quotient[i] = take;
            for j in 0..=width {
                remainder[j] = self.mux(remainder[j], candidate[j], take);
            }
        }
        // Division by zero is explicitly invalid, with zero output payload.
        for bit in &mut remainder {
            *bit = self.and_word(*bit, valid);
        }
        (quotient, remainder, valid)
    }
}

fn unpack(words: &Word, width: usize, lane: usize) -> usize {
    words[..width]
        .iter()
        .enumerate()
        .fold(0, |n, (i, word)| n | ((((word >> lane) & 1) as usize) << i))
}

fn bit(word: u64, lane: usize) -> bool {
    (word >> lane) & 1 != 0
}

#[derive(Debug, Default, PartialEq, Eq)]
struct ArithmeticReport {
    cases: usize,
    errors: [usize; 5],
    nonzero_divisors: usize,
    zero_divisors: usize,
}

fn qualify_suite(skills: &Skills, width: usize) -> ArithmeticReport {
    assert!((1..=8).contains(&width));
    let cases = 1_usize << (2 * width);
    let mask = (1_usize << width) - 1;
    let mut report = ArithmeticReport {
        cases,
        ..ArithmeticReport::default()
    };
    for start in (0..cases).step_by(64) {
        let lanes = (cases - start).min(64);
        let mut a = [0; 16];
        let mut b = [0; 16];
        for lane in 0..lanes {
            let address = start + lane;
            for i in 0..width {
                a[i] |= (((address >> i) & 1) as u64) << lane;
                b[i] |= (((address >> (width + i)) & 1) as u64) << lane;
            }
        }
        let (sum, carry) = skills.add_words(&a, &b, width, 0);
        let (difference, borrow) = skills.subtract(&a, &b, width);
        let product = skills.multiply(&a, &b, width);
        let (quotient, remainder, valid) = skills.divide(&a, &b, width);
        let mut equal = u64::MAX;
        for i in 0..width {
            equal = skills.and_word(
                equal,
                skills.xor_word(skills.xor_word(a[i], b[i]), u64::MAX),
            );
        }
        for lane in 0..lanes {
            let x = (start + lane) & mask;
            let y = (start + lane) >> width;
            let actual_sum = unpack(&sum, width, lane) | (usize::from(bit(carry, lane)) << width);
            report.errors[0] += usize::from(actual_sum != x + y);
            report.errors[1] += usize::from(
                unpack(&difference, width, lane) != x.wrapping_sub(y) & mask
                    || bit(borrow, lane) != (x < y),
            );
            report.errors[2] += usize::from(unpack(&product, 2 * width, lane) != x * y);
            report.errors[3] +=
                usize::from(bit(borrow, lane) != (x < y) || bit(equal, lane) != (x == y));
            let q = unpack(&quotient, width, lane);
            let r = unpack(&remainder, width + 1, lane);
            if y == 0 {
                report.zero_divisors += 1;
                report.errors[4] += usize::from(bit(valid, lane) || q != 0 || r != 0);
            } else {
                report.nonzero_divisors += 1;
                report.errors[4] += usize::from(!bit(valid, lane) || q != x / y || r != x % y);
            }
        }
    }
    report
}

fn run_suite(skills: &Skills) -> Result<(), Box<dyn Error>> {
    for width in [1, 2, 4, 8] {
        let report = qualify_suite(skills, width);
        for (operation, errors) in ["add", "subtract", "multiply", "compare", "divide"]
            .into_iter()
            .zip(report.errors)
        {
            println!(
                "SML_TUTOR_ARITHMETIC op={operation} width={width} errors={errors} cases={}",
                report.cases
            );
        }
        println!(
            "SML_TUTOR_DIVISION_DOMAIN width={width} valid={} zero={}",
            report.nonzero_divisors, report.zero_divisors
        );
        if report.errors != [0; 5] {
            return Err("arithmetic suite veto".into());
        }
    }
    println!("SML_TUTOR_ARITHMETIC_SUITE=PASS fitting=none routing=supplied");
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

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = env::args().skip(1).collect();
    let (suite, paths) = if args.first().is_some_and(|arg| arg == "--suite") {
        (true, &args[1..])
    } else {
        (false, args.as_slice())
    };
    if paths.len() != 2 {
        return Err("usage: [--suite] STUDENT_BANK PARITY3_GATE".into());
    }
    let skills = restore(
        &read_bounded(&paths[0], 256)?,
        &read_bounded(&paths[1], 128)?,
    )?;
    skills.verify_primitives()?;
    if suite {
        return run_suite(&skills);
    }
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
    fn arithmetic_suite_exhaustive_fixture() {
        let skills = restore(BANK, PARITY).unwrap();
        for width in [1, 2, 4, 8] {
            let report = qualify_suite(&skills, width);
            assert_eq!(report.errors, [0; 5]);
            assert_eq!(report.cases, 1 << (2 * width));
            assert_eq!(report.zero_divisors, 1 << width);
            assert_eq!(report.nonzero_divisors, report.cases - report.zero_divisors);
        }
        let mut broken = restore(BANK, PARITY).unwrap();
        broken.and.flip(3);
        assert_ne!(qualify_suite(&broken, 4).errors, [0; 5]);
    }

    #[test]
    fn corrupted_skills_cannot_pass() {
        let mut cancelled = restore(BANK, PARITY).unwrap();
        cancelled.and.flip(0);
        assert!(cancelled.verify_primitives().is_err());
        assert_eq!(qualify(&cancelled, 4).0, 0);
        let mut skills = restore(BANK, PARITY).unwrap();
        // Constant inversions cancel in the two XOR-combined carry terms.
        // Corrupt the conjunction monomial instead; primitive checks reject both.
        skills.and.flip(3);
        assert!(skills.verify_primitives().is_err());
        assert!(qualify(&skills, 4).0 > 0);
        let mut skills = restore(BANK, PARITY).unwrap();
        skills.parity.flip(0);
        assert!(skills.verify_primitives().is_err());
        assert!(qualify(&skills, 4).0 > 0);
        assert!(restore(BANK, "SML_TUTOR_PARITY3_V1\n0\n1\n").is_err());
    }
}
