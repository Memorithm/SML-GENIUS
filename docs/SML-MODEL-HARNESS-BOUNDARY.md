# SML Model / Harness boundary

Status: architectural decision.

## Repository boundary

`SML-GENIUS` is the **Model programme**.

Its objective is to produce and qualify the real SML 25B model. Code belongs
here only when it is part of the model itself, its inference-time execution
semantics, or the minimum tooling required to train and measure the model before
the 25B milestone.

The future **Harness** will live in a separate repository:

`Memorithm/SML-HARNESS`.

SML-HARNESS is intentionally not part of the current model-first critical path.
It begins as a dedicated programme once the first real SML-6 25B champion
exists.

## Independent Memorithm components

The Harness is not a monorepo that absorbs the Memorithm ecosystem.

Forge, ProofLab, TDI, KVLab, ElasticXxx, CCOS-Enterprise, RemoteOps, SciRust and
other useful Memorithm projects continue to evolve independently in their own
repositories, with their own contracts, tests and release cadence.

SML-HARNESS composes the capabilities it needs through explicit, versioned
adapters and contracts.

A capability remains owned by its upstream repository. The Harness may pin,
invoke, benchmark, orchestrate or propose changes to that capability; it does
not silently copy its implementation into the model.

## Responsibility split

### Model — SML-GENIUS

The model includes only mechanisms needed to execute learned SML computation,
including as qualified:

- learned parameter/circuit atlas;
- SBG state and transformations;
- MOR routing and expert selection;
- inference-time memory mechanisms;
- sparse expert/page activation;
- Elastic Weight runtime semantics required by inference;
- recurrent state rewriting;
- native decision and/or generation heads;
- minimal serialization/runtime contracts required to serve the learned model.

These mechanisms are part of the model because removing them changes the
learned inference machine.

### Harness — SML-HARNESS

The future Harness contains the environment around the model, including:

- recursive self-improvement loops;
- candidate generation and mutation;
- architecture/code search;
- training orchestration and curricula;
- benchmark and evaluation orchestration;
- evidence aggregation and Pareto selection;
- promotion, rollback and lineage;
- deployment/control-plane automation;
- cross-repository workflows;
- interfaces to independent Memorithm components.

Removing the Harness must not change the semantics of a frozen model artifact;
it removes the machinery used to create, improve, evaluate and operate future
generations.

## Autonomous growth after the first 25B champion

The first real SML-6 25B model is the bootstrap champion, not the final scale.

After that milestone, SML-HARNESS may let the current champion autonomously
drive the creation of larger descendants:

1. diagnose capability/resource bottlenecks;
2. propose architecture, curriculum, data and training changes;
3. invoke the required independent Memorithm components through Harness
   contracts;
4. launch bounded training runs for descendant candidates;
5. evaluate quality, robustness, active compute, memory, traffic and physical
   cost;
6. select candidates on explicit multi-objective/Pareto gates;
7. retain exact lineage and rollback;
8. promote a new generation only through the Harness promotion contract.

A larger parameter count is never sufficient evidence by itself. Growth is
retained only when useful capability improves while the SML invariant remains:
**total capacity may grow much faster than active compute per cycle**.

Therefore later generations may be 30B, 50B or larger if evidence supports
them. No post-25B size is frozen in advance.

The current champion may be the research agent that proposes and executes the
next training programme, but the new descendant is a new versioned model
artifact. The Harness owns the self-improvement process; the model artifact does
not embed the orchestration stack.

## Post-25B autonomous ecosystem loop

A representative loop is:

```text
SML champion
   |
   v
SML-HARNESS
   |
   +--> Forge -------- candidate algorithms/code
   +--> ProofLab ----- proofs/verification
   +--> TDI/KVLab ---- research/robustness benches
   +--> ElasticXxx --- resource/runtime planning
   +--> CCOS --------- evidence/provenance
   +--> RemoteOps ---- machines/runners/deployment
   +--> other independently evolving Memorithm components
   |
   v
training + qualification of descendant
   |
   v
new SML champion
   |
   +-------------------------------+
                                   |
                                   v
                              next cycle
```

Upstream components remain independently evolvable. When the model discovers an
improvement to one of them, the Harness may propose a candidate change to that
component's own repository and qualification process.

## Development order

1. Finish the SML model architecture.
2. Progress through evidence-gated learned scales.
3. Produce the first real SML-6 25B model.
4. Freeze a stable Model/Harness interface.
5. Create `Memorithm/SML-HARNESS`.
6. Integrate the independently evolving Memorithm capabilities required for
   training, evaluation, mutation, governance and operation.
7. Enable autonomous recursive training/evolution from the 25B champion.
8. Allow later model generations to grow beyond 25B only under explicit
   capability/resource scaling gates.

Until step 3, Harness-oriented research is retained only where required to
train or qualify the model; it is not a competing product-development track.

## Invariant

**The model is the thing being improved.  
The Harness is the system that enables improvement.  
The Memorithm ecosystem supplies independently evolving capabilities to the
Harness.**
