# GB-11 P0 / P0b — résultats sur données synthétiques (23 octobre 2026)

Protocole : `docs/GB11-RELAXED-SPARSE-ANF-LEARNING-PREREGISTRATION.md`, gelé au commit `7915aad`, puis amendement P0b gelé au commit `612cc37`. Code : `crates/sml-core/src/gb11_relaxed_anf.rs`, binaire `sml-gb11-relaxed-anf`. Le P0 a tourné sur la source `f86a7c7` et le P0b sur `612cc37`, dans un conteneur cloud à 4 cœurs (pas sur Dell ni Thor). Les données brutes sont dans `research/results/gb11/P0-prereg.csv` (60 lignes) et `research/results/gb11/P0b.csv` (40 lignes). Les valeurs des tableaux sont des médianes sur 5 graines. TEST est mesuré contre le professeur sans bruit. Le plafond VAL est 0,95, car les étiquettes VAL sont bruitées.

## Réponse courte

- **La relaxation seule ne suffit pas à apprendre le câblage**, c'est-à-dire quels bits forment l'adresse d'une banque et quelle banque traite l'exemple. Le P0 préenregistré échoue : la STE fait s'effondrer les modèles. Le P0b `relaxed_fixed`, sans STE et avec pénalité de diversité, plafonne à 0,59 sur T-sparse.
- **Une recherche discrète exacte du câblage, relancée plusieurs fois, réussit.** Les experts y sont réajustés en forme close par vote majoritaire. Sur T-sparse, les 5 graines retrouvent la fonction exacte : 100 % des bits et 100 % des octets sur TEST, pureté de routage 1,0, k médian 5, p95 6, max 8. Le modèle reste donc dans le régime k ≤ 8 où GB-9 mesure un gain moteur.
- **L'exactitude d'exécution est totale.** Les banques apprises compilées en `Pvp6Bank<64>` donnent 0 écart sur 4 912 162 (P0) et 3 151 883 (P0b) vérifications, avec les bras `Selected` et `Bitsliced`.
- **Le partage d'entrées est garanti par construction, mais le contrat actuel le fragmente.** `Pvp6Bank` exige une densité uniforme. Les 8 portes d'une banque se répartissent donc en 3 à 5 sous-banques sur T-sparse, soit autant de transpositions.
- **Si la tâche exige des fonctions denses, on ne garde pas la parcimonie sans perdre en qualité.** Sur T-dense, `hem_lut` atteint 100 % avec k médian 33. Le coût λ = 3·10⁻³ ne retire que 16 % des monômes et coûte 2,9 points. C'est le domaine de la LUT.

## P0 (préenregistré) : échec

| Professeur | Bras | TEST bit | k médian | Pureté |
|---|---|---:|---:|---:|
| T-sparse | lut_ste | 0,533 | 30 | 0,35 |
| T-sparse | anf_xor | 0,498 | 32 | 0,32 |
| T-sparse | anf_cost_s / m / l | 0,505 / 0,495 / 0,503 | 31 / 32 / 33 | 0,36 / 0,44 / 0,32 |
| T-sparse | anf_concrete | 0,530 | 11 | 0,32 |
| T-dense | tous | 0,497 à 0,511 | 13 à 33 | 0,26 à 0,32 |

C1 est vrai, C3 et C5 sont faux. C2 n'est vrai que parce que tous les bras échouent ensemble. Les causes, diagnostiquées sur un professeur de mise au point hors protocole, sont détaillées dans l'amendement : la phase STE fait exploser la perte et la densité, et la sélection d'entrées par gradient se fige sur de mauvais bits.

## P0b (amendement) : réussite sur T-sparse

