# HYB H-2c — expert de recopie après la tête H-1d : résultats sur la calibration

## Premier run : `COPY_NOT_FEASIBLE`, à cause d'un contrôle nul mal conçu

Run : [RemoteOps #858](https://github.com/Memorithm/RemoteOps/pull/858), SML `c7bba16e`, Dell, 25 octobre 2026. `h2c.tsv` : sha256 `1c27a4830d66cdc758667523fe801047abbf229e44461ecb281e140e376800f8`. Seuls CAL-FIT et CAL-SEL ont été lus. EVAL4, le DEV et TEST n'ont pas été ouverts.

Contrôle de non-régression : A1d retrouve sur CAL-SEL (39 633 597 octets) les valeurs de H-2, soit 2,5540 et 2,5584 BPC. P1 seul fait 2,7126.

| Variante | Couverture | Réussite | Graine 11 | Gain | Graine 23 | Gain |
|---|---:|---:|---:|---:|---:|---:|
| A1d seule | | | 2,5540 | | 2,5584 | |
| Recopie, contexte 6 (retenu) | 0,813 | 0,651 | **2,0113** | **0,5427** | **1,9951** | **0,5633** |
| Recopie, contexte 8 | 0,757 | 0,557 | 2,0967 | 0,4573 | 2,0882 | 0,4702 |
| Recopie, contexte 12 | 0,718 | 0,364 | 2,2276 | 0,3264 | 2,2261 | 0,3322 |
| Contrôle nul (rythme de la recopie, octet au hasard) | 0,718 | 0,004 | 2,5511 | 0,0029 | 2,5534 | 0,0050 |

Sur CAL-FIT, avec un contexte de 6, le bit attendu est juste dans 90 % des décisions pour une correspondance qui vient d'être trouvée. Ce taux passe à 99,9 % au-delà de 64 octets confirmés.

**Le verdict figé est `COPY_NOT_FEASIBLE`** : la porte de gain passe largement, mais le contrôle nul dépasse son seuil de 0,002 pour les deux graines.

**Pourquoi le contrôle nul gagne.** Il ne prédisait qu'aux positions où l'expert de contexte 12 trouvait une correspondance, et avec ses longueurs. Il savait donc où le texte se répète. Ses décalages appris sont opposés deux à deux selon que la tête penche ou non vers le bit tiré au hasard (par exemple +320 et −320) : ils ne font que changer la confiance de la tête à ces positions. Ce n'est pas une fuite du futur, et ce gain est cent fois plus petit que celui de la recopie. Mais la règle était figée.

Sur décision du propriétaire, l'étude est refaite sur la calibration avec un nul entièrement aléatoire et six autres contrôles (voir la révision dans `HYB-H2C-COPY-FEASIBILITY.md`). Le préenregistrement pour EVAL4 viendra seulement si elle passe.

## Second run : `COPY_NOT_FEASIBLE`, le gain est réel mais deux contrôles le contestent

Run : [RemoteOps #861](https://github.com/Memorithm/RemoteOps/pull/861), SML `286478f0`, Dell, 25 octobre 2026. `h2c.tsv` : sha256 `08e07c7fd45983f9649e336915a44f3fa57507595c061c834a69391bd1ce9286`. Seuls CAL-FIT et CAL-SEL ont été lus. Les critères étaient figés avant le run (révision dans `HYB-H2C-COPY-FEASIBILITY.md`). Les gains sont mesurés sur A1d, pour les graines 11 et 23.

| Critère | Mesure (graine 11 / 23) | Seuil | Résultat |
|---|---|---|---|
| Porte de gain (contexte 6 retenu) | 0,5427 / 0,5633 | ≥ 0,01 | passe |
| Nul aléatoire à toutes les positions | 0,0044 / 0,0056 | ≤ 0,002 | **échoue** |
| Donneur (recopie depuis un autre document) | 0,173 / 0,192 | ≤ 10 % du gain | **échoue** |
| Décalage de 4096 octets | 0,0075 / 0,0121 | ≤ 10 % du gain | passe |
| Bootstrap par document (IC 95 %) | [0,518 ; 0,573] / [0,542 ; 0,593] | borne basse ≥ 0,01 | passe |
| Rôles échangés (FIT ↔ SEL) : gain / nul | 0,580 / 0,598 ; nul 0,0050 / 0,0052 | nul ≤ 0,002 | **échoue** |
| Causalité (octets futurs empoisonnés) | 0 écart sur 1 129 099 positions | 0 | passe |

Mesures rapportées sans critère :

- Les 1530 documents de CAL-SEL gagnent tous. Gain médian par document 0,187 / 0,192, 10e centile 0,150 / 0,152.
- Nul au rythme de la recopie (celui du premier run) : 0,0029 / 0,0049.
- Un seul décalage global au lieu de 96 : 0,418 / 0,447 (décalages 416 / 424).
- Recopie sur P1 seul : 0,652 sur P1.
- COPY-1 (SML #725, mélange linéaire à 20 %, actif sur 77 % des décisions) : **−0,224 / −0,244**, donc pire que A1d seule.

**Le verdict figé est `COPY_NOT_FEASIBLE`.** EVAL4 reste fermé.

**Lecture.**

- **Les nuls (0,004 à 0,006) ne lisent aucun signal de recopie.** Leurs décalages sont opposés deux à deux selon que la tête penche ou non vers le bit tiré au hasard. Le mélangeur recalibre donc seulement la confiance de la tête A1d. Ce gain existe sans aucun expert, et il est cent fois plus petit que celui de la recopie. Il vient de la séparation des cellules selon l'accord avec la tête, pas d'une fuite.
- **Le donneur gagne 0,17 à 0,19.** Une part de la recopie (environ un tiers) est du vocabulaire partagé entre articles scientifiques. Le reste, environ 0,37, vient de répétitions internes au document.
- **Le gain est causal et robuste.** Rien ne vient du futur, il tient sur tous les documents, avec l'échange des rôles et le décalage.
- **COPY-1 nuit.** Un mélange linéaire fixe, sans tenir compte de la longueur de la correspondance, coûte plus qu'il ne rapporte.

Research only. DEV non lu. EVAL4 non lu. TEST fermé. NOT_PROMOTED.
