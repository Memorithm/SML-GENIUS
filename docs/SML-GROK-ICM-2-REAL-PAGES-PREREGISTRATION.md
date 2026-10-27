# GROK-ICM-2 — Guarding real PVP6 and Delta-KV page layouts: preregistration

Date: 2026-10-27. Stacked on GROK-ICM-1 (#741). Research example only.

## Question (frozen before code)
When ICM guards protect pages with the *actual* SML layouts — PVP6 ANF gate
coefficient bitplanes (`sml_core::pvp6_executor::Pvp6Gate::<4096>::coefficients()`,
64 u64 words) and Delta-KV value records (`DKV_CONTROL_VALUE_DIM`=2 Q12 `i16`
per record, `DKV_CONTROL_VALUE_BITS`=32, two records packed per u64 word,
64 words per page) — under a seeded write stream with data-path faults,
do they detect 100 % of corrupt writes of weight ≤ 2 with < 0.1 % false
rejection of clean writes?

## Arms
NONE (negative control), SECDED72_64 (carried by ICM-1 fallback rule),
CRC32/MPEG-2 page guard (detection baseline kept by ICM-1 observation).
Shared implementation: `crates/sml-core/examples/support/grok_icm_guard.rs`
(same algorithms as ICM-0, not an example target).

## Protocol
- Each page receives 2,000 writes. A write replaces one word with an intended
  value (PVP6: toggle one random monomial bit in that word, i.e. a valid ANF
  edit; DKV: replace one packed record with fresh Q12 values in ±DELTA_KV_ONE).
- The guard is updated with the *intended* value. With probability 1/8 the
  stored value suffers a data-path fault of weight k, k uniform in {1,2,3}.
- After every write the page is scrubbed (`check`). Clean write rejected =
  FALSE REJECT. Faulty write: DETECTED (erasure or correction back to intended)
  or SILENT (accepted with wrong data / miscorrection). Detected faults are
  repaired to intended before continuing (so faults do not accumulate).
- Splits: CAL seeds 0..4 (smoke only, may be inspected during development);
  **sealed TEST seeds 1000..1016** (16 seeds × 2 layouts), evaluated once,
  numbers recorded as produced. No parameter is tuned on TEST.

## Gates (frozen)
PASS for an arm iff on TEST: detection of k∈{1,2} faults = 100 % AND false
reject < 0.1 %. k=3 detection is reported, not gated.
Expected: NONE fails (0 % detection, negative control); SECDED passes (k=1
corrected, k=2 detected) but has silent k=3; CRC32 passes with 100 % k=3.

## Non-claims
Scrub-after-every-write cost is not a deployment policy; no timing claim.
No Pvp6Bank evaluation path is modified.

## Reproduction
`cargo test --locked -p sml-core --example grok_icm_real_pages`

## Results
Local execution, first and only TEST evaluation, byte-identical replay:

```
GROK-ICM-2 report v1
CAL Pvp6 NONE: clean 7000 false_reject 0 | k1 0/330 silent 330 | k2 0/347 silent 347 | k3 0/323 silent 323 | gate FAIL
CAL Pvp6 SECDED72_64: clean 7000 false_reject 0 | k1 330/330 silent 0 | k2 347/347 silent 0 | k3 69/323 silent 254 | gate PASS
CAL Pvp6 CRC32: clean 7000 false_reject 0 | k1 330/330 silent 0 | k2 347/347 silent 0 | k3 323/323 silent 0 | gate PASS
CAL Dkv NONE: clean 7041 false_reject 0 | k1 0/309 silent 309 | k2 0/326 silent 326 | k3 0/324 silent 324 | gate FAIL
CAL Dkv SECDED72_64: clean 7041 false_reject 0 | k1 309/309 silent 0 | k2 326/326 silent 0 | k3 79/324 silent 245 | gate PASS
CAL Dkv CRC32: clean 7041 false_reject 0 | k1 309/309 silent 0 | k2 326/326 silent 0 | k3 324/324 silent 0 | gate PASS
TEST Pvp6 NONE: clean 28059 false_reject 0 | k1 0/1304 silent 1304 | k2 0/1301 silent 1301 | k3 0/1336 silent 1336 | gate FAIL
TEST Pvp6 SECDED72_64: clean 28059 false_reject 0 | k1 1304/1304 silent 0 | k2 1301/1301 silent 0 | k3 298/1336 silent 1038 | gate PASS
TEST Pvp6 CRC32: clean 28059 false_reject 0 | k1 1304/1304 silent 0 | k2 1301/1301 silent 0 | k3 1336/1336 silent 0 | gate PASS
TEST Dkv NONE: clean 27935 false_reject 0 | k1 0/1378 silent 1378 | k2 0/1357 silent 1357 | k3 0/1330 silent 1330 | gate FAIL
TEST Dkv SECDED72_64: clean 27935 false_reject 0 | k1 1378/1378 silent 0 | k2 1357/1357 silent 0 | k3 322/1330 silent 1008 | gate PASS
TEST Dkv CRC32: clean 27935 false_reject 0 | k1 1378/1378 silent 0 | k2 1357/1357 silent 0 | k3 1330/1330 silent 0 | gate PASS
fnv1a64 6c8b25a206397890
```

All frozen expectations held: NONE FAIL (negative control, every fault silent);
SECDED72_64 and CRC32 PASS on both real layouts (100 % k≤2, 0 false rejects).
Negative kept: **SECDED is silent on ~77 % of weight-3 faults** (1038/1336
PVP6, 1008/1330 DKV, miscorrection); CRC32 detected 100 % of k=3 but corrects
nothing. Consequence for ICM-3/ICM-4: a correcting arm must be paired with a
detecting arm (SECDED+CRC32 composite) to avoid silent miscorrection.
