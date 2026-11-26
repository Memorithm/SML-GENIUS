# SML-TUTOR massive continual-learning engine — P1 foundation

Status: **software-qualified in a cloud container only**. No physical
qualification on Dell T430, Jetson Thor or any Memorithm runner has been done
for this crate yet. No new French, English, Rust or general mathematical
competence is claimed by this change.

## What exists

New crate `crates/sml-tutor` (std only, plus `sml-core` for SHA-256), with
explicit interfaces and unit tests for each component:

| Mission component | Module | Contract enforced by tests |
| --- | --- | --- |
| Lesson registry, dedup, idempotent registration | `registry.rs`, `lesson.rs` | content-addressed `LessonId`; re-registration is a no-op; acquisition refused until prerequisites are Certified/Consolidated |
| Explicit knowledge states | `domain.rs` | 10 states; whitelisted transitions; `Certified` reachable only from `Validating`, `Consolidated` only from `Certified` |
| Dependency graph (cross-domain) | `graph.rs` | prerequisites must exist (DAG by construction); late edges rejected if they close a cycle; transitive invalidation |
| Parallel scheduler, 9 lanes | `scheduler.rs` | priority queues per lane; weighted deficit round robin (no starvation between disciplines); per-lane backpressure; dedup of queued work; AIMD concurrency limit; controlled cancellation of queued work |
| Executor | `executor.rs` | work stealing across per-worker deques; output order independent of worker count; panic isolation; cooperative cancellation; `workers=1` is a true sequential baseline |
| Resource manager | `resources.rs` | only measured, online `AiCompute` machines get leases; `IosBuildReserved` (Mac mini) and `ControlPlane` are never selected; `Insufficient` is the backpressure signal |
| Teachers + independent oracle | `teacher.rs` | teachers are untrusted; proposals deduplicated by digest; oracle verdicts logged per teacher with bounded rejection reasons; teacher selection by admitted knowledge per second, untried first |
| Examiner (100 % rule) | `mastery.rs` | no threshold parameter; every cohort `correct == total`, `total > 0`, abstention = failure; TrainRecall, Test, Reload and one Retention cohort per prerequisite are mandatory; TRAIN/DEV/TEST/TESTTEST disjointness checked; `Certificate` cannot be constructed outside `certify` |
| Transactional cumulative memory | `store.rs` | only certified entries can be staged; optimistic CAS on version; exclusive LOCK file with owner record (lock of a dead same-host process is recovered, any other lock fails closed); first HEAD written under the lock; idempotent republish; contradiction aborts the whole transaction; crash before the final `HEAD` rename leaves the previous version intact; immutable manifests with parent chain; restore; exhaustive audit re-hashes every object |
| Crash-safe journal and resume | `journal.rs`, `engine.rs` | append-only, sequence-checked, torn tail truncated, corruption fails closed; in-flight lessons are reset and redone on reopen |
| Telemetry | `telemetry.rs` | per-domain and per-machine counters, JSON lines |
| Continual-learning loop | `engine.rs` | OBSERVE → GAP → TEACH → LEARN → EXAMINE → VALIDATE → CONSOLIDATE → REVISE; learning in parallel, publication as one transaction per wave |

Roles are separated in types: the scheduler never sees the examiner or the
store; the examiner is the only source of certificates; the store only accepts
certificate-backed entries.

## Engine-qualification workload

`anf_lesson.rs` is a native GF(2) ANF induction learner: from labelled rows of
an n-input Boolean function of degree ≤ d it solves the GF(2) linear system on
the monomial basis. If the system is under-determined it **abstains** (counted
as failure), which triggers the gap → re-teach loop with twice as many TRAIN
rows. An exact generator oracle re-checks every teacher label.

All lessons of `anf_curriculum` are Mathematics/M2 (Boolean algebra). They are
tagged across the four discipline lanes **only** to exercise lane fairness and
telemetry with identical load. They must not be reported as French, English or
Rust knowledge.

## Measured results (cloud container, 4 vCPU, NOT physical)

`cargo run --release -p sml-tutor --example sml_tutor_engine_bench`
(256 lessons, n = 10, d = 3, 176 unknowns, TRAIN up to 640 rows, DEV/TEST/TESTTEST 128 each):

| workers | consolidated | attempts | gaps detected / resolved | learn phase | speed-up | wall | certified/s | steals |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | 256/256 | 348 | 52 / 52 | 1003.6 ms | 1.000 | 1272.1 ms | 201.2 | 0 |
| 2 | 256/256 | 348 | 52 / 52 | 606.6 ms | 1.655 | 917.5 ms | 279.0 | 20 |
| 4 | 256/256 | 348 | 52 / 52 | 338.1 ms | 2.968 | 698.1 ms | 366.7 | 37 |

All three configurations publish byte-identical store entries. A crash after
two waves followed by automatic resume reset 57 in-flight lessons and reached
the same 256-entry store as the uninterrupted run.

Bottleneck found: the serial part (journal fsync per wave, single-writer
transaction, prerequisite checkpoint reload) costs about 270–360 ms of the
wall time, which caps end-to-end speed-up below the learn-phase speed-up.
Pipelining consolidation with the next wave is the next optimisation.

Single run per configuration; timings are indicative, not statistics.

## Not done yet

* Physical qualification through RemoteOps on Dell T430 (CPU) and the
  measured resource inventory feeding `ResourceManager`.
* Real French/English/Rust learners plugged into the engine (dictionary V1/V2
  shards, L1/L2 role learner, L3 relation graph, rustc as teacher/examiner).
* Linking this store to the existing V9 checkpoint store in RemoteOps.
* Streaming (non-wave) pipeline, multi-process writers on one store.
