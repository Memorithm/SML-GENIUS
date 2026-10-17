# Paired physical 50M-capacity RTRL vs BPTT qualification (v2)

Status: research, *not* an LM capability proof. SEC source rights remain `NOT_ADMITTED`, 2024 TEST `UNOPENED`.

## Preregistered comparison

The previous BPTT pilot on the Dell [RemoteOps run 39014301375](https://github.com/Memorithm/RemoteOps/actions/runs/39014301375) succeeded with 50,000,896 allocated parameters and 246,682 distinct touched, 12 KiB TRAIN, 8 KiB heldout and 7.981562979 bits/byte heldout. This is close to the 8 bits/byte uniform baseline and **not** an RTRL-controlled gain.

Two CLI modes now share exactly:
- Capacity: 48,829 physical pages × 1,024 f32 master scalars = 50,000,896 physical parameters, 32 resident pages, rate 0.20.
- Identical input, source SHA, per-document SHA/rights validation, ordered deduplicated documents, and independent fresh initial disk snapshots.
- First 64 KiB of consumed TRAIN prefixes and 32 KiB of separate heldout prefixes, at most 1,024 bytes of each SEC report. Different reports are selected for each split; the 10GiB SHA is verified but not all 10GiB is trained.
- The two-step input context, 128-byte recurrence truncation boundaries, per-document reset and read-only heldout evaluation. Exactly the same bit targets and output addresses are offered to both arms.
- Identical forward heads and expert initialization, but **different optimizers** by design: RTRL applies online eligibility updates; BPTT computes a frozen-window backward gradient before applying updates.

`sml-mor-anf-rtrl-fit --pilot-paired-rtrl CORPUS LEDGER SHA NEW_DIR` and `--pilot-paired-bptt` produce separate SHA-bound `paired_selection.tsv` manifests. Their TRAIN and DEV source/prefix fingerprints, document counts, bytes, events, and window must match **exactly**. A distinct actual touched-weight count is reported separately for each arm, not inferred from capacity.

## Evidence gates

Run both modes on the physical Dell as non-root, pin both SML and RemoteOps revisions, enforce full Rust 1.89 fmt/Clippy/tests, verify whole-corpus SHA and per-report SHA. Record heldout bits/byte, training bits/byte, train events, physical distinct touches, gate and recurrence updates, page I/O, elapsed wall time, and compare the two metrics without assuming which wins. Reject missing, nonfinite or inconsistent provenance metrics. Keep raw documents and full master snapshots local. No access to 2024 TEST, and no model/admission claims from this tiny research slice.

## Limitations

Same sample and capacity are necessary but not sufficient for equal compute. Online RTRL and BPTT use different update frequency and math; log a per-run wall time. An in-year SEC heldout split is not a 2023 out-of-time DEV, let alone conversational multilingual quality. Discrete ANF topology and hashed candidate-page selection are still fixed. No claim of generalized convergence or 25B training.
