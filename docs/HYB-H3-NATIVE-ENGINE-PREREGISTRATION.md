# HYB H-3 — le moteur hybride natif, de bout en bout, sur un échantillon neuf : préenregistrement

**Gelé le 25 octobre 2026, avant toute exécution H-3, avant toute lecture d'EVAL5 et avant de connaître le verdict H-2.** Le 25 octobre 2026, le propriétaire a demandé de continuer « sur le reste » pour obtenir un ensemble complet démontré : H-2 (routage MOR), puis H-3 (moteur natif). La proposition d'origine (option B) définit H-3 comme une campagne native Dell/Thor au commit fusionné, avec journaux et SHA comme GB-9.

Ce document ne révise aucun verdict publié (H-1d : GO ; H-2 : en cours au moment du gel).

## Question

H-1d et H-2 ont jugé des têtes sur des **traces** : P1 écrivait ses logits et ses stretches de banques sur disque, et les têtes étaient notées sur ces fichiers. Un vrai moteur ne voit pas l'octet avant de le décoder. H-3 demande :

> Le moteur hybride, exécuté comme un vrai compresseur sans perte (P1 gelé, plus la tête, plus un codeur arithmétique binaire), décision par décision comme un décodeur, donne-t-il exactement les probabilités jugées sur traces, retrouve-t-il ses octets, et compresse-t-il mieux que P1 seul sur un échantillon jamais lu, pour un coût par octet presque inchangé et des sorties identiques sur Dell et Thor ?

## Le moteur (`hyb_h3_engine.rs`)

- **P1 pas à pas.** `Model::frozen_blocks` puis `Model::mix` donnent le stretch de chaque décision à partir des seuls bits déjà décodés. `code_byte`, qui a produit toutes les traces, utilise désormais le même `mix` : c'est le même code. Lecture seule : le modèle gelé n'est jamais modifié.
- **Tête.** `FusedBankHead::correction` calcule la correction d'une seule décision, avec ce qu'un décodeur connaît à ce moment-là : les bits de la cible au-dessus, le logit et les banques de cette décision, le contexte, la confiance. Pour H-2, le routeur choisit l'expert à la décision 0. Un expert non admis laisse P1 seul.
- **Codeur.** Le codeur binaire de `bool_coder` (ENG-1d/BAC) reçoit la probabilité `squash(stretch + correction) << 16`, un entier. Le conteneur est la longueur (8 octets) suivie du flux codé. Les deux premiers octets d'un document sont codés à 1/2, comme P1 qui ne les prédit pas.
- **Oracle.** `oracle_p1s` recalcule les mêmes probabilités à la manière des traces : `code_document`, oracle scalaire de la tête, oracle scalaire du routeur.

## Têtes jugées (règle figée avant le verdict H-2)

- **Si le verdict H-2 est `GO`** : `routed`, avec les experts A2 des graines 11 et 23 (`experts-a2-seed{11,23}.tsv` du run H-2, vérifiés par `h2-sha256sums.txt`), leur routeur et leur masque d'admission.
- **Sinon** (`GO_DIAGNOSTIC_COST`, `NO_GO`, `NO_GO_CAL_GATE`, `NO_GO_NULL_CONTROL`) : `single`, avec les têtes A1d publiées de H-1d (`b77e3f13…` et `3c6192e8…`, vérifiées par le programme).
- Aucun apprentissage, aucune sélection en H-3. P1 est le modèle `bytes_q4` gelé de H-1d (`7f4c233d…`, vérifié par le programme).

## L'échantillon EVAL5

- **Source.** Le fichier peS2o-0005 (épinglé dans le stager). Ni le stage S5h, ni EVAL2, EVAL3 ou EVAL4 ne l'ont ouvert.
- **Règle, filtres, doublons, taille et audit.** Les mêmes que pour EVAL2 à EVAL4 : articles du seau DEV de la découpe S5h seulement, exclusion de tout article déjà présent dans S5h, arrêt à 32 Mio, audit de recouvrement `PASS_NARROW` avec les lignes EVAL5 étiquetées DEV.
- EVAL5 n'est préparé qu'au lancement de H-3 et n'est lu qu'une fois, par `sml-gb11-p1-s5h engine-h3-eval5`.
- **Limite déclarée.** EVAL5 ne contient que des articles peS2o.

