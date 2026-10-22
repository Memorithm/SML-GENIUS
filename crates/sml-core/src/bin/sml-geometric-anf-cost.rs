//! GB-4 matched CPU benchmark: geometric scan+LUT, binary+LUT, binary+ANF.
//!
//! Modes:
//!   cargo run --locked --release -p sml-core --bin sml-geometric-anf-cost -- deterministic
//!   cargo run --locked --release -p sml-core --bin sml-geometric-anf-cost -- benchmark
//!
//! A deterministic correctness run is not a performance measurement.
//! The benchmark prints raw local timing samples; neither CI VM results nor
//! this microbenchmark qualify a SML model-speed or hardware claim.

use std::hint::black_box;
use std::time::Instant;

use sml_core::geometric_boolean::Point;
use sml_core::geometric_fan::{pi_fan_mask, FanMask, RationalFan};
use sml_core::geometric_fan_anf::FanAnf;

const SEED: u64 = 0x6b4d_2026_1022_0001;

#[derive(Clone, Copy, Debug)]
enum Path {
    ScanLut,
    BinaryLut,
    BinaryAnf,
}

impl Path {
    const ALL: [Self; 3] = [Self::ScanLut, Self::BinaryLut, Self::BinaryAnf];

    const fn label(self) -> &'static str {
        match self {
            Self::ScanLut => "scan_lut",
            Self::BinaryLut => "binary_lut",
            Self::BinaryAnf => "binary_anf",
        }
    }
}

struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    const fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
}

/// Frozen query corpus. The geometry-specific boundary examples are
/// followed by the same seeded 4096 coordinate pairs at each level.
fn corpus(fan: &RationalFan) -> Vec<Point> {
    let mut samples = Vec::with_capacity(1088 + 2 * fan.sector_count() + 4096);
    for x in -16_i64..=16 {
        for y in -16_i64..=16 {
            if x != 0 || y != 0 {
                samples.push(Point { x, y });
            }
        }
    }
    for (index, &start) in fan.rays().iter().enumerate() {
        let end = fan.rays()[(index + 1) % fan.sector_count()];
        samples.push(start);
        samples.push(Point {
            x: start.x + end.x,
            y: start.y + end.y,
        });
    }
    let mut rng = SplitMix64::new(SEED);
    for _ in 0..4096 {
        let x = i64::from(rng.next_u64() as u32 as i32);
        let y = i64::from(rng.next_u64() as u32 as i32);
        samples.push(if x == 0 && y == 0 {
            Point { x: 1, y }
        } else {
            Point { x, y }
        });
    }
    samples
}

