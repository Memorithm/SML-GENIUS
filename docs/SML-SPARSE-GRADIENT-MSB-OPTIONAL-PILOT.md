# Opt-in MSB tree for the physical 4M / 50M sparse gradient pilots

This change makes the MSB-first output factorization available to the *existing* explicitly SHA-pinned, ledger-validated sparse SGD diagnostic CLI. It **does not change** the original 4M/50M pilot default output semantics, loss metrics, TRAIN/DEV selection or provenance rules.

Options:
- --pilot (unchanged independent-bit 4,194,304-slot master)
- --pilot-50m (unchanged independent-bit 50,000,896-slot master with page grouping)
- --pilot-msb (same 4M physical capacity but conditional MSB-tree output heads)
- --pilot-50m-msb (same 50M physical capacity, page grouping and conditional MSB-tree output heads)

Both MSB modes call SparseGradientAtlas::create_with_factorization(..., MsbTree). Checkpoint writes the versioned SML_SPARSE_GRADIENT_V2_MSB_TREE tag; model reopening restores this automatically. The separate Q4 nibble file has **no self-describing header**: this CLI must explicitly call QuantizedSparseAtlas::open_with_factorization(..., MsbTree), otherwise Q4 scoring silently uses incorrect independent heads. The implementation makes that call in all four modes, using the selected factorization.

The TSV model identifier remains **unchanged** for historical modes; new modes have _msb_tree_v3 suffix, distinguishing results without changing the existing schema. Both continue to print touched weights, physical parameter capacity, F32 and Q4 held-out loss, latency and source SHA. No additional model parameter count is manufactured and neither path claims full 50M convergence or SML25B training.

**Critical rights and data rules:** this CLI continues to require exactly the existing corpus SHA-256 and original per-document ledger validation. It must **not** be used on any SEC dataset until G0 rights admission is resolved; neither this PR nor its tests accesses SEC documents. The 2024 frozen TEST remains UNOPENED. Production activation of MSB requires held-out independent-source evidence and Q4 check; see SML #628 and its RemoteOps native transfer evidence. The MSB mode is opt-in specifically because IID-uniform transfer regressed in that study. There is no automatic selection from DEV, and no GPU/CUDA stack dependency.

Minimal tests assert exact old/new CLI flag mapping. CI must run rustfmt, strict Clippy and full workspace tests before merge; no rights-uncleared pilot is executed as a part of this PR.
