# PB-6 — matched-control Pascal task-utility observation

Status: retained positive task-utility evidence under
`SML-PASCAL-BOOLEAN-PB6-TASK-UTILITY-PREREGISTRATION.md`.

PB-6 tests a deliberately narrow claim: exact K=2048 subset-parity queries on
the frozen synthetic workload. It does not reopen the closed PGF2
representation-mixing branch and it does not qualify Pascal for model
integration.

## Execution identity

- protocol PR: #216;
- protocol merge: `d88d0297f905c20afa0fda4a171c0545f95ea4eb`;
- implementation PR: #217;
- executed head: `ac92c829a64bfcbe214ba30bfa96ed24c8bfc86d`;
- implementation merge: `77224e2048979d37bdbc5b7d6a25e367464c60bb`;
- GitHub Actions run: `37437015636`;
- job: `112181206853`;
- runner: `tarek-scirust-arm64-01`, machine `tarek`;
- Rust 1.89.0, target `aarch64-unknown-linux-gnu`;
- deterministic replay:
  `research/pascal-boolean-pb6/DETERMINISTIC-REPLAY.csv`;
- deterministic replay SHA-256:
  `358dda85f5b387aca11262a0ccd2bde88bf24b93d6720350ca3ccacddf1d22d4`;
- physical replay: `research/pascal-boolean-pb6/PHYSICAL-REPLAY.csv`;
- physical replay SHA-256:
  `9d88f11ef2dcd43fab3bc2085859ebe8ddc0b0b91de3b08da0b1694dfe4a8c8c`.

CI executed deterministic mode twice and required byte-identical stdout before
the physical benchmark.

## Deterministic result

U1 through U4 hold.

All three arms and the independent Boolean oracle produce identical answers at
every frozen query prefix Q=1,16,64,128,256,512,1024,2048. Source collisions,
Feistel inverse failures, ANF coefficient/evaluation mismatches and answer
mismatches are all zero.

The exact structural crossover is visible rather than inferred. At Q=128,
DIRECT_ON_DEMAND performs 10,870 source reads per state while the conservative
P-ZETA accounting is 11,392 operations, so the frozen Pascal path is not yet
structurally cheaper. At Q=256, direct work rises to 22,648 source reads while
P-ZETA requires 11,520 conservative operations. U2 therefore holds exactly at
all preregistered moderate/high query loads.

For the full domain, GENERIC_FULL_PRECOMPUTE performs 177,147 direct source
reads per state versus 11,264 P-ZETA preprocessing XOR updates, a ratio above
15. Both precompute arms retain the same 256-byte logical answer vector and
P-ZETA uses no learned routing bits.

## Physical result

At Q=2048 over all 64 states, after three warmups and twenty measured
repetitions:

| Arm | median | p95 | states/s |
| --- | ---: | ---: | ---: |
| P_ZETA_PRECOMPUTE | 3.368060 ms | 3.398170 ms | 19,002 |
| DIRECT_ON_DEMAND | 13.582555 ms | 13.624861 ms | 4,711 |
| GENERIC_FULL_PRECOMPUTE | 13.344107 ms | 13.368177 ms | 4,796 |

The faster matched control is GENERIC_FULL_PRECOMPUTE. Relative to it, P-ZETA
is 3.961956x faster by median elapsed time, corresponding to a 74.7599% median
time reduction. P-ZETA also wins all 20/20 same-index paired repetitions. All
timed arms retain checksum `2fab696bfdac6864`.

U5 measured utility and U6 benchmark sanity therefore both hold.

The retained decision is:

`PB6_DECISION,RETAIN_TASK_UTILITY`

## Interpretation and boundary

PB-6 upgrades Pascal from exact functional evidence to measured operational
utility for one workload aligned with its subset-lattice semantics. This is a
material positive result because the advantage survives complete matched
P-FEISTEL/ANF pipeline timing rather than operation counting alone.

The result remains task-bounded. It does not establish:

- lower language-model loss or perplexity;
- semantic-retrieval benefit;
- SBG or MOR routing benefit;
- Delta-KV benefit;
- trainability;
- a universal 3.96x speedup;
- model-level promotion.

The next justified work is a prospectively declared generalization campaign
that maps where Pascal utility survives changes in width, query load, address
population and matched non-Pascal controls, followed only then by a
model-aligned task protocol. The new TDI/ITD research campaigns may explore and
independently replicate this direction, but they do not alter the SML promotion
boundary.
