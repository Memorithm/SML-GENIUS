# Cumulative math supervisor — bounded qualification mode

`cumulative_math=1` is simulation-only. Requires a separate root, plan_ahead=1,
max_cycles in 1..64, at most 64 lessons/cycle, inputs at most 10, degree at
most 3, and no L1 arguments. The root is capped at 64 cumulative cycles across
restarts. The job ledger fixes the curriculum parameters and seeds. Each
successful job corresponds to one atomic cumulative manifest version, and
the previous cycle's functions are exhaustively re-examined from the shared
store before every new admission.

Recovery accepts a target at the completed ledger version or exactly one
version ahead with a pending job. In the latter case the old cycle directory
is kept aside, the same curriculum is re-examined to rebuild the certificates,
and exact replay verifies all records without advancing HEAD. The Succeed event
counts the acquisition once. A post-publication verification failure forces
emergency stop, never an automatic destructive shared rollback. Quarantine
blocks later seeds. This is not a general durable receipt protocol for arbitrary
jobs, and full manifest history growth and job-table compaction remain open.
The existing legacy simulator and V9 stores are not migrated.

Qualification command (fresh temporary root, only via RemoteOps on Dell):
```bash
sml-tutord root="$stage/cumulative" cumulative_math=1 plan_ahead=1 max_cycles=2 per_domain=4 inputs=7 degree=2 workers=2
```
Then reopen the same root for a further two cycles with identical curriculum
parameters. Verify cumulative HEAD version 4, 16 entries, 12 prior lessons
re-examined, no duplicate acquisition, and unchanged V9. The unit test models
the interruption window after publication but before Succeed by replaying
the same job against its already committed target; a real kill-window test
and physical qualification are still required.

Main admission: #1074 was merged to the stacked parent, not main. This
change republishes the daemon slice on the squash-merged cumulative engine.
An additional test uses the real supervisor log and unwinds between HEAD
publication and Succeed, reopens the log and checks the exact accounting.
This is a fault-injection test, not a physical kill or production deployment.
