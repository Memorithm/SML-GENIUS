//! Clean-room posit8 exact arithmetic oracle and bounded Boolean circuit discovery.
//! Research only: never claim native speed, full ADD/MUL synthesis or standard compliance.
#![forbid(unsafe_code)]

use std::fmt::Write;
const SCALE_BITS: u32 = 24;
const TRAIN_SAMPLES: usize = 192;
const MAX_GATES: usize = 32;

/// A 2022-style posit8 with two exponent bits, base-16 regime, NaR=0x80.
/// Finite values are exact integers in units of 2^-24.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Posit8(pub u8);

impl Posit8 {
    pub fn decode_units(self) -> Option<i64> {
        let raw = self.0;
        if raw == 0x80 { return None; }
        if raw == 0 { return Some(0); }
        let negative = raw & 0x80 != 0;
        let positive = if negative { raw.wrapping_neg() } else { raw };
        let mut cursor = 6i32;
        let first = (positive >> 6) & 1;
        let mut count = 0i32;
        while cursor >= 0 && ((positive >> cursor) & 1) == first {
            count += 1;
            cursor -= 1;
        }
        let regime = if first != 0 { count - 1 } else { -count };
        if cursor >= 0 { cursor -= 1; }
        let mut exponent = 0i32;
        for _ in 0..2 {
            exponent <<= 1;
            if cursor >= 0 {
                exponent |= i32::from((positive >> cursor) & 1);
                cursor -= 1;
            }
        }
        let fraction_bits = (cursor + 1) as u32;
        let fraction_mask = (1u32 << fraction_bits) - 1;
        let significand = (1i64 << fraction_bits)
            + i64::from(u32::from(positive) & fraction_mask);
        let shift = SCALE_BITS as i32 + 4 * regime + exponent - fraction_bits as i32;
        assert!((0..=48).contains(&shift));
        let units = significand << shift;
        Some(if negative { -units } else { units })
    }
    pub fn neg(self) -> Self { Self(self.0.wrapping_neg()) }
    /// Exact nearest encoding by integer distances, ties to even LSB;
    /// saturates nonzero under/overflow. NOT an independently audited oracle.
    fn round_scaled(target: i128, denominator_shift: u32) -> Self {
        if target == 0 { return Self(0); }
        let negative = target < 0;
        let magnitude = target.abs();
        let denominator = 1i128 << denominator_shift;
        let mut best = 1u8;
        let mut best_distance = i128::MAX;
        for encoding in 1..=127u8 {
            let candidate = i128::from(Self(encoding).decode_units().unwrap()) * denominator;
            let distance = (candidate - magnitude).abs();
            if distance < best_distance
                || (distance == best_distance && encoding & 1 == 0 && best & 1 != 0)
            {
                best_distance = distance;
                best = encoding;
            }
        }
        Self(if negative { best.wrapping_neg() } else { best })
    }
    pub fn add(self, rhs: Self) -> Self {
        match (self.decode_units(), rhs.decode_units()) {
            (Some(a), Some(b)) => Self::round_scaled(i128::from(a) + i128::from(b), 0),
            _ => Self(0x80),
        }
    }
    pub fn mul(self, rhs: Self) -> Self {
        match (self.decode_units(), rhs.decode_units()) {
            (Some(a), Some(b)) => Self::round_scaled(i128::from(a) * i128::from(b), SCALE_BITS),
            _ => Self(0x80),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Truth([u64; 4]);
fn is_train(x: u8) -> bool { x % 8 != 5 && x % 8 != 6 }
impl Truth {
    fn from_fn(f: impl Fn(u8) -> bool) -> Self {
        let mut result = [0u64; 4];
        for input in 0..=255u8 {
            if f(input) {
                result[usize::from(input / 64)] |= 1u64 << (input % 64);
            }
        }
        Self(result)
    }
    fn bit(self, input: u8) -> bool {
        self.0[usize::from(input / 64)] & (1u64 << (input % 64)) != 0
    }
    fn apply(self, op: Binary, other: Self) -> Self {
        Self(std::array::from_fn(|i| match op {
            Binary::And => self.0[i] & other.0[i],
            Binary::Or => self.0[i] | other.0[i],
            Binary::Xor => self.0[i] ^ other.0[i],
        }))
    }
    fn matches_training(self, target: Self) -> bool {
        (0..=255u8).filter(|&x| is_train(x))
            .all(|x| self.bit(x) == target.bit(x))
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Binary { And, Or, Xor }
impl Binary {
    fn symbol(self) -> &'static str {
        match self { Self::And => "&", Self::Or => "|", Self::Xor => "^" }
    }
}
#[derive(Clone, Copy, Debug)]
enum Expr { Input(u8), Gate { op: Binary, left: usize, right: usize } }
#[derive(Clone, Copy, Debug)]
struct Node { expr: Expr, truth: Truth, depth: usize }
#[derive(Debug)]
pub struct Synthesized { nodes: Vec<Node>, outputs: [usize; 8] }

impl Synthesized {
    /// Exact one-gate enumerative expansion on TRAIN only. The teacher
    /// supplies prefix-OR intermediate targets; this is NOT unrestricted
    /// end-to-end circuit synthesis for posit addition/multiplication.
    fn learn_signal(&mut self, desired: Truth) -> Result<usize, &'static str> {
        if let Some(i) = self.nodes.iter().position(|node| node.truth.matches_training(desired)) {
            return Ok(i);
        }
        if self.nodes.len() >= MAX_GATES + 8 { return Err("gate budget exceeded"); }
        let mut candidates = Vec::new();
        for left in 0..self.nodes.len() {
            for right in (left + 1)..self.nodes.len() {
                for op in [Binary::And, Binary::Or, Binary::Xor] {
                    let truth = self.nodes[left].truth.apply(op, self.nodes[right].truth);
                    if truth.matches_training(desired) {
                        let depth = 1 + self.nodes[left].depth.max(self.nodes[right].depth);
                        candidates.push((depth, left, right, op, truth));
                    }
                }
            }
        }
        candidates.sort_by_key(|(depth, left, right, op, _)| {
            let rank = match op {
                Binary::And => 0, Binary::Or => 1, Binary::Xor => 2,
            };
            (*depth, *left, *right, rank)
        });
        let Some((depth, left, right, op, truth)) = candidates.into_iter().next() else {
            return Err("no one-gate candidate fits the training target");
        };
        let id = self.nodes.len();
        self.nodes.push(Node { expr: Expr::Gate { op, left, right }, truth, depth });
        Ok(id)
    }
    pub fn learn_negation() -> Result<Self, &'static str> {
        let mut model = Self {
            nodes: (0..8).map(|bit| Node {
                expr: Expr::Input(bit),
                truth: Truth::from_fn(|x| (x >> bit) & 1 != 0),
                depth: 0,
            }).collect(),
            outputs: [0; 8],
        };
        for bit in 0..8usize {
            if bit > 0 {
                // A teacher-directed intermediate signal, fitted by gates.
                let prefix = Truth::from_fn(|x| {
                    u16::from(x) & ((1u16 << bit) - 1) != 0
                });
                model.learn_signal(prefix)?;
            }
            let desired = Truth::from_fn(|x| {
                (x.wrapping_neg() >> bit) & 1 != 0
            });
            model.outputs[bit] = model.learn_signal(desired)?;
        }
        Ok(model)
    }
    pub fn evaluate(&self, input: u8) -> u8 {
        let mut values = Vec::with_capacity(self.nodes.len());
        for node in &self.nodes {
            let bit = match node.expr {
                Expr::Input(bit) => (input >> bit) & 1,
                Expr::Gate { op, left, right } => match op {
                    Binary::And => values[left] & values[right],
                    Binary::Or => values[left] | values[right],
                    Binary::Xor => values[left] ^ values[right],
                },
            };
            values.push(bit);
        }
        self.outputs.iter().enumerate().fold(0u8, |raw, (bit, &id)| {
            raw | (values[id] << bit)
        })
    }
    pub fn gates(&self) -> usize { self.nodes.len() - 8 }
    pub fn max_depth(&self) -> usize {
        self.outputs.iter().map(|&i| self.nodes[i].depth).max().unwrap_or(0)
    }
    pub fn heldout_errors(&self) -> usize {
        (0..=255u8)
            .filter(|&x| !is_train(x) && self.evaluate(x) != x.wrapping_neg())
            .count()
    }
    pub fn verify_all(&self) -> Result<(), &'static str> {
        if self.gates() > MAX_GATES { return Err("gate budget exceeded"); }
        if (0..=255u8).any(|x| self.evaluate(x) != Posit8(x).neg().0) {
            return Err("exhaustive truth-table mismatch");
        }
        Ok(())
    }
    /// Emits a loop-free, bounded Rust kernel compiled by rustc/LLVM to
    /// the CPU's actual native ISA. No handwritten assembly or GPU dependency.
    pub fn emit_rust(&self) -> String {
        let mut out = String::from(
            "#![forbid(unsafe_code)]\n#[no_mangle]\n#[inline(never)]\npub extern \"C\" fn learned_posit8_neg(x: u8) -> u8 {\n"
        );
        for (index, node) in self.nodes.iter().enumerate() {
            match node.expr {
                Expr::Input(bit) => {
                    writeln!(&mut out, "    let w{index}: u8 = (x >> {bit}) & 1;").unwrap();
                }
                Expr::Gate { op, left, right } => {
                    writeln!(&mut out, "    let w{index}: u8 = w{left} {} w{right};", op.symbol()).unwrap();
                }
            }
        }
        let terms = self.outputs.iter().enumerate().map(|(bit, &id)| {
            format!("(w{id} << {bit})")
        }).collect::<Vec<_>>().join(" | ");
        writeln!(&mut out, "    {terms}\n}}").unwrap();
        out
    }
}
fn main() {
    let learned = Synthesized::learn_negation().expect("bounded train search");
    learned.verify_all().expect("exhaustive negation");
    if std::env::args().any(|arg| arg == "--emit-rust") {
        print!("{}", learned.emit_rust());
    } else {
        println!("POSIT8_CIRCUIT_NEG PASS train={} holdout=64 gates={} depth={} exhaustive=256 errors=0",
            TRAIN_SAMPLES, learned.gates(), learned.max_depth());
        println!("POSIT8_ARITHMETIC_ORACLE add_mul=65536_pairs_each; add_mul_circuit_synthesis=NOT_IMPLEMENTED");
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn posit8_decode_endpoints_and_symmetry() {
        assert_eq!(Posit8(0).decode_units(), Some(0));
        assert_eq!(Posit8(0x80).decode_units(), None);
        assert_eq!(Posit8(1).decode_units(), Some(1));
        assert_eq!(Posit8(0x7f).decode_units(), Some(1i64 << 48));
        assert_eq!(Posit8(0x40).decode_units(), Some(1i64 << SCALE_BITS));
        assert_eq!(Posit8(0xc0).decode_units(), Some(-(1i64 << SCALE_BITS)));
        for x in 0..=255u8 {
            if x != 0x80 {
                assert_eq!(Posit8(x).decode_units().unwrap(),
                    -Posit8(x.wrapping_neg()).decode_units().unwrap());
            }
        }
        let positives: Vec<_> = (1..=127u8)
            .map(|x| Posit8(x).decode_units().unwrap()).collect();
        assert!(positives.windows(2).all(|w| w[0] < w[1]));
    }
    #[test]
    fn exhaustive_arithmetic_65536_pairs_per_operation() {
        let zero = Posit8(0);
        let one = Posit8(0x40);
        for ia in 0..=255u8 {
            let a = Posit8(ia);
            assert_eq!(a.add(zero), a);
            assert_eq!(a.mul(one), a);
            for ib in 0..=255u8 {
                let b = Posit8(ib);
                assert_eq!(a.add(b), b.add(a));
                assert_eq!(a.mul(b), b.mul(a));
                if ia == 0x80 || ib == 0x80 {
                    assert_eq!(a.add(b).0, 0x80);
                    assert_eq!(a.mul(b).0, 0x80);
                } else {
                    assert_eq!(a.add(b), a.neg().add(b.neg()).neg());
                    assert_eq!(a.mul(b), a.neg().mul(b).neg());
                }
            }
        }
        assert_eq!(one.add(one), Posit8(0x48));
        assert_eq!(one.mul(one), one);
        assert_eq!(one.add(one.neg()), zero);
        assert_eq!(Posit8(1).mul(Posit8(1)), Posit8(1));
        assert_eq!(Posit8(0x7f).mul(Posit8(0x7f)), Posit8(0x7f));
    }
    #[test]
    fn train_holdout_and_full_negation_truth_table() {
        let graph = Synthesized::learn_negation().unwrap();
        assert_eq!((0..=255u8).filter(|&x| is_train(x)).count(), TRAIN_SAMPLES);
        assert_eq!((0..=255u8).filter(|&x| !is_train(x)).count(), 64);
        assert!(graph.gates() <= MAX_GATES);
        assert_eq!(graph.heldout_errors(), 0);
        graph.verify_all().unwrap();
        assert!(graph.emit_rust().contains("extern \"C\" fn learned_posit8_neg"));
    }
}