## Bras

- **B0 — moteur booléen seul** : P1 gelé dans le même moteur et le même codeur.
- **B1 — moteur hybride**, graines 11 et 23 : P1 gelé plus la tête retenue ci-dessus.

Chaque document d'EVAL5 est compressé puis décompressé par chacun des trois bras. Les bras sont alternés document par document.

## Critères (figés)

1. **Exactitude** (sinon `NO_GO_EXACTNESS`) :
   - Sur toutes les décisions d'EVAL5 et pour les trois bras, les probabilités appliquées par le moteur doivent être identiques à celles de l'oracle des traces (`engine_oracle_mismatches` = 0).
   - Chaque document doit être retrouvé octet pour octet (`round_trip_failures` = 0).
2. **Qualité** (sinon `NO_GO`). Pour les deux graines, en bits de conteneur par octet d'EVAL5 :
   - BPC(B1) < BPC(B0) − 0,02 ;
   - et le gain BPC(B0) − BPC(B1) > 2 × l'écart entre les graines.
3. **Coût** (sinon `GO_DIAGNOSTIC_COST`) :
   - Pour les deux graines, le rapport des temps de compression B1/B0 et le rapport des temps de décompression B1/B0 sont ≤ 1,5.
   - Ces temps sont mesurés sur le Dell, dans la même boucle, de bout en bout (moteur et codeur).
   - Ce « moteur booléen » de référence est P1 dans le même codeur, pas le moteur Pvp6Bank de GB-9. GB-9 n'a pas de modèle de texte et ne peut pas compresser EVAL5.
4. **Déterminisme Dell/Thor.**
   - `engine-h3-synthetic` écrit `deterministic.csv`. Ce fichier ne lit aucune donnée : il utilise un P1 entraîné sur du texte synthétique, les trois formes de tête (aucune, simple, routée avec un expert non admis) et huit documents, dont les longueurs limites 0 à 3, du texte, des octets aléatoires et des zéros.
   - Pour chaque cellule, le fichier contient le SHA-256 du conteneur et celui des probabilités appliquées, l'égalité avec l'oracle et l'aller-retour. Il contient aussi le SHA-256 des tables `ln`/`exp` de P1.
   - Le SHA-256 de `deterministic.csv` doit être identique sur Dell et Thor, et toutes les cellules doivent passer. Sinon, le verdict est `NO_GO_DETERMINISM`, quel que soit le reste.
   - Les temps synthétiques de chaque hôte sont publiés sans critère.
5. **Lecture.**
   - **GO.** Le moteur hybride est démontré de bout en bout. Il est exact, réversible, meilleur que le moteur booléen seul sur un troisième échantillon neuf, d'un coût borné, et identique sur deux architectures. L'ensemble H-0 à H-3 est alors complet.
   - **Sinon.** Le verdict dit quelle propriété manque.
   - Dans tous les cas, l'écart entre les bits de modèle (`model_bpc`) et les bits de conteneur mesure ce que coûte le codeur. Il est publié.
6. **Rapport.** `h3.tsv` publie tout, sans sélection a posteriori :
   - les SHA du manifeste, d'EVAL5, des poids, des tables et des têtes ;
   - la forme de tête ;
   - pour chaque bras : octets de conteneur, BPC de conteneur et de modèle, µs par octet en compression et en décompression, poids de tête admis, SHA-256 de la suite des conteneurs ;
   - les gains, l'écart entre graines, les rapports de coût ;
   - `dev_read false`, `test UNOPENED`, le commit source et le verdict.
   - Les conteneurs eux-mêmes restent sur le Dell.

## Ce que H-3 ne démontre pas

- Ni une généralisation au-delà d'articles peS2o, ni un gain d'énergie (non mesurée, GB-7).
- Ni un modèle qui apprend en ligne : P1 et les têtes sont gelés.
- Ni une comparaison au moteur Pvp6Bank de GB-9, qui n'est pas un modèle de texte. La comparaison au témoin de 15 M paramètres (3,655 BPC sur le DEV S5h) n'est pas refaite : EVAL5 est un autre échantillon.

Research only. TEST reste fermé. NOT_PROMOTED.
