# SML-50M S1a — open-license CC0 and attributed CC-BY corpus admission addendum

Date: 2026-10-20. Commit this *before* a new training campaign. Additive to `docs/SML-50M-LARGE-CORPUS-STREAM-PREREGISTRATION.md`; no retrospective changes to S0 results.

## Exact source
RemoteOps established [native HF500 status](https://github.com/Memorithm/RemoteOps/blob/main/docs/SML_HF500_LICENSE_AND_INGEST.md): `PleIAs/common_corpus` Hub revision `307910e4c5d040d6f318e6edf2a2b97849155771`, single Parquet SHA256 `4ee719a130b7f86978b08c20cc9f490309e2bae2a5c0df4f04fd427365530f07`, decoded `corpus.utf8` SHA256 `684aa9f2bd96055ea4c95ad450533a4d4da888a30b3e6e4e49f3afe70e68eed8`. Physical host path `/home/remoteops/sml-corpora/PleIAs-CommonCorpus-500MiB-307910e4/`. Existing native evidence: 28,294 docs, **CC0 6,974 / 30,624,630 bytes**; **CC_BY_ATTRIBUTION 2,665 / 87,977,487 bytes**; **PD_CLAIM 18,655 / 405,730,229 bytes**.

## Narrow S1a choice
For the first real larger-than-prefix fit, **exclude all PD_CLAIM** (jurisdictional uncertainty). Permit explicit model manifest rights labels `CC0_REVIEWED` and `CC_BY_ATTRIBUTION_REVIEWED` in addition to existing `OWNED_REVIEWED` and `PUBLIC_DOMAIN_REVIEWED`. This applies only to independently reviewed source licensing and rights; adding a parser value does not attest legal clearance itself. CC-BY needs retained upstream `identifier`, `creator`, `title`, `license_source`, full identifier supplement, and attribution at downstream dissemination. No ambiguous labels, NC/SA, or authorless CC-BY by default. Independent review and data-group decontamination remain mandatory; fail closed, do not silently reclassify.

For the first **64-MiB TRAIN minimum**, use only explicitly verified CC0/CC-BY docs <=2MiB. Start with group-disjoint, deterministically SHA-assigned ~90/10 TRAIN/DEV by independent author/creator identity, not per individual document. Target >=64MiB TRAIN and >=4MiB DEV. If author identities or rights cannot be adequately validated, stop **before training** and retain a machine-readable audit, not a synthetic success. Do not concatenate all records into a single flowing training sample. Never give the model PD_CLAIM, SEC G0 or 2024 TEST.

## Native run / criteria
- First train exactly the S0 `sml-sparse-gradient-stream-fit --stream-50m-msb` (no tuning), with 48,829 disk pages, 32 LRU, LR .15, grouped 4096 byte steps.
- Pin immutable SML source SHA and RemoteOps workflow SHA; on Dell `hostname=debian`, nonroot `remoteops` actor. Enforce RAM virtual limit, disk free and bounded CPU/job time; use exclusive Dell concurrency group.
- Confirm physical `corpus.utf8` size 524,360,640 and global SHA. Read eight-column `provenance.tsv`, match each record's bytes and SHA against stream, preserve full identifiers and raw license metadata as root-local receipts. Source and generated data directories must be non-symlinks; neither raw text nor master/Q4 leaves host.
- Split/reject duplicates across roles and authors/work-group overlaps; protect against partial records, buffer overruns, invalid UTF-8, path escapes. A preflight must generate a roster and document shards on local disk with exact SHA and size. Do not assert the 500-MiB dataset is trained if only the CC slice passes.
- Before exceeding quota, verify actual resource headroom. First native output metrics should report **actual TRAIN bytes/events, unique touched learned weights, F32 and Q4 heldout BPC, wall time, throughput and run identities**. Q4 should be verified on same DEV.
- Model is research-only and never auto-promoted even after success; no proof of superiority over ngram4 or SML25B.
