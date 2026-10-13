# Run the prospective TESTTEST bench

Author: Memorithm / Tarek Zekriti. Date: 2026-10-13.

The registration and eligibility amendment are
[the scientific contract](ENG-1C-TESTTEST-PREREGISTRATION.md) and
[the pre-measurement roster correction](ENG-1C-TESTTEST-ROSTER-AMENDMENT.md).
Current frozen panel: Hugo/Zola/Descartes/Rousseau/Bernard/Fabre and
Dickens/Shelley/Emerson/Mill/Darwin/Faraday. No Verne, Flaubert, Dumas,
Austen or Candide. FR129 and EN64 never change in this experiment.

On a Linux CPU host with Rust 1.89 and curl:

```bash
cargo build --locked --release -p sml-core --bin sml-eng1c-testtest
bash scripts/eng1c/run-testtest.sh /tmp/testtest-first-run ALL
```

The root must not already exist. The script downloads only the declared
source URLs, verifies the immutable manifest on each phase, seals classical
DEV choice, scores TEST and removes source/model/sort files after recording
compact evidence. A source outage, drift, quota or incomplete work stops
execution. It never drops an author to obtain a favorable outcome.

For two concurrent language workers (at most two, one work per worker):

```bash
bash scripts/eng1c/run-testtest.sh /tmp/testtest-fr-first FR > /tmp/testtest-fr.log 2>&1 &
fr_pid=$!
bash scripts/eng1c/run-testtest.sh /tmp/testtest-en-first EN > /tmp/testtest-en.log 2>&1 &
en_pid=$!
wait "$fr_pid"
wait "$en_pid"
mkdir /tmp/testtest-panel-first
cp /tmp/testtest-fr-first/evidence/* /tmp/testtest-panel-first/
cp /tmp/testtest-en-first/evidence/* /tmp/testtest-panel-first/
target/release/sml-eng1c-testtest analyze scripts/eng1c/testtest-manifest.tsv \
  /tmp/testtest-panel-first /tmp/testtest-panel-first/panel.tsv
cat /tmp/testtest-panel-first/panel.tsv
```

Check both waits succeeded before combining results. A command success
means a complete valid execution, not a scientific PASS. The `DECISION`
row distinguishes `PANEL_CONFIRMATION_PASS` from
`PANEL_SUPERIORITY_NOT_ESTABLISHED`. Read the scope and all group estimates.
Raw record probabilities/losses are finite and at round-trip precision;
human summary rows round to twelve decimals. Selection and decision logic
use the unrounded values. Individual files use tagged TSV keys; `panel.tsv`
contains a summary table plus STRATUM/AUTHOR/DECISION tagged records.

For manual separated phases:

```bash
target/release/sml-eng1c-testtest seal scripts/eng1c/testtest-manifest.tsv \
  /tmp/verified-raw /tmp/testtest-work 17489 /tmp/testtest-evidence/17489-seal.tsv
target/release/sml-eng1c-testtest test scripts/eng1c/testtest-manifest.tsv \
  /tmp/verified-raw /tmp/testtest-work 17489 /tmp/testtest-evidence/17489-seal.tsv \
  /tmp/testtest-evidence/17489-test.tsv
```

Create the evidence directory first. Raw inputs use `<ebook-id>.txt`.
Sources and executable must match the seal. Adjacent `.sha256` files detect
accidental corruption; they are not signatures or hostile-host attestations.
DEV is not added to TRAIN. All classical TEST depths are diagnostic; only
the independently sealed DEV depth is the primary comparator.

`fingerprint RAW_DIR NEW_MANIFEST` performs identity/split hashing without
model evaluation. It cannot register new scientific data: `seal`, `test`
and `analyze` reject any manifest whose SHA256 differs from the compiled
registration. Replacing data requires a separately preregistered study.

The `ENG-1c TESTTEST prospective multi-author confirmation` GitHub Actions
workflow executes both language panels at the exact PR head, then checks
and analyzes all twelve authors. Its manual `workflow_dispatch` entry point
is suitable for RemoteOps-controlled execution; it does not duplicate the
RemoteOps control plane or need a resident service. The CI conclusion is
green for a fully measured negative scientific result. Corpus/model files
are never uploaded. Evidence artifacts expire after 30 days; enduring
scientific records must be committed as compact results.

## Reuse and interpretation

The Rust `paired_panel` module is predictor-independent: checked paired
unit gains and weights support exact sign-flip inference, macro and weighted
estimates, grouped paired bootstrap and two-test Holm. Its author-vs-token
reversal fixture prevents counting a large book as many independent studies.
Later SML components can use the same module without copying the statistics.
SciRust, ElasticXxx and RemoteOps can consume compact evidence records;
their model/data/endpoint contracts need destination qualification. No new
cross-repository dependency or unrelated behavioral change is introduced.

This small historical convenience panel has no modern web, dialogue, code,
additional languages or cross-author transfer task. It trains separately
on each author's prefix. A positive result concerns this protocol's paired
within-work prediction task only. Other domains need independent registered
panels; running this TEST repeatedly is replication, not another confirmation.
