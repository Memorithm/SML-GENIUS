//! Bounded compositional gap discovery over four restored recurrent gates.
//! Exhaustive expression trees, three external inputs, no supplied target catalog,
//! no constants, no DAG sharing, no fitting, and no checkpoint publication.
use sml_core::anf_local::LocalAnfGate;
use std::{env, error::Error, fs::File, io::Read};

const INPUTS: [u8; 3] = [0xaa, 0xcc, 0xf0];
const MAX_COST: usize = 3;
const MARKERS: [&str; 4] = [
    "SML_TUTOR_RECURRENT_V1",
    "SML_TUTOR_TOGGLE_V1",
    "SML_TUTOR_SET_V1",
    "SML_TUTOR_CLEAR_V1",
];

fn bits(address: usize) -> [bool; 3] {
    [address & 1 != 0, address & 2 != 0, address & 4 != 0]
}

fn oracle(kind: usize, [a, b, c]: [bool; 3]) -> bool {
    match kind {
        0 => {
            if c {
                b
            } else {
                a
            }
        }
        1 => a ^ (b & c),
        2 => a | (b & c),
        3 => a & !(b & c),
        _ => unreachable!(),
    }
}

fn restore(raw: &str, kind: usize) -> Result<LocalAnfGate, Box<dyn Error>> {
    let lines: Vec<_> = raw.lines().collect();
    if raw.len() > 128 || lines.len() != 3 || lines[0] != MARKERS[kind] || lines[1] != "0,1,2" {
        return Err("checkpoint schema".into());
    }
    let gate = LocalAnfGate::new(vec![0, 1, 2], vec![lines[2].parse()?], 3)?;
    if (0..8).any(|i| gate.evaluate(&bits(i)) != oracle(kind, bits(i))) {
        return Err("retained gate behavior".into());
    }
    Ok(gate)
}

