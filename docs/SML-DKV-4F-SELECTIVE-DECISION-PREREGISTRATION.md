# DKV-4f — selective downstream decision preregistration

Status: frozen before implementation/execution.
Base: `2265b09661542fb13d54cc70a10bc4184e1500b2`.

## Motivation

DKV-4e showed that native recurrent MOR hysteresis can clear a real
precision-escalation flag: a 40,000-ppm first margin became 70,000 ppm on the
next native route and avoided a frozen-decision hard-budget abstention.

That result changed a control flag but did not change task correctness. DKV-4f
asks the stronger question: what happens when route resolution is connected to
a bounded downstream decision gate?

The experiment is deliberately adversarial. It contains both aligned routing
and equally explicit misrouting controls so that increased coverage cannot be
mistaken for improved correctness.

## Frozen MOR policy

Use native `MorRouterRegistry -> MixtureOfRouters` with `MorPolicy::default()`
except:

- `base_expert_mass_threshold_ppm = 530_000`.

All other default thresholds and hysteresis values remain unchanged.

Each task uses two singleton router groups and two experts. The favored expert
has proposal score 1,000,000 ppm; the other has 920,000 ppm.

The frozen expected geometry is:

### first route, empty PreviousRoute

- ranked scores: 500,000 and 460,000 ppm;
- margin: 40,000 ppm;
- precision escalation: true;
- 530,000-ppm mass target requires both experts, so selected set size = 2.

### second route, previous primary retained

Native previous-route hysteresis is expected to produce:

- ranked scores: 545,000 and 475,000 ppm;
- margin: 70,000 ppm;
- precision escalation: false;
- the favored primary alone exceeds the 530,000-ppm mass target, so selected
  set size = 1.

These values must be measured by the native engine. If they do not occur, the
fixture hypothesis is refuted; thresholds must not be changed after observation.

## Expert and task semantics

The downstream task is a bounded Boolean decision:

- PageId(0) expert emits `true`;
- PageId(1) expert emits `false`;
- the DKV memory target is `V[0] = true`;
- a routed answer is `V[0] AND expert_output`.

The external task oracle is used **only for evaluation**. It is not visible to
MOR, DKV-4, the recurrent kernel or the downstream decision gate.

Four frozen task families are crossed with both STABLE and CORRECT_ONCE
semantic states:

1. ALIGNED_TRUE:
   - routing favors PageId(0);
   - oracle = true.
2. ALIGNED_FALSE:
   - routing favors PageId(1);
   - oracle = false.
3. MISALIGNED_FALSE:
   - routing favors PageId(0);
   - oracle = false.
4. MISALIGNED_TRUE:
   - routing favors PageId(1);
   - oracle = true.

The two misaligned controls intentionally separate route certainty from task
correctness.

## Semantic state fixtures

- STABLE: `V[0]` is already true before the first recurrent cycle.
- CORRECT_ONCE: `V[0]` starts false; the first DKV-2b read repairs it to true,
  and a second cycle is required by the stability control to confirm no further
  semantic change.

All arms use the same DKV matrix, key, target and depth bound.

## Downstream decision gates

### SELECTIVE_SINGLETON

The gate emits an answer only when:

- the recurrent controller terminates with `StopCandidate`; and
- the current native MOR decision selects exactly one expert.

Otherwise it abstains.

The gate does not inspect the oracle.

### FIRST_PRIMARY

A deliberately more aggressive control. After the semantic recurrence required
by the stability-only arm, use the primary expert from the first native MOR
decision even when that decision requested precision escalation and selected
two experts.

This control exposes whether DKV-4f gains correctness or merely buys a stricter
route-evidence condition.

## Comparison arms

For every task/state pair report:

1. stability-only recurrence + SELECTIVE_SINGLETON on the first MOR decision;
2. frozen-decision DKV-4c + SELECTIVE_SINGLETON;
3. dynamic native MOR + DKV-4d(dynamic) + SELECTIVE_SINGLETON;
4. stability-only recurrence + FIRST_PRIMARY.

For the dynamic arm, native MOR is recomputed each cycle with PreviousRoute
carried forward exactly as in DKV-4e.

## Frozen hypotheses

H1: all first routes measure 40,000-ppm margin, precision escalation=true and
two selected experts; all second routes measure 70,000 ppm, escalation=false
and one selected favored expert.

H2: first-route SELECTIVE_SINGLETON abstains on every task because two experts
are selected.

H3: frozen-decision DKV-4c also abstains on every task because the first MOR
decision never changes.

H4: dynamic DKV-4f SELECTIVE_SINGLETON answers all aligned tasks and those
answers match the oracle.

H5: dynamic DKV-4f also answers the deliberately misaligned tasks and those
answers fail the oracle. If so, route resolution increases coverage but is not
a correctness guarantee.

H6: FIRST_PRIMARY has the same aligned/misaligned correctness pattern as the
dynamic singleton arm at no greater recurrent depth. If so, DKV-4f has not
demonstrated an accuracy/cost advantage over the aggressive control.

H7: STABLE dynamic rows require one extra cycle beyond stability-only to resolve
route evidence, while CORRECT_ONCE route resolution fits inside the two semantic
repair/stability cycles.

H8: Missing/Conflict provenance remains zero-read abstention and Unverified
never StopCandidate.

All failed hypotheses are retained.

## Measurements

Retain per task/state/arm:

- recurrent depth and read count;
- final V bit;
- first/terminal MOR margin, precision flag, primary and selected set;
- DKV-4 terminal decision;
- downstream answered/abstained state;
- emitted Boolean value if any;
- oracle value;
- correctness among answered rows.

Aggregate separately:

- coverage;
- errors among answered rows;
- aligned versus misaligned results;
- recurrent cycles;
- matrix reads.

Coverage is never reported without selective risk.

## Interpretation boundary

A narrower native route is not authenticated truth. If the misaligned controls
become confidently wrong after route resolution, that is a required negative
result and blocks any claim that MOR confidence can replace an independent
decision/calibration/verifier layer.

The experiment uses synthetic expert outputs and routing scores. It does not
establish learned-language routing quality, calibration, long-context quality
or model-level gains.

Scientific/model promotion remains BLOCKED.
