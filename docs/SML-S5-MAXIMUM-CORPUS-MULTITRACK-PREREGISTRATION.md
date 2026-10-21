# SML S5 — large-corpus portfolio, segregated rights and source-first scaling

Preregistered 2026-10-21 **before the catalog/code implementation**. Model/scientific authority: SML-GENIUS. Host-only acquisition and resource enforcement: RemoteOps. Extends, but does not retroactively change, S1b/S2a, S3a/S3b, S4a/S4b/S4c/S4d. No new benchmark, no training started, and held-out 2024 TEST remains UNOPENED.

## Scientific motivation

Do **not** conflate total published dataset size with useful independently admissible UTF-8 training bytes. The 2026-10-21 S3a nine-shard audit found only 9,256,557 incremental candidates under its restrictive reviewed CC0/CC-BY policy; the original 500 MiB PleIAs harvest contains 405,730,229 bytes labelled only `PD_CLAIM`, not independently reviewed public-domain works. Broaden the *source discovery and review portfolio* instead of relabelling unreviewed sources or repeating S2a exposures. Existing 50M slots are addressable, not 50M trained parameters or a 25B model.

## Four independent acquisition/admission lanes

1. **OPEN/ATTRIBUTION REVIEW** — separately verified CC0, legally established public domain, CC-BY, Apache-2.0, MIT, French Licence Ouverte v2 and other specifically cleared original licenses, retaining attribution, notice, original issuer and document identifiers. Only explicit compatible SML `*_REVIEWED` labels after independent operator review may reach an existing model-fitting manifest.
2. **CONDITIONAL RESEARCH / SHAREALIKE REVIEW** — CC-BY-SA, ODbL, institutionally research-only sources and noncommercial conditions are **not ignored**. Track them as first-class source candidates, acquire permitted *metadata* first, and give them a separately authorized research path if their exact upstream terms, jurisdiction, institutional conditions, copyright and permissions allow the proposed act. Never silently merge weights/outputs across rights lanes. Neither a ShareAlike obligation on original text nor a research-only claim automatically determines the downstream model-weight license; review that question explicitly. No training authorization from this catalog.
3. **WEB/TDM REVIEW** — ODC-BY-labelled compilations such as FineWeb-Edu, FineWeb2 and Dolma; Common Crawl-derived original pages may carry independent copyright, opt-outs and publisher terms. Discover now, pin bounded public metadata and plan rights/opt-out screening, but never equate a *database* ODC-BY notice with a license grant over each web page. Consider lawful research/TDM bases only after documented applicability and opt-out/privacy review, not as a blanket permit.
4. **MIXED/UNKNOWN SOURCE QUARANTINE** — source maps, source-level attribution and opt-out lists may be inspected as inert metadata; raw input is not promoted to model TRAIN without explicit independent rights and privacy clearance. Includes synthetic cyber traces, multiple-license dataset wrappers and unlabeled mirrors. Synthetic instructions are data, never privileged prompts.

These lanes are **review work queues**, not new `sml-sparse-gradient-stream-fit` rights labels. No blanket "ACADEMIC_RESEARCH_ONLY_REVIEWED" manifests, implied redistributability, automatic legal clearance, unsafe code execution, or mixing of test and train.

## Candidate inventory and sources

The machine-readable `research/corpus/source_catalog_v1.tsv` is a **non-executable, metadata-only catalog**. It inventories Common Corpus (2.27 trillion published tokens), Common Pile v0.1 (8 TB, 30+ sources), corpus subsets for Wikimedia/StackExchange/PubMed/peS2o/USPTO, FineWeb-Edu (1.3T published tokens), FineWeb2 (1000+ languages), Dolma (~3T published tokens), Common Crawl, Wikimedia/Wikidata, PMC OA, OpenAlex, DILA LEGI and the three current cyber candidates. Read source-of-truth publisher terms at the URLs in each row. These are published sizes, **not local acquisition or unique TRAIN estimates**.

