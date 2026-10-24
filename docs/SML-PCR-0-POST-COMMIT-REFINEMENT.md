# SML-PCR-0 post-commit refinement (design note)

Status: **proposal**. Nothing in this note is implemented or measured. It makes
no quality, speed, memory or scaling claim. Every mechanism below is a
hypothesis until a preregistered test on a research branch says otherwise.

Résumé (FR) : après le COMMIT d'une réponse, le modèle peut poursuivre une
« réaction en chaîne » bornée pour mieux comprendre la requête et réviser sa
réponse. Trois niveaux séparés (Modèle, Mémoire, Harness), vérificateurs
externes obligatoires, réponses révisées toujours signalées, jamais remplacées
en silence. Plan PCR-1 → PCR-3 avec contrôles et critères d'acceptation.

Audience: AI coding agents (GPT, Claude, Grok, others) and humans who may
contribute. Read `AGENTS.md`, `CONTRIBUTING.md` and
`docs/SML-MODEL-HARNESS-BOUNDARY.md` first.

## 1. Idea

When a user queries the model, the answer is produced and COMMITted as today.
Afterwards a bounded, optional chained reaction may continue: the model
re-reads (query, committed answer), keeps working on its internal state, and
may produce a **revised** answer. Verified corrections can be stored for reuse
without retraining, and, outside this repository, (first, refined) pairs can
become candidate training data.

## 2. Goals and non-goals

Goals:

- Define a post-commit refinement mechanism that can be tested on toy tasks in
  `HybridLogicEngine` before any larger use.
- Keep it inside existing budgets (active compute, memory, cycle count) and
  fail closed.
- Separate Model, Memory and Harness responsibilities per
  `SML-MODEL-HARNESS-BOUNDARY.md`.
- Make every improvement claim depend on an external verifier and matched
  controls.

Non-goals:

- No claim that refinement improves answers; that is the question PCR-1 asks.
- No online self-training inside the model repository.
- No silent replacement of a committed answer.
- No Q/K/V attention, softmax token scoring, or Transformer KV cache (see §7).
- No dependency on SML-HARNESS for PCR-1 or PCR-2.

## 3. Three levels

### 3.1 Model level (SML-GENIUS): post-commit recurrent cycles

The HLE engine already runs `min_cycles..=max_cycles` recurrent workspace
cycles, then COMMITs (see `SML-HLE-0-HYBRID-DIFFERENTIABLE-LOGIC.md` on the
HLE research branches). PCR adds a second, separate phase:

1. COMMIT the first answer `A0` exactly as today. `A0` is always returned or
   recorded first.
2. Compute **uncertainty triggers** from signals that already exist or are
   cheap:
   - hard/relaxed disagreement (as measured in HLE-1: disagreement and mean
     `|p - hard bit|`);
   - route instability (selected page set changes across the last cycles, or
     Hamming-distance margin to the next page below a threshold);
   - workspace non-convergence (max cycles reached without a fixed point).
3. If no trigger fires, stop. Otherwise run up to `pcr_max_cycles` extra cycles
   whose input is (query bits, `A0` bits, workspace), producing `A1`.
4. Return `A1` only as a flagged revision alongside `A0`, with the triggers,
   cycle count and budget used.

Constraints:

- `pcr_max_cycles` and the active-page/memory budget are hard limits; exceeding
  one stops refinement and keeps `A0` (fail closed).
- Invalid configuration (zero budget with refinement enabled, width mismatch,
  etc.) is an error, not a silent fallback.
- Deterministic given inputs, parameters and seed; the run records them.

### 3.2 Memory level (SML-GENIUS): verified write-back

Only **verified** corrections (see §5) may be written to a bounded associative
memory (e.g. an existing atlas/page or Delta-KV-style store, not a Transformer
KV cache) keyed by a Boolean signature of the query.

- Each entry carries provenance: query signature, `A0`, `A1`, verifier id and
  version, model/parameter hash, timestamp, seed.
- Fixed capacity with an explicit eviction policy (e.g. LRU or lowest verified
  utility), tested.
- Lookup is Hamming-nearest with a distance threshold; a miss means normal
  inference.
- Entries can be listed, audited and deleted; deletion is total.
- No user content is stored unless the deployment explicitly enables it (§4).

### 3.3 Harness level (SML-HARNESS, separate repository)

(first, refined) pairs plus verifier verdicts may be exported as **candidate**
training data. Promotion into any training set or weight happens only in
SML-HARNESS, after held-out evaluation, with rollback to the previous
checkpoint. SML-GENIUS defines at most the export record format; it does not
implement promotion.

## 4. Risks

