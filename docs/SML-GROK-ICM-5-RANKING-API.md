# SML-GROK-ICM-5 — API de ranking documentaire (experimental, NON validée BEIR)

Prereg: `docs/SML-GROK-ICM-5-DOCUMENT-RANKING-PREREGISTRATION.md`.
Consommateur: CCOS-Enterprise [#241](https://github.com/Memorithm/CCOS-Enterprise/pull/241)
(`58db2a45aaf63c5cb7d1cd996fbb8730d37ce66d`), bras `sml-grok-icm-concurrent`.

> **Avertissement.** Ce module ne prétend à aucune qualité de retrieval.
> Chaque résultat porte `status = NotBeirValidated`. CCOS doit rester sur
> `UnavailableSmlRetrieval` tant que la campagne PR3 n'est pas CONFIRMED.

## Surface (`sml_core::icm_rank`, feature `icm-rank` ⇒ `icm`, default off, CPU-only)
- `trait DocumentRanker { backend_id(); rank(&mut self, query, k) -> Result<RankedList, RankError> }`
- `RankedList { arm_id = "sml-grok-icm-concurrent", backend_id, status, candidates: Vec<RankedCandidate { id, score: f32 }> }`
- `RankError::{EmptyQuery, Unavailable, DuplicateDocument, CorruptPage}`
- `UnavailableRanker` : miroir honnête de `UnavailableSmlRetrieval`.
- `SignaturePageRanker` (`sml-icm-rank-sigpage-v0-experimental`).

Contrat aligné sur `GovernedSmlRetrieval::recall`: requête vide = erreur,
`k = 0` = liste vide, scores finis dans `[0,1]`, tri `score desc, id asc`,
coupe à `k`, pas de doublon, déterministe à seed fixé.

## Design `sigpage-v0`
Signature binaire 4096 bits (64 × u64) par document: chaque token distinct
(ASCII alnum minuscule, longueur ≥ 2) pose 2 bits via
SplitMix64(FNV-1a-64(token) ⊕ seed). La page est stockée dans un
`icm_guard::GuardedPage` et scrubée avant chaque scoring; page
irréparable ⇒ `CorruptPage` (fail-closed, jamais ignorée). Score = coefficient
d'Ochiai `|q∧d| / √(|q|·|d|)`. ICM garantit l'intégrité de l'index, pas le
classement.

## Limites connues
- Pas d'IDF, pas de stemming, pas de longueur de document: attendu
  **inférieur à BM25** tant que non amélioré (c'est précisément ce que PR3 doit
  mesurer, et un NEGATIVE sera conservé).
- Collisions de hachage sur 4096 bits: saturation sur longs documents.
- Scan linéaire O(N·64) mots par requête; scrub à chaque requête (coût non mesuré).
- Tokenizer ASCII uniquement.

## Prochain run (PR3)
SciFact test (primaire), NFCorpus/FiQA (secondaires), seeds {0,1,2},
nDCG@10 vs `bm25` et `docmem_fused_minilm`, règles du prereg.
