# F2b-4b — Preregistered end-to-end frozen-prior spectral routing

**Frozen 2026-10-20 before implementation or measuring new data.** The F2b-4a core gate PR #640 was merged. All F2b-1/2/3 modes, source SHAs and prior results remain untouched.

## Question, new streams, control tasks

Does requiring two independent TRAIN calibration partitions to beat a majority prior **fitted on TRAIN_fit only** stop promoting IID null targets, including the biased P(Y=1)=0.2 case (for which a trivial constant predictor already obtains 20% error), while preserving predictive Boolean features?

Create an explicit opt-in CLI mode --f2b4 with task xor|mixed|null|null20, noise 0|40, seed 83|97|101|113 and candidate source count 32|508. New distinct deterministic RNG salts and internal seed indices 503,509,521,523. Each native host must run exactly **48 trials**: (2 causal tasks ×2 noise conditions +2 null types) ×4 seeds ×2 candidate grids. Null tasks accept CLI noise=0 only. null20 replaces *every* label bit independently with Bernoulli(0.2) bits, disjoint from the raw inputs and from the underlying teacher, in all FIT, internal-fit-VAL, TRAIN_cal1, TRAIN_cal2, and DEV phases. Fair IID null replaces all labels with Bernoulli(0.5). Same-seed n32/n508 share sources, so they do NOT provide independent statistical trials.

Fitted TRAIN uses first 128 words ×64 bits (clean) or 512 words ×64 bits (40% noisy), after 128 halo steps, including independently generated FIT labels. Compute the frozen constant bit prior **only** using those supervised TRAIN_fit labels after the halo; tie→0. Search via the real existing C12h W3 and W2+Sketch/MITM proposers, shared native fitter and committed-network simulator, using separately generated internal validation (640×64). Measure real search+fitting cost. Score both frozen networks and frozen constant predictor on **two** new disjoint 640×64 TRAIN_cal streams. Freeze before generating a final separate 4096×64 DEV stream. Score selected route once or abstain (zero selected predictions), plus independent DEV shadow models and constant prior for diagnostics only.

## Fixed decision and schema

New TRAIN-only prior-relative gate: at least 40960 calibrated bit targets per partition; candidate errors <=490000 ppm and **at least 10000 ppm improvement over frozen constant prior** separately in each partition; W3 must additionally beat Hybrid by >=10000 ppm on both partitions; Hybrid CPU search+fit+CAL <=500000 ns/target; W3 <=1000000 ns/target. Else Abstain. For n508 skip W3 **before it runs** if its admissible triplet search space exceeds 1,000,000, report unavailable W3 as absent -1, not as zero error. Thresholds and seeds are fixed before these observations. This is not a calibrated false-discovery-rate theorem.

Emit exactly **35 fields** per trial labeled F2B_ROUTE_V4. Keep all 27 base F2b1 fields, followed by n, skipW3, fixed absolute maxErrorPpm=490000 (V3), and then FIT positive count, CAL1 prior errors, CAL2 prior errors, DEV prior errors, fixed prior-improvement minimum 10000 ppm. Preserve old V1/V2/V3 schemas and call paths. With null targets use independent target RNG at every phase; no hidden teacher indices to proposer.

## Engineering and scientific gates

Engineering PASS: 48/48 distinct valid V4 records on each nonroot native Dell x86_64 and Thor ARM64, source SHA attested, rustfmt, Clippy -D warnings, targeted unit tests, release build, no W3 at n508, paired counts and route/shadow consistency, artifact SHA256. **Do not fail CI based on a scientific failure**: adverse outcomes must remain observable.

Tentative within-generator scientific PASS only if **zero of eight IID-null rows per null family and host are promoted**, and at least one new XOR3/40% n32 task selects W3 with ~40% DEV error (compare to Hybrid and prior). Publish selective coverage/abstention and costs for all causal tasks, not only accepted loss. Two CPU replicas and paired candidate grids do not multiply the number of independent new streams.

Limits: does NOT address Markov/autocorrelation null labels or unbiased posterior confidence under multiple-hypothesis adaptation. F2b4c must compare a *frozen causal Markov predictor* and authorship-separated rights-cleared text before runtime promotion. No SEC data (G0 NOT_ADMITTED), frozen 2024 TEST remains UNOPENED, no GPU/NVIDIA software SDK, no trained 25B or general LPN guarantee.
