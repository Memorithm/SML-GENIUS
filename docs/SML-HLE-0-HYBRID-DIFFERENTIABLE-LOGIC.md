# SML-HLE-0 hybrid Boolean engine with differentiable logic-gate pages

Status: executable reference contract. No language-model, scaling, speed,
memory or quality claim is made. The only evidence is the unit tests listed
below.

## Scope

Two modules in `sml-core`:

- `diff_logic` — differentiable two-input logic-gate networks (DLGN-0):
  relaxed training mode, straight-through mode, hard bit-sliced inference,
  hand-written backward pass, SGD and Adam.
- `hybrid_logic_engine` — Boolean routing over atlas pages, recurrent
  workspace with 1..R cycles, then COMMIT. Pages are hard networks produced
  by `diff_logic`. Reuses `PageId` and `ExecutionCounters`.

## Forbidden mechanisms (respected)

No Q/K/V projection, no softmax (neither for token scoring nor for gate
selection), no dense score matrix, no Transformer KV cache.

## Gate relaxation

A gate reads wires `a, b` and owns four latents `theta[k]` (`k = 2a + b`),
with relaxed table bits `p[k] = sigmoid(theta[k])`:

    y = (1-a)(1-b) p0 + (1-a) b p1 + a (1-b) p2 + a b p3

This is the expected output of a gate whose truth-table bits are independent
Bernoulli(`p[k]`), i.e. a mixture over all 16 two-input functions with
factorised (not softmax) weights. Corners recover the probabilistic
relaxations, e.g. AND = `ab`, OR = `a + b - ab`, XOR = `a + b - 2ab`.

Hard mode keeps table bit `k` iff `theta[k] > 0` and evaluates 64 samples
per `u64` word. Straight-through mode uses the hard bits forward and the
sigmoid derivative backward. `SteBits` provides clipped-STE bit weights.

## Engine cycle

1. signature = `input XOR workspace`; select the `k` pages with smallest
   `popcount(signature XOR key)` (ties by `PageId`);
2. evaluate each selected page on `input ++ workspace` bits;
3. XOR page outputs into the workspace;
4. repeat for `min_cycles..=max_cycles`; stop at a fixed point; COMMIT.

## What the tests show

| Test | Assertion |
|---|---|
| `relaxed_gate_corners_are_the_16_boolean_functions` | relaxed corners and hard gates equal all 16 truth tables |
| `relaxed_and_or_xor_match_probabilistic_formulas` | relaxed AND/OR/XOR equal `ab`, `a+b-ab`, `a+b-2ab` |
| `backward_matches_finite_differences` | manual gradient matches central differences (3-layer net) |
| `learns_xor_and_hard_mode_matches_after_discretization` | Adam reduces XOR loss >20x; hard net exact; rounded relaxed == hard |
| `learns_three_bit_parity_with_a_two_layer_network` | for at least one of 16 wiring seeds, 3-bit parity is learned exactly; loss decreases on every tried seed |
| `straight_through_training_reaches_exact_hard_function` | some seed reaches exact `(a∧b, a∨c)` with STE fine-tune not raising hard loss |
| `sgd_decreases_loss` | SGD reduces AND loss >5x |
| `trained_pages_drive_recurrent_cycles_and_commit` | trained pages route, rewrite the workspace and commit at a fixed point |
| `cycle_budget_bounds_oscillation_and_min_cycles_is_honoured` | max/min cycle bounds |
| `routing_is_hamming_nearest_with_id_tiebreak`, `run_is_deterministic`, `*_fail_closed` | routing, determinism, validation |

## Limitations

- Toy truth tables only; parity needed a seed search (wiring is fixed random).
- Engine pages are trained independently; there is no end-to-end gradient
  through routing or recurrence yet.
- Workspace and input are each limited to 64 bits; single-query execution
  uses one lane of the packed evaluator.
- Not connected to existing MOR, SBG or atlas manifests beyond `PageId`.

## HLE-1: end-to-end relaxed engine, learned wiring, learned router, 64 lanes

Status: executable reference contract. Only the measurements below are
claimed; they are toy-task, single-host (agent sandbox, x86_64, 8 CPUs,
Rust 1.89) numbers, **not** Dell/Thor evidence.

### Mechanisms

1. **Learned wiring** (`RelaxedHybridEngine::wiring_sweep`). GB-11 found the
   relaxation alone does not learn input selection and an exact discrete
   search does. HLE-1 therefore interleaves BPTT (Adam) with a greedy discrete
   sweep: for every gate, every source pair of the previous layer and every
   one of the 16 hard tables is tried; a move is kept only if it lowers
   `(hard-circuit error, relaxed loss)` lexicographically.
