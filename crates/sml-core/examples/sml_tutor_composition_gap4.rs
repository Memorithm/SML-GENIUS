//! Bounded compositional gap discovery over four restored recurrent gates.
//! Exhaustive expression trees, four external inputs, no supplied target catalog,
//! no constants, no DAG sharing, no fitting, and no checkpoint publication.
use sml_core::anf_local::LocalAnfGate;
use std::{env, error::Error, fs::File, io::Read};

const INPUTS: [u16; 4] = [0xaaaa, 0xcccc, 0xf0f0, 0xff00];
const UNIVERSE: usize = 65536;
const MAX_COST: usize = 3;
const MARKERS: [&str; 4] = [
    "SML_TUTOR_RECURRENT_V1",
    "SML_TUTOR_TOGGLE_V1",
    "SML_TUTOR_SET_V1",
    "SML_TUTOR_CLEAR_V1",
];

fn gate_bits(address: usize) -> [bool; 3] {
    [address & 1 != 0, address & 2 != 0, address & 4 != 0]
}

fn bits(address: usize) -> [bool; 4] {
    [
        address & 1 != 0,
        address & 2 != 0,
        address & 4 != 0,
        address & 8 != 0,
    ]
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
    if (0..8).any(|i| gate.evaluate(&gate_bits(i)) != oracle(kind, gate_bits(i))) {
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
    Gate { kind: usize, children: [u16; 3] },
}

struct Closure {
    levels: Vec<Vec<u16>>,
    recipes: Vec<Option<Recipe>>,
}

// Every tree has a root gate and three subtrees. Replacing a subtree
// with its cheapest equivalent truth signature cannot increase tree cost.
// Thus the first discovered signature has minimum tree cost within this grammar.
fn discover(gates: &[LocalAnfGate], budget: usize) -> Closure {
    let mut recipes = vec![None; UNIVERSE];
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
                                ) as u16;
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
    signature: u16,
    input: &[bool; 4],
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
            for address in 0..16 {
                let (value, actual_cost) = replay(closure, gates, signature, &bits(address));
                if value != (signature & (1 << address) != 0) || actual_cost != cost {
                    return Err("composition witness replay mismatch".into());
                }
            }
        }
    }
    Ok(())
}

fn full_support(signature: u16) -> bool {
    (0..4).all(|bit| {
        (0..16).any(|i| (signature >> i) & 1 != (signature >> (i ^ (1 << bit))) & 1)
    })
}

// Exact multi-source shortest-path distance on the 15-dimensional Boolean
// hypercube of zero-preserving truth tables. Every edge flips one output bit.
fn distances(closure: &Closure) -> Vec<u8> {
    let mut distance = vec![u8::MAX; UNIVERSE];
    let mut queue = Vec::with_capacity(UNIVERSE / 2);
    for &signature in closure.levels.iter().flatten() {
        distance[usize::from(signature)] = 0;
        queue.push(signature);
    }
    let mut cursor = 0;
    while cursor < queue.len() {
        let signature = queue[cursor];
        cursor += 1;
        let next_distance = distance[usize::from(signature)] + 1;
        for bit in 1..16 {
            let neighbor = signature ^ (1 << bit);
            let entry = &mut distance[usize::from(neighbor)];
            if *entry == u8::MAX {
                *entry = next_distance;
                queue.push(neighbor);
            }
        }
    }
    distance
}

// Rank uncovered full-support functions by exact distance to the bounded
// closure. A gap is cost-limited, not a proof of logical unlearnability.
fn select_gap(closure: &Closure) -> Option<(u16, u8)> {
    let distance = distances(closure);
    let mut best = None;
    for signature in (0u16..=u16::MAX).step_by(2) {
        if !full_support(signature) || closure.recipes[usize::from(signature)].is_some() {
            continue;
        }
        let residual = distance[usize::from(signature)];
        if best.is_none_or(|(_, old_distance)| residual > old_distance) {
            best = Some((signature, residual));
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
            "SML_TUTOR_COMPOSITION4 cost={cost} new_signatures={}",
            level.len()
        );
    }
    let reachable: usize = closure.levels.iter().map(Vec::len).sum();
    println!(
        "SML_TUTOR_CLOSURE4 reachable={reachable} universe=65536 zero_preserving_universe=32768 tree_budget={MAX_COST}"
    );
    match select_gap(&closure) {
        Some((signature, distance)) => println!(
            "SML_TUTOR_COMPOSITION4_NEXT signature={signature:04x} residual={distance} addresses=16"
        ),
        None => println!("SML_TUTOR_COMPOSITION4_NEXT=none"),
    }
    println!("SML_TUTOR_COMPOSITION4_REPLAY=PASS");
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
        let mut expected = vec![false; UNIVERSE];
        for signature in INPUTS {
            expected[usize::from(signature)] = true;
        }
        for kind in 0..4 {
            for a in 0..4 {
                for b in 0..4 {
                    for c in 0..4 {
                        let mut signature = 0u16;
                        for address in 0..16 {
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
        assert_eq!(
            closure
                .recipes
                .iter()
                .map(Option::is_some)
                .collect::<Vec<_>>(),
            expected
        );
        verify(&closure, &gates).unwrap();
    }

    #[test]
    fn bounded_closure_replays_and_preserves_zero() {
        let gates = fixture();
        let closure = discover(&gates, MAX_COST);
        verify(&closure, &gates).unwrap();
        assert_eq!(
            closure.levels.iter().map(Vec::len).collect::<Vec<_>>(),
            vec![4, 91, 1840, 11681]
        );
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
        assert_eq!(select_gap(&closure), Some((0x099e, 3)));
        let distance = distances(&closure);
        assert_eq!(distance[0x099f], u8::MAX);
        for signature in [0u16, 0x099e, 0xfffe, 0xaaaa, 0x8420] {
            let brute = closure
                .levels
                .iter()
                .flatten()
                .map(|&known| (known ^ signature).count_ones())
                .min()
                .unwrap();
            assert_eq!(u32::from(distance[usize::from(signature)]), brute);
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
