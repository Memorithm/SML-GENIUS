//! Hybrid Boolean engine (HLE-0): Boolean routing + selected atlas pages +
//! a bounded recurrent workspace with an explicit COMMIT.
//!
//! Execution per query:
//!
//! 1. **Route.** The router signature is `input XOR workspace` (low `W`
//!    bits). Each page owns a `u64` key; distance is `popcount(sig ^ key)`.
//!    The `k` nearest pages are selected (ties broken by `PageId`). This is
//!    an integer scan over page keys: no Q/K/V projection, no softmax, no
//!    token-pair score matrix and no KV cache.
//! 2. **Execute.** Each selected page is a [`HardLogicNetwork`] (trained in
//!    relaxed mode in [`crate::diff_logic`], then discretised). It reads the
//!    `input_bits + workspace_bits` wires and emits `workspace_bits` outputs.
//! 3. **Rewrite.** The workspace is XOR-updated with every selected page's
//!    output.
//! 4. **Cycle / COMMIT.** Steps 1–3 repeat for at least `min_cycles` and at
//!    most `max_cycles`. The loop stops early when the workspace reaches a
//!    fixed point. The final workspace is returned as a [`HybridCommit`].
//!
//! Counters reuse [`ExecutionCounters`]. No performance or quality claim is
//! made; tests only check semantics and determinism.

use core::fmt;

use crate::{diff_logic::HardLogicNetwork, ExecutionCounters, PageId};

/// One atlas page: a routing key plus a hard logic network.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HybridPage {
    pub id: PageId,
    pub key: u64,
    pub network: HardLogicNetwork,
}

/// Engine configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HybridConfig {
    pub input_bits: usize,
    pub workspace_bits: usize,
    /// Pages selected per cycle.
    pub pages_per_cycle: usize,
    pub min_cycles: usize,
    pub max_cycles: usize,
}

/// Why the recurrent loop stopped before COMMIT.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HaltReason {
    FixedPoint,
    CycleBudget,
}

/// Committed result of one query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HybridCommit {
    pub workspace: u64,
    pub cycles: usize,
    pub halt: HaltReason,
    /// Pages selected in each cycle, in execution order.
    pub route_trace: Vec<Vec<PageId>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HybridError {
    InvalidConfig(&'static str),
    DuplicatePage(PageId),
    PageShape {
        page: PageId,
        inputs: usize,
        outputs: usize,
    },
    InputOutOfRange,
    /// A page key has bits at or above `workspace_bits`; such bits could
    /// never match a signature and are rejected instead of silently masked.
    KeyOutOfRange(PageId),
    /// `run_lanes` accepts 1..=64 queries with matching workspaces.
    LaneCount,
}

impl fmt::Display for HybridError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfig(why) => write!(f, "invalid hybrid config: {why}"),
            Self::DuplicatePage(id) => write!(f, "duplicate page {}", id.0),
            Self::PageShape {
                page,
                inputs,
                outputs,
            } => write!(
                f,
                "page {} has shape {inputs}->{outputs}, incompatible with config",
                page.0
            ),
            Self::InputOutOfRange => write!(f, "input has bits above input_bits"),
            Self::KeyOutOfRange(id) => write!(f, "page {} key exceeds workspace_bits", id.0),
            Self::LaneCount => write!(f, "lane batch must hold 1..=64 queries"),
        }
    }
}

impl std::error::Error for HybridError {}

/// Hybrid Boolean engine over a fixed set of hard pages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HybridLogicEngine {
    config: HybridConfig,
    pages: Vec<HybridPage>,
}

fn mask(bits: usize) -> u64 {
    if bits >= 64 {
        u64::MAX
    } else {
        (1u64 << bits) - 1
    }
}

