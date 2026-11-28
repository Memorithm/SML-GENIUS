# Cumulative store: bounded object verification work per audit

The former exhaustive audit parsed every manifest twice and re-read every
object once for every historical reference. A long cumulative history repeats
the same objects in many manifests. This change pins HEAD, traverses the
complete chain once, checks contiguous versions, hashes each distinct
content-addressed object once per call, and rejects a concurrent HEAD change.
No persistent verification cache is used. Objects omitted by a restore are
still verified through their historical references. The existing audit()
return value and manifest/checkpoint formats are unchanged.

The deterministic 32-cycle test with four distinct payloads per cycle has
2,112 historical references, 32 manifests and 128 distinct objects. Thus
object reads fall from 2,112 to 128 (16.5x fewer reads), while every reference
is checked via its verified content digest. This is a work-count comparison,
not a measured wall-clock speedup. It also checks a restored HEAD, historical
object corruption and malformed chain numbering. Full manifest history still
has quadratic size under cumulative full snapshots; this change does not
solve that separate storage scaling problem or enable production by itself.
No Dell or Thor execution is required for this PR.
