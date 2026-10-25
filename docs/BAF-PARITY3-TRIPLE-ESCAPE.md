# BAF-PARITY3: bounded triple-flip escape

3-bit parity (`e ^ s ^ x`) is exact at ANF degree 1 but needs three degree-1
atoms on one page at once; no single or pair flip strictly improves from the
plateau reliably (pairs-only: parity3 train mean 18.75, range 0..36).

`JointFocusPolicy::max_triple_flips` (default 128; needed C(7,3)*2 = 70 per
round) enumerates same-page degree-1 triples (i<j<k, page IDs ascending)
after singles and pairs each round. A triple is taken only if it strictly beats
the best single/pair. Over-budget fails closed (`CandidateBudget`); triple
evaluations count toward `max_candidate_evaluations`. Validation remains
sealed until the TRAIN search freezes; the same non-regression veto applies.

Hardened compare, 8 seeds, mean[min..max] error bits (train,val,test,ood):

| family | anf_degree1 | anf_degree2 |
|---|---|---|
| conj2 | 17.50[14..20] (unchanged, degree-2 target) | 0/0/0/0 |
| xor2 | 0/0/0/0 | 0/0/0/0 |
| parity3 | **0/0/0/0 all seeds** | 0/0/0/0 |
| and3 | 8.88 (not representable ≤ deg 2) | 8.12[4..12] |

Caveat: k-flip escapes scale as C(n,k); this is a fixed-width fixture result,
not evidence of general parity learnability for larger n.
