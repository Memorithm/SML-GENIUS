//! BAF-3b: hardened, multi-seed, matched-write routing comparison.
//!
//! Run with `cargo run -p sml-core --release --example baf_hardened_compare`.
//! Research-only. Output is CSV of Boolean next-state error bits; no timing,
//! RSS, speed, quality or energy claims. The full `hybrid_logic_engine` is NOT
//! an arm here (it executes learned HardLogicNetwork XOR payloads, which would
//! break write matching); `hamming_key_router` reuses only its routing contract.

use sml_core::{
    boolean_associative_focus::{
        AnfActivation, AssociativeFocusConfig, AssociativeFocusPage, BooleanAssociativeFocus,
    },
    boolean_associative_joint::{fit_joint_focus, JointFocusPolicy},
    boolean_associative_train::{FocusFitPolicy, FocusTransition},
    PageId,
};

const N_PAGES: usize = 4;
const HAMMING_KEYS: u64 = 16;
const HAMMING_MAX_ROUNDS: usize = 8;
const SEEDS: u64 = 8;
const ARMS: [&str; 5] = [
    "inactive",
    "unconditional",
    "anf_degree1",
    "anf_degree2",
    "hamming_key_router",
];
const SPLITS: [&str; 4] = ["train", "validation", "test", "ood"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Family {
    /// Positive control: each writer bit is an event-bit AND state-bit.
    Conj2,
    /// XOR of one event and one state bit (ANF degree 1).
    Xor2,
    /// Parity of three bits (ANF degree 1, but non-monotone).
    Parity3,
    /// Hard negative: three-way conjunction, not representable at degree 2.
    And3,
    /// Hard negative: target bits are seeded random labels (no structure).
    Random,
}

const FAMILIES: [Family; 5] = [
    Family::Conj2,
    Family::Xor2,
    Family::Parity3,
    Family::And3,
    Family::Random,
];

impl Family {
    fn name(self) -> &'static str {
        match self {
            Family::Conj2 => "conj2",
            Family::Xor2 => "xor2",
            Family::Parity3 => "parity3",
            Family::And3 => "and3",
            Family::Random => "random",
        }
    }
}

fn cfg() -> AssociativeFocusConfig {
    AssociativeFocusConfig {
        event_bits: 3,
        state_bits: 4,
        max_selected_pages: 2,
    }
}

