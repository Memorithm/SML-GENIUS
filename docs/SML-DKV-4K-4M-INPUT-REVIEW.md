# Owner-supplied DKV-4k/4l/4m patch — current-stack input review

Input reviewed 2026-10-05:

- uploaded file: `sml-dkv-0008-dkv-4k-4l-4m.diff`;
- SHA-256: `39d7e8b66c55ef0d0ebd66614df697a5680fb7570aa32e7be7bdab5dc17bb9d7`;
- 1,668 lines;
- 12 paths changed;
- approximately 1,544 insertions and 17 deletions.

This note preserves the useful experimental information without applying the
patch to the current repository state.

## Useful findings in the supplied patch

The supplied DKV-4k/4l probes localize multi-hop failures to associative reads
at specific chain positions rather than to the controller alone.

The supplied DKV-4m geometry probe then reports the mechanism more directly:

- chain nodes share M and R;
- under the supplied ALL-field candidate, same-chain projected-key cosine is
  about 0.67;
- a V-only mask reduces that geometry to about 0.03 at K=1024/G=4;
- V-only improves low-load chain recall but collapses context separation on
  twin controls sharing V and differing in M/R;
- therefore dropping M/R is not a free solution.

That trade-off is scientifically useful and agrees with the direction later
measured independently on the current stack.

## Mapping to the current stack

The current repository has already advanced beyond the supplied implementation:

- N5 localized the remaining low-load failure to exact-state teacher-forced
  associative retrieval;
- N6 showed SELF and count-matched cross-chain controls are perfect while
  same-chain shared-M/R keys remain about 0.67 correlated and reproduce most
  L2/L3 failures;
- N7 conditioned the projection domain on exact V and restored K1024 chain
  capability, but one-bit V perturbations became near-orthogonal and next-value
  recovery fell to 0/96;
- N8 retained the fixed-width q=3 hybrid, which lowers same-chain correlation
  while preserving a non-zero local-continuity path;
- N9/N10 showed that route continuity and route-conflict isolation cannot be
  solved by a single interpolated key alone;
- N11 therefore separated shared PRIMARY retrieval from explicit ORDERED
  overrides;
- N12 requalified that two-bank composition against current exact
  source/record/subject verification;
- N13B now retains the same composition under balanced, empirical-skew and
  heavy-skew page populations.

The supplied patch therefore predates the mechanism now retained on `main`.

## Integration decision

Do not import the supplied `DeltaKvFieldMask`, `context_weight` or historical
candidate constructors into the current core.

Reasons:

1. the field-mask result itself is a trade-off, not a promoted mechanism;
2. current N7/N8 measure the same address-correlation problem without deleting
   semantic fields from the core projector;
3. current N10/N11 provide a stronger solution to incompatible continuity and
   isolation semantics by separating retrieval roles rather than tuning one
   monolithic key;
4. the supplied tests target an older projector/API shape and would not be a
   faithful replay of the current q=3/two-bank/verified-identity stack;
5. unrelated lockfile churn in the supplied diff is not part of the scientific
   result.

The twin-context idea remains a useful optional regression pattern: same V with
different M/R should not be silently treated as one semantic identity. It does
not reopen the current promotion gate because N13B's next declared gate is
physical resource qualification.

## Non-claims

This review does not independently reproduce the supplied Rust 1.97 run or
authenticate its preregistration chronology. It only compares the supplied diff
with the current repository evidence and records the integration decision.
