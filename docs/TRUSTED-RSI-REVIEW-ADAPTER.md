# Trusted RSI review adapter v1

`sml-publication::promotion_review` connects the existing dependency-free
`sml-rsi` eligibility verifier to primary GitHub observations and the existing
SciRust-Verify evidence authentication contract. `sml-rsi` stays unchanged and
the model does not depend on the optional operator tool, HTTP client or external
verification owner.

The public review operation independently reads the SML main and selected PR
from the fixed HTTPS GitHub API, then re-reads them after evidence validation.
It accepts only an open, non-draft, same-repository PR against main, with the
pinned repository identity and coherent exact main/head SHAs. Redirects are
disabled, TLS verification remains enabled, responses are bounded, and a read
credential is supplied by the trusted operator. Caller JSON or producer URLs
cannot replace these primary observations. A changed main/head fails closed;
the resulting receipt is a snapshot and must be recomputed for a later review.

## Independent policy and signed record

Place the policy, owner executable and allowed-signers outside the producer
dossier, with trusted operator filesystem ownership. The policy is strict JSON:

```json
{
  "schema_version": 1,
  "repository": "Memorithm/SML-GENIUS",
  "pull_request": 560,
  "evaluators": ["<independently admitted evidence evaluator>"],
  "verifier_sha256": "<trusted owner executable SHA-256>",
  "allowed_signers_sha256": "<approved public trust bytes SHA-256>"
}
```

Use the shared SciRust-Verify authentication owner; no new signature algorithm,
trust service or merge controller is implemented. A required owner custom check
named `rsi-promotion-review` emits the strict
`sml.rsi-trusted-review@1.0.0` review JSON. Its sealed stdout record is
`logs/custom-rsi-promotion-review.out.log`. The owner verifies its detached
signature under the independently pinned ACL and the adapter rechecks the exact
authenticated bundle/record bytes. The record binds repository/PR, parent/head,
branch, class/objective, budgets, rollback, typed evidence references, metric
comparisons and known regressions.

Evidence entries have name, kind, role (`selection`, `protected`, `auxiliary`),
relative path, full metadata SHA-256 and locator. Below `aggregates/`, each
strict record contains only schema version 1, scope (`development_fixture` or
`aggregate_only`), candidate head, kind, original `source_evidence_sha256`, and
validation passed/total counters. The counters describe successful validation
checks, not model accuracy. Raw examples, labels and arbitrary extra fields are
not accepted by this metadata schema; the adapter does not retrieve raw domain
or final-holdout payloads. The evaluator remains responsible for the truth,
protocol qualification and original-payload identity of its signed assertions.
Duplicate original evidence identities are refused even when wrapped metadata
or partition labels differ. Mixed fixture/real scopes, hash drift, traversal,
symlinks, unknown kinds/roles and incompatible/unsuccessful records fail closed.

The adapter constructs `RsiCandidateManifest` and `PromotionReviewContext` from
those authenticated records and independent observations, then invokes the
existing baseline verifier. Original selection/protected separation, hard-gate
regressions, required evidence and rollback rules remain authoritative. It
never invokes a protected evaluator, changes a study or selects a model.

```sh
GITHUB_TOKEN=<operator read credential> cargo run --locked -p sml-publication \
  --bin sml-promotion-review -- DOSSIER RUN OWNER POLICY ALLOWED_SIGNERS
```

An opaque receipt can be serialized for review, but only reports structural
eligibility. Its scientific qualification is `not_evaluated` and merge
authorization is always false. Authenticating aggregate assertions does not
re-execute their domain validation, establish protected-data independence or
authorize scientific/model promotion. The existing scientific, human and final
evaluation gates remain necessary. The optional tool has no merge operation.

## Qualification scope

The dedicated CI gate uses a real primary-provider read credential and the
pinned owner at `662a30dc774dd19c68a79bed3889036a91930123`. On the exact candidate
it makes a temporary independent Development evaluator and four synthetic
aggregate records, authenticates them, performs two live main/head observations,
and reuses the native verifier to produce `EligibleForHumanReview` with no merge
authority. Wrong evaluator, altered aggregate and missing signature are refused.
No actual model or protected labels are evaluated by this fixture.

On the integrated commit, the same provider client resolves its actual merged
PR and verifies its closed/merged identity, then refuses it as a new promotion
candidate. Existing owner-authentication/RAM tests and the full RSI contract
tests also run on the integrated source. Local tests qualify the bounded parser
and native consumer; the live private GitHub path is qualified by the dedicated
hosted gate, not inferred from local fixtures or code compilation.
