//! BAF-3b: hardened, multi-seed, matched-write routing comparison.
//!
//! Run with `cargo run -p sml-core --release --example baf_hardened_compare`.
//! Research-only. Output is CSV of Boolean next-state error bits plus a
//! per-fit stats section (candidate evaluations, wall-time) for scaling
//! bookkeeping only; no speed, quality or energy claims.
//!
//! BAF-SCALE flags (defaults reproduce BAF-3b exactly: width 7, 8 seeds,
//! degrees 1..2, five families, default joint budgets):
//!   --width N            input bits (event = N-4, state = 4); 7..=16
//!   --seeds N            seeds 0..N (prereg: 8 CI, 64 RemoteOps)
//!   --max-degree D       add anf_degree3 arm when D = 3
//!   --parity4            add the parity4 family (needs --max-quad-flips > 0
//!                        for the quadruple escape; otherwise fails closed /
//!                        stays partial and that is reported)
//!   --max-pair-flips N / --max-triple-flips N / --max-quad-flips N
//!   --max-evals N        max_candidate_evaluations per fit
//!   --max-basis N        max_basis_terms per fit
//!   --max-samples N      max train+validation samples per fit (default 8192;
//!                        width 16 needs >= 40960, else every fit errors with
//!                        TooManySamples = status 2)
//!   --search S           kflip (default) | beam | gf2 (BAF-SEARCH-COMPARE)
//!   --beam-width N       beam width for --search beam (default 4)
//!   --json PATH          write a JSON summary (means, admits, fit stats)
//! Over-budget fits FAIL CLOSED (inactive atlas, not admitted, status=budget). The full `hybrid_logic_engine` is NOT
//! an arm here (it executes learned HardLogicNetwork XOR payloads, which would
//! break write matching); `hamming_key_router` reuses only its routing contract.

use std::{sync::OnceLock, time::Instant};

use sml_core::{
    boolean_associative_focus::{
        AnfActivation, AssociativeFocusConfig, AssociativeFocusPage, BooleanAssociativeFocus,
    },
    boolean_associative_joint::{fit_joint_focus, JointFocusError, JointFocusPolicy},
    boolean_associative_search::{fit_joint_focus_beam, fit_joint_focus_gf2},
    boolean_associative_train::{FocusFitPolicy, FocusTransition},
    PageId,
};

const N_PAGES: usize = 4;
const HAMMING_KEYS: u64 = 16;
const HAMMING_MAX_ROUNDS: usize = 8;
const SEEDS: u64 = 8;
const ARMS: [&str; 6] = [
    "inactive",
    "unconditional",
    "anf_degree1",
    "anf_degree2",
    "hamming_key_router",
    "anf_degree3",
];

/// Run-wide harness options; tests use the BAF-3b defaults.
#[derive(Debug, Clone)]
struct Opts {
    width: usize,
    seeds: u64,
    max_degree: usize,
    parity4: bool,
    policy: JointFocusPolicy,
    json: Option<String>,
    search: SearchKind,
    beam_width: usize,
}

/// BAF-SEARCH-COMPARE: TRAIN-side search strategy for the anf_degree* arms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SearchKind {
    /// `fit_joint_focus` (single flips + k-flip escapes), the default.
    KFlip,
    /// `fit_joint_focus_beam` (single flips, beam of `--beam-width`).
    Beam,
    /// `fit_joint_focus_gf2` (exact GF(2) solve on the feature bank).
    Gf2,
}

impl SearchKind {
    fn name(self) -> &'static str {
        match self {
            SearchKind::KFlip => "kflip",
            SearchKind::Beam => "beam",
            SearchKind::Gf2 => "gf2",
        }
    }
}

impl Default for Opts {
    fn default() -> Self {
        Opts {
            width: 7,
            seeds: SEEDS,
            max_degree: 2,
            parity4: false,
            policy: JointFocusPolicy::default(),
            json: None,
            search: SearchKind::KFlip,
            beam_width: 4,
        }
    }
}

static OPTS: OnceLock<Opts> = OnceLock::new();

fn opts() -> &'static Opts {
    OPTS.get_or_init(Opts::default)
}

