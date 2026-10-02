# DKV-3a-R1-A2 — complementary projection and interference

Status at creation: prospective Development protocol, committed before this
implementation and its executions. Base: `ffb34484d9c95848e016aa34a3348666a92e7dca`
(PR #100). The commit creating this file is its protocol identity. Preserve this
file unchanged after seeing results. A bug fix requires a separate amendment,
retained run identity and disclosure. No new protected/final evaluation.

## Prior knowledge and question

R1-A is known: 96/144 correct for DKV versus 126/144 for direct memory; repeated
reads equal a cached read when both key and matrix are frozen. Its files and
retained outputs must remain byte-identical. The owner's independent counterpart
is not consulted. This is not a blind replication or a learned-language test.

The existing DKV-2a projector folds all V/M/A/R fields, then takes bucket signs.
In the R1-A address construction, most M/A/R coordinates are shared. We ask
whether a paired, complementary projection changes key collisions/interference
and latent correctness, with its extra projection cost explicitly paid. Any
improvement must be distinguished from merely supplying source-derived answers.

## Frozen construction

Retain R1-A's 64-bit fields, V[0..48) address, scratch [48,49], output [50,51],
zero/ambiguous initial scratch/output, two complementary R signatures and actual
`SemanticMorBridge -> MorRouterRegistry -> MixtureOfRouters` Top-1 decision.
The same engine decision is used for both projections of an address. No label,
payload, expected answer, raw text or source identity enters either projection.
The numerical Delta-KV rule and DKV-2b kernel are unchanged.

For an observed address state s, T(s) complements ONLY its V[0..48) bits. M, A,
R, scratch and output are unchanged. Let p(s) be existing DKV-2a projection with
default weights (1,8). Compare:

- `standard`: key p(s), one projection per address;
- `contrast`: key (p(s) - p(T(s)))/2, two projections per address;
- `direct`: existing 32-slot, two-bit BoundedAssociativeMemory using exact
  subject/group identity, ReplaceExisting. Unexpected address collisions fail.

The division is exact for the +/-4096/0 projector alphabet. No extra key
normalization, policy tuning or threshold tuning. This is a deterministic
projection ablation, NOT InfoNCE, CLM training, a new learned encoder or a claim
that common terms cancel exactly through the upstream sign nonlinearity. Log
actual norms, dot products and full keys. Changed norms are a declared possible
confound, not suppressed. Do not infer a probability from Q12 magnitude.

An upstream all-zero projection or all-zero contrast is a rejected projected
key: do not write it, count the rejection, and retain its query as an unknown
latent outcome. Never drop the cell or silently substitute a fallback key.
Other construction/numeric errors are implementation failures, not experiments
to omit. An absent source is not inferred from a large matrix response.

## Paired grid and data boundary

Dimensions [32,64,128], projector seeds [0,1,2], groups [0,1], and all four
payload bit pairs crossed independently. Unlike R1-A, payload is not coupled
to seed. Eight families retain R1-A's definitions: isolated, old, recent,
overwrite, conflict, missing, near_old, near_recent. Target subject is 1;
distractors are subjects 2..8 with the opposite source payload. At most nine
writes per episode. Near addresses flip the same declared two address positions
as R1-A; far addresses use its deterministic mixer. Do not assume an intended
semantic neighbour geometry survives projection.

Overwrite explicitly replaces the old target payload; conflict retains two
inconsistent observations without a supersession rule; missing has no target.
The oracle expects unknown on conflict/missing. All source facts and write
order are supplied equally to the three arms. Direct latest-only conflict
blindness remains a declared weak point of that control, not evidence against
all direct memories. A source-history resolver would be a separate mechanism.

There are 3*3*2*4*8 = 576 episodes per arm and 1728 retained result rows. All
configurations are Development. Dimensions/seeds reuse the same synthetic
families; no statistical-significance or language-generalization claim.

## Frozen inference and authority

Use existing DKV-2b with threshold 128 and a cached first read. A wrapper feeds
the prior cycle's scratch into the same hand-authored two-stage circuit as A:
page 0 returns XOR/AND; page 1 returns OR/XNOR. Unknown scratch yields unknown
output. Adaptive halting min 1/max 4, zero Hamming threshold, no within-episode
writes or re-projection. Run a fresh-read control for software equivalence,
not as a separate headline-quality arm. Verify fixed key and unaffected fields.

No source receipt, raw resolver, gold answer or history-membership gate enters
latent inference. This isolates projection/memory, NOT source-safe answering.
Outputs are research observations, not accepted sourced facts or action grants.
PR #99 source verification and the original context-promotion boundary remain
unchanged. Zero ambiguity is not calibrated confidence. No DKV-2c re-keying.

## Accounting and diagnostics

For each configuration retain: complete projected write/query keys; dot products
and squared norms; exact cross-subject projected aliases (separate from repeated
writes to the same subject); query aliases; rejected write/query keys; latent
correctness, wrong-known outputs, unknowns; scratch Q12 values and ambiguity;
cycle trace and halt reason; native MOR call count; actual projector coordinate
and bucket counts; key-subtraction coordinates; ingestion prediction MACs,
weight updates, saturations; query reads/MACs; matrix bits and direct memory bits.
Direct projection counters are zero. Each arm is constructed/executed serially;
report the per-DKV-arm matrix clone required by the owned DKV-2b API. Metadata,
retained report, Vec spare capacity, source history and true RSS/latency are not
included in matrix-bit counts. Do not turn logical counts into speedup claims.

The experiment produces unfiltered rows and per-family/arm summaries, plus
paired contrast-minus-standard counts and projections. Compare correctness at
identical task information, NOT at equal total compute: contrast pays two
projector evaluations. Source verification is not run or claimed. Key equality
is not evidence of a real source; alias diagnostics are evaluator-side only.

## Hypotheses and checks

H1: contrast improves answerable-case correctness over standard projection.
H2: contrast reduces cross-subject exact projected aliases.
H3: contrast reduces average normalized squared target/distractor correlation
on far addresses. Export exact (dot,norms); zero norms remain separately invalid.
H4: any gain survives on both old and near_old, not only recently written keys.
H5: missing/conflict wrong-known outputs may persist without a dedicated
non-oracular uncertainty mechanism; report them separately.
All are reported even if false, unchanged or mixed. No chosen seed is promoted.

Software assertions only: actual MOR, shape/key alphabet, complement involution,
address construction independent of payload, exact complementary arithmetic,
zero-key rejection, unchanged identity/M/R, frozen keys, complete grid, two
identical report reruns, and cached/fresh state-and-trace equivalence. Independent
truth tables score outputs only after recurrence. Do not assert favorable quality.

## Scope, execution and next decision

New Rust example and tests only; no production API, dependency or numeric-core
change. RemoteOps owns execution; exact source/protocol hashes, Rust version,
runner identity and output hash are retained. Two Cargo build jobs, 20-minute
workflow cap, CPU only, <=16 records per construction and max four cycles.
These are bounded test inputs/task controls, not a hostile-code sandbox or RSS
limit. No shared working tree or running service is modified.

Keep all original A code/evidence immutable for comparison. A2 may reject the
projection hypothesis; do not retune and overwrite the original panel. Further
learned routing, projected-key construction, uncertainty or replay research
requires a separately frozen next protocol. SML-CLM remains a separate programme.
Candidate A and A2 remain individually identifiable for the owner's comparison.
Context promotion stays BLOCKED regardless of CI/Development scores: no physical
resource, learned-model, source-safe context or independent confirmation result.
Reusable conformance/telemetry lessons may later inform SciRust/TDI/ElasticXxx/
Forge/CCOS after destination-contract review, not automatic dependency changes.
