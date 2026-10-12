//! Preregistered WideSparseANF k8..k1024 native CPU cost and truth-panel.
//! A synthetic kernel benchmark, NOT a trained SML model-quality test.

use std::{collections::BTreeSet, error::Error, hint::black_box, time::Instant};

use sml_core::{
    anf_large::LargeAnfGate,
    anf_local::{LocalAnfNetwork, LocalAnfShape, LocalAnfWiring},
    anf_wide_sparse::{WideAnfBackend, WideAnfGate, WideAnfLimits, WideAnfNetwork},
    toy_tasks::SplitMix64,
};

const WIDTHS: [usize; 7] = [8, 16, 32, 64, 128, 256, 1024];
const BENCH_ITERATIONS: usize = 8192;
const BENCH_REPEATS: usize = 5;
const BENCH_WARMUP: usize = 128;
const TRAJECTORY_STEPS: usize = 256;
const INPUT_PATTERNS: usize = 8;
const LANES: usize = 64;

fn limits() -> WideAnfLimits {
    WideAnfLimits::new(2048, 256, 8192, 8192, 8192)
}

/// Exactly the three registered sparse families and the single k8 dense control.
fn active_terms(width: usize, family: &str) -> Vec<Vec<usize>> {
    match family {
        "linear32" => (0..width.min(32))
            .map(|index| vec![index * width / width.min(32)])
            .collect(),
        "mixed16" => (0..16)
            .map(|index| {
                let degree = 2 + index % 3;
                let mut term = (0..degree)
                    .map(|part| (index + part * (width / 4 + 1)) % width)
                    .collect::<Vec<_>>();
                term.sort_unstable();
                term
            })
            .collect(),
        "full_and" => vec![(0..width).collect()],
        "dense128" if width == 8 => (0..128)
            .map(|mask| (0..width).filter(|&i| mask & (1 << i) != 0).collect())
            .collect(),
        _ => panic!("only preregistered family/width combinations"),
    }
}

fn patterned_inputs(width: usize, stream: u64) -> Vec<Vec<u64>> {
    let mut rng = SplitMix64::new(stream);
    let mut patterns = (0..INPUT_PATTERNS)
        .map(|_| (0..width).map(|_| rng.next_u64()).collect::<Vec<_>>())
        .collect::<Vec<_>>();
    patterns[0].fill(0);
    patterns[1].fill(u64::MAX);
    patterns[2] = (0..width)
        .map(|i| if i % 2 == 0 { u64::MAX } else { 0 })
        .collect();
    patterns[3] = (0..width).map(|i| 1u64 << (i % LANES)).collect();
    patterns[4] = (0..width)
        .map(|i| 0x5555_aaaa_3333_cccc ^ i as u64)
        .collect();
    patterns
}

/// Independent literal oracle, not using the production prefix graph or
/// the production packed bitmask evaluator.
fn literal_oracle(terms: &[Vec<usize>], source: &[u64], lane: usize) -> bool {
    terms.iter().fold(false, |acc, monomial| {
        acc ^ monomial
            .iter()
            .all(|&index| ((source[index] >> lane) & 1) != 0)
    })
}

fn validate_family(
    gate: &WideAnfGate,
    terms: &[Vec<usize>],
    patterns: &[Vec<u64>],
) -> Result<usize, Box<dyn Error>> {
    let mut scratch = vec![0u64; gate.scratch_words()];
    let mut compared = 0usize;
    for (pattern, source) in patterns.iter().enumerate() {
        let output = gate.evaluate_lanes(source, &mut scratch)?;
        for lane in 0..LANES {
            let expected = literal_oracle(terms, source, lane);
            if ((output >> lane) & 1 != 0) != expected {
                return Err(format!(
                    "native k{} vs literal mismatch at pattern {pattern} lane {lane}",
                    gate.fan_in()
                )
                .into());
            }
            compared += 1;
        }
    }
    Ok(compared)
}

fn bench_gate(
    gate: &WideAnfGate,
    patterns: &[Vec<u64>],
) -> Result<(f64, f64, f64, u64), Box<dyn Error>> {
    let mut scratch = vec![0u64; gate.scratch_words()];
    let mut checksum = 0u64;
    for index in 0..BENCH_WARMUP {
        checksum ^= black_box(gate.evaluate_lanes(
            black_box(&patterns[index % patterns.len()]),
            black_box(&mut scratch),
        )?);
    }
    let mut durations = [0f64; BENCH_REPEATS];
    for (run, duration) in durations.iter_mut().enumerate() {
        let start = Instant::now();
        for index in 0..BENCH_ITERATIONS {
            let chosen = black_box(&patterns[(index + run) % patterns.len()]);
            let value = gate.evaluate_lanes(chosen, &mut scratch)?;
            checksum ^= black_box(value.rotate_left((index % LANES) as u32));
        }
        *duration = start.elapsed().as_nanos() as f64 / BENCH_ITERATIONS as f64;
    }
    durations.sort_by(f64::total_cmp);
    Ok((
        durations[BENCH_REPEATS / 2],
        durations[0],
        durations[BENCH_REPEATS - 1],
        checksum,
    ))
}

