# DKV-4h — learned expert-applicability observation

Status: Development observation under the frozen protocol in
`SML-DKV-4H-LEARNED-APPLICABILITY-PREREGISTRATION.md`.

Protocol commit: `ae67b3ab11b4951b535b83011211deaa67971be5`.
Protocol merge commit: `9f9fd9db29bcc3152b8e9f130d4f645a18a1e622`.
Executed code: `6d6f3037c2e05e236bf2dcfa488eb1133b729078`.
Retained-evidence head before this observation:
`849e61f6e4fa01c31fe93476a2f4623f255ac7b6`.
RemoteOps run: `37232292279`.
Host: `debian`, x86_64. Rust: 1.89.0.
Output SHA-256:
`d23c1d9ecd7918a0c750aad3b008642ebdc4387da48eed985e6674117794d264`.

The complete retained output is in `research/dkv-4h/`. Two explicit
executions were byte-identical. No candidate feature set, split, MOR score,
truth-table threshold, recurrent budget or classification rule was changed
after observation.

## 1. Training / development separation

The page-specific applicability learner uses the existing bounded
`search_feature_pair_expert` path.

Per page:

- training records: 64;
- development records: 64;
- candidate semantic features: 8;
- candidate pairs: 28;
- bounded training selection evaluations: 28,672;
- executed training selection evaluations: 28,672.

The declared record-id ranges are disjoint:

- Page0 training: 10,000-10,063;
- Page0 development: 20,000-20,063;
- Page1 training: 30,000-30,063;
- Page1 development: 40,000-40,063;
- routed test rows: 50,000-50,127.

Development is read only after training has frozen a feature pair and truth
table. It may reject that proposal but does not select a replacement.

## 2. Both training searches recover the preregistered hidden pair

For PageId(0), training selects:

- slots [3,5];
- `ContextValue(7)` and `ContextValue(13)`;
- truth-table code 9, i.e. XNOR;
- training errors: 0;
- development errors: 0;
- development missing selected features: 0;
- development input-pattern coverage: 0b1111.

For PageId(1), training selects the same two semantic coordinates with
truth-table code 6, XOR, also with zero training/development errors and complete
development input-pattern coverage.

The deliberately weak fixed baseline produces 32/64 errors on both training and
development for each page.

Therefore H1 and H2 are supported on this finite Boolean identification task.

## 3. Untouched routed test result

The routed test consists of 64 metadata records crossed with STABLE and
CORRECT_ONCE recurrence states: 128 scored rows.

The frozen generator creates exactly:

- 64 aligned routed rows;
- 64 deliberately misaligned routed rows.

All 128 retained rows exhibit the preregistered native MOR geometry:

- first route margin = 40,000 ppm;
- first selected-expert count = 2;
- second route margin = 70,000 ppm;
- second selected-expert count = 1.

No retained row violates that geometry.

### Unguarded dynamic route

| metric | result |
|---|---:|
| answered | 128/128 |
| correct answered | 64 |
| errors | 64 |
| selective risk | 50% |
| recurrent cycles | 256 |
| matrix reads | 256 |

Every deliberately misaligned route becomes an answered error, reproducing the
control problem identified in DKV-4f.

### Dynamic MOR + learned applicability

| metric | result |
|---|---:|
| answered | 64/128 |
| coverage | 50% |
| correct answered | 64/64 |
| errors | 0 |
| selective risk | 0% |
| recurrent cycles | 256 |
| matrix reads | 256 |

The learned gate preserves all 64 aligned proposals and suppresses all 64
misaligned proposals. It never substitutes the output of another expert.

The aligned subgroup is 64/64 answered and correct. The misaligned subgroup is
0/64 answered; its selective risk is therefore undefined, not empirically zero.

## 4. FIRST_PRIMARY remains the matched cheaper control

The exact same learned page-specific gate is applied to the first native MOR
primary after the simpler stability-only recurrence.

Result:

| metric | dynamic learned | FIRST_PRIMARY learned |
|---|---:|---:|
| answered | 64/128 | 64/128 |
| correct answered | 64 | 64 |
| errors | 0 | 0 |
| selective risk | 0% | 0% |
| recurrent cycles | 256 | 192 |
| matrix reads | 256 | 192 |

FIRST_PRIMARY therefore preserves the same coverage/error profile while using
64 fewer recurrent cycles, a 25% reduction relative to dynamic MOR on this
panel.

The learned applicability boundary is the component associated with removing
the deliberate route-mismatch errors. The additional dynamic MOR resolution
still provides no task-quality advantage under the same gate.

## 5. Missing metadata and provenance fail closed

Sixteen inference-only safety rows hide observation of one selected
applicability feature. All 16 abstain. There is no fallback to MOR margin,
expert identity, a default Boolean value or the task oracle.

For both favored pages:

- Missing provenance -> zero matrix reads;
- Conflict provenance -> zero matrix reads;
- Unverified provenance -> bounded depth 4 and
  `Abstain(UnverifiedEvidence)`, never StopCandidate.

The learned gate does not weaken the existing provenance boundary.

## 6. Frozen hypotheses

- **H1 holds.** Both page-specific training searches are accepted by
  development without retuning.
- **H2 holds.** The selected pair is [3,5] / V[7],V[13] for both pages; Page0
  selects XNOR (code 9) and Page1 selects XOR (code 6).
- **H3 holds.** DYNAMIC_UNGUARDED answers all 128 rows and is wrong on the 64
  deliberately misaligned rows.
- **H4 holds.** DYNAMIC_LEARNED answers the 64 aligned rows, suppresses the 64
  misaligned rows and has zero errors among answered rows.
- **H5 holds.** FIRST_PRIMARY_LEARNED has the same coverage/error profile at
  lower cost: 192 versus 256 cycles.
- **H6 holds.** UNIVERSAL_ABSTAIN has zero coverage and explicitly undefined
  selective risk.
- **H7 holds.** All 16 missing-metadata rows abstain.
- **H8 holds.** Missing/Conflict remain zero-read; Unverified never
  StopCandidate.
- **H9 holds.** All 128 scored rows retain the native 40,000 -> 70,000 ppm MOR
  transition with 2 -> 1 selected experts.

## 7. Interpretation

This is stronger than DKV-4g only in one narrow sense: the applicability rule is
not supplied directly at inference. Training chooses a bounded feature pair and
Boolean table, development may reject it, and the routed test set is not used
for fitting.

It is still **not a generalization result**.

Both page-specific training sets cover all four Boolean combinations of the
hidden applicability features. The learned function class contains all 16
two-input truth tables. The untouched test therefore measures split hygiene,
feature-selection robustness under the frozen nuisance shift, routing
composition and fail-closed behavior. It does not demonstrate extrapolation
beyond the finite function class, language understanding or calibrated risk.

The next stronger experiment should use representative learned semantic states
and a genuinely held-out task distribution where applicability cannot be
reduced to a completely enumerated two-bit truth table. That experiment must
preserve train/development/test separation and compare dynamic MOR against
FIRST_PRIMARY under the same verifier.

No context/model promotion follows.

**Scientific/model promotion remains BLOCKED.**
