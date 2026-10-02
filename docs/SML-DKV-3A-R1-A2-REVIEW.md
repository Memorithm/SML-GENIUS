# A2 implementation review after the frozen run

This is an engineering review, not a new or amended experimental hypothesis.
The original protocol, first executed code and complete output remain retained.
The scientific grid, seeds, payloads, values, policies and all outputs MUST NOT
change. The new artifact-replay test enforces equality with the first output.

Two defensive implementation edits are admitted before final software merge:

1. Count contrast arithmetic only after both upstream projection results exist.
   In the first implementation the counter was charged before checking their
   validity. The frozen grid has no zero projection/rejected key, so this edit
   must leave every output byte unchanged. The rejection path is now reported
   as actual work rather than attempted subtraction work.
2. Validate the fixture's actual selected MOR page against its R signature
   inside the router, including paths whose projected key is later rejected.
   This is a routing-conformance check, not an extra source/gold input or
   inferred action permission. Both declared fixture signatures already satisfy it.

Additional tests bind the retained full output, compact summary, checksum and
576 paired groups to the executable. They were added AFTER observing the panel
and are disclosed as regression/integrity tests, never prospective confirmation.
RemoteOps and final exact-head CI must revalidate these changes. If output
changes, stop and retain the discrepancy; do not regenerate the golden evidence.
