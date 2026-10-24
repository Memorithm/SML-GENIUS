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

## HLE-2: exact routing index and a byte-level BPC proxy (research branch)

Status: research only, stacked on HLE-1 (`feat/hybrid-difflogic-e2e`), not on main. Nothing below is integrated into MOR/SBG production routing. Measurements were taken in a shared 4-core cloud container, not on Dell or Thor; `scripts/hle2-bench.sh` reproduces them on a target host.

### 1. `hamming_index::MultiIndexHamming`

Exact Hamming top-k over keys of a validated width (1..=64). The key is split into `m` substrings with exact-match buckets; substring radius grows until `k` examined keys lie within the pigeonhole bound `m*(s+1)-1`, so the result equals the full scan ordered by `(distance, PageId)`, ties included. When probing would cost more than about a full scan, an exact full-scan fallback is used and counted. There is no approximate mode. Width, key range, query range, duplicate `PageId` and `k = 0` all fail closed. Tests compare it with `full_scan_top_k` on random keys (widths 1, 7, 16, 33, 64; m in {1,2,3,4,8}; k up to beyond n) and adversarial keys (all equal, single-bit, complements, 0 and all-ones). The index is a standalone module; `HybridLogicEngine` still uses its own scan.

Bench (`hle2_index_bench`, k = 4, 2000 queries, `m` automatic; recall checked against full scan):

| keys | width | pages | m | index ns/q | scan ns/q | candidates/q | fallback rate | recall |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| uniform | 64 | 16 | 16 | 346 | 328 | 16 | 1.00 | 1.0 |
| uniform | 64 | 256 | 8 | 2 443 | 1 984 | 264 | 1.00 | 1.0 |
| uniform | 64 | 4 096 | 6 | 44 211 | 27 139 | 4 985 | 1.00 | 1.0 |
| uniform | 64 | 65 536 | 4 | 696 546 | 477 126 | 67 755 | 0.99 | 1.0 |
| clustered | 64 | 16 | 16 | 259 | 227 | 16 | 1.00 | 1.0 |
| clustered | 64 | 256 | 8 | 815 | 754 | 23 | 0.03 | 1.0 |
| clustered | 64 | 4 096 | 6 | 3 455 | 10 986 | 142 | 0.00 | 1.0 |
| clustered | 64 | 65 536 | 4 | 29 351 | 232 665 | 1 312 | 0.00 | 1.0 |
| uniform | 16 | 16 | 4 | 222 | 205 | 16 | 1.00 | 1.0 |
| uniform | 16 | 256 | 2 | 1 600 | 1 030 | 203 | 0.73 | 1.0 |
| uniform | 16 | 4 096 | 2 | 6 588 | 12 317 | 278 | 0.00 | 1.0 |
| uniform | 16 | 65 536 | 1 | 1 334 | 233 052 | 17 | 0.00 | 1.0 |

Reading: exact MIH is sub-linear only when keys are dense in a small width or clustered around queries (clustered = 32 centres with 3-bit noise). On uniform 64-bit keys the nearest neighbour is far, MIH degenerates to the fallback and is **slower** than a plain scan (up to about 1.6x, because of the probes wasted before falling back). Timings are single runs on a noisy shared host.

### 2. Byte-level BPC proxy (`hle2_byte_lm`) — partial harness, not GB-11 P1

GB-11 has no real-text BPC yet: its results are on synthetic teachers, and P1 (S5h corpus against the ~15M-parameter `--stream-50m-msb` witness at BPC 3.66 on S5h DEV) is only sketched and needs owner approval and preregistration. The S5h corpus is not in the repository, so no comparison against GB-11 banks or the S5h witness is made here.

The harness predicts each byte MSB-first as 8 binary decisions. Inputs: bits of the previous 2 bytes, known high target bits, one-hot of the bit position. The page is chosen by exact Hamming top-1 over the previous byte with `MultiIndexHamming`. Each page is a relaxed logic network [256, 256, 64]; p(1) is the smoothed mean of its 64 outputs. Hard BPC uses the discretized network with p = (ones + 0.5)/65. Proxy corpus: the repository's own `docs/*.md`, files sorted, every 10th file is DEV; first 400 000 TRAIN bytes and 60 000 DEV bytes, 1 epoch.

| arm | parameters | relaxed DEV BPC | hard DEV BPC |
|---|---:|---:|---:|
| order-0 byte counts | counts | 5.128 | — |
| order-1 byte counts | counts | 4.028 | — |
| order-2 byte counts | counts | 3.350 | — |
| order-3 byte counts | counts | 3.119 | — |
| HLE, learned routing (16 pages = 16 most frequent previous bytes) | 36 864 | 6.001 | 6.018 |
| HLE, frozen random routing keys | 36 864 | 6.109 | 6.125 |
| HLE, single page (no routing) | 2 304 | 6.209 | 6.226 |
| HLE, frozen gates (untrained) | 36 864 | 7.999 | 8.171 |

