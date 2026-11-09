# DACR-2E — support-qualified, class-balanced pairwise address learning

Frozen protocol **2026-11-09**, committed before implementation. Independent from PRs #913/#915/#916; starts from `main` SHA `3b639c52d85838ac378a1d3818bf3e87ce0c7b55` and modifies only DACR-2A's research `dacr_latent` module. No production inference, source-authority or physical benchmark claims.

## Question / mechanism

Current `DiscreteAddress::train_pairwise` ranks SBG coordinates by the **raw** score `(# different-pair disagreements - # same-pair disagreements)`. This couples coordinate choice to how many labels of each type happen to be present and does not require *per-coordinate known-mask support*. With strongly imbalanced supervision, a nuisance bit can score better than a weak but cleanly discriminating semantic bit.

Implement a **separate opt-in** `train_pairwise_balanced(pairs, budget, policy)` while keeping the previous method's behavior byte-for-byte unchanged. For coordinate `j`, only count pair observations where **both** masks say the bit is known. Record `S_j` / `D_j` numbers of observed same/different pairs and `s_j` / `d_j` numbers of disagreements. Require `S_j >= min_same_observed`, `D_j >= min_different_observed` and positive normalized advantage

`A_j = (d_j / D_j) - (s_j / S_j) > 0.`

The integer fraction is `num_j / den_j`, `num_j = d_j*S_j - s_j*D_j`, `den_j = S_j*D_j`, rejecting nonpositive numerators. Rank descending by **exact** fraction cross-products, break ties by ascending coordinate index, select `address_bits`, freeze positions sorted ascending. No floating-point scoring or probabilistic inference at query. Preserve the same hard SBG address encoder and `CandidateIndex` semantics. Emit selected-coordinate evidence counters, exposing per-feature support and disagreement counts.

To bound cross-products and training loop cost, reject `budget.max_pairs > 1_000_000` **only in this opt-in API**, and reject zero support policy floors. With `pairs.len() <= 1_000_000`, every num/den is at most `10^12`, so pairwise compare products are at most `10^24 < 2^128`. No unchecked overflows. Return existing `AddressError::InvalidBudget`, `InsufficientPairs` or `InsufficientSignal` as appropriate. Preserve deterministic masks and training accounting. No hidden adaptive testing on the final holdout.

## Fixed counterexample fixture (predeclared)

`BITS=16`, `address_bits=1`, positions `1` nuisance and `4` signal. Train `4` SAME pairs, each with nuisance bit 1 differing and signal bit 4 equal. Train `32` DIFFERENT pairs: 24 differ only on nuisance bit 1; 4 differ only on signal bit 4; 4 differ only on a high distractor bit 15. All masks known. The legacy raw scorer returns nuisance bit 1 (`24-4 = 20`) over signal bit 4 (`4-0=4`). The balanced policy (per-coordinate support floors `4` and `32`) rejects nuisance bit 1 because its rate difference is `24/32 - 4/4 = -1/4`. Signal bit 4 has positive rate difference `4/32-0/4 = 1/8`; bit 15 ties at `1/8`, so the frozen tie break selects bit 4. All labels here describe a *synthetic pairwise supervision fixture*, not meaningful language labels or unseen conceptual classes.

Independent TEST on two classes identified by signal bit 4, with nuisance bit 1 inverted between index-insert addresses and query addresses. Each class has one segment. New learned coordinate bit 4 must recover 2/2 IDs; the legacy coordinate bit 1 must recover fewer (with this dataset exactly 0/2). Only nuisance configurations are held out, not class identities. Same epoch, stream, source inventory, address bits, budgets and indexing for both methods. Score is candidate recall, **not** source integrity, model accuracy or cross-domain generalization.

## Required negatives

1. Per-feature known masks with too little observed SAME or DIFFERENT evidence reject the coordinate even when raw disagreements seem favorable; fully unknown bits never acquire evidence.
2. No usable signal -> `InsufficientSignal`. Missing one pair label type -> `InsufficientPairs`.
3. Deterministic selection, selected evidence, and retrieval outputs across two identical trainings; tie break stable.
4. Invalid zero support floors, oversized max-pairs, and oversized actual training slice reject before training.
5. Existing unbalanced `train_pairwise` and exact known-mask checks remain unchanged; regular module unit tests continue to pass.

All qualification is on the exact PR head: `cargo fmt --all -- --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings`, `cargo test --locked --workspace`. No cherry-picking results, no hardware access without Memorithm/RemoteOps, no new vendor SDK. Do not merge until required CI is entirely green. Reporting should explicitly state this one fixture does **not** establish superior general retrieval or downstream model quality.