/// Identical atlas for every arm: two writers (bit 2, bit 3), two inert pages.
fn pages() -> Vec<AssociativeFocusPage> {
    [(10, 4), (20, 8), (30, 0), (40, 0)]
        .into_iter()
        .map(|(id, mask)| AssociativeFocusPage {
            id: PageId(id),
            priority: 100 - id,
            activation: AnfActivation {
                bias: false,
                terms: vec![],
            },
            write_mask: mask,
            write_value: mask,
        })
        .collect()
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        // splitmix64
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

/// Seeded hidden target. Gate inputs: event bits 0..3 and state bits 0..2
/// (state bits 2,3 are the write targets and never used as gate inputs).
struct Target {
    family: Family,
    e: [u64; 2],
    s: [u64; 2],
    extra: [u64; 2],
    table: [u64; 128],
}

impl Target {
    fn new(family: Family, seed: u64) -> Self {
        let mut rng = Rng(seed.wrapping_mul(1_000_003) ^ family as u64);
        let mut table = [0u64; 128];
        for slot in &mut table {
            *slot = rng.below(4);
        }
        Target {
            family,
            e: [rng.below(3), rng.below(3)],
            s: [rng.below(2), rng.below(2)],
            extra: [rng.below(3), rng.below(3)],
            table,
        }
    }
    fn gate(&self, k: usize, event: u64, state: u64) -> bool {
        let eb = (event >> self.e[k]) & 1 == 1;
        let sb = (state >> self.s[k]) & 1 == 1;
        // Third input: a different event bit than e[k] when possible.
        let x = (self.e[k] + 1 + self.extra[k] % 2) % 3;
        let xb = (event >> x) & 1 == 1;
        match self.family {
            Family::Conj2 => eb && sb,
            Family::Xor2 => eb ^ sb,
            Family::Parity3 => eb ^ sb ^ xb,
            Family::And3 => eb && sb && xb,
            Family::Random => (self.table[((event << 4) | state) as usize] >> k) & 1 == 1,
        }
    }
    fn next_state(&self, event: u64, state: u64) -> u64 {
        state
            | if self.gate(0, event, state) { 4 } else { 0 }
            | if self.gate(1, event, state) { 8 } else { 0 }
    }
}

/// Splits over all 128 (event,state) inputs. OOD = event bit 2 AND state
/// bit 2 both set (16 inputs, never seen elsewhere). The remaining 112 are
/// seed-shuffled into TRAIN 56 / VALIDATION 24 / TEST 32.
fn panels(target: &Target, seed: u64) -> [Vec<FocusTransition>; 4] {
    let mut inputs: Vec<(u64, u64)> = (0..8u64)
        .flat_map(|e| (0..16u64).map(move |s| (e, s)))
        .collect();
    let is_ood = |&(e, s): &(u64, u64)| e & 4 != 0 && s & 4 != 0;
    let ood: Vec<_> = inputs.iter().copied().filter(is_ood).collect();
    inputs.retain(|p| !is_ood(p));
    let mut rng = Rng(seed ^ 0xBAF3_B000);
    for i in (1..inputs.len()).rev() {
        let j = rng.below(i as u64 + 1) as usize;
        inputs.swap(i, j);
    }
    let rows = |slice: &[(u64, u64)], offset: u64| -> Vec<FocusTransition> {
        slice
            .iter()
            .enumerate()
            .map(|(i, &(event, state))| FocusTransition {
                case_id: offset + i as u64,
                event,
                state,
                expected_next_state: target.next_state(event, state),
            })
            .collect()
    };
    [
        rows(&inputs[..56], 0),
        rows(&inputs[56..80], 1000),
        rows(&inputs[80..], 2000),
        rows(&ood, 3000),
    ]
}

fn errors<F: Fn(u64, u64) -> u64>(rows: &[FocusTransition], run: F) -> u64 {
    rows.iter()
        .map(|r| (run(r.event, r.state) ^ r.expected_next_state).count_ones() as u64)
        .sum()
}

fn baf(model: &BooleanAssociativeFocus) -> impl Fn(u64, u64) -> u64 + '_ {
    move |e, s| model.step(e, s).expect("input within width").next_state
}

fn route_hamming(
    pages: &[AssociativeFocusPage],
    keys: &[u64; N_PAGES],
    event: u64,
    state: u64,
) -> u64 {
    let signature = (event ^ state) & 0b1111;
    let mut ranking: Vec<_> = pages
        .iter()
        .enumerate()
        .map(|(i, p)| ((signature ^ keys[i]).count_ones(), p.id.0, i))
        .collect();
    ranking.sort_unstable_by_key(|&(d, id, _)| (d, id));
    let mut selected: Vec<_> = ranking
        .iter()
        .take(cfg().max_selected_pages)
        .map(|&(_, _, i)| i)
        .collect();
    selected.sort_unstable_by_key(|&i| (u32::MAX - pages[i].priority, pages[i].id.0));
    let (mut next, mut claimed) = (state, 0u64);
    for i in selected {
        let eff = pages[i].write_mask & !claimed;
        next = (next & !eff) | (pages[i].write_value & eff);
        claimed |= eff;
    }
    next
}

/// Train-only coordinate search over keys, one validation veto (as BAF-3).
fn fit_hamming(
    pages: &[AssociativeFocusPage],
    train: &[FocusTransition],
    validation: &[FocusTransition],
) -> ([u64; N_PAGES], bool) {
    let initial = [0u64, 1, 2, 3];
    let mut keys = initial;
    let mut incumbent = errors(train, |e, s| route_hamming(pages, &keys, e, s));
    let baseline = incumbent;
    for _ in 0..HAMMING_MAX_ROUNDS {
        let mut winner = None;
        let mut best = incumbent;
        for i in 0..N_PAGES {
            for key in 0..HAMMING_KEYS {
                let mut trial = keys;
                trial[i] = key;
                let loss = errors(train, |e, s| route_hamming(pages, &trial, e, s));
                if loss < best {
                    best = loss;
                    winner = Some(trial);
                }
            }
        }
        let Some(w) = winner else { break };
        keys = w;
        incumbent = best;
    }
    let base_val = errors(validation, |e, s| route_hamming(pages, &initial, e, s));
    let cand_val = errors(validation, |e, s| route_hamming(pages, &keys, e, s));
    let admitted = incumbent < baseline && cand_val <= base_val;
    (if admitted { keys } else { initial }, admitted)
}

