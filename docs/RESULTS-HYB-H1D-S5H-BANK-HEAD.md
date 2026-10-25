# HYB H-1d — tête ternaire qui voit chaque banque, jugée sur EVAL3 : résultats

**Verdict préenregistré : GO_DIAGNOSTIC_COST.** Sur `bytes_q4`, la tête H-1d gagne 0,159 et 0,152 BPC sur EVAL3, avec un écart entre graines de 0,007. Le critère de gain est donc largement passé. Le coût vaut 1,74×, au-dessus de la limite de 1,5× : c'est le seul critère raté. Le résultat est publié tel quel, conformément à `HYB-H1D-S5H-BANK-HEAD-PREREGISTRATION.md`. H-2 n'est pas autorisé tant que le coût n'est pas réglé.

## Exécution

- **Code :** SML `d8ca5c39` (fusions de #701 et #702).
- **Run :** RemoteOps #827, run 40126579964, les 24 et 25 octobre 2026. Sur le Dell : P1 `run-h1d`, porte, `trace-h1d-eval3`, puis jugement. Ensuite, rejeu des têtes sur Thor.
- **Corpus :** manifeste S5h `966eb65f…`.
  - CAL-FIT : 41 249 607 positions ; CAL-SEL : 39 633 597 positions.
  - EVAL3 : 40 articles du seau DEV de peS2o-0003, 33 556 838 positions. EVAL3 n'a été tracé qu'après la porte, et lu une seule fois.
  - Le DEV S5h n'a pas été ouvert (`dev_read false`).
- **Contrôles :**
  - les poids P1 sont ceux des runs H-1b et H-1c (`7f4c233d…`) ;
  - les têtes jugées sur EVAL3 sont identiques octet pour octet à celles de la porte ;
  - 0 écart entre le noyau popcount et l'oracle sur EVAL3 ;
  - le rejeu des 2 têtes sur Thor donne les mêmes empreintes que sur le Dell ;
  - `h1d.tsv` : SHA-256 `d7d42500…05db`.

## Étape 1 : porte CAL-SEL (seuil 0,03)

| Graine | A0 (P1) | A1d | Gain | A1c (contrôle H-1c) | τ | λ | Pas `unit` | Poids nuls |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 11 | 2,712566 | 2,554043 | **0,1585** | 2,692960 | 2 | 1 | 108 à 145 | 88 % |
| 23 | 2,712566 | 2,558374 | **0,1542** | 2,693043 | 1 | 1/2 | 70 à 113 | 76 % |

La porte est passée pour les deux graines.

## Étape 2 : EVAL3 (BPC)

| A0 (P1) | A1c, graine 11 | A1c, graine 23 | A1d, graine 11 | A1d, graine 23 | Écart entre graines |
| --- | --- | --- | --- | --- | --- |
| 2,594717 | 2,579212 (−0,0155) | 2,579360 (−0,0154) | **2,435427** (−0,1593) | **2,442609** (−0,1521) | 0,0072 |

## Critères

| Critère | Résultat |
| --- | --- |
| Porte CAL-SEL ≥ 0,03 pour les deux graines | Oui (0,159 et 0,154) |
| Exactitude | 0 écart, Dell = Thor |
| Gain EVAL3 > 0,02 pour les deux graines | **Oui** (0,159 et 0,152) |
| Gain > 2 × l'écart entre graines (0,014) | Oui |
| Coût ≤ 1,5× | **Non** : 1,74× (P1 0,870 µs + tête 0,647 µs) |
| Verdict | **GO_DIAGNOSTIC_COST** |

## Lecture (diagnostic, hors critères)

- **Les entrées par banque font tout le gain.** Sur le même CAL et le même EVAL3, la tête H-1c de contrôle ne gagne que 0,015 BPC, comme dans H-1c sur EVAL2. Avec un thermomètre de stretch par banque, le gain est multiplié par dix. Le mélangeur fixe de P1 laissait donc beaucoup d'information inexploitée.
- **Le gain se transfère de CAL-SEL à EVAL3.** Il vaut 0,158 et 0,154 sur CAL-SEL, puis 0,159 et 0,152 sur EVAL3. Contrairement à H-1c, il n'y a pas de perte notable entre la sélection et le jugement.
- **Les deux graines retiennent des (τ, λ) différents,** mais l'écart final reste faible (0,007 BPC).
- **Le coût vient du chemin d'évaluation.** À chaque profondeur, la tête reconstruit les 172 entrées, y compris les thermomètres des 8 banques. Un noyau qui construit l'activation une seule fois par octet donnerait des corrections identiques. Il n'a pas été mesuré dans ce run.
- **Rien ici n'autorise H-2.** EVAL3 a maintenant été lu une fois. Une suite, par exemple une re-mesure du seul coût avec les mêmes têtes, demande un préenregistrement et l'accord du propriétaire.

## Ce que H-1d ne démontre pas

- Ni le routage MOR (H-2), ni un gain moteur natif (H-3), ni un gain d'énergie.
- Ni une généralisation au-delà d'articles scientifiques peS2o.
- Ni que la tête bat un mélangeur logistique continu : ce bras n'a pas été testé.

Research only. TEST reste fermé. NOT_PROMOTED.
