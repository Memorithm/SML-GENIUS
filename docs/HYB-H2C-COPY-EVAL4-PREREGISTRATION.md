# HYB H-2c — expert de recopie sur EVAL4 : préenregistrement

**Statut : lancé le 6 novembre 2026, verdict `GO`** (voir `RESULTS-HYB-H2C-COPY-EVAL4.md`). EVAL4 a été lu une fois.

## Ce qui est jugé

L'expert de recopie de la troisième conception, qui a réussi sur la calibration (`COPY_FEASIBLE`, RemoteOps #864, résultats dans `RESULTS-HYB-H2C-COPY-FEASIBILITY.md`). Il est ajouté après la tête H-1d A1d :

- **Expert** : un modèle LZP par article, avec un contexte de 6 octets et une table de 18 bits. Il ne voit que les octets déjà décodés de l'article.
- **Mélangeur** : 48 décalages (longueur de la correspondance × profondeur), sans case « accord ». Chaque décalage est coupé à la marge qui reste avant la saturation (±2047), la même vers les deux bits.
- **Têtes** : les têtes A1d publiées (graines 11 et 23, `head-a1d-seed{11,23}.tsv`, sha256 `b77e3f13…` et `3c6192e8…`) et les poids P1 `7f4c233d…`, inchangés.

## Ce qui est figé

- **Les décalages.** Avant d'ouvrir EVAL4, la commande réapprend les décalages sur CAL-FIT, de façon déterministe. Elle refuse de continuer s'ils ne sont pas exactement ceux du troisième run (`h2c.tsv` sha256 `4644dca1…`). Cela vaut pour la recopie, pour le nul et pour le donneur, pour les deux graines. Les valeurs attendues sont écrites dans le code (`EVAL4_FROZEN_SHIFTS`). Rien n'est appris ni choisi sur EVAL4.
- **Le code** : `sml-hyb-h2-s5h copy-eval4`, au commit SML qui fusionne ce préenregistrement. Le workflow RemoteOps est épinglé sur ce SHA.
- **Les seuils**, ci-dessous.

## Données

EVAL4, ce sont les articles DEV de peS2o-0004 (split 5), préparés par `sml-pes2o-s5h-stage --eval4` exactement comme le prévoyait H-2 :

- même revue des droits peS2o ;
- plafond de 32 Mio ;
- audit de recouvrement `PASS_NARROW` contre tout S5h ;
- tracés par le P1 H-1d (`trace-h2-eval4`) dans ce run seulement.

EVAL4 n'a jamais été tracé ni lu : H-2 s'était arrêté à sa porte de calibration. Aucun article d'EVAL4 ne peut être aussi un article CAL (la commande le vérifie). Le DEV S5h, EVAL2, EVAL3, EVAL5 et TEST ne sont pas ouverts.

## Bras

Pour chaque graine :

- **A1d** : la tête seule (référence).
- **A1d + recopie**.
- **A1d + nul** : un octet et une longueur au hasard à chaque position, avec les décalages du nul appris sur CAL-FIT.
- **A1d + donneur** : la même recopie, mais elle cherche ses correspondances dans l'article EVAL4 suivant, jamais dans l'article codé.

Sont aussi publiés, sans seuil : la recopie sur P1 seul, la couverture et le taux de réussite de l'expert, la part des articles qui gagnent, et le gain médian par article.

## Critères (graines 11 et 23, toutes deux requises)

1. **Causalité.** Dans un article sur 16, la seconde moitié est remplacée par du bruit. Toutes les prédictions jusqu'au milieu doivent rester identiques. Une seule différence donne le verdict `COPY_CAUSALITY_FAIL`.
2. **Nul.** Le gain du nul sur A1d doit rester sous 0,002 BPC en valeur absolue. Sinon, le verdict est `NO_GO_NULL_CONTROL`.
3. **GO.** Les quatre conditions suivantes doivent toutes tenir :
   - le gain de la recopie sur A1d atteint au moins 0,01 BPC sur tout EVAL4 ;
   - la borne à 2,5 % de ce gain, dans un bootstrap de 1 000 tirages d'articles avec remise, atteint au moins 0,01 BPC ;
   - le gain de la recopie moins celui du donneur atteint au moins 0,01 BPC ;
   - la borne à 2,5 % de cette différence, dans le même bootstrap, atteint au moins 0,01 BPC.
4. Sinon, le verdict est `NO_GO`.

Pour mémoire, et sans valeur de critère : sur CAL-SEL, le gain valait 0,52 / 0,55 BPC, et 0,36 / 0,37 au-delà du donneur.

## Déroulement

Le run passe par un workflow RemoteOps sur le Dell, avec quatre étapes :

1. Il vérifie les sorties P1 et H-1d conservées, et qu'EVAL4 n'a jamais été tracé.
2. Il prépare EVAL4 et fait l'audit de recouvrement.
3. Il trace EVAL4 avec P1.
4. Il lance `copy-eval4` une seule fois.

La sortie `h2c-eval4.tsv` va dans un dossier neuf, nommé d'après le SHA, et n'est jamais écrasée. Le workflow ne s'exécute pas sur le Dell pendant un autre run en cours.

## Ce qui suit

- **Si c'est GO**, la recopie sera intégrée au moteur natif (coût ≤ 1,5× le moteur booléen, aller-retour exact, Dell = Thor), comme H-3 l'a fait pour la tête H-1d. Cela se fera par un préenregistrement séparé, sur un échantillon jamais lu.
- **Sinon**, le résultat est publié tel quel.

## Limites déclarées

- La conception a été retouchée trois fois sur la calibration avant de réussir. EVAL4 est son premier jugement sur un échantillon jamais vu, et c'est lui qui fait foi.
- Le coût n'est pas mesuré ici. Il le sera dans le moteur natif.
- Le corpus ne contient que des articles peS2o. Les deux premiers octets d'un article ne sont pas tracés par P1, donc l'expert ne les voit pas.

Research only. DEV non lu. EVAL4 non lu à ce jour. TEST fermé. NOT_PROMOTED.

## Erratum de transcription (6 novembre 2026, avant toute lecture de EVAL4 par `copy-eval4`)

Le premier lancement RemoteOps #1285 (run `42188087747`) s'est arrêté à la vérification des décalages CAL-FIT, avant l'ouverture de la trace EVAL4 par l'évaluateur. La constante figée du nul, graine 23, ne comportait que 47 valeurs au lieu des 48 prévues : un `0` avait été omis lors de la transcription. Le correctif rétablit **à l'identique** la suite de 48 décalages publiée dans le troisième run CAL (`40290453340`, `h2c.tsv` sha256 `4644dca13d6b6c35f100ea1b48c95d9a7ee5c12b11203bbba42fe1330af09fe8`). Les prédictions, algorithmes, seuils et modèles ne changent pas. Les traces EVAL4 déjà générées doivent être vérifiées par leurs sommes SHA-256, puis réutilisées sans nouvel entraînement ni nouveau tracé ; l'évaluation H-2c conserve une seule lecture sur EVAL4. Toute divergence doit bloquer l'exécution.
