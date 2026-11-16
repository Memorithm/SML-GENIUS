//! EXP-MUL1: real posit8 multiplication with a learned *integer significand*
//! multiplier, explicit (engineered) regime/exponent decoding and exact rounding.
//! The Boolean five-gate adder is learned; the field/parser/rounder is NOT.
//! CPU-only bounded experimental bridge, not a fully learned posit circuit.
#![forbid(unsafe_code)]

#[path = "posit8_learned_byte_multiply.rs"]
#[allow(dead_code)]
mod learned_multiplier;

#[path = "posit8_circuit_synth.rs"]
#[allow(dead_code)]
mod declared_oracle;

#[path = "posit8_independent_oracle.rs"]
#[allow(dead_code)]
mod independent;

#[path = "posit8_active_regime.rs"]
#[allow(dead_code)]
mod learned_regime;

use learned_multiplier::LearnedByteMultiplier;

// Posit Standard (2022), Sections 3.2–3.3 and 4.1: n=8, es=2,
// minPos=2^-24, maxPos=2^24, NaR=0x80, nearest with even-LSB midpoint.
const SCALE_BITS: u32 = 24;
const NAR: u8 = 0x80;
const MAX_POS: u8 = 0x7f;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Fields {
    negative: bool,
    significand: u8,
    shift: u32,
}

/// Value = sign * significand * 2^shift in integer units of 2^-24.
/// None represents NaR; zero has an explicit zero significand.
fn fields(raw: u8) -> Option<Fields> {
    if raw == NAR {
        return None;
    }
    if raw == 0 {
        return Some(Fields {
            negative: false,
            significand: 0,
            shift: 0,
        });
    }
    let negative = raw & 0x80 != 0;
    let magnitude = if negative { raw.wrapping_neg() } else { raw };
    // Actively selected XNOR / AND gates replace the imperative prefix loop.
    // Only gate identities are learned; wiring and regime arithmetic remain fixed.
    let (regime_bit, run_len) = learned_regime::detector().run_length(magnitude);
    let run_length = i32::from(run_len);
    let mut cursor = 6 - run_length;
    let regime = if regime_bit == 1 {
        run_length - 1
    } else {
        -run_length
    };
    if cursor >= 0 {
        cursor -= 1; // regime terminator
    }
    let mut exponent = 0i32;
    for _ in 0..2 {
        exponent <<= 1;
        if cursor >= 0 {
            exponent |= i32::from((magnitude >> cursor) & 1);
            cursor -= 1;
        }
    }
    let fraction_bits = (cursor + 1) as u32;
    let fraction_mask = (1u16 << fraction_bits) - 1;
    let significand = ((1u16 << fraction_bits) | (u16::from(magnitude) & fraction_mask)) as u8;
    let scale = SCALE_BITS as i32 + 4 * regime + exponent - fraction_bits as i32;
    assert!(
        (0..=48).contains(&scale),
        "posit8 field scale outside exact range"
    );
    Some(Fields {
        negative,
        significand,
        shift: scale as u32,
    })
}

fn candidate_squared_units(raw: u8) -> u128 {
    let decoded = fields(raw).expect("positive finite posit8 encoding");
    debug_assert!(!decoded.negative && decoded.significand > 0);
    u128::from(decoded.significand) << (decoded.shift + SCALE_BITS)
}

/// Round a positive exact product in units of 2^-48 to posit8.
/// This is an engineered monotone binary-search rounder, not learned gates.
fn round_product(magnitude: u128) -> u8 {
    if magnitude == 0 {
        return 0;
    }
    if magnitude <= candidate_squared_units(1) {
        return 1;
    }
    if magnitude >= candidate_squared_units(MAX_POS) {
        return MAX_POS;
    }
    let mut low = 1u8;
    let mut high = MAX_POS;
    while high - low > 1 {
        let mid = low + (high - low) / 2;
        if candidate_squared_units(mid) <= magnitude {
            low = mid;
        } else {
            high = mid;
        }
    }
    let left = magnitude - candidate_squared_units(low);
    let right = candidate_squared_units(high) - magnitude;
    if left < right || (left == right && low & 1 == 0) {
        low
    } else {
        high
    }
}

pub struct Posit8LearnedSignificandMul {
    learned: LearnedByteMultiplier,
}

