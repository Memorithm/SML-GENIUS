# SML-EAC: Elastic Activation Controller (reference v1)

Status: Rust-native deterministic **reference controller**, not a trained 25B
model, production serving path, nor measured inference speedup.

## Contract and authority

- SML owns **model semantics**, including inference-visible MOR evidence, the
  elastic active-page budget, recurrence-depth budget and scientific acceptance.
- MOR supplies a **ranked and eligible** expert shortlist. EAC cannot invent a
  router, expert, ANF bank, or an eligibility edge.
- The existing ElasticWeightPlanner owns the **transactional page transition**,
  RAM/VRAM residency, bit accounting and selected-page hard budget.
- An optional, versioned ElasticXxx adapter can later schedule qualified resource
  actions; the model retains standalone semantics with no external dependency.
- No dataset label, protected answer, oracle truth, or hidden outcome may enter
  EAC inference evidence.

## Controller

Inputs are top-1/top-2 margin, structural disagreement, explicitly observed
verification failure, recurrence stability, memory pressure, and an eligible,
MOR-ranked page shortlist. Evidence is parts per million and validated.

- Under ambiguity or verification failure, grow **width** to a hard maximum
  before growing **recurrent depth**.
- Under sustained stable signals, contract **depth** then **width**.
- Under memory pressure, contract immediately, overriding expansion cooldown.
- Use a configurable cooldown and stable-cycle threshold to avoid oscillation.
- An active minimum and maximum, candidate membership and per-cycle planner
  envelope are checked before anything changes.
- Respect explicit RAM and VRAM headroom; do not call payload-only accounting
  real physical RSS or device consumption.
- Forbid planner **precision transitions** until an authenticated pathway
  materializes the learned destination representation. Merely reinterpreting
  Boolean bits as 2/4-bit learned weights is invalid.

Planning returns an immutable proposal and does not mutate either controller or
weight pages. Commit first delegates to the planner's exact-generation and
source-state validation, then advances EAC state **only on successful commit**.
Stale plan replay, unknown/duplicate candidates, unachievable minimum activity,
precision promotion without materialization, invalid evidence and envelope
overflow fail closed.

The requested percentage is an **upper budget**, not an exact active proportion.
If the MOR candidate pool contains too few eligible pages, the selected count may
be lower; if it cannot reach the declared minimum EAC returns an error. Pages are
indivisible in v1. 100% activation requires full eligible coverage and sufficient
physical budgets; it cannot be assumed by setting max_active_ppm=1000000.

## Reproducibility controls

The unit suite tests read-only planning, exact commit accounting, adaptive
expansion 25%/50%/75%/100%, recurrence deepening, stability contraction,
pressure overrides, stale replay, headroom budget rejection, invalid candidates,
invalid evidence, and refusal of unmaterialized precision transitions.

## Still required before model qualification

1. Connect MOR-H2 actual shortlist telemetry and SBG ambiguity observations to
   EAC via a non-oracle native inference pathway.
2. Integrate the chosen active ANF bank and recurrent executor; EAC v1 only
   produces budgets and a planner transition and does **not** execute inference.
3. Measure read/write traffic, page faults, physical RSS/VRAM, prefetch, switching
   overhead, cold/warm token latency and power on frozen hardware.
4. Train the model at multiple active fractions and depths. At a fixed learned
   model and held-out tasks, compare fixed 0.1%/1%/10%/50%/100%, adaptive EAC,
   and budget-matched random/constant policies; record quality versus cost.
5. Calibrate thresholds without test-set feedback, evaluate robustness,
   abstention and cross-language French/English conversational quality.
6. Only after evidence: consider precision promotion with learned weights and
   a versioned ElasticXxx actuation adapter. No promotion to production based on
   a routing-count win alone.

Model plan: https://github.com/Memorithm/SML-GENIUS/blob/main/docs/ROADMAP.md
