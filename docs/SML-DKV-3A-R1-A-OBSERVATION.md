# DKV-3a-R1-A — Development observation and comparison handoff

Status: completed bounded Development run, NOT full context or model qualification.
No comparison with the owner's forthcoming code has been made.

## Reproducible identities

- Base after source hardening: `9a8828ba3f009822f040b01d6e1dca05cc85489e`.
- Protocol creation before implementation/execution: `d7bf16fb879dc9f822522a2c1d7173fb2f8d42e4`.
- Executed Rust code: `7c00952e4ab49141ab45f7d91b13d61363382fa2`.
- Evidence commit: `37bdd599de7b679b1a8351c834a2f1cbf200ce96`.
- RemoteOps run: [37063200143](https://github.com/Memorithm/RemoteOps/actions/runs/37063200143), success.
- Observed host: `tarek`, `aarch64`, Rust/Cargo 1.89.0.
- Full evidence: [`research/dkv-r1a/development.csv`](../research/dkv-r1a/development.csv).
- SHA-256/code binding: [`research/dkv-r1a/MANIFEST.txt`](../research/dkv-r1a/MANIFEST.txt).
- Output SHA-256: `f2fbbf61edaadb2ed5f110f9e8e0dbeaac5081dfe1a4617d684298b3508cabff`.

The first execution required no experiment-driven fix or parameter retuning.
Formatting and relocating a test-only import were committed before compilation
and before any execution. Two explicit complete report reruns were byte-identical;
the conformance suite also checks determinism. Later documentation commits do
not change the executed code or protocol. The manifest, not a moving branch name,
is the reproduction identity.

## What actually runs

An observed address-only SBG state goes through SemanticMorBridge, the native
MorRouterRegistry and MixtureOfRouters, then the existing DKV-2a projector.
MOR-selected Page 0 and Page 1 execute different declared two-input operations.
The DKV-2b kernel reads a fixed key into scratch coordinates; the same hand-authored
expert consumes the previous cycle's scratch in every arm. No payload is supplied
to the address constructor. No learned encoder, trained expert, text decoder or
natural-language understanding is claimed.

The hardening layer independently verifies raw identity/provenance. Its answer
is never substituted for the measured latent result. A separate shadow matrix
exists for this instrumentation and is counted, not disguised as a deployment
memory saving. Exact storage/element counts are not process RAM measurements.

## Complete aggregate result

Each arm has 144 episodes: eight families x two routing groups x three projector
seeds x three key dimensions. The total is 864 rows, not 864 independent tasks.
Correct unknown outputs count as correct on conflict/missing cases. `wrong_known`
means an incorrect non-ambiguous latent output, NOT calibrated confidence.

| Arm | Correct / 144 | Wrong-known | Unknown | Total cycles | Matrix query reads | Query MACs |
|---|---:|---:|---:|---:|---:|---:|
| no-memory adaptive | 36 | 0 | 144 | 576 | 0 | 0 |
| direct adaptive | 126 | 18 | 18 | 450 | 0 | 0 |
| direct fixed | 126 | 18 | 18 | 576 | 0 | 0 |
| DKV adaptive | 96 | 48 | 0 | 432 | 432 | 64,512 |
| DKV fixed | 96 | 48 | 0 | 576 | 576 | 86,016 |
| DKV cached adaptive | 96 | 48 | 0 | 432 | 144 | 21,504 |

The no-memory ablation always remains unknown; its 36 correct outputs are the
unanswerable cases, not retrieved facts. Direct memory retains only the latest
payload per exact identity and is deliberately blind to historical contradiction
inside its latent path. Source-level conflict resolution is a separate control.
Zero matrix MACs in a direct/no-memory arm does NOT mean zero compute: direct
reads, expert operations, state copies and authority work are separate.

## Interpretation of the frozen questions

H1: memory provides useful signal relative to the no-memory ablation on this
synthetic panel. It does not establish learned reasoning or language quality.

H2: non-inferiority to direct adaptive memory fails; DKV has 96 correct episodes
versus 126. Do not promote the mechanism because some isolated/recent cases work.
The exact-address direct control is intentionally strong and its input identity
is available to both representations; capacity and representation are different.

H3: fresh and cached adaptive DKV have identical final states AND recurrent
traces in all 144 paired episodes. The cached control uses 144 versus 432 reads
and 21,504 versus 64,512 query MACs. That is two-thirds fewer query reads/MACs,
NOT a measured threefold end-to-end speedup. The cached implementation still
retains a matrix and performs the same synthetic expert/halting work.

H4: fixed four-cycle DKV produces the same final states as adaptive DKV while
using more reads. This is expected for a frozen key, static matrix and converged
deterministic expert. The extra cycles propagate/confirm a signal; they cannot
introduce new source information under this contract.

H5: latent wrong-known outputs remain visible (48 for adaptive DKV); the source
verification path is neither an oracle supplied to the recurrence nor a repair
of its score. Individual source outcomes, actual keys, dot products/norms and
collisions remain in the full CSV. Inspect those paired rows before attributing
errors to a particular family or key geometry.

## Software evidence and limits

Nine new conformance tests cover actual MOR pages, projection/provenance
separation, address-only construction, isolated full truth-table behavior,
fresh/cached state and trace equivalence, source/latent isolation, typed
conflict/overwrite/missing controls, actual projected geometry, input rejection
and the deterministic full grid. One test may check several of these contracts.
The complete workspace, format/check/Clippy and lockfile checks ran through
RemoteOps. The ordinary PR CI must also pass on the final head before merge.

The panel is development evidence, not an untouched confirmation. Seed/dimension
repetitions use the same tiny task families. Shared M/A/R structure can strongly
correlate projected keys; the observed geometry is exported, not assumed uniform.
The two-stage expert is hand-authored and the memory/projector is not trained.
Source checks establish byte identity against the admitted ledger, not source
truth, authenticity, entailment or permission for an external action.

## How to confront the independent implementations

Use these exact code/protocol identities rather than merging both proposals
before review. First compare information available to each address, query, router,
recurrent step and evaluator. Then compare actual MOR admission, projected-key
construction, frozen/re-keyed semantics, authority boundaries and stopping.
Only compare quality after task/input budgets, source access and controls match.

The shared comparison should retain: no-memory/direct controls, fresh/cached
reads, fixed/adaptive depth, old/recent and conflict/missing cases, actual key
geometry, and separate latent/source outcomes. A different recurrence must state
what fresh information or state-dependent transformation changes after cycle one.
Do not borrow this panel's observed results to claim a new blind validation.

The natural reusable engineering result is conditional caching under immutable
matrix/frozen-query semantics; no such optimization is automatically propagated
to changing-key or changing-memory systems. Destination contracts and independent
validation are required for ecosystem reuse.

Context promotion remains **BLOCKED**. Physical resource qualification, durable
sidecar recovery, representative tasks, learned representations and independent
confirmation remain open. This candidate is software/research infrastructure,
not a promoted 25B model or an answer to those open questions.