2. **Learned router, jointly.** Keys are logits; relaxed Hamming distance
   `d = sum_j s_j + q_j - 2 s_j q_j` with `s = x + w - 2xw`, `q = sigmoid(kappa)`.
   Straight-through top-k: hard top-k forward, per-page surrogate
   `sigmoid((tau - d)/T)` backward (`tau` = k/k+1 midpoint, constant). No
   softmax, no cross-page normalisation, no Q/K/V, no KV cache. Gated page
   outputs fold with relaxed XOR; BPTT across all cycles trains tables and
   keys together (`DiffLogicNetwork::backward_with_input_grad` chains pages).
3. **64-lane batching** (`HybridLogicEngine::run_lanes`): up to 64 queries
   bit-sliced per `u64`; each lane is routed independently; each selected
   page is evaluated once per cycle on the packed wires and XOR-applied under
   the mask of lanes that chose it; lanes halt at their own fixed point.
4. **Routing cost / safety.** `select_nth_unstable` partial selection, then
   only the `k` winners are sorted; pages are addressed by index (no
   search-by-id). Keys with bits at or above `workspace_bits` are rejected
   (`KeyOutOfRange`) instead of being masked.
5. **Discretisation measured independently** (`evaluate`): hard accuracy is
   computed by the discretised integer engine via `run_lanes`, separately
   from relaxed loss/accuracy, plus disagreement and mean `|p - hard bit|`.

### Controls (task: 4-bit parity into workspace bit 0, `I=4, W=2`, 4 single-layer pages, `k=1`, 3 cycles; 16 seeds; 4 epochs x 100 Adam steps, lr 0.1)

| arm | seeds exact (hard) | mean hard acc | mean relaxed acc | mean relaxed loss | mean disagreement | mean abs gap |
|---|---:|---:|---:|---:|---:|---:|
| full (wiring+routing) | 3/16 | 0.914 | 0.906 | 0.157 | 0.055 | 0.117 |
| wiring frozen | 0/16 | 0.543 | 0.582 | 0.585 | 0.258 | 0.440 |
| routing frozen | 3/16 | 0.855 | 0.883 | 0.263 | 0.105 | 0.223 |
| both frozen | 0/16 | 0.500 | 0.523 | 0.662 | 0.359 | 0.496 |

Identical in debug and release (deterministic). Reading: on this toy, the
wiring search accounts for most of the gain (as in GB-11); learned routing
adds mean accuracy but not exact seeds. 3/16 exact is a low success rate.

### Routing timing (sandbox host, release, `hle1_route_bench`, 8-bit signatures, k=4)

| pages | Hamming top-k ns/query | key reads/query | `DirectRouteTable` ns/query | lookups/query |
|---:|---:|---:|---:|---:|
| 16 | 112.7 | 16 | 0.9 | 1 |
| 256 | 1935.0 | 256 | 1.0 | 1 |
| 4096 | 15674.9 | 4096 | 0.9 | 1 |

The Hamming router is a linear key scan; the existing direct-address router
is one lookup. The comparison is with `DirectRouteTable`, not with the full
`MorPolicy` stack. An index (e.g. multi-index hashing) is not implemented.

### Tests

| Test | Assertion |
|---|---|
| `bptt_matches_finite_differences_with_routing_held_fixed` | BPTT over 3 cycles equals central differences (surrogate off, routing kinks skipped) |
| `routing_surrogate_trains_key_logits` | keys receive gradient |
| `joint_bptt_learns_two_cycle_parity_and_hard_engine_agrees` | 3-bit parity (impossible in one cycle for one single-layer page) learned; hard exact; rounded relaxed == hard; distinct pages across cycles |
| `learned_wiring_and_routing_solve_four_bit_parity_over_three_cycles` | full arm reaches an exact hard circuit for some seed in 0..16 with zero relaxed/hard disagreement |
| `wiring_search_never_worsens_the_hard_objective` | sweep is monotone in the objective |
| `lanes_match_single_query_runs_with_per_lane_routing` | 64 lanes with distinct routes equal 64 single `run` calls (workspace, cycles, halt, route trace) |
| `partial_select_matches_full_sort_reference`, `keys_wider_than_workspace_fail_closed` | routing equivalence, key validation |

### Reproduce / Dell–Thor

`bash scripts/hle1-bench.sh` prints `HLE1_ENV`, the controls table and the
routing timing. Dell/Thor numbers must be produced by running that script on
those hosts through a RemoteOps workflow pinned to an exact SHA; SML-GENIUS
self-hosted jobs only run integrated `main`, so this was not run here.

### Limitations

Toy tasks; seed-dependent success; ST routing gradient is biased; the wiring
sweep is exhaustive over pairs (cost grows quadratically with layer width)
and evaluates all pages; widths remain <= 64 bits; not connected to
MOR/SBG/atlas manifests beyond `PageId`.
