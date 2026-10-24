# HYB H-1c — tête ternaire moyennée, jugée sur EVAL2 : résultats

**Verdict préenregistré : NO_GO pour les deux bases.** La tête est désormais stable : l'écart entre graines tombe à 0,0002 BPC sur `bytes_q4`. Le coût passe aussi sur `bytes_q4` (1,32×, limite 1,5×). En revanche, le gain sur EVAL2 n'atteint pas le seuil de 0,02 BPC : il vaut 0,0159 et 0,0157. Le résultat est publié tel quel, conformément à `HYB-H1C-S5H-STABLE-RESIDUAL-PREREGISTRATION.md`.

## Exécution

- **Code :** SML `d262cafc` (fusion de #695).
- **Run :** RemoteOps #825, run 40109672713, le 24 octobre 2026. Le P1 `run-h1c` et H-1c ont tourné sur le Dell, puis les têtes ont été rejouées sur Thor.
- **Corpus :** manifeste S5h `966eb65f…`.
  - CAL-FIT : 41 249 607 positions ; CAL-SEL : 39 633 597 positions.
  - EVAL2 : 42 articles du seau DEV de peS2o-0002, 33 567 121 positions, lus une seule fois, à la fin.
  - Le DEV S5h n'a pas été ouvert par H-1c (`dev_read false`).
- **Contrôles :**
  - les poids P1 ont les mêmes SHA-256 que ceux du run H-1b ;
  - 0 écart entre le noyau popcount et l'oracle ;
  - le rejeu des 4 têtes sur Thor donne les mêmes empreintes que sur le Dell ;
  - `h1c.tsv` : SHA-256 `e872e890…8971`.

## Résultats (EVAL2, BPC)

| Base | A0 (P1) | A0c (320 o) | A1, graine 11 | A1, graine 23 | Écart entre graines |
| --- | --- | --- | --- | --- | --- |
| `search_q4` | 3,049590 | 3,049590 (λ = 0) | 3,038602 (−0,0110) | 3,038115 (−0,0115) | 0,0005 |
| `bytes_q4` | 2,591276 | 2,591276 (λ = 0) | **2,575425** (−0,0159) | 2,575597 (−0,0157) | 0,0002 |

Les réglages retenus sur CAL-SEL :

| Base | Graine | τ | λ | Pas `unit` (stretch) | Poids nuls | Gain sur CAL-SEL |
| --- | --- | --- | --- | --- | --- | --- |
| `search_q4` | 11 | 1 | 1/4 | 46 à 122 | 82 % | 0,0131 |
| `search_q4` | 23 | 1 | 1/4 | 46 à 123 | 83 % | 0,0132 |
| `bytes_q4` | 11 | 1 | 1/8 | 69 à 92 | 79 % | 0,0196 |
| `bytes_q4` | 23 | 1 | 1/8 | 69 à 92 | 79 % | 0,0195 |

## Critères

| Critère | `search_q4` | `bytes_q4` |
| --- | --- | --- |
| Gain > 0,02 pour les deux graines | Non (0,0110 et 0,0115) | **Non** (0,0159 et 0,0157) |
| Gain > 2 × l'écart entre graines | Oui | Oui |
| Coût ≤ 1,5× | Non : 1,82× (P1 0,329 µs + tête 0,268 µs) | Oui : 1,32× (P1 0,847 µs + tête 0,271 µs) |
| Exactitude | 0 écart, Dell = Thor | 0 écart, Dell = Thor |
| Verdict | NO_GO | NO_GO |

## Lecture (diagnostic, hors critères)

- **La moyenne sur plusieurs époques a réglé la stabilité.** L'écart entre graines passe de 0,0143 (H-1b, DEV) à 0,0002 (EVAL2). Les deux graines retiennent maintenant les mêmes (τ, λ).
- **Le gain plafonne avant le seuil, dès CAL-SEL.** Sur `bytes_q4`, le gain vaut 0,0196 sur CAL-SEL et 0,0158 sur EVAL2. Avec ces entrées, la tête semble avoir épuisé ce qu'elle peut corriger. Elle ne voit que le logit déjà mélangé, la confiance par banque, les octets de contexte et le registre de mot. Elle ne peut pas repondérer les banques, alors que la règle de mélange de P1 est fixe.
- **EVAL2 est plus facile que le DEV S5h** (2,59 contre 3,02 BPC pour `bytes_q4`), car il ne contient que des articles peS2o. Les gains ne sont pas comparables à ceux de H-1b sur le DEV.
- **Le coût de la tête vient encore du chemin d'évaluation.** Les 0,27 µs incluent la reconstruction des entrées à chaque profondeur. La tête fusionnée de #699 descend à environ 24 ns par octet sur un banc synthétique ; elle n'a pas été mesurée dans ce run.
- **Rien ici n'autorise H-2.** EVAL2 a maintenant été lu une fois. Toute suite demande un nouveau préenregistrement, un échantillon jamais lu et l'accord du propriétaire.

NOT_PROMOTED.