fn read_gate(path: &str, kind: usize) -> Result<LocalAnfGate, Box<dyn Error>> {
    let mut raw = String::new();
    File::open(path)?.take(129).read_to_string(&mut raw)?;
    restore(&raw, kind)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Recipe {
    Input(usize),
    Gate { kind: usize, children: [u8; 3] },
}

struct Closure {
    levels: Vec<Vec<u8>>,
    recipes: [Option<Recipe>; 256],
}

// Every tree has a root gate and three subtrees. Replacing a subtree
// with its cheapest equivalent truth signature cannot increase tree cost.
// Thus the first discovered signature has minimum tree cost within this grammar.
fn discover(gates: &[LocalAnfGate], budget: usize) -> Closure {
    let mut recipes = [None; 256];
    for (i, signature) in INPUTS.into_iter().enumerate() {
        recipes[usize::from(signature)] = Some(Recipe::Input(i));
    }
    let mut levels = vec![INPUTS.to_vec()];
    for cost in 1..=budget {
        let mut next = Vec::new();
        for left in 0..cost {
            for middle in 0..(cost - left) {
                let right = cost - 1 - left - middle;
                for &a in &levels[left] {
                    for &b in &levels[middle] {
                        for &c in &levels[right] {
                            let children = [a, b, c];
                            for (kind, gate) in gates.iter().enumerate() {
                                let mut scratch = [0; 8];
                                let signature = gate.evaluate_lanes(
                                    &[u64::from(a), u64::from(b), u64::from(c)],
                                    &mut scratch,
                                ) as u8;
                                let slot = &mut recipes[usize::from(signature)];
                                if slot.is_none() {
                                    *slot = Some(Recipe::Gate { kind, children });
                                    next.push(signature);
                                }
                            }
                        }
                    }
                }
            }
        }
        levels.push(next);
    }
    Closure { levels, recipes }
}

// Scalar replay uses native evaluate, independently of the bit-sliced search.
fn replay(
    closure: &Closure,
    gates: &[LocalAnfGate],
    signature: u8,
    input: &[bool; 3],
) -> (bool, usize) {
    match closure.recipes[usize::from(signature)].expect("discovered recipe") {
        Recipe::Input(i) => (input[i], 0),
        Recipe::Gate { kind, children } => {
            let [a, b, c] = children.map(|child| replay(closure, gates, child, input));
            (gates[kind].evaluate(&[a.0, b.0, c.0]), 1 + a.1 + b.1 + c.1)
        }
    }
}

fn verify(closure: &Closure, gates: &[LocalAnfGate]) -> Result<(), Box<dyn Error>> {
    for (cost, level) in closure.levels.iter().enumerate() {
        for &signature in level {
            for address in 0..8 {
                let (value, actual_cost) = replay(closure, gates, signature, &bits(address));
                if value != (signature & (1 << address) != 0) || actual_cost != cost {
                    return Err("composition witness replay mismatch".into());
                }
            }
        }
    }
    Ok(())
}

fn full_support(signature: u8) -> bool {
    (0..3).all(|bit| (0..8).any(|i| (signature >> i) & 1 != (signature >> (i ^ (1 << bit))) & 1))
}

// Restrict lessons to zero-preserving functions depending on all three inputs.
// All retained gates preserve zero; unreachable non-zero-preserving functions
// are reported separately rather than being mistaken for a cost-limited gap.
fn select_gap(closure: &Closure) -> Option<(u8, u32)> {
    let mut best = None;
    for signature in 0u8..=u8::MAX {
        if signature & 1 != 0 || !full_support(signature) {
            continue;
        }
        if closure.recipes[usize::from(signature)].is_some() {
            continue;
        }
        let distance = closure
            .levels
            .iter()
            .flatten()
            .map(|&known| (known ^ signature).count_ones())
            .min()
            .expect("input signatures");
        if best.is_none_or(|(_, old_distance)| distance > old_distance) {
            best = Some((signature, distance));
        }
    }
    best
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = env::args().skip(1).collect();
    if args.len() != 4 {
        return Err("usage: OVERWRITE_GATE TOGGLE_GATE SET_GATE CLEAR_GATE".into());
    }
    let gates = args
        .iter()
        .enumerate()
        .map(|(kind, path)| read_gate(path, kind))
        .collect::<Result<Vec<_>, _>>()?;
    let closure = discover(&gates, MAX_COST);
    verify(&closure, &gates)?;
    for (cost, level) in closure.levels.iter().enumerate() {
        println!(
            "SML_TUTOR_COMPOSITION cost={cost} new_signatures={}",
            level.len()
        );
    }
    let reachable: usize = closure.levels.iter().map(Vec::len).sum();
    println!(
        "SML_TUTOR_CLOSURE reachable={reachable} universe=256 zero_preserving_universe=128 tree_budget={MAX_COST}"
    );
    match select_gap(&closure) {
        Some((signature, distance)) => println!(
            "SML_TUTOR_COMPOSITION_NEXT signature={signature:02x} residual={distance} addresses=8"
        ),
        None => println!("SML_TUTOR_COMPOSITION_NEXT=none"),
    }
    println!("SML_TUTOR_COMPOSITION_REPLAY=PASS");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Vec<LocalAnfGate> {
        MARKERS
            .iter()
            .zip([98, 66, 194, 130])
            .enumerate()
            .map(|(kind, (marker, word))| {
                restore(&format!("{marker}\n0,1,2\n{word}\n"), kind).unwrap()
            })
            .collect()
    }

    #[test]
    fn one_gate_matches_independent_scalar_enumeration() {
        let gates = fixture();
        let closure = discover(&gates, 1);
        let mut expected = [false; 256];
        for signature in INPUTS {
            expected[usize::from(signature)] = true;
        }
        for kind in 0..4 {
            for a in 0..3 {
                for b in 0..3 {
                    for c in 0..3 {
                        let mut signature = 0u8;
                        for address in 0..8 {
                            let input = bits(address);
                            if oracle(kind, [input[a], input[b], input[c]]) {
                                signature |= 1 << address;
                            }
                        }
                        expected[usize::from(signature)] = true;
                    }
                }
            }
        }
        assert_eq!(closure.recipes.map(|r| r.is_some()), expected);
        verify(&closure, &gates).unwrap();
    }

    #[test]
    fn bounded_closure_replays_and_preserves_zero() {
        let gates = fixture();
        let closure = discover(&gates, MAX_COST);
        verify(&closure, &gates).unwrap();
        assert!(!closure.levels[1].is_empty());
        assert!(!closure.levels[2].is_empty());
        assert!(closure.levels.iter().flatten().all(|s| s & 1 == 0));
        let smaller = discover(&gates, 1);
        for (i, recipe) in smaller.recipes.iter().enumerate() {
            if recipe.is_some() {
                assert_eq!(*recipe, closure.recipes[i]);
            }
        }
        assert_eq!(closure.levels, discover(&gates, MAX_COST).levels);
        let initial = discover(&gates, 0);
        let (candidate, distance) = select_gap(&initial).unwrap();
        assert!(full_support(candidate));
        assert_eq!(candidate & 1, 0);
        assert!(distance > 0);
        if let Some((candidate, distance)) = select_gap(&closure) {
            assert!(closure.recipes[usize::from(candidate)].is_none());
            assert!(distance > 0);
        }
    }

    #[test]
    fn rejects_corrupt_retained_gate_and_corrupt_witness() {
        assert!(restore("SML_TUTOR_SET_V1\n0,1,2\n2\n", 2).is_err());
        assert!(restore("SML_TUTOR_CLEAR_V1\n0,1,2\n130\n", 2).is_err());
        let gates = fixture();
        let mut closure = discover(&gates, 1);
        closure.recipes[usize::from(INPUTS[0])] = Some(Recipe::Input(1));
        assert!(verify(&closure, &gates).is_err());
    }
}