impl Posit8LearnedSignificandMul {
    pub fn new() -> Self {
        Self {
            learned: LearnedByteMultiplier::train(),
        }
    }

    pub fn teacher_queries(&self) -> usize {
        self.learned.membership_queries()
    }

    pub fn multiply(&self, a: u8, b: u8) -> u8 {
        let (Some(lhs), Some(rhs)) = (fields(a), fields(b)) else {
            return NAR;
        };
        if lhs.significand == 0 || rhs.significand == 0 {
            return 0;
        }

        // The only significand multiplication is the actual learned 8x8
        // shift-and-add network, not the posit arithmetic teacher.
        let product = self.learned.multiply(lhs.significand, rhs.significand);
        let shift = lhs.shift + rhs.shift;
        let rounded = round_product(u128::from(product) << shift);
        if lhs.negative ^ rhs.negative {
            rounded.wrapping_neg()
        } else {
            rounded
        }
    }

    /// Exhaustive audit, separate from the runtime computation.
    pub fn audit_declared_oracle(&self) -> usize {
        let mut errors = 0usize;
        for a in 0..=255u8 {
            for b in 0..=255u8 {
                errors += usize::from(
                    self.multiply(a, b)
                        != declared_oracle::Posit8(a).mul(declared_oracle::Posit8(b)).0,
                );
            }
        }
        errors
    }
}

impl Default for Posit8LearnedSignificandMul {
    fn default() -> Self {
        Self::new()
    }
}

fn main() {
    let model = Posit8LearnedSignificandMul::new();
    match std::env::args().nth(1).as_deref() {
        None | Some("--audit") => {
            let errors = model.audit_declared_oracle();
            println!(
                "POSIT8_LEARNED_SIGNIFICAND_MUL,pairs=65536,teacher_queries={},regime_gate_queries={},errors={},full_posit_gate_learning=false",
                model.teacher_queries(),
                learned_regime::detector().membership_queries(),
                errors
            );
            if errors != 0 {
                std::process::exit(1);
            }
        }
        _ => panic!("valid option: --audit"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_encoding_has_correct_field_value() {
        for raw in 0..=255u8 {
            let parsed = fields(raw);
            let expected = declared_oracle::Posit8(raw).decode_units();
            assert_eq!(
                parsed.map(|f| {
                    let units = i128::from(f.significand) << f.shift;
                    if f.negative {
                        -units
                    } else {
                        units
                    }
                }),
                expected.map(i128::from),
                "raw=0x{raw:02x}"
            );
        }
    }

    #[test]
    fn boundary_and_exception_products() {
        let m = Posit8LearnedSignificandMul::new();
        assert_eq!(m.multiply(0, 0), 0);
        assert_eq!(m.multiply(NAR, 0), NAR);
        assert_eq!(m.multiply(0, NAR), NAR);
        assert_eq!(m.multiply(0x40, 0x40), 0x40);
        assert_eq!(m.multiply(0xc0, 0x40), 0xc0);
        assert_eq!(m.multiply(1, 1), 1); // nonzero underflow saturates
        assert_eq!(m.multiply(MAX_POS, MAX_POS), MAX_POS);
        assert_eq!(m.multiply(0x81, 0x7f), 0x81);
    }

    #[test]
    fn learned_regime_gate_budget_and_exhaustive_reference() {
        let learned = learned_regime::detector();
        assert!(learned.membership_queries() > 0);
        assert!(learned.membership_queries() <= 8);
        assert_eq!(learned.audit_all_magnitudes(), 0);
    }

    #[test]
    fn full_multiplication_matches_two_distinct_declared_oracles() {
        let m = Posit8LearnedSignificandMul::new();
        assert!(m.teacher_queries() > 0 && m.teacher_queries() <= 8);
        let mut count = 0usize;
        for a in 0..=255u8 {
            for b in 0..=255u8 {
                let actual = m.multiply(a, b);
                assert_eq!(
                    actual,
                    declared_oracle::Posit8(a).mul(declared_oracle::Posit8(b)).0,
                    "primary a={a} b={b}"
                );
                assert_eq!(
                    actual,
                    independent::IndependentPosit8(a)
                        .mul(independent::IndependentPosit8(b))
                        .0,
                    "independent a={a} b={b}"
                );
                count += 1;
            }
        }
        assert_eq!(count, 65_536);
    }
}
