# SML architecture decision gates

Date: 2026-10-07. Status: active validation programme; architecture not frozen.
This records the user-approved sequence. Engineering targets remain targets.
Current evidence and failures are retained in the off-main ecosystem roadmap.

| Gate | Required evidence | Decision |
| --- | --- | --- |
| A1: shared ANF inputs/outputs | Versioned coefficients, schedules, truth and complete query outputs; independent oracle; deterministic replay; corruption/invalidation controls | Freeze a common comparison endpoint |
| A2: task execution | Same fixture and full outputs for direct sparse/submask, Pascal CPU, native SIMD and optional GPU; activation and query costs declared; resident lookup separate | Select eligible execution candidates per workload |
| A3: physical confirmation | Separate native Thor/Dell cohorts; frozen ordering/repetitions/processes/caps; complete correctness; controlled GPU admission through RemoteOps; latency distribution, memory and transfer accounting | Establish measured hardware-specific benefits and regressions |
| A4: recurrent integration | Complete routing/state/memory/output loop; varied recurrent depth and sequence lengths; coefficient update/invalidation; exact reference equivalence for execution-only substitutions | Establish destination correctness and end-to-end cost |
| A5: learning | Small end-to-end trained candidate; frozen train/validation/test partition and task; multiple fixed seeds; learned/active parameter and training-cost accounting | Establish learning and held-out utility |
| A6: architectural ablations | With/without ANF, bank dimensions, recurrence and memory variants under matched data/budgets; preregistered primary metric and acceptable regression margins | Select useful components and dimensions |
| A7: scaling and autonomy | Progressive capacity ladder; end-to-end quality/latency/throughput/RAM/VRAM/page traffic; internal Rust implementation requalification without mandatory partners | Freeze one version for a declared capacity and deployment envelope |

## Current first slice

[PVP6 fixture contract](SML-PVP6-ANF-FIXTURE-CONTRACT.md) defines a 2048-address,
16-gate PB8 reference. It is synthetic and is not a trained-model export.
A1 requires both this reference and a later declared integration/learned-bank
fixture; completing the reference does not close the whole gate.
A2's native SIMD/GPU timing and A3's performance confirmation remain open.
The existing PB8 scalar benchmark is historical evidence, not the new
complete-output timing endpoint.

## Rules before each experiment

Freeze code/data identities, comparison lifecycle, seeds, primary metric,
acceptance/rejection margins, repetitions, ordering and resource caps before
collecting selection evidence. An exact execution substitution must match all
declared outputs; architectural/training changes require quality evaluation rather
than equality to the old model. Do not choose thresholds after seeing results.
Retain failed, slower, unavailable and admission-rejected panels with their reasons.
Separate exploratory results from confirmatory evaluation; select thresholds on
development data and assess them on held-out workloads.

A component failing its quality gate remains rejected even if its kernel is fast.
The closed PGF2 global/block-local SBG representation-mixing result stays closed:
ANF function evaluation does not promote Pascal as a general SBG mixer.
No blanket SBG/MOR/Delta-KV promotion follows from PVP.

## Ownership and implementation

SML owns semantics and the architecture decision. SciRust qualifies SIMD,
FLAT qualifies portable kernels, NNIS qualifies optional sessions.
Shared fixtures and exact outputs are the first reusable contribution to those
partners; adapters must be independently requalified in each destination.
RemoteOps is the reference control plane for remote physical campaigns.
Final SML is self-sufficient, with Rust CPU and optional WGPU/WGSL execution;
only the installed GPU driver may be vendor-specific.
SML-HARNESS is a separate future orchestration surface.

A definitive version means a versioned, validated architecture for the declared
task/capacity/hardware envelope. It does not claim a universal winner.