- PleIAs: https://huggingface.co/datasets/PleIAs/common_corpus; already pinned `307910e4c5d040d6f318e6edf2a2b97849155771` in RemoteOps's HF500 pilot; keep source-by-source `license`, `identifier`, `creator` and `title`.
- Common Pile: https://www.commonpile.org/ and https://huggingface.co/common-pile; collection of individually licensed/published datasets, **not** one blanket license. Start PubMed and peS2o with individual article terms, other open-source families independently.
- FineWeb-Edu: https://huggingface.co/datasets/HuggingFaceFW/fineweb-edu, which declares ODC-BY and Common Crawl ToU; FineWeb2: https://huggingface.co/datasets/HuggingFaceFW/fineweb-2; Dolma: https://huggingface.co/datasets/allenai/dolma. All must pass per-origin copyright/TDM/opt-out review.
- Common Crawl: https://commoncrawl.org/terms-of-use — explicitly warns original crawled content is subject to separate site-owner terms.
- Wikidata copyright: https://www.wikidata.org/wiki/Wikidata:Copyright; avoid blanket CC0 claims on unrelated Wikimedia text (https://foundation.wikimedia.org/wiki/Policy:Terms_of_Use).
- PMC official text-mining distribution: https://pmc.ncbi.nlm.nih.gov/tools/textmining/ — verify per-paper license, retractions and versions.
- OpenAlex snapshot: https://registry.opendata.aws/openalex/ — CC0 metadata does **not** license third-party fulltext.
- DILA LEGI: https://www.data.gouv.fr/datasets/legi-codes-lois-et-reglements-consolides — keep Licence Ouverte attribution and document family.
- Cyber candidate protocols: `docs/SML-50M-S4A-CYBER-DATASET-RIGHTS-PILOT-20261021.md` and `docs/SML-50M-S4C-APACHE-MIT-OPERATOR-REVIEWED-LABELS.md`.

## Operational scaling without an unbounded local mirror

The Dell inventory `RemoteOps/docs/DELL_STORAGE_SML_2026-10-16.md` last reported about 715 GiB available on primary NVMe and 500 GiB on secondary *at its observation time*, and explicitly rejected the alleged /dev/sda1 HDD as unqualified. Storage must be remeasured, not assumed current. Do not download 8TB/2T-token collections wholesale, overwrite S1b/S2a, or delete previous corpus / backup data.

1. Freeze source repository commit or publisher release ID, per-shard checksum and upstream terms, and a non-root RemoteOps host capability receipt. Source rows in this catalog remain `SOURCE_CANDIDATE_ONLY` and `PIN_PENDING` until recorded.
2. Inspect publisher metadata and original rights/opt-outs, then permit a bounded, revision-pinned **single-shard non-executable pilot**. Retain local SHA-256 receipts, resource use, language, author/work/incident group key, provenance and actual unique UTF-8 bytes (including source license provenance), with public CI logs restricted to aggregates.
3. Build exact and near-dedup across S1b/S2a/S3/S4 families and across training/heldout groups; use stable author/work/incident/session partitioning. Preserve original benchmark TEST, never recruit it into TRAIN.
4. Accept an independently reviewed manifest for precisely its permissible training lane and model lineage; run a matched ngram4 and unchanged checkpoint control on genuinely novel TRAIN and held-out DEV. Report learned slots, packed-bit accounting, throughput and BPC with intervals; failed gates stay failed.
5. Process **many independently bounded slices** (for example 512 MiB first and up to existing 16 GiB stream-fit per TRAIN slice), release disposable compressed staging only after verified receipts and with host-local data retention rules. Store catalog index and dedup digests, not the complete global corpus on one disk. Use incremental quotas, no resident background CPU/GPU workload. Larger logical corpora need a separate preregistered trainer/chunk-resume contract: current stream-fit has `MAX_FILES=100000`, `MAX_TRAIN_BYTES=16GiB`, `MAX_DEV_BYTES=256MiB`, which are **not** relaxed by this S5 catalog.

## S5 catalog contract

`sml-corpus-source-plan` consumes a bounded TSV with exact fields `id/domain/policy_lane/source_class/source_url/terms_url/revision/review_state`. All entries are initially `SOURCE_CANDIDATE_ONLY`. `revision=PIN_PENDING` is never a sha or admission. The planner rejects malformed/duplicate fields, unknown policy lanes, untrusted URI syntax, noncanonical names and any claim of review. Its aggregate counters are a *discovery portfolio only*. It performs no remote downloads, data extraction, model load/fit or test access.

S5a is complete only when its catalog and validator compile/test on exact PR head. S5b is RemoteOps's versioned, source-pinned, budgeted per-origin pilot with independent rights and PII/provenance checks. S5c is a new scientifically preregistered grouped and deduplicated multi-source fit plus matched baselines; no volume/performance claims in the absence of measurements.