fn parse_opts() -> Result<Opts, String> {
    let mut o = Opts::default();
    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        if flag == "--parity4" {
            o.parity4 = true;
            continue;
        }
        let value = args.next().ok_or(format!("{flag} needs a value"))?;
        let num = || value.parse::<u64>().map_err(|e| format!("{flag}: {e}"));
        match flag.as_str() {
            "--width" => o.width = num()? as usize,
            "--seeds" => o.seeds = num()?,
            "--max-degree" => o.max_degree = num()? as usize,
            "--max-pair-flips" => o.policy.max_pair_flips = num()?,
            "--max-triple-flips" => o.policy.max_triple_flips = num()?,
            "--max-quad-flips" => o.policy.max_quad_flips = num()?,
            "--max-evals" => o.policy.max_candidate_evaluations = num()?,
            "--max-basis" => o.policy.local.max_basis_terms = num()? as usize,
            "--max-samples" => o.policy.local.max_samples = num()? as usize,
            "--json" => o.json = Some(value),
            "--search" => {
                o.search = match value.as_str() {
                    "kflip" => SearchKind::KFlip,
                    "beam" => SearchKind::Beam,
                    "gf2" => SearchKind::Gf2,
                    _ => return Err(format!("--search: unknown strategy {value}")),
                }
            }
            "--beam-width" => o.beam_width = num()? as usize,
            _ => return Err(format!("unknown flag {flag}")),
        }
    }
    if !(7..=16).contains(&o.width) {
        return Err("--width must be in 7..=16".into());
    }
    if !(2..=3).contains(&o.max_degree) {
        return Err("--max-degree must be 2 or 3".into());
    }
    if o.seeds == 0 {
        return Err("--seeds must be > 0".into());
    }
    Ok(o)
}

fn event_bits() -> usize {
    opts().width - 4
}
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
    /// BAF-SCALE (opt-in): parity of four bits (ANF degree 1, needs four
    /// simultaneous atoms -> quadruple escape).
    Parity4,
}

const FAMILIES: [Family; 5] = [
    Family::Conj2,
    Family::Xor2,
    Family::Parity3,
    Family::And3,
    Family::Random,
];

fn families() -> Vec<Family> {
    let mut f = FAMILIES.to_vec();
    if opts().parity4 {
        f.push(Family::Parity4);
    }
    f
}

/// Arm indices to report: BAF-3b five, plus anf_degree3 when requested.
fn arms() -> Vec<usize> {
    let mut a = vec![0, 1, 2, 3, 4];
    if opts().max_degree >= 3 {
        a.push(5);
    }
    a
}

impl Family {
    fn name(self) -> &'static str {
        match self {
            Family::Conj2 => "conj2",
            Family::Xor2 => "xor2",
            Family::Parity3 => "parity3",
            Family::And3 => "and3",
            Family::Random => "random",
            Family::Parity4 => "parity4",
        }
    }
}