/// The same three frozen truth-table families for each geometry.
fn masks(fan: &RationalFan) -> [(&'static str, FanMask); 3] {
    let width = fan.sector_count() as u8;
    let domain = if width == 64 {
        u64::MAX
    } else {
        (1_u64 << width) - 1
    };
    [
        ("sparse", FanMask::new(width, 1).unwrap()),
        (
            "alternating",
            FanMask::new(width, 0xaaaa_aaaa_aaaa_aaaa & domain).unwrap(),
        ),
        ("pi", pi_fan_mask(fan, 0).unwrap()),
    ]
}

fn evaluate(path: Path, fan: &RationalFan, mask: FanMask, anf: FanAnf, point: Point) -> bool {
    match path {
        Path::ScanLut => {
            let sector = fan.classify_scan(point).expect("valid nonzero direction");
            mask.bits() & (1_u64 << sector) != 0
        }
        Path::BinaryLut => mask
            .contains_point(fan, point)
            .expect("matching fan and valid direction"),
        Path::BinaryAnf => anf
            .evaluate_point(fan, point)
            .expect("matching fan and valid direction"),
    }
}

/// Independent semantic parity for every sample, before the timing loop.
/// A sum alone is explicitly not accepted as evidence of parity.
fn validate(fan: &RationalFan, mask: FanMask, anf: FanAnf, points: &[Point]) -> u64 {
    assert_eq!(anf.reconstruct_mask(), Ok(mask));
    let mut ones = 0_u64;
    for &point in points {
        let scanned = fan.classify_scan(point).unwrap();
        let binary = fan.classify(point).unwrap();
        assert_eq!(scanned, binary, "classifiers disagree for {point:?}");
        let expected = evaluate(Path::ScanLut, fan, mask, anf, point);
        assert_eq!(
            evaluate(Path::BinaryLut, fan, mask, anf, point),
            expected,
            "binary LUT disagrees for {point:?}"
        );
        assert_eq!(
            evaluate(Path::BinaryAnf, fan, mask, anf, point),
            expected,
            "ANF disagrees for {point:?}"
        );
        ones += u64::from(expected);
    }
    ones
}

/// Measure exactly one path, using black-boxed queries and outcomes to
/// suppress trivially removable whole-loop work under release optimization.
fn measure(path: Path, fan: &RationalFan, mask: FanMask, anf: FanAnf, points: &[Point]) -> (u128, u64) {
    let start = Instant::now();
    let mut ones = 0_u64;
    for &point in points {
        let answer = black_box(evaluate(
            black_box(path),
            black_box(fan),
            black_box(mask),
            black_box(anf),
            black_box(point),
        ));
        ones += u64::from(answer);
    }
    (start.elapsed().as_nanos(), black_box(ones))
}

fn execute(mode: &str) {
    match mode {
        "deterministic" => println!("level,width,family,samples,ones,monomials"),
        "benchmark" => println!(
            "arch,os,level,width,family,samples,repeat,order,path,elapsed_ns,ones"
        ),
        _ => panic!("mode must be deterministic or benchmark"),
    }

    for level in 0..=3 {
        let fan = RationalFan::new(level).unwrap();
        let all_points = corpus(&fan);
        for (family, mask) in masks(&fan) {
            // Conversion is explicitly outside the timed evaluation loop.
            let anf = FanAnf::from_mask(mask);
            for count in [1024, 4096, all_points.len()] {
                let points = &all_points[..count];
                let expected = validate(&fan, mask, anf, points);
                if mode == "deterministic" {
                    println!(
                        "{level},{},{family},{count},{expected},{}",
                        mask.width(),
                        anf.monomial_count()
                    );
                    continue;
                }

                // 3 warmups and 9 measured repeats, rotating path order.
                for repetition in 0..12_usize {
                    for order in 0..3_usize {
                        let path = Path::ALL[(order + repetition) % 3];
                        let (elapsed, ones) = measure(path, &fan, mask, anf, points);
                        assert_eq!(ones, expected, "measured path altered predicate result");
                        if repetition >= 3 {
                            println!(
                                "{},{},{level},{},{family},{count},{},{order},{},{elapsed},{ones}",
                                std::env::consts::ARCH,
                                std::env::consts::OS,
                                mask.width(),
                                repetition - 3,
                                path.label()
                            );
                        }
                    }
                }
            }
        }
    }
}

fn main() {
    let mode = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "deterministic".to_owned());
    execute(&mode);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frozen_corpora_are_reproducible_and_nonzero() {
        for level in 0..=3 {
            let fan = RationalFan::new(level).unwrap();
            let a = corpus(&fan);
            assert_eq!(a, corpus(&fan));
            assert_eq!(a.len(), 1088 + 2 * fan.sector_count() + 4096);
            assert!(a.iter().all(|point| point.x != 0 || point.y != 0));
        }
    }

    #[test]
    fn all_frozen_families_match_independent_geometric_and_anf_paths() {
        for level in 0..=3 {
            let fan = RationalFan::new(level).unwrap();
            let points = corpus(&fan);
            for (_, mask) in masks(&fan) {
                let anf = FanAnf::from_mask(mask);
                let ones = validate(&fan, mask, anf, &points);
                assert!(ones <= points.len() as u64);
                assert_eq!(mask.width(), fan.sector_count() as u8);
            }
        }
    }

    #[test]
    fn splitmix64_known_first_value_is_deterministic() {
        let mut left = SplitMix64::new(SEED);
        let mut right = SplitMix64::new(SEED);
        assert_eq!(left.next_u64(), right.next_u64());
        assert_eq!(left.next_u64(), right.next_u64());
    }
}
