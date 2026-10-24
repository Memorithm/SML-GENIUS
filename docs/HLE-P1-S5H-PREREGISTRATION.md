# HLE-P1 — mémoire booléenne et circuits hybrides sur S5h (préenregistrement)

Préenregistré le 2026-10-24, avant toute exécution sur S5h. Ce document suit le format de
`GB11-P1-S5H-PREREGISTRATION.md`. Code : `crates/sml-core/src/hle_p1.rs` et le binaire
`sml-hle-p1-s5h`. Statut : recherche, `NOT_PROMOTED`.

## Question

Sur des données massives, à budget égal à celui du témoin GB-11 P1, une question se pose.
Est-ce que des circuits logiques différentiables apportent un gain au-delà des comptes
d'ordre k et d'une mémoire cache+match ? Ces circuits sont routés, entraînés en straight-through
sur le résidu du mélangeur, avec ou sans espace de travail récurrent. Sur le proxy `docs/*.md`
(HLE-3/HLE-4), la réponse était non. P1 teste si cette conclusion tient à 10M, 100M et 1,2G
octets d'entraînement.

## Témoin

Il s'agit du run GB-11 P1 S5h, manifeste SHA-256
`966eb65f37c58488f8b449eabd7b7594fc5a056ca10db98df009adc789b8a09e`. Il a 15 068 983
paramètres et un contexte de 2 octets.

| Variante | DEV BPC | État |
|---|---|---|
| f32 | 3,655162232 | 200 003 584 o |
| q4 | 3,785251745 | 25 000 448 o |

Les meilleurs bras GB-11 P1 servent de repères (ils ne sont pas réexécutés ici) :

| Bras | DEV BPC |
|---|---|
| search_q4 | 3,500 |
| bytes_q4 | 3,021 |

## Données

Le corpus est S5h, mis en scène à chaque run par les stagers GB-11 (RemoteOps,
`ops/sml/hf500`, `ops/sml/s5h_pes2o`). Ses sources se trouvent dans `~/sml-corpora` du Dell.

| Partition | Fichiers | Octets |
|---|---|---|
| TRAIN | 48 981 | 1 206 546 716 |
| DEV | 9 512 | 60 256 970 |

- **TRAIN** est lu dans l'ordre du manifeste, en une passe.
- **DEV** compte 60 237 946 événements : on prédit les offsets `2..len` de chaque fichier, et le contexte est remis à zéro à chaque fichier.
- **TEST** n'est jamais lu. Le lecteur refuse toute ligne TEST.

Chaque document est lu seul. Le lecteur vérifie sa longueur, son SHA-256 et sa validité UTF-8,
avec les mêmes règles d'admission que GB-11 P1. La mémoire est donc bornée par les tables, pas
par le corpus.

Le balayage d'échelle porte sur un préfixe TRAIN : les documents entiers sont pris jusqu'à
atteindre ou dépasser 10 000 000 octets (10M), puis 100 000 000 octets (100M), puis TRAIN
complet (`full`). DEV reste toujours complet.

## Modèle et budget

L'état déployé de chaque bras est ≤ 25 000 448 octets, soit le budget q4 du témoin. Il
comprend :
- une table de comptes hachée (u16×2 par case, divisés par deux à saturation) ;
- la mémoire de match (2^19 positions) ;
- la fenêtre de cache (1 024 octets) ;
- les circuits durcis (4 bits par porte) ;
- le mélangeur.

La table de comptes reçoit le reste du budget. `state_bytes` est imprimé pour chaque bras.

Côté données, le budget correspond au témoin au niveau `full` : une passe sur TRAIN.

## Apprentissage

L'apprentissage est préquentiel : chaque octet TRAIN est codé, puis appris.

- **Comptes** : backoff Dirichlet (β = 2) sur les ordres 0..k.
- **Mélangeur** : logistique, un par profondeur de bit, taux 0,002. Il prend en entrée les stretch des estimations de chaque ordre, le cache, deux signaux de match, les sorties de circuit et un biais.
- **Circuits** : 16 pages routées par l'octet précédent, de largeurs [256, 256, 32], soit 4 sorties par profondeur. Chaque circuit est évalué une fois par octet. Il est entraîné en `StraightThrough` (forward dur) sur le gradient du mélangeur, c'est-à-dire sur son résidu, avec Adam (0,01) par lots de 16 octets par page.
- **Espace de travail récurrent** : un réseau logique [128, 32] prend l'état précédent (32 bits), l'octet courant et l'octet précédent. Son état entre dans les circuits. Il est entraîné par BPTT tronqué sur 4 octets, et remis à zéro à chaque document.

DEV est évalué une fois, avec tous les paramètres gelés.

## Bras

| Bras | Contenu | Graines |
|---|---|---|
| `order2` … `order6` | comptes seuls, ordre k | déterministe |
| `order5_memory` | ordre 5 + cache + match + mélangeur | déterministe |
| `residual_ste_s{g}` | + circuits entraînés | 1, 2, 3 |
| `residual_untrained_s{g}` | contrôle : circuits non entraînés | 1, 2, 3 |
| `residual_frozen_routing_s{g}` | contrôle : routage par position, pas par contexte | 1, 2, 3 |
| `residual_random_bits_s{g}` | contrôle : sorties pseudo-aléatoires | 1, 2, 3 |
| `recurrent_tbptt4_s{g}` | + espace de travail récurrent | 1, 2, 3 |
| `recurrent_state_reset_s{g}` | contrôle : état remis à zéro à chaque octet | 1, 2, 3 |

