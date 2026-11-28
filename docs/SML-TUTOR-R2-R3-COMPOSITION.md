# Native R2/R3 prerequisite learning in SML-TUTOR

Refactors the previously qualified R2 and R3 native examples into reusable
library modules, retaining their CLI, closed grammars, oracle and receipts.
The composition engine now registers two real Rust lessons with R2 as a
mandatory R3 dependency. R3 receives the actual consolidated R2 bytes via
Engine::prereq_checkpoints, learns one additional operation and must pass
both its independent compiler/reload exam and the retained R2 compiler exam.
R2 and R3 are each one bank of seven programs. The shared store publishes the
two complete certificates atomically with the existing engine publication API.
Direct re-examination uses the persisted R2/R3 pair and no learner.

CI uses oracle-generated fixtures, not a new external teacher inference.
The test publishes four mathematical functions and the two Rust banks,
checks idempotent replay, old records and all 458752 input/task cases per
bank, rejects incorrect R2 labels, and proves no partial publication.
The original native example CLIs remain usable by the physical workflows.
This is the engine integration; the next step is scheduling the chain in
the daemon using independently verified external corpus receipts. No Dell
execution or production activation occurs.