impl HybridLogicEngine {
    pub fn new(config: HybridConfig, pages: Vec<HybridPage>) -> Result<Self, HybridError> {
        if config.input_bits == 0 || config.input_bits > 64 {
            return Err(HybridError::InvalidConfig("input_bits must be 1..=64"));
        }
        if config.workspace_bits == 0 || config.workspace_bits > 64 {
            return Err(HybridError::InvalidConfig("workspace_bits must be 1..=64"));
        }
        if config.min_cycles == 0 || config.min_cycles > config.max_cycles {
            return Err(HybridError::InvalidConfig(
                "require 1 <= min_cycles <= max_cycles",
            ));
        }
        if config.pages_per_cycle == 0 || config.pages_per_cycle > pages.len() {
            return Err(HybridError::InvalidConfig(
                "pages_per_cycle must be 1..=page count",
            ));
        }
        let mut ids: Vec<u32> = pages.iter().map(|p| p.id.0).collect();
        ids.sort_unstable();
        if let Some(w) = ids.windows(2).find(|w| w[0] == w[1]) {
            return Err(HybridError::DuplicatePage(PageId(w[0])));
        }
        for p in &pages {
            if p.key & !mask(config.workspace_bits) != 0 {
                return Err(HybridError::KeyOutOfRange(p.id));
            }
            let (i, o) = (p.network.input_count(), p.network.output_count());
            if i != config.input_bits + config.workspace_bits || o != config.workspace_bits {
                return Err(HybridError::PageShape {
                    page: p.id,
                    inputs: i,
                    outputs: o,
                });
            }
        }
        Ok(Self { config, pages })
    }

    /// Indices of the `pages_per_cycle` nearest pages by Hamming distance,
    /// ordered by `(distance, PageId)`. Uses partial selection
    /// (`select_nth_unstable`) and only sorts the selected `k`.
    fn route_indices(&self, signature: u64, counters: &mut ExecutionCounters) -> Vec<usize> {
        let k = self.config.pages_per_cycle;
        let mut ranked: Vec<(u32, u32, usize)> = self
            .pages
            .iter()
            .enumerate()
            .map(|(i, p)| ((signature ^ p.key).count_ones(), p.id.0, i))
            .collect();
        counters.route_lookups += ranked.len() as u64;
        if k < ranked.len() {
            ranked.select_nth_unstable(k - 1);
            ranked.truncate(k);
        }
        ranked.sort_unstable();
        ranked.into_iter().map(|(_, _, i)| i).collect()
    }

    /// Select the `pages_per_cycle` nearest pages by Hamming distance.
    pub fn route(&self, signature: u64, counters: &mut ExecutionCounters) -> Vec<PageId> {
        self.route_indices(signature, counters)
            .into_iter()
            .map(|i| self.pages[i].id)
            .collect()
    }

    pub fn config(&self) -> HybridConfig {
        self.config
    }

    /// Run 1..=max_cycles of route/execute/rewrite, then COMMIT.
    pub fn run(
        &self,
        input: u64,
        initial_workspace: u64,
        counters: &mut ExecutionCounters,
    ) -> Result<HybridCommit, HybridError> {
        let c = self.config;
        if input & !mask(c.input_bits) != 0 {
            return Err(HybridError::InputOutOfRange);
        }
        let wmask = mask(c.workspace_bits);
        let mut ws = initial_workspace & wmask;
        let mut trace = Vec::new();
        let mut halt = HaltReason::CycleBudget;
        let mut wires = vec![0u64; c.input_bits + c.workspace_bits];
        for (i, w) in wires.iter_mut().enumerate().take(c.input_bits) {
            *w = (input >> i) & 1;
        }
        for cycle in 1..=c.max_cycles {
            let selected = self.route_indices((input ^ ws) & wmask, counters);
            for (j, w) in wires[c.input_bits..].iter_mut().enumerate() {
                *w = (ws >> j) & 1;
            }
            let mut delta = 0u64;
            for &i in &selected {
                let page = &self.pages[i];
                let out = page
                    .network
                    .eval_packed(&wires)
                    .expect("page shape validated at construction");
                counters.boolean_ops += page.network.gate_count() as u64;
                for (j, w) in out.iter().enumerate() {
                    delta ^= (w & 1) << j;
                }
            }
            trace.push(selected.iter().map(|&i| self.pages[i].id).collect());
            let next = ws ^ delta;
            let fixed = next == ws;
            ws = next;
            if fixed && cycle >= c.min_cycles {
                halt = HaltReason::FixedPoint;
                break;
            }
        }
        Ok(HybridCommit {
            workspace: ws,
            cycles: trace.len(),
            halt,
            route_trace: trace,
        })
    }