fn cfg() -> AssociativeFocusConfig {
    AssociativeFocusConfig {
        event_bits: event_bits(),
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
/// At width > 7 the extra event bits are distractors (same RNG draws as
/// BAF-3b, so width-7 targets are bit-identical).
struct Target {
    family: Family,
    e: [u64; 2],
    s: [u64; 2],
    extra: [u64; 2],
    table: Vec<u64>,
}

impl Target {
    fn new(family: Family, seed: u64) -> Self {
        let mut rng = Rng(seed.wrapping_mul(1_000_003) ^ family as u64);
        let mut table = vec![0u64; 1usize << opts().width];
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
        // Fourth input (parity4 only): the remaining event bit of 0..3.
        let yb = (event >> (3 - self.e[k] - x)) & 1 == 1;
        match self.family {
            Family::Conj2 => eb && sb,
            Family::Xor2 => eb ^ sb,
            Family::Parity3 => eb ^ sb ^ xb,
            Family::And3 => eb && sb && xb,
            Family::Random => (self.table[((event << 4) | state) as usize] >> k) & 1 == 1,
            Family::Parity4 => eb ^ sb ^ xb ^ yb,
        }
    }
    fn next_state(&self, event: u64, state: u64) -> u64 {
        state
            | if self.gate(0, event, state) { 4 } else { 0 }
            | if self.gate(1, event, state) { 8 } else { 0 }
    }
}

/// Splits over all 2^width (event,state) inputs. OOD = event bit 2 AND state
/// bit 2 both set (1/4 of inputs, never seen elsewhere). The rest are
/// seed-shuffled into TRAIN 7/12 / VALIDATION 1/4 / TEST rest (width 7:
/// OOD 32, then 56 / 24 / 16 of 96, exactly BAF-3b).
fn panels(target: &Target, seed: u64) -> [Vec<FocusTransition>; 4] {
    let mut inputs: Vec<(u64, u64)> = (0..(1u64 << event_bits()))
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
    let n_train = inputs.len() * 7 / 12;
    let n_val = inputs.len() / 4;
    let stride = 1u64 << 20;
    [
        rows(&inputs[..n_train], 0),
        rows(&inputs[n_train..n_train + n_val], stride),
        rows(&inputs[n_train + n_val..], 2 * stride),
        rows(&ood, 3 * stride),
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

/// Per-fit scaling bookkeeping (not a speed claim).
#[derive(Debug, Clone, Copy, Default)]
struct FitStats {
    evaluations: u64,
    micros: u128,
    /// 0 ok, 1 fail-closed CandidateBudget, 2 other fitter error.
    status: u8,
}

/// Fail-closed: a fitter error falls back to the inactive atlas (not admitted).
#[allow(dead_code)] // used by tests
fn fit_anf(
    pages: &[AssociativeFocusPage],
    train: &[FocusTransition],
    validation: &[FocusTransition],
    degree: usize,
) -> (BooleanAssociativeFocus, bool) {
    let p = opts().policy;
    fit_anf_with(
        pages,
        train,
        validation,
        degree,
        p.max_pair_flips,
        p.max_triple_flips,
    )
}

#[allow(dead_code)] // used by tests
fn fit_anf_with(
    pages: &[AssociativeFocusPage],
    train: &[FocusTransition],
    validation: &[FocusTransition],
    degree: usize,
    max_pair_flips: u64,
    max_triple_flips: u64,
) -> (BooleanAssociativeFocus, bool) {
    let (m, a, _) = fit_anf_stats(
        pages,
        train,
        validation,
        degree,
        JointFocusPolicy {
            max_pair_flips,
            max_triple_flips,
            ..opts().policy
        },
    );
    (m, a)
}

fn fit_anf_stats(
    pages: &[AssociativeFocusPage],
    train: &[FocusTransition],
    validation: &[FocusTransition],
    degree: usize,
    base: JointFocusPolicy,
) -> (BooleanAssociativeFocus, bool, FitStats) {
    let policy = JointFocusPolicy {
        local: FocusFitPolicy {
            max_degree: degree,
            ..base.local
        },
        ..base
    };
    let start = Instant::now();
    let editable = [PageId(10), PageId(20)];
    let fitted = match opts().search {
        SearchKind::KFlip => fit_joint_focus(cfg(), pages, &editable, train, validation, policy),
        SearchKind::Beam => fit_joint_focus_beam(
            cfg(),
            pages,
            &editable,
            train,
            validation,
            policy,
            opts().beam_width,
        ),
        SearchKind::Gf2 => fit_joint_focus_gf2(cfg(), pages, &editable, train, validation, policy),
    };
    let micros = start.elapsed().as_micros();
    match fitted {
        Ok(r) => (
            r.model,
            r.report.admitted,
            FitStats {
                evaluations: r.report.examined_train_candidates,
                micros,
                status: 0,
            },
        ),
        Err(e) => (
            {
                if e != JointFocusError::CandidateBudget {
                    eprintln!("baf_hardened_compare: fit error (status 2): {e:?}");
                }
                BooleanAssociativeFocus::new(cfg(), pages.to_vec()).expect("valid atlas")
            },
            false,
            FitStats {
                evaluations: 0,
                micros,
                status: if e == JointFocusError::CandidateBudget {
                    1
                } else {
                    2
                },
            },
        ),
    }
}

struct RunOut {
    errs: [[u64; 4]; 6],
    adm: [bool; 6],
    sizes: [usize; 4],
    /// Fit stats for anf_degree1, anf_degree2, anf_degree3 (if run).
    stats: [Option<FitStats>; 3],
}

/// Returns errors[arm][split] and admitted flags per arm (BAF-3b arms 0..5).
#[allow(dead_code)] // used by tests
fn run_one(family: Family, seed: u64) -> ([[u64; 4]; 5], [bool; 5], [usize; 4]) {
    let r = run_full(family, seed);
    let mut errs = [[0u64; 4]; 5];
    errs.copy_from_slice(&r.errs[..5]);
    let mut adm = [false; 5];
    adm.copy_from_slice(&r.adm[..5]);
    (errs, adm, r.sizes)
}

fn run_full(family: Family, seed: u64) -> RunOut {
    let target = Target::new(family, seed);
    let data = panels(&target, seed);
    let atlas = pages();
    let inactive = BooleanAssociativeFocus::new(cfg(), atlas.clone()).expect("valid");
    let mut up = atlas.clone();
    for p in &mut up {
        p.activation.bias = true;
    }
    let uncond = BooleanAssociativeFocus::new(cfg(), up).expect("valid");
    let base = JointFocusPolicy {
        // parity4 is the only family that receives the opt-in quad budget.
        max_quad_flips: if family == Family::Parity4 {
            opts().policy.max_quad_flips
        } else {
            0
        },
        ..opts().policy
    };
    let (d1, a1, s1) = fit_anf_stats(&atlas, &data[0], &data[1], 1, base);
    let (d2, a2, s2) = fit_anf_stats(&atlas, &data[0], &data[1], 2, base);
    let d3 = (opts().max_degree >= 3).then(|| fit_anf_stats(&atlas, &data[0], &data[1], 3, base));
    let (keys, ah) = fit_hamming(&atlas, &data[0], &data[1]);
    let mut out = [[0u64; 4]; 6];
    for (k, rows) in data.iter().enumerate() {
        out[0][k] = errors(rows, baf(&inactive));
        out[1][k] = errors(rows, baf(&uncond));
        out[2][k] = errors(rows, baf(&d1));
        out[3][k] = errors(rows, baf(&d2));
        out[4][k] = errors(rows, |e, s| route_hamming(&atlas, &keys, e, s));
        if let Some((m, _, _)) = &d3 {
            out[5][k] = errors(rows, baf(m));
        }
    }
    let sizes = [data[0].len(), data[1].len(), data[2].len(), data[3].len()];
    let a3 = d3.as_ref().is_some_and(|d| d.1);
    RunOut {
        errs: out,
        adm: [false, false, a1, a2, ah, a3],
        sizes,
        stats: [Some(s1), Some(s2), d3.map(|d| d.2)],
    }
}

fn main() {
    let parsed = match parse_opts() {
        Ok(o) => o,
        Err(e) => {
            eprintln!("baf_hardened_compare: {e}");
            std::process::exit(2);
        }
    };
    OPTS.set(parsed).expect("options set once");
    let o = opts();
    let seeds = o.seeds;
    println!("family,seed,model,split,cases,error_bits,admitted");
    let mut summary = Vec::new();
    let mut fit_rows = Vec::new();
    for family in families() {
        let mut all = Vec::new();
        for seed in 0..seeds {
            let r = run_full(family, seed);
            for a in arms() {
                for (k, split) in SPLITS.iter().enumerate() {
                    println!(
                        "{},{seed},{},{split},{},{},{}",
                        family.name(),
                        ARMS[a],
                        r.sizes[k],
                        r.errs[a][k],
                        r.adm[a]
                    );
                }
            }
            for (d, st) in r.stats.iter().enumerate() {
                if let Some(st) = st {
                    fit_rows.push((family, seed, d + 1, *st, r.errs[[2, 3, 5][d]][0]));
                }
            }
            all.push((r.errs, r.adm));
        }
        summary.push((family, all));
    }
    println!();
    println!("# summary over {seeds} seeds: mean[min..max] error bits");
    println!("family,model,train,validation,test,ood");
    let mut json_cells = Vec::new();
    for (family, all) in &summary {
        for a in arms() {
            let arm = ARMS[a];
            let mut means = Vec::new();
            let cells: Vec<String> = (0..4)
                .map(|k| {
                    let v: Vec<u64> = all.iter().map(|r| r.0[a][k]).collect();
                    let mean = v.iter().sum::<u64>() as f64 / v.len() as f64;
                    means.push(mean);
                    format!(
                        "{mean:.2}[{}..{}]",
                        v.iter().min().unwrap(),
                        v.iter().max().unwrap()
                    )
                })
                .collect();
            println!("{},{arm},{}", family.name(), cells.join(","));
            let admits = all.iter().filter(|r| r.1[a]).count();
            json_cells.push(format!(
                "{{\"family\":\"{}\",\"model\":\"{arm}\",\"mean_train\":{:.4},\"mean_validation\":{:.4},\"mean_test\":{:.4},\"mean_ood\":{:.4},\"admitted\":{admits}}}",
                family.name(),
                means[0],
                means[1],
                means[2],
                means[3]
            ));
        }
    }
    println!();
    if o.search != SearchKind::KFlip {
        println!(
            "# search: {} (beam_width={})",
            o.search.name(),
            o.beam_width
        );
    }
    println!("# fit stats (scaling bookkeeping only; status 0=ok 1=budget-fail-closed 2=error)");
    println!("family,seed,degree,evaluations,micros,status,train_error_bits");
    let mut json_fits = Vec::new();
    for (family, seed, degree, st, train_err) in &fit_rows {
        println!(
            "{},{seed},{degree},{},{},{},{train_err}",
            family.name(),
            st.evaluations,
            st.micros,
            st.status
        );
        json_fits.push(format!(
            "{{\"family\":\"{}\",\"seed\":{seed},\"degree\":{degree},\"evaluations\":{},\"micros\":{},\"status\":{}}}",
            family.name(),
            st.evaluations,
            st.micros,
            st.status
        ));
    }
    if let Some(path) = &o.json {
        let p = o.policy;
        let search = if o.search == SearchKind::KFlip {
            String::new()
        } else {
            format!(
                "\"search\":\"{}\",\"beam_width\":{},",
                o.search.name(),
                o.beam_width
            )
        };
        let json = format!(
            "{{{search}\"width\":{},\"seeds\":{seeds},\"max_degree\":{},\"parity4\":{},\"policy\":{{\"max_pair_flips\":{},\"max_triple_flips\":{},\"max_quad_flips\":{},\"max_candidate_evaluations\":{},\"max_basis_terms\":{}}},\"summary\":[{}],\"fits\":[{}]}}\n",
            o.width,
            o.max_degree,
            o.parity4,
            p.max_pair_flips,
            p.max_triple_flips,
            p.max_quad_flips,
            p.max_candidate_evaluations,
            p.local.max_basis_terms,
            json_cells.join(","),
            json_fits.join(",")
        );
        std::fs::write(path, json).expect("write --json summary");
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
                assert_eq!(seen.len(), 1 << opts().width);
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

    /// BAF-XOR2-DEGREE1-NEGATIVE / pair-escape: xor2 IS expressible at ANF
    /// degree 1 (oracle `e ^ s` exact on every split). Single-flip-only descent
    /// (`max_pair_flips = 0`) plateaus; the bounded same-page pair-flip escape
    /// (default policy) recovers the exact rule on all splits.
    #[test]
    fn xor2_degree1_single_flip_plateaus_pair_escape_recovers() {
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
                assert_eq!(errors(rows, baf(&model)), 0, "seed {seed}: oracle exact");
            }
            let (single, _) = fit_anf_with(&pages(), &data[0], &data[1], 1, 0, 0);
            assert!(
                errors(&data[0], baf(&single)) > 0,
                "seed {seed}: single-flip-only now solves xor2; update docs"
            );
            let (paired, admitted) = fit_anf(&pages(), &data[0], &data[1], 1);
            assert!(admitted, "seed {seed}: pair escape not admitted");
            for rows in &data {
                assert_eq!(errors(rows, baf(&paired)), 0, "seed {seed}: pair escape");
            }
        }
    }

    /// BAF-PARITY3: 3-bit parity needs three simultaneous degree-1 atoms.
    /// Pairs-only (`max_triple_flips = 0`) stays partial on some seed; the
    /// bounded same-page triple escape (default) is exact on all splits.
    #[test]
    fn parity3_degree1_triple_escape_recovers() {
        let mut pair_only_residual = 0;
        for seed in 0..SEEDS {
            let target = Target::new(Family::Parity3, seed);
            let data = panels(&target, seed);
            let pair_max = JointFocusPolicy::default().max_pair_flips;
            let (pairs, _) = fit_anf_with(&pages(), &data[0], &data[1], 1, pair_max, 0);
            pair_only_residual += errors(&data[0], baf(&pairs));
            let (model, admitted) = fit_anf(&pages(), &data[0], &data[1], 1);
            assert!(admitted, "seed {seed}: triple escape not admitted");
            for rows in &data {
                assert_eq!(errors(rows, baf(&model)), 0, "seed {seed}: triple escape");
            }
        }
        assert!(
            pair_only_residual > 0,
            "pairs alone now solve parity3; update docs"
        );
    }

    /// BAF-SCALE: and3 is not representable at degree <= 2 but is a single
    /// degree-3 monomial; the opt-in degree-3 bank recovers it exactly.
    #[test]
    fn and3_degree3_is_exact_at_width7() {
        for seed in 0..SEEDS {
            let data = panels(&Target::new(Family::And3, seed), seed);
            let (m, admitted, st) =
                fit_anf_stats(&pages(), &data[0], &data[1], 3, JointFocusPolicy::default());
            assert_eq!(st.status, 0, "seed {seed}: deg3 fits default budgets");
            assert!(st.evaluations > 0);
            assert!(admitted, "seed {seed}: deg3 not admitted");
            for rows in &data {
                assert_eq!(errors(rows, baf(&m)), 0, "seed {seed}: and3 deg3");
            }
        }
    }

    /// BAF-SCALE parity4: triples alone stay partial on some seed; the opt-in
    /// quadruple escape is exact on every split at width 7.
    #[test]
    fn parity4_quad_escape_recovers_at_width7() {
        let mut no_quad_residual = 0;
        for seed in 0..SEEDS {
            let data = panels(&Target::new(Family::Parity4, seed), seed);
            let base = JointFocusPolicy::default();
            let (m0, _, _) = fit_anf_stats(&pages(), &data[0], &data[1], 1, base);
            no_quad_residual += errors(&data[0], baf(&m0));
            let quad = JointFocusPolicy {
                max_quad_flips: 128,
                ..base
            };
            let (m, admitted, st) = fit_anf_stats(&pages(), &data[0], &data[1], 1, quad);
            assert_eq!(st.status, 0);
            assert!(admitted, "seed {seed}: quad escape not admitted");
            for rows in &data {
                assert_eq!(errors(rows, baf(&m)), 0, "seed {seed}: parity4 quad");
            }
        }
        assert!(
            no_quad_residual > 0,
            "triples alone now solve parity4; update docs"
        );
    }

    /// Over-budget quadruple escape fails closed (inactive atlas, status 1).
    #[test]
    fn quad_escape_over_budget_fails_closed() {
        let data = panels(&Target::new(Family::Parity4, 0), 0);
        let quad = JointFocusPolicy {
            max_quad_flips: 69, // C(7,4) * 2 pages = 70
            ..JointFocusPolicy::default()
        };
        let (m, admitted, st) = fit_anf_stats(&pages(), &data[0], &data[1], 1, quad);
        assert_eq!(st.status, 1);
        assert!(!admitted);
        let inactive = BooleanAssociativeFocus::new(cfg(), pages()).expect("valid");
        assert_eq!(errors(&data[0], baf(&m)), errors(&data[0], baf(&inactive)));
    }
}
