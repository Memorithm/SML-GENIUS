# DKV-2a-N13A — prototype-native runtime census observation

Status: current-prototype census under
`SML-DKV-2A-N13A-RUNTIME-CENSUS-PREREGISTRATION.md`.

## Execution identity

- protocol PR: #191;
- execution PR: #192;
- executed scientific head: `14a6916ea718702335d75e31ca3c12ea37878082`;
- GitHub Actions run: `37363273578`;
- job: `111942608354`;
- Rust 1.89.0;
- two release-mode executions were byte-identical;
- retained output: `research/dkv-2a-n13a/CURRENT-CENSUS.csv`;
- manifest: `research/dkv-2a-n13a/MANIFEST.json`;
- post-merge main CI #662: SUCCESS.

This is **current-prototype evidence only**. It is not claimed to represent a
future 25B model.

## A — learned SML-1 routing is strongly unbalanced

The learned SML-1 holdout remains exact:

- tasks: 1,536/1,536 exact;
- trace records: 18,432;
- nonzero pages: 9/10.

Exact page population:

| Page | Count | Share |
|---:|---:|---:|
| 0 | 256 | 1.3888% |
| 1 | 512 | 2.7777% |
| 2 | 6,784 | 36.8055% |
| 3 | 256 | 1.3888% |
| 4 | 512 | 2.7777% |
| 5 | 768 | 4.1666% |
| 6 | 256 | 1.3888% |
| 7 | 1,152 | 6.2500% |
| 8 | 7,936 | 43.0555% |
| 9 | 0 | 0% |

The largest nonzero page is therefore 31x the smallest nonzero page.

The population has an exact integer reduction by 128:

`[2,4,53,2,4,6,2,9,62,0]`, total 144.

This reduced vector is the frozen empirical population arm for N13B.

Routing is also temporally sticky:

- transitions: 16,896;
- self-transition fraction: 59.4696%;
- longest same-page run: 17.

Largest transition cells:

- 8 -> 8: 5,040;
- 2 -> 2: 4,096;
- 2 -> 8: 1,920;
- 8 -> 2: 1,920;
- 7 -> 7: 896;
- 5 -> 8: 720.

Two low-frequency cross-page controls are retained for N13B:

- 0 -> 7: 256;
- 7 -> 3: 256.

This establishes that a balanced page-population assumption is not valid even
for the current learned SML-1 prototype.

## B — recurrent ambiguity is sparse in the retained q=3 proxy

At G=4:

- 448 states;
- A=0 for 100%;
- all states have A<=16;
- stable-state fraction 75%;
- M/R delta is exactly zero.

At G=16:

- 1,792 states;
- A=0 for 94.4754%;
- all states have A<=8;
- no state has A>16;
- stable-state fraction 70.9635%;
- M/R delta is exactly zero.

The nonzero G=16 ambiguity tail is:

- A=1: 30 states;
- A=2: 22;
- A=3-4: 37;
- A=5-8: 10.

This is supportive of ActiveOnly on this retained proxy, but it is not evidence
that a future learned 25B SBG distribution will remain sparse.

## C — dynamic MOR stress remains deliberately narrow

Across the retained native dynamic-MOR controls:

- cycles: 23;
- one-expert routes: 3;
- two-expert routes: 20;
- primary page: page 0 in all 23 cycles;
- route-switch fraction: 0%;
- precision-escalation fraction: 78.2608%;
- disagreement is either 0 or 500k ppm in the retained fixtures.

This stratum is useful for compatibility and control semantics, not for
estimating deployment page populations.

## Frozen invariants

All six N13A invariants hold:

- learned-task integrity;
- learned-route completeness;
- recurrent accounting;
- no hidden M/R rewrite;
- dynamic-MOR fixture compatibility;
- byte-identical determinism.

## Decision

N13A selects no runtime policy.

It supplies the frozen empirical distribution evidence required for N13B.
The next question is whether the retained q=3 two-bank + N12 verified identity
semantics remain valid under balanced, observed-skew and independently fixed
heavy-skew page populations, while making cross-primary sharing explicit rather
than accidental.

Model/core promotion remains blocked.
