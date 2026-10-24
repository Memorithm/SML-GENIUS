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
