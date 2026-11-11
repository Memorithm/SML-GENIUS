# HYB H-3c — l'expert de recopie dans le moteur natif, sur un échantillon neuf : préenregistrement

**Gelé le 6 novembre 2026, avant tout run H-3c et avant toute lecture d'EVAL6.** Sur EVAL4, l'expert de recopie H-2c a obtenu `GO` (RemoteOps #1285, `RESULTS-HYB-H2C-COPY-EVAL4.md`). Son préenregistrement prévoyait l'étape suivante : l'intégrer au moteur natif comme H-3 l'a fait pour la tête H-1d, avec un coût ≤ 1,5× le moteur booléen, un aller-retour exact, Dell = Thor, et par un préenregistrement séparé sur un échantillon jamais lu. C'est ce document. Le propriétaire a demandé, le 6 novembre 2026, que le programme avance en autonomie, en prenant l'option recommandée.

## Question

> Ajoutée au moteur hybride natif (P1 gelé, tête A1d, codeur arithmétique binaire), décision par décision comme un décodeur, la recopie donne-t-elle exactement les probabilités calculées à la manière des traces, retrouve-t-elle ses octets, et compresse-t-elle mieux que la tête seule sur un échantillon jamais lu, pour un coût par octet borné et des sorties identiques sur Dell et Thor ?

## Le moteur (`hyb_h3_engine.rs`, `HybridHead::single_with_h2c_copy`)

- **P1 et tête.** Les mêmes que H-3 : P1 `bytes_q4` gelé de H-1d (`7f4c233d…`), têtes A1d publiées (`b77e3f13…`, `3c6192e8…`). Le programme vérifie les trois SHA.
- **Recopie.** C'est l'expert jugé sur EVAL4 : un `MatchModel` LZP par document, avec un contexte de 6 octets et une table de 2^18 entrées. Il ne voit que les octets déjà décodés à partir de la position 2, comme sur les traces. À chaque décision, tant que les bits déjà décodés de l'octet courant s'accordent avec l'octet prédit, la recopie fournit un bit attendu. Le stretch de la tête est alors déplacé par le décalage de sa case (longueur de la correspondance × profondeur), coupé à la marge qui reste avant la saturation (`CopyMixer::stretch`).
- **Décalages.** Ce sont les 48 décalages par graine du troisième run CAL (`h2c.tsv` 4644dca1…), ceux qui ont été revérifiés et jugés sur EVAL4. Ils sont écrits dans le code (`EVAL4_GO_COPY_SHIFTS`), et un test vérifie qu'ils sont identiques à ceux de `copy-eval4`. Rien n'est appris ni choisi en H-3c.
- **Codeur.** C'est celui de H-3 : le conteneur est la longueur sur 8 octets suivie du flux codé, et les deux premiers octets sont codés à 1/2.
- **Oracle.** `oracle_p1s` refait le calcul à la manière des traces. Il passe d'abord la recopie sur les octets tracés, puis applique `code_document`, l'oracle scalaire de la tête et le même mélangeur.
- Avec des décalages nuls, le moteur redonne exactement H-1d (test). Le fichier de déterminisme de H-3 (`deterministic_csv`) n'est pas modifié.

## L'échantillon EVAL6

- **Source.** Le fichier peS2o-0008 (épinglé dans le stager, sha256 `e1180b6d…`).
- **Règle.** Seuls les articles du seau DEV de la découpe S5h (1 article sur 32) sont pris. Le stage S6 a utilisé peS2o-0008 pour son TRAIN, mais un article du seau DEV n'entre jamais dans le TRAIN. Le DEV de S6 est celui de S5h, pris en entier dans peS2o-0001. Aucun article d'EVAL6 n'a donc jamais été lu.
- **Filtres, doublons, taille, audit.** Les mêmes que pour EVAL2 à EVAL5 :
  - exclusion de tout article déjà présent dans S5h ;
  - arrêt à 32 Mio ;
  - audit de recouvrement `PASS_NARROW`, avec les lignes EVAL6 étiquetées DEV.
- EVAL6 n'est préparé qu'au lancement de H-3c. Il n'est lu qu'une fois, par `sml-gb11-p1-s5h engine-h3c-eval6`.
- peS2o-0006 (BAF-H4) et peS2o-0007 (corpus G1) restent réservés et ne sont pas ouverts.

## Bras

- **B0** : P1 seul.
- **B1** : P1 et tête A1d, graines 11 et 23. C'est le moteur H-3.
- **B2** : P1, tête A1d et recopie, graines 11 et 23.

Chaque document d'EVAL6 est compressé puis décompressé par les cinq bras, alternés document par document.

## Critères (figés)

1. **Exactitude** (sinon `NO_GO_EXACTNESS`) : pour les cinq bras, il faut 0 écart avec l'oracle sur toutes les décisions et 0 échec d'aller-retour.
2. **Qualité** (sinon `NO_GO`). Pour les deux graines, en bits de conteneur par octet :
   - BPC(B1) − BPC(B2) > 0,02 ;
   - ce gain doit aussi dépasser 2 × l'écart de BPC(B2) entre les graines.
3. **Coût** (sinon `GO_DIAGNOSTIC_COST`). Pour les deux graines :
   - le rapport des temps de compression B2/B0 et celui des temps de décompression B2/B0 sont ≤ 1,5 ;
   - ces temps sont mesurés sur le Dell, dans la même boucle, de bout en bout.
   - Les rapports B2/B1 sont publiés sans critère.
4. **Déterminisme Dell/Thor** (sinon `NO_GO_DETERMINISM`, quel que soit le reste) :
   - `engine-h3c-synthetic` écrit `deterministic.csv`, sans lire aucune donnée. Il reprend les lignes de H-3 et ajoute la tête avec recopie, ainsi qu'un document très répétitif.
   - Toutes les cellules doivent passer, oracle et aller-retour compris.
   - Le SHA-256 du fichier doit être identique sur Dell et Thor.
5. **Lecture.**
   - **GO.** La recopie est démontrée dans le moteur natif : exacte, réversible, meilleure que la tête seule sur un quatrième échantillon neuf, d'un coût borné et identique sur deux architectures.
   - **Sinon.** Le verdict dit quelle propriété manque.
6. **Rapport.** `h3c.tsv` publie tout, sans sélection : SHA du manifeste, d'EVAL6, des poids, des tables et des têtes, décalages, puis pour chaque bras les octets et BPC de conteneur et de modèle, les µs/octet et le SHA de la suite des conteneurs. Il publie aussi les gains, l'écart entre graines, les rapports de coût, `dev_read false`, `test UNOPENED`, le commit source et le verdict. Les conteneurs restent sur le Dell.

## Ce que H-3c ne démontre pas

- Une généralisation au-delà d'articles peS2o.
- Un modèle qui apprend en ligne : P1, la tête et le mélangeur sont gelés. Seule la table de recopie se remplit, document par document.
- Un gain d'énergie, qui n'est pas mesuré.

Research only. DEV non lu. TEST fermé. NOT_PROMOTED.