fn native_recurrent_bridge(seed: u64) -> Result<(), Box<dyn Error>> {
    let native = LocalAnfNetwork::random(
        LocalAnfShape {
            state_bits: 128,
            fan_in: 8,
            readout_fan_in: 8,
            wiring: LocalAnfWiring::Local,
        },
        seed,
    )?;
    let adapted = WideAnfNetwork::from_local(&native, limits())?;
    let mut rng = SplitMix64::new(seed ^ 0x1d2e_3f4a_5b6c_7d8e);
    let inputs = (0..TRAJECTORY_STEPS)
        .map(|_| rng.next_u64())
        .collect::<Vec<_>>();
    let wide_outputs = adapted.run_lanes(&inputs)?;
    let mut source = vec![0u64; native.source_width()];
    let mut next = vec![0u64; native.state_bits()];
    let mut scratch = vec![0u64; native.scratch_len()];
    let mut mismatches = 0u64;
    for (step, &input) in inputs.iter().enumerate() {
        source[native.state_bits()] = input;
        native.step_lanes(&mut source, &mut next, &mut scratch);
        let reference = native.readout().evaluate_lanes(&source, &mut scratch);
        mismatches += u64::from((reference ^ wide_outputs[step]).count_ones());
    }
    if mismatches != 0 {
        return Err(format!("native network bridge mismatch on seed {seed}").into());
    }
    println!(
        "WIDE_ANF_BRIDGE_NATIVE,seed={seed},steps={TRAJECTORY_STEPS},lanes={LANES},state_bits=128,fan_in=8,bit_disagreements={mismatches},oracle=PASS"
    );
    Ok(())
}

fn existing_large_bridge() -> Result<(), Box<dyn Error>> {
    let mask = (1u32 << 30) | 1;
    let existing = LargeAnfGate::new(31, vec![0, 1, 3, 1u32 << 20, 1u32 << 30, mask])
        .map_err(|e| format!("invalid legacy large ANF test gate: {e:?}"))?;
    let wide = WideAnfGate::from_large(&existing, (0..31).collect(), 31, limits())?;
    let mut rng = SplitMix64::new(91);
    let inputs = (0..31).map(|_| rng.next_u64()).collect::<Vec<_>>();
    let mut scratch = vec![0u64; wide.scratch_words()];
    let output = wide.evaluate_lanes(&inputs, &mut scratch)?;
    let mut compared = 0usize;
    for lane in 0..LANES {
        let query = inputs.iter().enumerate().fold(0u32, |acc, (i, &word)| {
            acc | ((((word >> lane) & 1) as u32) << i)
        });
        if ((output >> lane) & 1 != 0)
            != existing
                .evaluate(query)
                .map_err(|e| format!("legacy large ANF evaluation failed: {e:?}"))?
        {
            return Err(format!("existing 31-bit sparse bridge mismatch lane {lane}").into());
        }
        compared += 1;
    }
    println!(
        "WIDE_ANF_BRIDGE_LARGE,address_bits=31,monomials=6,lanes={compared},bit_disagreements=0,oracle=PASS"
    );
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    println!(
        "WIDE_ANF_CONFIG,widths=8;16;32;64;128;256;1024,families=linear32;mixed16;full_and,dense_k8_control=dense128,iterations={BENCH_ITERATIONS},repeats={BENCH_REPEATS},warmup={BENCH_WARMUP},input_patterns={INPUT_PATTERNS},lanes={LANES},dense_threshold=one_quarter,oracle=literal_bitwise"
    );
    let mut passed_rows = 0usize;
    for width in WIDTHS {
        let mut families = vec!["linear32", "mixed16", "full_and"];
        if width == 8 {
            families.push("dense128");
        }
        for family in families {
            let terms = active_terms(width, family);
            let unique = terms
                .iter()
                .map(|term| term.iter().copied().collect::<BTreeSet<_>>())
                .collect::<BTreeSet<_>>();
            if unique.len() != terms.len() {
                return Err(format!("duplicate monomial in {family} width={width}").into());
            }
            let started = Instant::now();
            let gate =
                WideAnfGate::from_monomials((0..width).collect(), terms.clone(), width, limits())?;
            let construction_ns = started.elapsed().as_nanos();
            let should_dense = width <= 8 && terms.len() >= (1usize << width).div_ceil(4);
            if (gate.backend() == WideAnfBackend::Dense) != should_dense {
                return Err(format!("adaptive representation violated at {width}/{family}").into());
            }
            let patterns = patterned_inputs(width, width as u64 ^ 0xcace_fa11_0a00_fade);
            let compared = validate_family(&gate, &terms, &patterns)?;
            if compared != INPUT_PATTERNS * LANES {
                return Err("literal-oracle comparison census mismatch".into());
            }
            let (median_ns, min_ns, max_ns, checksum) = bench_gate(&gate, &patterns)?;
            println!(
                "WIDE_ANF_ROW,width={width},family={family},backend={},active_monomials={},fanin={},prefix_nodes={},stored_mask_words={},logical_payload_bytes={},scratch_words={},oracle_bits={},median_ns_per_call={median_ns:.3},min_ns_per_call={min_ns:.3},max_ns_per_call={max_ns:.3},construction_ns={construction_ns},checksum={checksum:016x},oracle=PASS",
                if should_dense { "dense" } else { "sparse" },
                gate.active_monomials(),
                gate.fan_in(),
                gate.prefix_nodes(),
                gate.stored_mask_words(),
                gate.payload_bytes(),
                gate.scratch_words(),
                compared,
            );
            passed_rows += 1;
        }
    }
    native_recurrent_bridge(11)?;
    native_recurrent_bridge(29)?;
    existing_large_bridge()?;
    if passed_rows != 22 {
        return Err("WideSparseANF preregistered 22-row census mismatched".into());
    }
    println!(
        "WIDE_ANF_COMPLETE,rows={passed_rows},literal_oracle_pass={passed_rows},native_recurrent_bridges=2,existing_large_bridges=1,unsafe_code=forbidden,dense_limit_k8_is_optimization_only=true"
    );
    Ok(())
}
