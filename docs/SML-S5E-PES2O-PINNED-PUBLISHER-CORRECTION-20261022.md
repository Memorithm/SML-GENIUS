# SML S5e — correct peS2o publisher identity and immutable source revision

Preregistered 2026-10-22 before updating the S5 portfolio catalog. SML-GENIUS owns source-selection science; RemoteOps owns source acquisition and host execution. The 60-row source catalog built in #657/#659 currently points `common_pile_pes2o` to the unverified URL `https://huggingface.co/datasets/common-pile/peS2o` and leaves it unpinned.

Authoritative *observed* acquisition evidence is RemoteOps #719 (first archive), #721 (one-file row census), #722 (eight more SHA256-verified archives), #726 (restricted license claims correctly distinguished), and forthcoming #727 (cross-nine-shard narrow dedup). The actual publisher dataset is [common-pile/peS2o_filtered](https://huggingface.co/datasets/common-pile/peS2o_filtered), at pinned revision `297747513bfb0ff1fbf61ddad3b03319d0f04597` for those files. Earlier unused URLs are discovery mistakes, not admissible authorities.

## Controlled correction

Modify **only** the existing `common_pile_pes2o` TSV row to the real publisher dataset URL (and its as-yet-to-be independently reviewed terms page), and replace `PIN_PENDING` with the observed 40-hex revision. Preserve `source_class=CURATED_COLLECTION`, `policy_lane=ATTRIBUTION_REVIEW` and `review_state=SOURCE_CANDIDATE_ONLY`. Add one Rust test asserting exact URL/revision/claim-only lane, in addition to the existing 60-row schema and no-promotion checks. Do not include full archive payloads, sample data, new benchmark examples or hidden IDs in git.

This pins **source bytes**, not article-level licenses, opt-outs, author grouping, near-dedup or permission to fit. The S5e RemoteOps census does not promote the catalog rights state, and S1b/S2a checkpoints and the historical 2024 TEST remain untouched. This source correction itself yields zero newly certified TRAIN/DEV bytes.
