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

Research only. DEV non lu. EVAL4 non lu. TEST fermé. NOT_PROMOTED.
