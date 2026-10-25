# HYB H-1d-cost — re-mesure du coût de la tête H-1d avec un noyau fusionné : résultats

**Verdict préenregistré : COST_PASS. H-1d passe donc de GO_DIAGNOSTIC_COST à GO.** Avec le noyau fusionné, les mêmes têtes donnent les mêmes corrections sur tout EVAL3 et coûtent 0,151 µs par position au lieu de 0,662 µs. Le ratio de coût tombe de 1,74× à 1,17×, sous la limite de 1,5×. Le protocole est `HYB-H1D-COST-PREREGISTRATION.md`, gelé avant la mesure.

## Exécution

- **Code :** SML `b4360549` (fusion de #710).
- **Run :** RemoteOps #833, run 40181869799, le 25 octobre 2026, sur le Dell seul.
- **Entrées :** les sorties du run H-1d (RemoteOps #827), restées sur le Dell. Les sommes SHA-256 de P1 et du run sont vérifiées, ainsi que `h1d.tsv` (`d7d42500…`) et les deux têtes (`b77e3f13…`, graine 11 ; `3c6192e8…`, graine 23).
- **Pas de nouvelle donnée, pas d'entraînement, pas de sélection.** EVAL3 est relu seulement pour vérifier l'identité et pour chronométrer. `cost.tsv` : SHA-256 `c223bcdc…4a03`.

## Identité

| Contrôle | Résultat |
| --- | --- |
| Corrections du noyau fusionné = noyau H-1d, sur les 33 556 838 positions et pour les 2 graines | 0 écart |
| BPC EVAL3, graine 11 | 2,435427109 (identique à H-1d) |
| BPC EVAL3, graine 23 | 2,442609158 (identique à H-1d) |
| A0 (P1) | 2,594717482 (identique à H-1d) |

## Coût (µs par position, EVAL3)

| P1 (figé, mesuré pendant H-1d) | Tête, noyau fusionné | Tête, noyau H-1d (contrôle, même boucle) | Ratio |
| --- | --- | --- | --- |
| 0,8696 | **0,1505** | 0,6616 | **1,173** (limite 1,5) |

Le contrôle mesuré dans la même boucle (0,662 µs) retrouve le coût publié par H-1d (0,647 µs). Le gain vient donc bien du noyau, et pas d'une machine plus rapide ou moins chargée.

## Critères H-1d après re-mesure

| Critère | Résultat |
| --- | --- |
| Porte CAL-SEL ≥ 0,03 | Oui (0,159 et 0,154) |
| Exactitude | 0 écart, Dell = Thor (H-1d), et noyau fusionné = noyau H-1d |
| Gain EVAL3 > 0,02 pour les deux graines | Oui (0,159 et 0,152) |
| Gain > 2 × l'écart entre graines | Oui (écart 0,007) |
| Coût ≤ 1,5× | **Oui : 1,17×** |
| Verdict H-1d | **GO** |

## Ce que ce résultat ne démontre pas

- Ni le routage MOR (H-2), ni un gain moteur natif (H-3), ni un gain d'énergie.
- Ni une généralisation au-delà d'articles scientifiques peS2o.
- Le temps P1 n'a pas été re-mesuré : c'est celui de H-1d, comme prévu par le préenregistrement.
- H-2 demande son propre préenregistrement et l'accord du propriétaire.

Research only. TEST reste fermé. NOT_PROMOTED.
