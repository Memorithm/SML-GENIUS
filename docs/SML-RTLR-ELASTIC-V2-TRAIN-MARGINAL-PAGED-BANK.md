# RTLR-ELASTIC v2: TRAIN-paired marginal utility and SHA-paged dormant cell weights

## Experiment scope

The native RTLR-ELASTIC v1 control proved scalar equivalence to RTRL-2, and that the scheduler could add and remove cells, but elastic settings did **not** improve heldout accuracy: RTRL-2 3.616609952 DEV bits/byte versus v1 cap8/cap16 3.618128627 on previously examined SEC DEV (RemoteOps [report](https://github.com/Memorithm/RemoteOps/blob/main/docs/SML_RTLR_ELASTIC_50M_DELL_MEASURED_20261017.md)). V2 is a **new preregistered algorithmic ablation**, not a claimed performance gain. Exact scalar local eligibility and the original physical paged expert/router gradients stay unchanged.

## Exact-local recurrence

The anchor cell has the original weights [0.65,0.3,0.2] and exact three-dimensional scalar eligibility. Active cells are independent, each with 3 weights, one state and three local sensitivities. A bounded residual gate mixes the anchor feature and the mean extra-cell feature. No stochastic UORO, dense cross-state Jacobian or attention matrix is required. For frozen recurrent weights, local derivatives are exact; online weight changes do not imply full optimization-path derivatives.

## V2 capacity controller — strictly TRAIN-only

For each supervised bit, BEFORE either expert page or MOR gate is updated, compute two predictive probabilities from the **same current physical page weights and router gate**: the active recurrent feature (already computed by the standard training method) and an auxiliary `h_anchor` shadow feature. The shadow performs **no update** to the experts, router or recurrent cells. Sum bitwise binary cross-entropies for both features on the same observed TRAIN byte. This is a local, pre-update shared-weight counterfactual; it is **not** a separately trained RTRL-2 comparator.

After the completed TRAIN document (never DEV), define:

```text
L_live    = TRAIN loss bits / predicted byte
L_anchor  = TRAIN shadow anchor loss bits / predicted byte
EWMA_old  = previous TRAIN-only EWMA of L_live
stagnated = L_live >= EWMA_old - plateau_tolerance
utility   = L_anchor - L_live - marginal_cell_cost*(active-1)
```

With minimum 2 TRAIN warmup documents, 2-document consecutive-evidence patience, alpha=0.25, tolerance=0.05 bits/byte and cell cost=0.001 bits/byte per additional active cell:
- If more than one cell is active AND `utility <= 0`, build pruning patience; at threshold, park the newest cell.
- Otherwise, if `stagnated` and free capacity remains, build growth patience; at threshold allocate or restore the next cell.
- Otherwise reset streaks. EWMA is updated **after** observing each document's decision.

These are a priori research hyperparameters; 0.001 bits/byte is an explicit **regularization proxy**, not measured wall-clock CPU cost. The test does not prove actual compute-cost optimality. Different documents can vary in difficulty; TRAIN-only marginal utility under shared weights is not a causal treatment-effect estimate. Fair claims require fresh sealed heldout with independent trials.

## Dormant bank in bounded disk files

When v2 reduces active width, it writes the three trained recurrent f64 weights of the removed cell to a file `rtlr_v2_dormant/cell-NNN.tsv`. It uses `create_new`, IEEE-754 exact bitwise-hex encoding and a SHA256 of the first canonical record, then `sync_all`. It removes the cell object from the active vector only AFTER a successful file write. On re-expansion, it validates path type, file bounds, schema, cell index, SHA256 and finite parameter range [-2,2], loads those weights and deletes that parked file. It deliberately **resets volatile recurrent state and eligibility** on reactivation. Missing files represent never-before-created cells and initialize deterministically with independent weights; corrupted files fail closed rather than silently resetting.

The bank stores one small file per inactive trained cell: this is **on-disk inactive storage**, not paged `mmap` or an LRU variable-size atlas. Resident accounting counts only active-cell f64 payload; allocator reservations, OS file cache, filesystem metadata and process RSS are excluded. We do not claim that `Vec::pop` returns process RSS to the operating system. Bank directory must be new and non-symlink at setup; restoration revalidates directory. This is a bounded prototype, not a hardened concurrent multi-tenant store.

## Native ablation protocol

Compare the original continuous `--pilot-rtrl2-continuous` baseline, `--pilot-rtlr-v2-1` exact-equivalence control, `--pilot-rtlr-v2-8` and `--pilot-rtlr-v2-16`; optionally the merged `--pilot-rtlr-elastic8` v1 baseline. Every arm uses an independently initialized identical physical MOR/ANF expert bank of 50,000,896 addressable f32 slots (only ~376k touched on prior pilots), identical 64KiB TRAIN and 32KiB separate SEC DEV ordered SHA-attested source prefixes, original 10GiB source SHA, same 32 resident page cap and unchanged 8 bit labels per predicted byte. The DEV was already used before: this protocol verifies mechanics and potential direction, **not new generalization**. Do not adapt policy parameters from its results. Record bank parks/restores, peak dormant files, active/peak width and new read-only shadow document counts, plus loss, physical touched weights and native elapsed time.

## Checkpoint and governance

The output `rtlr_elastic_weights.tsv` and `rtlr_v2_stats.tsv` are **audit sidecars only**. The complete integrated model, including original physical expert pages, learned MOR gate, parked cell bank, active cell weights and trained controller history, is **NOT restorable** as a full versioned checkpoint. Exact continuation would additionally require in-flight recurrent states and optimizer/eligibility. The explicit restore API here restores **one dormant cell's weights only**, not the complete model. Production checkpoint work remains issue [#605](https://github.com/Memorithm/SML-GENIUS/issues/605).

SEC commercial rights G0 **NOT_ADMITTED**, 2024 TEST **UNOPENED**. 50M slots are addressable master capacity and not 50M actually trained; no 25B model or language performance claim.