/// Fail-closed: a fitter error falls back to the inactive atlas (not admitted).
fn fit_anf(
    pages: &[AssociativeFocusPage],
    train: &[FocusTransition],
    validation: &[FocusTransition],
    degree: usize,
) -> (BooleanAssociativeFocus, bool) {
    let policy = JointFocusPolicy {
        local: FocusFitPolicy {
            max_degree: degree,
            ..FocusFitPolicy::default()
        },
        ..JointFocusPolicy::default()
    };
    match fit_joint_focus(
        cfg(),
        pages,
        &[PageId(10), PageId(20)],
        train,
        validation,
        policy,
    ) {
        Ok(r) => (r.model, r.report.admitted),
        Err(_) => (
            BooleanAssociativeFocus::new(cfg(), pages.to_vec()).expect("valid atlas"),
            false,
        ),
    }
}

/// Returns errors[arm][split] and admitted flags per arm.
fn run_one(family: Family, seed: u64) -> ([[u64; 4]; 5], [bool; 5], [usize; 4]) {
    let target = Target::new(family, seed);
    let data = panels(&target, seed);
    let atlas = pages();
    let inactive = BooleanAssociativeFocus::new(cfg(), atlas.clone()).expect("valid");
    let mut up = atlas.clone();
    for p in &mut up {
        p.activation.bias = true;
    }
    let uncond = BooleanAssociativeFocus::new(cfg(), up).expect("valid");
    let (d1, a1) = fit_anf(&atlas, &data[0], &data[1], 1);
    let (d2, a2) = fit_anf(&atlas, &data[0], &data[1], 2);
    let (keys, ah) = fit_hamming(&atlas, &data[0], &data[1]);
    let mut out = [[0u64; 4]; 5];
    for (k, rows) in data.iter().enumerate() {
        out[0][k] = errors(rows, baf(&inactive));
        out[1][k] = errors(rows, baf(&uncond));
        out[2][k] = errors(rows, baf(&d1));
        out[3][k] = errors(rows, baf(&d2));
        out[4][k] = errors(rows, |e, s| route_hamming(&atlas, &keys, e, s));
    }
    let sizes = [data[0].len(), data[1].len(), data[2].len(), data[3].len()];
    (out, [false, false, a1, a2, ah], sizes)
}

