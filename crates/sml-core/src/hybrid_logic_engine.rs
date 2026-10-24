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

    /// Select the `pages_per_cycle` nearest pages by Hamming distance.
    pub fn route(&self, signature: u64, counters: &mut ExecutionCounters) -> Vec<PageId> {
        let mut ranked: Vec<(u32, PageId, usize)> = self
            .pages
            .iter()
            .enumerate()
            .map(|(i, p)| ((signature ^ p.key).count_ones(), p.id, i))
            .collect();
        counters.route_lookups += ranked.len() as u64;
        ranked.sort_unstable_by_key(|&(d, id, _)| (d, id.0));
        ranked
            .into_iter()
            .take(self.config.pages_per_cycle)
            .map(|(_, id, _)| id)
            .collect()
    }

    fn page(&self, id: PageId) -> &HybridPage {
        self.pages
            .iter()
            .find(|p| p.id == id)
            .expect("routed id exists")
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
            let selected = self.route((input ^ ws) & wmask, counters);
            for (j, w) in wires[c.input_bits..].iter_mut().enumerate() {
                *w = (ws >> j) & 1;
            }
            let mut delta = 0u64;
            for &id in &selected {
                let page = self.page(id);
                let out = page
                    .network
                    .eval_packed(&wires)
                    .expect("page shape validated at construction");
                counters.boolean_ops += page.network.gate_count() as u64;
                for (j, w) in out.iter().enumerate() {
                    delta ^= (w & 1) << j;
                }
            }
            trace.push(selected);
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