Reading: the circuits learn something (about 2 bits/byte below the frozen control), and learned routing helps a little over random routing and one page. But the model is **worse than an order-0 unigram** and far behind short n-grams on this proxy. Its output head (mean of hard gates) is coarse, and training is one short epoch. These numbers do not show a BPC gain on real text.

### What GB-11 P1 would still need

The S5h corpus and its frozen DEV/TEST split on the host (or a pinned fetch approved by the owner); a preregistered P1 protocol with matched parameter and token budget against the 15M witness; and a stronger output head (for example a per-page count table addressed by the hard circuit bits) before any comparison is meaningful.

## HLE-3: count-table head and learned routing keys (research branch)

Status: research only, stacked on HLE-2. Measured in a shared 4-core cloud container, not on Dell or Thor (`scripts/hle3-bench.sh` reproduces). Same proxy corpus and split as HLE-2 (repository `docs/*.md`, 400 000 TRAIN / 60 000 DEV bytes). This is not S5h, so there is no GB-11 or witness comparison.

### 1. Count-table output head (`hle3`)

The circuits are trained as in HLE-2 (one epoch, relaxed mean head). Then, in one alternation, a count table is fitted. It is addressed by the circuit's **hard** output bits for each bit decision (tree node = bit position plus known high target bits). Smoothing is interpolated additive (Dirichlet) backoff, β = 2, coarse to fine; it is not Kneser-Ney. Byte-context rows use the same backoff with the previous bytes as context.

| head | trained, learned routing | trained, frozen random routing | trained, single page | untrained gates (random circuit bits) |
|---|---:|---:|---:|---:|
| node → page → 12 circuit bits | 4.003 | 4.169 | 4.693 | 4.150 |
| node → page → 8 circuit bits | 4.172 | 4.291 | 4.715 | 4.319 |
| order-1 bytes + 6 circuit bits | 3.955 | 3.971 | 3.989 | 3.929 |
| order-2 bytes + 6 circuit bits | 3.269 | 3.269 | 3.269 | 3.269 |

Byte-context baselines (same backoff): order 0 5.128, order 1 4.022, order 2 3.234, order 3 2.635 DEV BPC.

Reading:
- The count head moves the circuit model from about 6.0 (HLE-2 mean head) to 4.00 BPC. With learned routing it is about 0.15 BPC better than random circuit bits and better than frozen routing (4.17) or one page (4.69). That is the measured contribution of training and routing.
- It is still **worse than order-2 byte counts (3.234)**, which use exactly the same information (the circuit only sees the previous 2 bytes). The circuit acts as a lossy compression of that context.
- Adding circuit bits to order-2 counts cannot add information, because the code is a deterministic function of the order-2 context. Every arm gives the same 3.269, slightly worse than 3.234 because of the extra smoothing level.
- With order-1 counts, 6 circuit bits help a little (4.022 → 3.955), but **untrained random bits help slightly more (3.929)**. The trained circuit bits are not better features than random projections here.
- Limits: a single alternation (no joint training), one epoch, one seed, a proxy corpus. Circuits that see more context than the count baseline would be the next meaningful test.

### 2. Learned routing keys and the index in the engine

`hamming_index::hamming_k_majority` is deterministic k-means in Hamming space (bitwise-majority centres, exact assignment through `MultiIndexHamming`, duplicate centres perturbed). `HybridLogicEngine::with_route_index(true)` routes through `MultiIndexHamming`; the default is unchanged (scan). Routing results and full `run` commits are tested to be identical to the scan path on random and clustered engines (widths 6, 12, 30 and 64; ids not in position order). With the index on, `route_lookups` counts examined candidates.

Bench (`hle3_route_bench`): keys are k-majority centres (2 iterations) over 64-bit signatures of the previous 8 bytes of TRAIN text; queries are 2 000 DEV signatures; k = 4. Results are asserted equal to the scan.

| keys | pages | scan ns/q | index ns/q | scan lookups/q | index candidates/q |
|---|---:|---:|---:|---:|---:|
| k-majority on text | 4 096 | 22 826 | 39 240 | 4 096 | 1 692 |
| uniform random | 4 096 | 19 690 | 28 308 | 4 096 | 4 976 (fallback) |
| k-majority on text | 65 536 | 391 474 | 104 476 | 65 536 | 5 008 |
| uniform random | 65 536 | 384 644 | 338 396 | 65 536 | 67 942 (fallback) |

Reading: with learned text keys the index is about 3.7x faster at 65 536 pages and examines about 7.6 % of the keys. At 4 096 pages it is about 1.7x **slower** than the scan, even though it examines fewer keys, because of probe and hash overhead. Uniform keys gain nothing. These are single runs on a noisy shared host.