fn main() {
    println!("family,seed,model,split,cases,error_bits,admitted");
    let mut summary = Vec::new();
    for family in FAMILIES {
        let mut all = vec![[[0u64; 4]; 5]; 0];
        for seed in 0..SEEDS {
            let (errs, adm, sizes) = run_one(family, seed);
            for (a, arm) in ARMS.iter().enumerate() {
                for (k, split) in SPLITS.iter().enumerate() {
                    println!(
                        "{},{seed},{arm},{split},{},{},{}",
                        family.name(),
                        sizes[k],
                        errs[a][k],
                        adm[a]
                    );
                }
            }
            all.push(errs);
        }
        summary.push((family, all));
    }
    println!();
    println!("# summary over {SEEDS} seeds: mean[min..max] error bits");
    println!("family,model,train,validation,test,ood");
    for (family, all) in summary {
        for (a, arm) in ARMS.iter().enumerate() {
            let cells: Vec<String> = (0..4)
                .map(|k| {
                    let v: Vec<u64> = all.iter().map(|r| r[a][k]).collect();
                    let mean = v.iter().sum::<u64>() as f64 / v.len() as f64;
                    format!(
                        "{mean:.2}[{}..{}]",
                        v.iter().min().unwrap(),
                        v.iter().max().unwrap()
                    )
                })
                .collect();
            println!("{},{arm},{}", family.name(), cells.join(","));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_are_disjoint_and_cover_all_inputs() {
        for family in FAMILIES {
            for seed in 0..SEEDS {
                let data = panels(&Target::new(family, seed), seed);
                let mut seen = std::collections::HashSet::new();
                let mut ids = std::collections::HashSet::new();
                for panel in &data {
                    for r in panel {
                        assert!(seen.insert((r.event, r.state)), "pair leaked across splits");
                        assert!(ids.insert(r.case_id), "case id reused");
                    }
                }
                assert_eq!(seen.len(), 128);
                assert!(data[3].iter().all(|r| r.event & 4 != 0 && r.state & 4 != 0));
                assert!(data[..3]
                    .iter()
                    .flatten()
                    .all(|r| !(r.event & 4 != 0 && r.state & 4 != 0)));
            }
        }
    }

    #[test]
    fn seeds_change_splits_deterministically() {
        let t = Target::new(Family::Xor2, 1);
        let a = panels(&t, 1);
        let b = panels(&t, 1);
        let c = panels(&t, 2);
        assert_eq!(a[0], b[0]);
        assert_ne!(a[0], c[0]);
    }

    #[test]
    fn evaluation_splits_never_change_fitted_models() {
        // Fail-closed isolation: corrupting TEST/OOD labels must not alter fit.
        let t = Target::new(Family::Conj2, 3);
        let data = panels(&t, 3);
        let (m1, _) = fit_anf(&pages(), &data[0], &data[1], 2);
        let (k1, _) = fit_hamming(&pages(), &data[0], &data[1]);
        let mut poisoned = data.clone();
        for r in poisoned[2..].iter_mut().flatten() {
            r.expected_next_state ^= 0b1100;
        }
        let (m2, _) = fit_anf(&pages(), &poisoned[0], &poisoned[1], 2);
        let (k2, _) = fit_hamming(&pages(), &poisoned[0], &poisoned[1]);
        assert_eq!(k1, k2);
        for r in &data[2] {
            assert_eq!(baf(&m1)(r.event, r.state), baf(&m2)(r.event, r.state));
        }
    }

    #[test]
    fn positive_control_degree2_conjunction_is_learnable() {
        let (errs, adm, _) = run_one(Family::Conj2, 0);
        assert!(adm[3]);
        assert_eq!(errs[3][0], 0);
    }

    /// BAF-XOR2-DEGREE1-NEGATIVE: xor2 IS expressible at ANF degree 1 under
    /// the set-only write semantics (oracle activation `e ^ s` is exact on
    /// every split), yet greedy single-flip descent never reaches it: a lone
    /// XOR atom does not strictly improve TRAIN (plateau). Locks the negative
    /// result as a *search* failure, not a hypothesis-class failure.
    #[test]
    fn xor2_degree1_representable_but_greedy_misses() {
        use sml_core::boolean_associative_focus::AnfMonomial as M;
        for seed in 0..SEEDS {
            let target = Target::new(Family::Xor2, seed);
            let data = panels(&target, seed);
            let mut oracle = pages();
            for k in 0..2 {
                oracle[k].activation.terms = vec![
                    M {
                        event_mask: 1 << target.e[k],
                        state_mask: 0,
                    },
                    M {
                        event_mask: 0,
                        state_mask: 1 << target.s[k],
                    },
                ];
            }
            let model = BooleanAssociativeFocus::new(cfg(), oracle).expect("valid");
            for rows in &data {
                assert_eq!(
                    errors(rows, baf(&model)),
                    0,
                    "seed {seed}: oracle must be exact"
                );
            }
            let (fitted, _) = fit_anf(&pages(), &data[0], &data[1], 1);
            assert!(
                errors(&data[0], baf(&fitted)) > 0,
                "seed {seed}: greedy now solves xor2; update docs/BAF-XOR2-DEGREE1-NEGATIVE.md"
            );
        }
    }
}