Cela fait 24 bras par échelle, soit 72 au total.

## Critères (sur DEV, moyenne des 3 graines)

On note Δ une différence de BPC sur DEV, à l'échelle `full` sauf mention contraire.

- **C1, cohérence** : chaque bras imprime `test=UNOPENED`, `dev_events = 60 237 946` et `state_bytes ≤ 25 000 448`. Sinon, le run est invalide.
- **C2, circuits** : `residual_ste` bat `order5_memory` d'au moins 0,010 BPC, et les 3 graines sont meilleures. Il bat aussi chacun des trois contrôles d'au moins 0,010 BPC.
- **C3, récurrence** : `recurrent_tbptt4` bat `residual_ste` d'au moins 0,010 BPC et `recurrent_state_reset` d'au moins 0,010 BPC, et les 3 graines vont dans le même sens.
- **C4, échelle** : le gain de C2 (ou de C3) à `full` est au moins égal à celui de 10M. On mesure si l'avantage croît avec les données.
- **C5, repère** : on compare le meilleur bras au témoin q4 (3,785) et à `bytes_q4` (3,021). C5 est descriptif, sans seuil.

Si C2 échoue, le résultat est négatif pour les circuits et il est publié tel quel. La
sélection éventuelle d'un bras se fait sur DEV seulement.

## Règles d'arrêt

- **S1** : un coût TRAIN non fini arrête le bras, et le run est marqué en échec.
- **S2** : un document dont la longueur, le SHA-256 ou l'UTF-8 diffère arrête le run.
- **S3** : un manifeste différent de `966eb65f…` arrête le run avant tout calcul.
- **S4** : le timeout du job s'applique ; aucune relance automatique. Un bras non terminé n'a pas de résultat.

Aucun hyperparamètre n'est réglé sur S5h entre les échelles.

## Traces

- `hle-p1-results.tsv` : une ligne `SML_HLEP1_V1` par bras, également diffusée dans le log du job.
- Une ligne `SML_HLEP1_PROGRESS` tous les 64 Mio de TRAIN.
- `SML_HLEP1_DONE` en fin de run.

## Exécution

L'exécution passe par le workflow RemoteOps `sml-hle-p1-s5h.yml`, déclenché manuellement.

- **Thor en priorité** (aarch64, 14 cœurs ; le code est CPU seul). Le job Thor n'entraîne que si un S5h mis en scène et vérifié (manifeste `966eb65f…`) est déjà présent. Sinon, il s'arrête avec `SML_HLEP1_S5H_ABSENT_ON_THOR`.
- **Une entrée Dell optionnelle** : elle reprend la mise en scène GB-11 telle quelle, dans le groupe de concurrence `remoteops-dell-t430`.
- Les bras tournent sur 12 fils, avec `nice`.

Estimations par bras (TRAIN + DEV complet), à partir du proxy mesuré sur la machine de
développement x86. Elles ne sont pas mesurées sur Thor et l'incertitude est d'environ ×2 :

| Bras | µs/octet | 10M | 100M | full |
|---|---|---|---|---|
| ordres 2–6 | ~1,5–3 | ~3 min | ~6 min | ~50 min |
| `order5_memory` | ~2,5 | ~3 min | ~7 min | ~55 min |
| résiduel, non entraîné | ~6 | ~7 min | ~16 min | ~2 h |
| résiduel entraîné, routage figé | ~18 | ~21 min | ~48 min | ~6,3 h |
| récurrent | ~50 | ~1 h | ~2,2 h | ~17,6 h |

Durée murale estimée avec 12 fils :

| Échelle | Durée |
|---|---|
| 10M | ~1 h |
| 100M | ~2,5 h |
| full | ~18–20 h (chemin critique : les bras récurrents) |

## Fumée locale (proxy `docs/*.md`, pas S5h)

`scripts/hle-p1-proxy-smoke.sh` utilise 313 fichiers TRAIN (2,13 Mo) et 44 fichiers DEV
(267 750 événements), avec la graine 1. DEV BPC :

| Bras | DEV BPC |
|---|---|
| order2 | 3,095 |
| order3 | 2,496 |
| order4 | 2,488 |
| order5 | 3,187 |
| order6 | 4,425 |
| order5_memory | 2,1458 |
| residual_ste | 2,1432 |
| residual_untrained | 2,1462 |
| residual_frozen_routing | 2,1456 |
| residual_random_bits | 2,1456 |
| recurrent_tbptt4 | 2,1442 |
| recurrent_state_reset | 2,1407 |

Sur ce proxy, les ordres élevés seuls sont mal calibrés par le backoff Dirichlet ; le
mélangeur corrige ce problème. Les écarts dus aux circuits sont ≤ 0,005 BPC, avec une seule
graine. Ils ne montrent rien : c'est un test de fumée.

## Ce que P1 ne montre pas

- Pas de résultat TEST.
- Pas de généralisation hors S5h.
- Pas de mesure d'énergie ni de latence sur matériel logique.
- Pas d'optimalité des hyperparamètres, qui sont fixés ici sans réglage sur S5h.
