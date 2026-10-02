# DKV-3a-R1-A2 — retained Development observation

Protocol: `SML-DKV-3A-R1-A2-PREREGISTRATION.md`, committed at
`473b3f48bccd594dd2b39a8903f315f0ec9d5ff5` before implementation/execution.
Executed example: `37aed3c34e014fe0cda78e7264440b592ef58e78`.
Evidence commit: `11afae8b0254208e79563323a1e9f16586b44575`.
RemoteOps run: https://github.com/Memorithm/RemoteOps/actions/runs/37065074816
Host `tarek`, aarch64, Rust 1.89.0. Two explicit full executions produced
byte-identical output SHA-256
`084b3de117f5ad1859528e1dce237618fa1bdd1e8bea9fec875783a042e04448`.

All 1728 rows are in `research/dkv-r1a2/development.csv`; its compact projection
is `research/dkv-r1a2/SUMMARY.csv`. Tests added after the run validate replay,
summary extraction and paired row integrity, not a newly selected quality goal.
The original A example, tests, preregistration and evidence are unchanged.

## Results and denominators

576 episodes per arm: 432 answerable and 144 requiring an unknown output
(72 missing and 72 contradictory). The four payloads are crossed independently
of the three projector seeds; these repeated synthetic cases are not 1728
independent real-world questions.

| arm | correct overall / 576 | correct answerable / 432 | wrong-known | unknown |
| --- | ---: | ---: | ---: | ---: |
| standard DKV-2a | 360 | 360 | 216 | 0 |
| complementary contrast | 372 | 368 | 192 | 16 |
| exact direct memory | 504 | 432 | 72 | 72 |

A correct unknown counts in both 'correct' and 'unknown'; these columns are
not mutually exclusive. Contrast has 368 correct known outputs, 192 wrong
known outputs and 16 unknowns. Four of those unknowns are correct missing
answers; twelve lose answerable old facts. Direct has 432 correct known outputs,
72 wrong known outputs (conflicts), and 72 correct missing unknowns.

Overall contrast-minus-standard gain is 12/576 = 2.0833 percentage points
(62.5% to 64.5833%). On answerable cases the gain is 8/432 = 1.8519 points
(83.3333% to 85.1852%). This is a limited aggregate gain, not non-inferiority
on every cell, independent confirmation, or a quality claim for SML language.

| family | standard correct / 72 | contrast correct / 72 | direct correct / 72 |
| --- | ---: | ---: | ---: |
| isolated | 72 | 72 | 72 |
| old | 36 | 44 | 72 |
| recent | 72 | 72 | 72 |
| overwrite | 72 | 72 | 72 |
| conflict | 0 | 0 | 0 |
| missing | 0 | 4 | 72 |
| near_old | 36 | 36 | 72 |
| near_recent | 72 | 72 | 72 |

## Geometry and cost

Cross-subject exact write-key alias occurrences decrease from 288 to 112;
wrong-subject query-key aliases decrease from 152 to 80. These are aggregate
occurrences across repeated payload/family cells, not counts of independent
collision events in a deployed workload. No write or query key was rejected
as zero in this grid. Missing/conflicting sources remain separate from aliases.

The exported mean normalized squared query/distractor correlation on far-address
rows is 1126.895844911181/2016 = 0.558976113547 for standard and
159.430558532380/2016 = 0.079082618320 for contrast. This is mean cos^2, not the
mean cosine, and says nothing about semantic correctness. Every contributing
dot product and pair of norms is retained. Contrast changes norms as well as
angles; this experiment does not isolate angular change from fixed-point
rounding or threshold effects. Sign nonlinearity prevents a claim that the
common SBG fields have been removed exactly.

Both DKV arms perform 548352 ingestion prediction MACs and 86016 query MACs.
They each use 576 fresh query reads, one cached read per episode. However,
contrast invokes the projector 8496 times versus 4248 and reads 2174976
semantic coordinates versus 1087488, plus complementary-state construction
and key arithmetic. The actual MOR call counts remain paired. Direct has no
projection/matrix MACs, but its own routing, memory operations and storage are
not free. No end-to-end timing, physical RAM or throughput comparison is made.

## Preregistered hypotheses

H1 is supported on the aggregate answerable Development panel: 368 versus 360.
H2 is supported for the declared alias-occurrence counters: 112 versus 288.
H3 is supported for the exported mean squared correlation on the fixed far panel.
H4 is NOT supported: old improves in aggregate, but near_old remains 36/72.
H5's anticipated failure mode remains: contrast produces wrong-known outputs
on all 72 conflicts and 68/72 missing cases. Its 128-scale ambiguity policy is
not a calibrated abstention mechanism. Direct latest-only conflict blindness
also remains; this is not a comparison against a provenance-aware direct store.

No hypothesis permits promotion of this latent-only path to source-safe context.
The gain does not match the direct control and does not come at equal compute.
The negative near_old/conflict results are retained, not retuned away.

## Review limits and next independent slice

The kernel sees source payloads only through memory writes/reads. It never sees
evaluator gold, source-history membership diagnostics or a raw resolver. The
code emits a research observation, not an Accept flag. PR #99 receipts and the
source-safe context boundary are not relaxed or reimplemented here.

For the direct arm, the generic KEY query field reads 'rejected' because no
projected vector exists; the authoritative row field query_rejected=0 and
arm=direct mean NOT APPLICABLE, not a direct-memory key rejection. Matrix peak
instances count the setup matrix and owned-kernel clone only; the report,
metadata, projector scratch, recurrent state and allocator costs are additional.
The contrast key is passed to the unchanged DKV-2b evidence container for API
compatibility; its first-call counters are not the aggregate projection cost.
SETUP counters explicitly include both calls. With no zero projections in this
grid, contrast-coordinate counts equal actual subtractions; rejection cases
outside this grid would require separating attempted and executed arithmetic.

Next investigation should isolate two open questions instead of enlarging this
panel until a favorable answer appears: (1) retaining old associations under
near projected keys at matched storage/work, and (2) recognizing missing and
contradictory evidence without evaluator truth. Any such implementation needs
its own prospective protocol. Source verification cannot be counted as an
improvement in latent memory quality. The owner's counterpart remains a
separate code/protocol comparison target. No new cross-repository runtime
dependency or unqualified propagation was introduced.

**Context promotion: BLOCKED.** This is a reproducible software/Development
result, not a trained model, learned recurrence, language benchmark, source
verification experiment or general scaling law. Original R1-A remains frozen.
