# SML-GROK-ICM-5 — Document ranking arm `sml-grok-icm-concurrent` (préenregistrement)

Status: **PREREGISTERED** — écrit et mergé **avant** tout code de ranking et
avant toute exécution BEIR. Ce document ne contient **aucun chiffre de run**.
Série: GROK-ICM-0..4 (#740–#744). Consommateur: CCOS-Enterprise
[#241](https://github.com/Memorithm/CCOS-Enterprise/pull/241) (merge
`58db2a45aaf63c5cb7d1cd996fbb8730d37ce66d`), crate
`ccos-enterprise-sml-retrieval`, trait `SmlRetrievalAdapter`, bras
`SML_CONCURRENT_ARM_ID = "sml-grok-icm-concurrent"`, backend actuel
`UnavailableSmlRetrieval` (erreur explicite, jamais de scores vides).

## Cadrage
- `sml_core::icm_guard::GuardedPage` est une garde d'**intégrité**
  (SECDED(72,64) + CRC-32), **pas** un ranker. ICM-5 introduit une API de
  ranking **distincte**, qui peut **composer** avec ICM pour stocker son index
  dans des pages gardées, mais dont la fonction de score est une conception
  Grok indépendante.
- Hors périmètre: cloner les lignes Claude ternary/hybrid (HYB-*) ou GPT
  skyrmion (SKY-*); dépendance NVIDIA/CUDA (chemin CPU-only obligatoire, cf.
  AGENTS.md « hardware sovereignty »).

## Hypothèse
H1: un ranker déterministe CPU-only fondé sur des signatures binaires de
documents (pages de mots u64, éventuellement gardées ICM) atteint, sur BEIR
SciFact **test**, un nDCG@10 **≥ BM25** (même tokenisation, mêmes qrels).
H2 (secondaire, ambitieuse): nDCG@10 ≥ `docmem_fused_minilm`
(DocumentMemory fused MiniLM, CCOS-Enterprise #239 « BEIR family
GENERALISES », merge `5075f336`). H2 n'est attendue **ni** du stub PR2 **ni**
de la première campagne; elle est préenregistrée pour qu'un échec soit
consigné comme NEGATIVE et non oublié.

## Phases
1. **PR1 (ce document)** — prereg seul.
2. **PR2 — API minimale** `sml_core::icm_rank` (feature `icm-rank`, default
   off, marquée `experimental`): texte → candidats `(id, score: f32 fini)`,
   tri `score desc, id asc`, coupe à `k`, requête vide = erreur, aucun doublon
   — exactement le contrat que `GovernedSmlRetrieval::recall` vérifie côté
   CCOS. Tests unitaires sur fixtures/oracles écrits à la main, **pas BEIR**.
   Chaque résultat porte le label d'arm et le statut `NotBeirValidated`.
3. **PR3 — campagne** (ultérieure, séparée): exécution figée ci-dessous.
   Toute modification de la fonction de score après PR3-DEV et avant TEST
   invalide la campagne (nouveau prereg requis).

## Campagne figée (PR3)
- Datasets: **SciFact test (primaire)**; NFCorpus test et FiQA test
  (secondaires, rapportés, ne décident pas du verdict primaire).
- Métrique primaire: **nDCG@10** (trec_eval convention, qrels BEIR officiels,
  gains gradés tels que fournis). Secondaires rapportées: Recall@100, MRR@10.
- Arms: `bm25` (référence lexicale du harness CCOS #217), `docmem_fused_minilm`
  (#239), `sml-grok-icm-concurrent` (ce travail). Même corpus, mêmes requêtes,
  même k=10 (Recall@100 avec k=100).
- Seeds: le ranker est déterministe; seeds `{0,1,2}` ne servent qu'au hachage
  de signature (`seed` du hash FNV/SplitMix). Rapport: moyenne et min des 3.
  Le verdict utilise **le min** sur les 3 seeds (pas de cherry-pick).
- Incertitude: bootstrap apparié sur requêtes, 10 000 rééchantillonnages,
  seed 20261029, IC 95 % de Δ nDCG@10.
- Budget: CPU-only, ≤ 1 h mur par dataset×seed sur un hôte x86_64 ou Thor
  (via RemoteOps uniquement), ≤ 4 GiB RSS. Dépassement = run INVALID, pas de
  résultat partiel promu.
- DEV: tuning éventuel (taille de signature, pondération) **uniquement** sur
  SciFact **train** (qrels train). TEST exécuté une seule fois par seed.

## Règles de verdict (par comparaison, sur SciFact test)
- **CONFIRMED**: Δ nDCG@10 (SML − baseline) > 0 avec borne basse IC95 > 0,
  pour les 3 seeds.
- **NEGATIVE**: borne haute IC95 < 0 pour au moins un seed, ou Δ moyen < 0.
- **INCONCLUSIVE**: sinon.
- Les secondaires (NFCorpus, FiQA) reçoivent leur propre étiquette avec les
  mêmes règles; « GENERALISES » n'est réclamé que si SciFact CONFIRMED **et**
  les deux secondaires CONFIRMED.

## Engagements
- Les résultats NEGATIVE/INCONCLUSIVE sont mergés et conservés tels quels;
  pas de retuning après TEST, pas de suppression de bras.
- Aucune affirmation de performance avant PR3. L'API PR2 documente
  explicitement qu'elle **n'est pas validée BEIR**.
- CCOS reste sur `UnavailableSmlRetrieval` jusqu'à une campagne CONFIRMED
  au moins contre BM25; le branchement éventuel est une PR CCOS séparée.
- Merge uniquement avec CI verte sur le SHA exact de la tête de PR
  (`gh pr merge --squash --match-head-commit`).

## Résultats
Aucun. (Section remplie par PR3.)