| Professeur | Bras | VAL bit | TEST bit | TEST octet | k méd. / p95 / max | Monômes | Pureté | Sous-banques / banque |
|---|---|---:|---:|---:|---:|---:|---:|---:|
| T-sparse | relaxed_fixed | 0,578 | 0,588 | 0,031 | 1 / 3 / 4 | 43 | 0,38 | 2 |
| T-sparse | hem_lut | 0,948 | **1,000** | **1,000** | 5 / 6 / 8 | 150 | **1,00** | 3,5 |
| T-sparse | hem_anf_cost_m | 0,948 | 1,000 | 1,000 | 5 / 6 / 8 | 150 | 1,00 | 3,5 |
| T-sparse | hem_anf_cost_l | 0,948 | 1,000 | 1,000 | 5 / 6 / 8 | 150 | 1,00 | 3,5 |
| T-dense | relaxed_fixed | 0,527 | 0,527 | 0,007 | 1 / 3 / 3 | 42 | 0,26 | 2 |
| T-dense | hem_lut | 0,949 | 1,000 | 1,000 | 33 / 40 / 40 | 1 063 | 1,00 | 7 |
| T-dense | hem_anf_cost_m | 0,948 | 0,999 | 0,991 | 32 / 40 / 40 | 1 051 | 1,00 | 7 |
| T-dense | hem_anf_cost_l | 0,925 | 0,971 | 0,861 | 28 / 37 / 40 | 893 | 1,00 | 7 |

Critères P0b sur T-sparse. Le bras retenu est `hem_anf_cost_l`, le plus grand λ à moins d'1 point en VAL de `hem_lut`.
- C1 : vrai (0 écart).
- C2 : vrai (1,000 contre 1,000).
- C3 : vrai (k médian 5 ≤ 8, p95 6 ≤ 16, 150 monômes, autant que `hem_lut`).
- C4 : vrai (écart relaxé − dur de 0).
- C5 : vrai (pureté 1,0, charge minimale 0,247).

**Verdict : la méthode hybride est démontrée sur ce professeur synthétique. La relaxation seule ne l'est pas.**

## Lecture honnête

- **La part du gradient est faible ici.** Le câblage gagnant vient 3 fois sur 5 de l'initialisation relaxée (redémarrage n° 0) et 2 fois sur 5 d'un redémarrage aléatoire. L'affinage ANF avec coût ne change rien sur T-sparse, car le vote majoritaire tombe déjà sur l'optimum exact. Sur ce problème, ce sont la recherche exacte et le réajustement en forme close qui font le travail.
- **La parcimonie vient du fait que la fonction est retrouvée exactement, pas de la régularisation.** Le Möbius d'une table **exacte** redonne une ANF parcimonieuse. Une table approximative donne une ANF dense, ce qu'on observe en P0 avec `lut_ste`. Le risque matériel annoncé est donc réel, mais il porte sur l'erreur d'apprentissage, pas sur la binarisation elle-même.
- **Il reste des monômes en trop.** L'élève compte 150 monômes contre 134 pour le professeur (max 8 contre 6), alors que la fonction est identique sur TEST. Hypothèse non vérifiée : les cellules d'adresse jamais visitées par une banque sont mises à 0 par défaut, alors qu'une affectation libre (« don't care ») permettrait une ANF plus courte. Une minimisation ANF qui exploite ces cellules est une amélioration directe.
- **Mise à l'échelle.** Une passe de recherche coûte créneaux × d × N. C'est trivial à d = 32. Avec un état de milliers de bits, il faudra présélectionner les candidats, et c'est là que le gradient de la relaxation peut servir de proposeur.
- **Limites.** Données iid, professeur de la même famille que l'élève, problème identifiable : ce sont des conditions favorables. Aucune qualité de modèle de langage, aucune généralisation hors distribution, aucun temps natif ni énergie n'est revendiqué.

## Conséquences pour le matériel

- GB-9 situe le premier R gagnant à 1 ou 2 portes par transposition quand k ≤ 8. Avec 8 portes réparties en 3,5 sous-banques en médiane, il reste environ 2,3 portes par transposition : on est dans le régime de gain, mais avec peu de marge.
- **Amendement de contrat recommandé** (PR séparée) : que `Pvp6Bank` accepte une densité déclarée comme borne supérieure, pour qu'une banque apprise n'ait qu'une seule transposition.

## Suites (accord du propriétaire requis)

1. **P1 — corpus réel** contre le témoin S5h (≈15 M paramètres, BPC 3,66), à budget égal. Il faut d'abord préenregistrer la présélection des candidats d'adresse et la minimisation « don't care ».
2. **P2 — mesure native Dell/Thor** des banques apprises via RemoteOps, protocole GB-9.
3. **Contrat `Pvp6Bank`** avec densité en borne supérieure, et parité exacte.
