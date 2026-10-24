# HYB H-1b — tête ternaire résiduelle calibrée sur S5h : résultats

**Verdict préenregistré : NO_GO pour les deux bases.** La tête apporte désormais du BPC sur le DEV : jusqu'à −0,041 sur `bytes_q4` et −0,029 sur `search_q4`. Mais pour une graine sur chaque base, le gain ne dépasse pas deux fois l'écart entre les graines. Le coût dépasse aussi la limite de 1,5×. Le résultat est publié tel quel, conformément à `HYB-H1B-S5H-CALIBRATED-RESIDUAL-PREREGISTRATION.md`.

## Exécution

- **Code :** SML `00504204`, dont l'arbre est identique au commit de main `4a7d2fe9` (squash de #689).
- **Run :** RemoteOps #823, run 40093854277, le 24 octobre 2026. Le P1 `run-h1b` et H-1b ont tourné sur le Dell, puis les têtes ont été rejouées sur Thor.
- **Corpus :** manifeste S5h `966eb65f…`. CAL représente 1/16 des fichiers TRAIN, exclus de la recherche et de l'entraînement :
  - CAL-FIT : 41 249 607 positions ;
  - CAL-SEL : 39 633 597 positions ;
  - DEV : 60 237 946 positions, lu une seule fois. C'est sa seconde lecture : la première a eu lieu en H-1.
- **Contrôles :**
  - le BPC P1 recalculé depuis les traces est identique à `trace.json` ;
  - 0 écart entre le noyau et l'oracle sur tout le DEV ;
  - les SHA des 4 têtes sont identiques entre Dell et Thor.

## Résultats (DEV S5h, BPC)

| Base | A0 (P1) | A0c (320 o) | A1, graine 11 | A1, graine 23 | Écart entre graines |
| --- | --- | --- | --- | --- | --- |
| `search_q4` | 3,483650 | 3,483650 (λ = 0) | **3,454229** (−0,0294) | 3,466258 (−0,0174) | 0,0120 |
| `bytes_q4` | 3,019091 | 3,019091 (λ = 0) | **2,978200** (−0,0409) | 2,992529 (−0,0266) | 0,0143 |

Les réglages retenus sur CAL-SEL :

| Base | Graine | τ | λ | Pas `unit` (stretch) | Poids nuls |
| --- | --- | --- | --- | --- | --- |
| `search_q4` | 11 | 1 | 1/2 | 47 à 105 | 81 % |
| `search_q4` | 23 | 1 | 1/2 | 46 à 113 | 84 % |
| `bytes_q4` | 11 | 0,5 | 1/4 | 34 à 54 | 55 % |
| `bytes_q4` | 23 | 1 | 1/8 | 70 à 99 | 81 % |

Le témoin S5h f32 est à 3,655 BPC. `bytes_q4` seul fait déjà mieux de 0,64 BPC.

## Critères

| Critère | `search_q4` | `bytes_q4` |
| --- | --- | --- |
| Gain > 0,02 pour les deux graines | Non (graine 23 : 0,0174) | Oui (0,0409 et 0,0266) |
| Gain > 2 × l'écart entre graines | Non (graine 23 : 0,0174 < 0,0241) | **Non, de peu** (graine 23 : 0,0266 < 0,0287) |
| Coût ≤ 1,5× | Non : 3,25× (P1 0,334 µs + tête 0,750 µs) | Non : 1,89× (P1 0,843 µs + tête 0,749 µs) |
| Exactitude | 0 écart, Dell = Thor | 0 écart, Dell = Thor |
| Verdict | NO_GO | NO_GO |

## Lecture (diagnostic, hors critères)

- **Les deux corrections de H-1b ont fonctionné.** H-1 perdait 0,32 BPC face à P1. Le même type de tête, entraîné dans le régime du modèle gelé puis exporté avec une échelle, gagne maintenant de 0,017 à 0,041 BPC sur les quatre combinaisons base × graine. La table booléenne de même taille, elle, n'apporte toujours rien (λ = 0).
- **Ce qui manque relève de la stabilité, pas du signe.** Avec une seule époque, l'ordre des documents (la graine) suffit à changer le τ retenu sur `bytes_q4` (0,5 contre 1). Cela crée un écart de 0,014 entre graines. Le critère d'écart échoue alors d'environ 0,002 BPC.
- **Le coût vient de l'implémentation actuelle de la tête, pas du calcul ternaire.**
  - Les 0,75 µs par position correspondent à 8 profondeurs × (construction des entrées + `BitplaneActivation` + appel du noyau qui alloue un `Vec`).
  - Le calcul utile se réduit à 4 popcounts de mots u64 par profondeur.
  - Ce chiffre n'est pas une mesure du coût natif, qui relève de H-3.
- **Rien ici n'autorise H-2.** Toute suite demande un nouveau préenregistrement, dont le DEV serait la troisième lecture, et l'accord du propriétaire.

NOT_PROMOTED.