    /// Run up to 64 independent queries in one pass, lane `l` holding
    /// query `l`. Wires are bit-sliced (`wire[i]` bit `l` = lane `l`). Each
    /// cycle routes every active lane independently; each selected page is
    /// evaluated once on the packed wires and its output is XOR-applied
    /// under the mask of lanes that selected it. A lane stops updating at
    /// its own fixed point (after `min_cycles`). Results equal `run` per lane.
    pub fn run_lanes(
        &self,
        inputs: &[u64],
        initial_workspaces: &[u64],
        counters: &mut ExecutionCounters,
    ) -> Result<Vec<HybridCommit>, HybridError> {
        let c = self.config;
        let n = inputs.len();
        if n == 0 || n > 64 || initial_workspaces.len() != n {
            return Err(HybridError::LaneCount);
        }
        if inputs.iter().any(|&x| x & !mask(c.input_bits) != 0) {
            return Err(HybridError::InputOutOfRange);
        }
        let wmask = mask(c.workspace_bits);
        let mut ws: Vec<u64> = initial_workspaces.iter().map(|w| w & wmask).collect();
        let mut wires = vec![0u64; c.input_bits + c.workspace_bits];
        for (l, &x) in inputs.iter().enumerate() {
            for (i, w) in wires.iter_mut().enumerate().take(c.input_bits) {
                *w |= ((x >> i) & 1) << l;
            }
        }
        let mut active: u64 = mask(n);
        let mut traces = vec![Vec::new(); n];
        let mut halts = vec![HaltReason::CycleBudget; n];
        let mut lane_mask = vec![0u64; self.pages.len()];
        for cycle in 1..=c.max_cycles {
            if active == 0 {
                break;
            }
            lane_mask.iter_mut().for_each(|m| *m = 0);
            for l in 0..n {
                if (active >> l) & 1 == 0 {
                    continue;
                }
                let sel = self.route_indices((inputs[l] ^ ws[l]) & wmask, counters);
                traces[l].push(sel.iter().map(|&i| self.pages[i].id).collect::<Vec<_>>());
                for i in sel {
                    lane_mask[i] |= 1 << l;
                }
            }
            for (j, w) in wires[c.input_bits..].iter_mut().enumerate() {
                *w = ws
                    .iter()
                    .enumerate()
                    .fold(0, |acc, (l, &v)| acc | (((v >> j) & 1) << l));
            }
            let mut delta = vec![0u64; c.workspace_bits];
            for (page, &m) in self.pages.iter().zip(&lane_mask) {
                if m == 0 {
                    continue;
                }
                let out = page
                    .network
                    .eval_packed(&wires)
                    .expect("page shape validated at construction");
                counters.boolean_ops += page.network.gate_count() as u64;
                for (d, o) in delta.iter_mut().zip(out) {
                    *d ^= o & m;
                }
            }
            for l in 0..n {
                if (active >> l) & 1 == 0 {
                    continue;
                }
                let dl = delta
                    .iter()
                    .enumerate()
                    .fold(0u64, |acc, (j, &d)| acc | (((d >> l) & 1) << j));
                ws[l] ^= dl;
                if dl == 0 && cycle >= c.min_cycles {
                    halts[l] = HaltReason::FixedPoint;
                    active &= !(1 << l);
                }
            }
        }
        Ok((0..n)
            .map(|l| HybridCommit {
                workspace: ws[l],
                cycles: traces[l].len(),
                halt: halts[l],
                route_trace: std::mem::take(&mut traces[l]),
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff_logic::{truth_table_dataset, Adam, DiffLogicNetwork, TrainMode};

    /// Train a 1-gate page mapping (input bit, workspace bit) -> workspace delta.
    fn trained_page(id: u32, key: u64, f: impl Fn(u32) -> Vec<bool>) -> HybridPage {
        let (xs, ys) = truth_table_dataset(2, f);
        let mut net = DiffLogicNetwork::new(2, &[1], u64::from(id), 0.1).unwrap();
        let mut opt = Adam::new(net.parameter_count(), 0.1);
        let mut g = vec![0.0; net.parameter_count()];
        for _ in 0..300 {
            net.loss_and_grad(&xs, &ys, TrainMode::Relaxed, &mut g)
                .unwrap();
            let mut p = net.parameters().to_vec();
            opt.step(&mut p, &g);
            net.parameters_mut().copy_from_slice(&p);
        }
        HybridPage {
            id: PageId(id),
            key,
            network: net.discretize(),
        }
    }

    fn cfg(min: usize, max: usize) -> HybridConfig {
        HybridConfig {
            input_bits: 1,
            workspace_bits: 1,
            pages_per_cycle: 1,
            min_cycles: min,
            max_cycles: max,
        }
    }

    // Row r encodes wire0 = input = r&1, wire1 = workspace = r>>1.
    fn toggle_if_input(r: u32) -> Vec<bool> {
        vec![r & 1 == 1]
    }
    fn never(_: u32) -> Vec<bool> {
        vec![false]
    }

    #[test]
    fn routing_is_hamming_nearest_with_id_tiebreak() {
        let engine = HybridLogicEngine::new(
            HybridConfig {
                pages_per_cycle: 2,
                ..cfg(1, 1)
            },
            vec![
                trained_page(3, 0b0, never),
                trained_page(1, 0b1, never),
                trained_page(2, 0b1, never),
            ],
        )
        .unwrap();
        let mut c = ExecutionCounters::default();
        assert_eq!(engine.route(1, &mut c), vec![PageId(1), PageId(2)]);
        assert_eq!(engine.route(0, &mut c), vec![PageId(3), PageId(1)]);
        assert_eq!(c.route_lookups, 6);
    }

    #[test]
    fn trained_pages_drive_recurrent_cycles_and_commit() {
        // Page 7 (key 1) toggles the workspace when input=1; page 9 (key 0)
        // never writes. Signature = input ^ workspace selects between them.
        let pages = vec![
            trained_page(7, 1, toggle_if_input),
            trained_page(9, 0, never),
        ];
        let engine = HybridLogicEngine::new(cfg(1, 4), pages).unwrap();
        let mut c = ExecutionCounters::default();

        // input=1, ws=0: sig=1 -> page 7 toggles ws to 1; sig=0 -> page 9,
        // no change -> fixed point after cycle 2.
        let commit = engine.run(1, 0, &mut c).unwrap();
        assert_eq!(commit.workspace, 1);
        assert_eq!(commit.cycles, 2);
        assert_eq!(commit.halt, HaltReason::FixedPoint);
        assert_eq!(commit.route_trace, vec![vec![PageId(7)], vec![PageId(9)]]);

        // input=0, ws=0: page 9 immediately -> fixed point at cycle 1.
        let commit = engine.run(0, 0, &mut c).unwrap();
        assert_eq!((commit.workspace, commit.cycles), (0, 1));
        assert!(c.boolean_ops > 0);
    }

    #[test]
    fn cycle_budget_bounds_oscillation_and_min_cycles_is_honoured() {
        // A single page that always toggles never reaches a fixed point.
        let always = trained_page(1, 0, |_| vec![true]);
        let engine = HybridLogicEngine::new(cfg(1, 3), vec![always.clone()]).unwrap();
        let mut c = ExecutionCounters::default();
        let commit = engine.run(0, 0, &mut c).unwrap();
        assert_eq!(commit.halt, HaltReason::CycleBudget);
        assert_eq!((commit.cycles, commit.workspace), (3, 1));

        let idle = trained_page(2, 0, never);
        let engine = HybridLogicEngine::new(cfg(3, 5), vec![idle]).unwrap();
        let commit = engine.run(0, 0, &mut c).unwrap();
        assert_eq!((commit.cycles, commit.halt), (3, HaltReason::FixedPoint));
    }

    #[test]
    fn lanes_match_single_query_runs_with_per_lane_routing() {
        // 4 pages, 2-bit workspace, 3-bit input: lanes route differently.
        let mk = |id: u32, key: u64, seed: u64| {
            let mut net = DiffLogicNetwork::new(5, &[2], seed, 2.0).unwrap();
            // Large random tables give non-trivial hard functions.
            let p: Vec<f64> = net
                .parameters()
                .iter()
                .enumerate()
                .map(|(i, v)| if (i as u64 + seed) % 3 == 0 { -v } else { *v })
                .collect();
            net.parameters_mut().copy_from_slice(&p);
            HybridPage {
                id: PageId(id),
                key,
                network: net.discretize(),
            }
        };
        let engine = HybridLogicEngine::new(
            HybridConfig {
                input_bits: 3,
                workspace_bits: 2,
                pages_per_cycle: 2,
                min_cycles: 1,
                max_cycles: 5,
            },
            vec![mk(10, 0, 1), mk(4, 1, 2), mk(7, 2, 3), mk(2, 3, 4)],
        )
        .unwrap();
        let inputs: Vec<u64> = (0..64).map(|l| l % 8).collect();
        let ws0: Vec<u64> = (0..64).map(|l| (l / 8) % 4).collect();
        let mut c = ExecutionCounters::default();
        let lanes = engine.run_lanes(&inputs, &ws0, &mut c).unwrap();
        let mut distinct_routes = std::collections::BTreeSet::<Vec<u32>>::new();
        for l in 0..64 {
            let single = engine
                .run(inputs[l], ws0[l], &mut ExecutionCounters::default())
                .unwrap();
            assert_eq!(lanes[l], single, "lane {l}");
            distinct_routes.insert(single.route_trace[0].iter().map(|p| p.0).collect());
        }
        assert!(distinct_routes.len() > 1);
        assert_eq!(
            engine.run_lanes(&[0; 65], &[0; 65], &mut c),
            Err(HybridError::LaneCount)
        );
    }

    #[test]
    fn keys_wider_than_workspace_fail_closed() {
        let mut p = trained_page(1, 0, never);
        p.key = 0b10;
        assert_eq!(
            HybridLogicEngine::new(cfg(1, 1), vec![p]),
            Err(HybridError::KeyOutOfRange(PageId(1)))
        );
    }

    #[test]
    fn partial_select_matches_full_sort_reference() {
        let pages: Vec<HybridPage> = (0..16u32)
            .map(|i| trained_page(100 - i, u64::from(i * 7 % 16), never))
            .collect();
        let engine = HybridLogicEngine::new(
            HybridConfig {
                input_bits: 4,
                workspace_bits: 4,
                pages_per_cycle: 3,
                min_cycles: 1,
                max_cycles: 1,
            },
            pages
                .into_iter()
                .map(|mut p| {
                    let mut n = DiffLogicNetwork::new(8, &[4], 1, 0.1).unwrap();
                    n.parameters_mut().iter_mut().for_each(|v| *v = -1.0);
                    p.network = n.discretize();
                    p
                })
                .collect(),
        )
        .unwrap();
        for sig in 0..16u64 {
            let mut full: Vec<(u32, u32)> = engine
                .pages
                .iter()
                .map(|p| ((sig ^ p.key).count_ones(), p.id.0))
                .collect();
            full.sort_unstable();
            let want: Vec<PageId> = full.iter().take(3).map(|&(_, id)| PageId(id)).collect();
            assert_eq!(engine.route(sig, &mut ExecutionCounters::default()), want);
        }
    }

    /// Timing harness (ignored by default): Hamming top-k over `P` page keys
    /// versus the existing `DirectRouteTable` (one indexed lookup). Run on
    /// the target machine with
    /// `cargo test --release -p sml-core --lib hle1_route_bench -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn hle1_route_bench() {
        use crate::{BooleanWorkspace, DirectRouteTable};
        use std::time::Instant;
        for &p in &[16usize, 256, 4096] {
            let mut n = DiffLogicNetwork::new(16, &[8], 1, 0.1).unwrap();
            n.parameters_mut().iter_mut().for_each(|v| *v = -1.0);
            let hard = n.discretize();
            let pages = (0..p)
                .map(|i| HybridPage {
                    id: PageId(i as u32),
                    key: (i as u64).wrapping_mul(0x9E37_79B9) & 0xFF,
                    network: hard.clone(),
                })
                .collect();
            let engine = HybridLogicEngine::new(
                HybridConfig {
                    input_bits: 8,
                    workspace_bits: 8,
                    pages_per_cycle: 4,
                    min_cycles: 1,
                    max_cycles: 1,
                },
                pages,
            )
            .unwrap();
            let iters = 20_000u64;
            let mut c = ExecutionCounters::default();
            let t = Instant::now();
            let mut sink = 0u32;
            for q in 0..iters {
                sink ^= engine.route(q & 0xFF, &mut c)[0].0;
            }
            let ham = t.elapsed().as_nanos() as f64 / iters as f64;
            let table = DirectRouteTable::new(8, (0..256).map(PageId).collect()).unwrap();
            let wss: Vec<BooleanWorkspace> = (0..256u64)
                .map(|v| {
                    BooleanWorkspace::from_bits(
                        &(0..8).map(|j| (v >> j) & 1 == 1).collect::<Vec<_>>(),
                    )
                })
                .collect();
            let mut c2 = ExecutionCounters::default();
            let t = Instant::now();
            for q in 0..iters {
                sink ^= table.route(&wss[(q & 0xFF) as usize], &mut c2).unwrap().0;
            }
            let direct = t.elapsed().as_nanos() as f64 / iters as f64;
            println!(
                "pages={p} hamming_topk_ns={ham:.1} lookups/q={} direct_ns={direct:.1} lookups/q={} sink={sink}",
                c.route_lookups / iters,
                c2.route_lookups / iters
            );
        }
    }

    #[test]
    fn run_is_deterministic() {
        let engine = HybridLogicEngine::new(
            cfg(1, 4),
            vec![
                trained_page(7, 1, toggle_if_input),
                trained_page(9, 0, never),
            ],
        )
        .unwrap();
        let a = engine.run(1, 0, &mut ExecutionCounters::default()).unwrap();
        let b = engine.run(1, 0, &mut ExecutionCounters::default()).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn invalid_configurations_fail_closed() {
        let p = trained_page(1, 0, never);
        assert!(matches!(
            HybridLogicEngine::new(cfg(0, 1), vec![p.clone()]),
            Err(HybridError::InvalidConfig(_))
        ));
        assert!(matches!(
            HybridLogicEngine::new(cfg(2, 1), vec![p.clone()]),
            Err(HybridError::InvalidConfig(_))
        ));
        assert_eq!(
            HybridLogicEngine::new(cfg(1, 1), vec![p.clone(), p.clone()]),
            Err(HybridError::DuplicatePage(PageId(1)))
        );
        let wrong = HybridConfig {
            workspace_bits: 2,
            ..cfg(1, 1)
        };
        assert!(matches!(
            HybridLogicEngine::new(wrong, vec![p.clone()]),
            Err(HybridError::PageShape { .. })
        ));
        let engine = HybridLogicEngine::new(cfg(1, 1), vec![p]).unwrap();
        assert_eq!(
            engine.run(2, 0, &mut ExecutionCounters::default()),
            Err(HybridError::InputOutOfRange)
        );
    }
}
