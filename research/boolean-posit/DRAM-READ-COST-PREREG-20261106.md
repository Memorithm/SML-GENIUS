# BPB-3 / DRAM-RD-1 — preregistered physical read-cost qualification (2026-11-06)

Status: **preregistered protocol, NOT a result**. Model/runtime integration not authorized.
Scientific owner: Memorithm/SML-GENIUS. Physical execution: Memorithm/RemoteOps only.

## Motivation and null hypothesis

A BPB-3 block represents 64 quantized signed integer coefficients in three 64-bit planes plus 16 bits of scale; the Rust resident representation occupies 32 bytes, whereas a dense INT8 block occupies 64 bytes. One input mask occupies 8 bytes in either arm. Therefore equal sequential scans have a **nominal user-data footprint** of 40 vs 72 bytes/block including the common input: 44.444...% fewer logical bytes for BPB. This is an *accounting identity*, **not measured DRAM traffic**. Padding, metadata, page faults, cache prefetch, address mapping, compiler codegen and concurrent host traffic can change physical memory-controller transfers.

**Null hypothesis H0:** For the declared matched experiment, reduction of DRAM read CAS bytes with BPB compared with scalar INT8 is <= 0 (or too uncertain to establish a >0 benefit).

**Candidate H1:** The controlled median read-byte ratio BPB/INT8 is below 1 with a 95% paired uncertainty interval whose upper endpoint is below 1, and required validity controls hold. A more ambitious 10%-reduction target is secondary and requires an upper endpoint < 0.90.

## Measurement instrument — mandatory

Target Dell PowerEdge T430, Broadwell-EP Xeon E5 v4. Intel's Broadwellx iMC event UNC_M_CAS_COUNT.RD is EventSel 0x04/UMask 0x03; documented ScaleUnit 64Bytes. Sum read CAS counts over **all active exposed iMC channels / both physical sockets**, each event × 64 bytes. Do **not** infer bytes from reported CPU times or from the 26-byte BPB serialized payload.

The first operation is the manual, non-root RemoteOps read-only capability workflow `bpb3-dram-imc-probe.yml`. Log non-secret CPU/PMU identity, visible controller devices and cpumasks, kernel `perf_event_paranoid`, perf availability and a tiny validated read-count trial. Do not alter perf permissions, install packages, or authorize sudo merely to force a positive result. If no accessible iMC read event exists, verdict **NOT_MEASURABLE_ON_THIS_RUNNER**. CPU LLC misses, RSS, logical bytes and run time remain separate *proxies*, never replacements for DRAM iMC bytes.

A successful single-channel capability probe is **not** proof that all memory channels or sockets are covered. Verify all exposed socket/channel counters in the measurement workflow and record the coverage; insufficient coverage = **INCOMPLETE_COVERAGE**, no full-machine DRAM claim.

## Same-workload matched arms

Reuse exactly the quantized weight stream, bipolar input masks and independently verified integer dot values from the successful 256m full per-block experiment (RemoteOps run 42302134314). Freeze the generator seed, block count, source SHA, compiler release flags, thread count, CPU affinity and NUMA placement. Avoid downstream representation conversion inside the measured region. One arm scans resident 32-byte BPB blocks and 8-byte input masks; the other scans 64-byte dense INT8 blocks with the same 8-byte masks.

Generation and faulting/initialization must occur **outside the counted read region**, with explicit readiness handshakes and failure timeouts. Record the worker checksum, all counter raw values and measurement wall-time. Use no two concurrently active benchmark arms. Capture idle/background iMC controls to estimate system-wide noise; if measured idle traffic invalidates clean attribution, verdict **CONTAMINATED**, not a positive savings claim. Do not use destructive cache flushes or disrupt other workloads.

Freeze the first qualifying geometry at 32m blocks (2.048 billion coefficients, estimated 3.328 GB all resident paired arrays) because both independent layouts are much larger than the 80 MB combined last-level caches reported in earlier Dell inventory. If valid and resource-admitted, repeat at 128m blocks (8.192 billion coefficients, estimated 13.312 GB paired arrays). **Never** auto-escalate to 256m; resource admission and separate manual dispatch are mandatory. A new host RAM observation and guarded 50% available / 16 GiB reserve apply each time.

## Repeated and controlled observations

Conduct at least 12 valid matched BPB/INT8 pairs, alternating within quartets in ABBA / BAAB order; record invocation order and time drift; use distinct, declared deterministic seeds across independent sessions if model content influences cache behavior. Run low priority behind the existing Dell resource lock. Each arm's counters must have no `<not counted>`, `<not supported>`, multiplexing without scaling disclosure or other errors. Never count corpus generation/initial page faults as an inference read scan.

Collect (a) summed 64-byte iMC READ CAS bytes, (b) total iMC WRITE CAS bytes where qualified, (c) elapsed time, (d) CPU frequency/core affinity and observed host traffic proxies, (e) exact workload checksum and per-block oracle proof on an independently admitted corpus. Compare same-socket and total-system controller observations if practical; disclose if only aggregate host traffic is observable rather than per-process.

Predeclare a robust paired point estimate: median of per-pair BPB/INT8 DRAM read-byte ratios, not ratio of unpaired medians. Compute a 95% paired bootstrap confidence interval over pairs (fixed seed, >=10,000 resamples). A positive verdict requires upper 95% limit < 1 and no contamination or coverage failure. Nominal read traffic ratio 40/72 = 0.5556 is **not** a predicted measured value because cache behavior is unknown. Preserve all individual raw counters, workload checksums, metadata, and scripts in a non-secret run artifact.

## Stop / interpretation

- PASS_MEASURED_DRAM_REDUCTION: counters accessible, coverage and noise controls pass, full-oracle same-output condition holds, CI upper bound of DRAM read ratio < 1.
- NOT_DEMONSTRATED: ratio not below 1 with the declared uncertainty, or checksums/assumptions not satisfied.
- INCOMPLETE_COVERAGE / NOT_MEASURABLE_ON_THIS_RUNNER / CONTAMINATED: report limitations, do not report lower physical DRAM bytes as established.
- No model-level quality, real-task learning, energy, training or end-to-end performance claims from this component-only synthetic experiment.

## Evidence and primary sources

- Intel Broadwellx iMC PMU read CAS event and 64-byte scale: https://perfmon-events.intel.com/platforms/broadwellx/uncore-events/uncore/
- Linux Broadwellx perf PMU mapping: https://lkml.iu.edu/hypermail/linux/kernel/1702.1/01936.html
- First aggregate-only Dell 256m run: https://github.com/Memorithm/RemoteOps/actions/runs/42299615528
- Full independent blockwise Dell 256m run: https://github.com/Memorithm/RemoteOps/actions/runs/42302134314

The 2026-11-06 full-oracle result verifies only quantized dot-product equality, not hardware read traffic. Real success requires the separately instrumented prospective experiment above.