| Risk | Mitigation required |
|---|---|
| Self-reinforcing errors: the model "confirms" its own mistake without an external check | No write-back or promotion without an external verifier; measure degraded fraction, not only improved |
| Reward hacking: refinement learns to satisfy the verifier or trigger, not the task | Verifier independent of training signal; held-out tasks; audit samples where triggers change |
| Latency and compute | Hard cycle and active-compute budgets; report added cycles and pages per query |
| User transparency | Revised answers always flagged and shown with the original; never silent replacement |
| Privacy of stored interactions | Memory write-back off by default for user data; provenance, retention limits, deletion |
| Determinism / reproducibility | Seeds, parameter hashes and budgets recorded; same input ⇒ same `A0`, triggers and `A1` |
| Boundary drift | Training/promotion logic stays out of SML-GENIUS |

## 5. Required verifiers and gates

- **Unit tests**: triggers fire on constructed cases and not on converged,
  agreeing cases; budgets stop refinement; fail-closed paths; determinism.
- **Task verifiers**: on toy tasks the ground-truth function is the verifier
  (exact truth table). Larger tasks need a verifier that is not the model
  itself (tests, exact checkers, consistency checks against known constraints).
- **Consistency checks**: `A1` must not contradict facts that `A0` got right
  under the verifier; count regressions explicitly.
- **Held-out evaluation**: improvement must hold on held-out seeds/tasks not
  used to tune thresholds.
- **Gates**: a phase is accepted only if its acceptance criteria (§6) hold,
  CI is green on the exact PR head, and the owner approves.

## 6. Phased plan

All phases live on research branches (e.g. `feat/pcr-1-...`) until the owner
decides otherwise. Preregister thresholds, seeds and controls in the phase
document **before** running.

### PCR-1 — toy prototype in `HybridLogicEngine`

Scope: post-commit cycles on HLE toy tasks (parity, small truth tables).

Arms (same tasks, seeds, budgets):

- `no_refine`: `A0` only (control);
- `random_extra`: same number of extra cycles, triggered at random at the same
  rate as `pcr` (control for "more compute");
- `always_extra`: extra cycles on every query (control for the trigger);
- `pcr`: trigger-gated refinement.

Report per arm: fraction improved, fraction degraded, fraction unchanged
(vs exact verifier), added cycles/pages, trigger rate, with seed counts.

Acceptance (to preregister numerically): `pcr` improved − degraded > 0 and
better than `random_extra` at matched compute on held-out seeds; degraded
fraction reported; determinism test passes. A negative result is a valid,
publishable outcome and must be documented as such.

### PCR-2 — associative memory write-back

Scope: write verified `(signature → A1)` entries to a bounded store; reuse on
repeated or near queries.

Controls: no-memory; memory with unverified writes; memory with shuffled
entries. Evidence: hit rate, accuracy on repeats vs held-out non-repeats,
eviction behaviour at capacity, provenance completeness, deletion test.
Acceptance: verified-write arm beats no-memory on repeats without degrading
non-repeats; unverified-write arm is shown (expected to be worse or equal).

### PCR-3 — harness integration (SML-HARNESS)

Scope: export record format here; promotion pipeline there.
Evidence: held-out evaluation before/after promotion, rollback exercised in a
test, no regression on frozen suites. Not started before SML-HARNESS exists.

## 7. Repository contract compliance

PCR adds no Q/K/V projection, no softmax token scoring or gate selection, no
dense score matrix and no Transformer KV cache. Re-reading the committed
answer is done by feeding its bits into the existing Boolean routing and
recurrent workspace. Associative memory uses Hamming/Boolean signatures.

## 8. Instructions for contributing agents

Rules:

1. Follow `AGENTS.md` (read the ecosystem roadmap; fail closed if unreadable)
   and `CONTRIBUTING.md` (install hooks; fmt, clippy, tests; never
   `--no-verify`).
2. Work on a research branch; one significant change per PR; fill the PR
   template, naming which test calls which new code.
3. Claim nothing beyond what tests and preregistered runs show. State host,
   seeds and that toy results are not Dell/Thor evidence.
4. Always keep the controls (`no_refine`, `random_extra`, `always_extra`); do
   not drop an arm because it looks unfavourable.
5. Report degraded answers as prominently as improved ones.
6. Never merge without the owner's explicit approval; required CI must be green
   on the exact PR head.
7. Keep promotion/training logic out of this repository (Harness boundary).
8. Name follow-up docs `docs/SML-PCR-<n>-<TOPIC>.md`.

Open questions:

- Which trigger signal, alone or combined, best predicts a fixable answer?
- Should extra cycles reuse the same pages, or a dedicated refinement page set?
- How is `A0` encoded into the input bits within the 64-bit HLE limits?
- What verifier is acceptable beyond toy tasks where no exact checker exists?
- What memory store is the right base (atlas pages vs Delta-KV-style store)?
- How should revisions be surfaced to users in the eventual serving API?
