# HYB H-1d — tête ternaire qui voit chaque banque, jugée sur un échantillon neuf : préenregistrement

**Gelé le 24 octobre 2026, avant toute exécution H-1d et avant toute lecture d'EVAL3.** Le propriétaire a choisi, le 24 octobre 2026, l'option « H-1d : la tête voit chaque banque ».

Ce document fait suite à `RESULTS-HYB-H1C-S5H-STABLE-RESIDUAL.md` (verdict NO_GO). H-1d est un **nouvel essai, déclaré comme tel**. Il ne révise aucun verdict H-1, H-1b ou H-1c, qui restent publiés.

## Ce que H-1c a montré, et ce que H-1d change

Sur `bytes_q4`, H-1c est stable (écart entre graines 0,0002) et son coût passe (1,32×). Il échoue sur un seul critère : le gain sur EVAL2 vaut 0,0159 et 0,0157 BPC, sous le seuil de 0,02. Le gain plafonnait déjà sur CAL-SEL (0,0196).

La tête H-1c ne voit que le stretch déjà mélangé par P1, la confiance de chaque banque à la racine, les octets de contexte et le registre de mot. P1 mélange ses 6 banques par une moyenne pondérée fixe. La tête ne peut donc pas apprendre à faire davantage confiance à une banque qu'à une autre selon ce que chacune prédit.

**H-1d :** pour chaque décision, la tête reçoit en plus, par banque, un thermomètre signé du stretch propre à la banque. La correction devient une somme de fonctions en escalier du stretch de chaque banque : un mélangeur ternaire appris, ajouté au mélangeur fixe. Tout le reste est repris de H-1c sans changement.

Ce changement a été choisi **après** avoir vu les résultats H-1c sur EVAL2. EVAL2 a été lu une fois : il ne peut plus servir de critère. H-1d est jugé sur un échantillon **jamais lu**, EVAL3. Le DEV S5h n'est jamais ouvert.

## L'échantillon EVAL3

- **Source.** Le fichier peS2o-0003 (épinglé `ba0880d5…`). Ni le stage S5h (qui s'arrête dans peS2o-0001) ni EVAL2 (peS2o-0002) ne l'ont ouvert.
- **Règle, filtres, doublons, taille et audit.** Les mêmes que pour EVAL2 :
  - seuls les articles du seau DEV de la découpe S5h entrent ;
  - un article déjà présent dans S5h (identifiant, texte exact ou normalisé) est exclu ;
  - on s'arrête à 32 Mio ;
  - l'audit S4d sur le manifeste S5h, avec les lignes EVAL3 étiquetées DEV, doit rendre `PASS_NARROW`.
- **Sorties du stager** (`--eval3`) : `manifest.eval3.tsv` (rôle `EVAL3`), `eval3-attribution.tsv` et `eval3-preflight.tsv`. Le manifeste S5h reste celui du témoin (`966eb65f…`).
- **Limite déclarée.** Comme EVAL2, EVAL3 ne contient que des articles peS2o. Les critères comparent les bras entre eux, sur EVAL3.

## Run P1

- **Mode.** `sml-gb11-p1-s5h run-h1d` :
  - seul `bytes_q4` est entraîné, sans recherche ; sa configuration est choisie comme dans tous les runs P1 (préfixe TRAIN, CAL exclu) ;
  - CAL (`doc_index % 16 == 7`) reste hors entraînement ;
  - CAL (split 2) et EVAL3 (split 4) sont tracés avec le modèle gelé, après une passe EVAL3 sans trace qui mesure `eval3_us_per_position` ;
  - chaque trace a un fichier compagnon `.banks.bin` (`BankPlane`, 72 o par enregistrement, même ordre) ;
  - le DEV S5h n'est jamais ouvert (`dev_read false`).
- **Stretch par banque.** Pour chaque décision et chaque banque, le stretch de la cellule de la banque (le même que celui qui entre dans la moyenne), divisé par 16 en tronquant vers zéro et saturé à ±127.
- **Contrôle de déterminisme.** Le SHA-256 des poids `bytes_q4` doit être celui des runs H-1b et H-1c (`7f4c233d…`). Sinon, le run s'arrête avant la tête.

## Bras

Le code est dans `hyb_h1d_bank_head.rs` et le binaire `sml-hyb-h1d-s5h`. Ce qui ne change pas par rapport à H-1c :

- la perte, le pas `STEP_UNITS`, le taux 1/128 et l'initialisation ;
- CAL-FIT (`% 32 == 7`) et CAL-SEL (`% 32 == 23`) ;
- 4 époques permutées, la moyenne de Polyak des époques 2 à 4 et les clés d'ordre ;
- la règle d'export à échelle, les grilles τ et λ8 (λ = 0 inclus) ;
- les graines 11 et 23.

Les bras :

- **A0 — P1 seul.**
- **A1c — tête H-1c, contrôle.** Elle est réentraînée sur le CAL de ce run, avec les mêmes 124 entrées. Elle mesure ce que les entrées de banques ajoutent.
- **A1d — tête H-1d.**
  - Entrées : les 124 entrées H-1c, plus 6 bits par banque (8 emplacements, dont 6 utilisés par `bytes_q4`).
  - Les bits d'une banque de stretch `q` : `q ≥ 4`, `q ≥ 16`, `q ≥ 48`, `q ≤ −4`, `q ≤ −16`, `q ≤ −48` (soit 0,25, 1 et 3 nats).
  - Inférence : trois mots `u64`, 6 popcounts par profondeur, 448 o résidents.

A0c (table booléenne de même taille) est retiré : il a choisi λ = 0 dans H-1, H-1b et H-1c, sur les deux bases.

## Critères (figés, sur `bytes_q4`)

1. **Étape 1 : porte CAL-SEL, avant toute lecture d'EVAL3.**
   - Pour chaque graine, gain CAL-SEL = BPC_CAL-SEL(A0) − BPC_CAL-SEL(A1d).
   - Si ce gain est inférieur à 0,03 pour au moins une graine, le run s'arrête avec le verdict `NO_GO_CAL_GATE`. EVAL3 n'est pas ouvert et reste disponible.
   - Raison de la marge : en H-1c, le gain a baissé d'environ 0,004 entre CAL-SEL et EVAL2. Ouvrir EVAL3 pour un gain CAL-SEL de 0,02 à 0,03 gâcherait probablement l'échantillon.
2. **Exactitude.**
   - Sur le Dell, le noyau popcount et l'oracle scalaire doivent donner 0 écart sur toutes les positions EVAL3, pour les deux graines.
   - Les 2 têtes A1d sont rejouées sur Thor. Le SHA-256 des corrections i32 sur 65 536 enregistrements synthétiques doit être le même que sur le Dell.
3. **Go.**
   - Pour les deux graines, il faut BPC_EVAL3(A1d) < BPC_EVAL3(A0) − 0,02.
   - Cet écart doit aussi dépasser deux fois l'écart entre les graines.
4. **Coût.**
   - Formule : (`eval3_us_per_position` de P1 + temps tête) / `eval3_us_per_position` ≤ 1,5.
   - Le temps tête est le temps de scoring A1d moins le temps du coût de décision commun, mesuré comme en H-1c.
   - Si ce critère échoue, le verdict devient `GO_DIAGNOSTIC_COST`.
5. **Lecture.** Seul un GO autorise l'étape H-2, qui demandera encore un accord. A1c est un diagnostic : il ne change pas le verdict.
6. **Rapport.** `h1d.tsv` publie tout, sans sélection a posteriori :
   - les deux graines, A0, A1c et A1d, sur CAL-SEL et (si la porte passe) sur EVAL3 ;
   - (τ, λ), les pas `unit`, la fraction de poids nuls, le nombre d'instantanés moyennés ;
   - les temps et le ratio de coût ;
   - `dev_read false` et `eval3_read`.

## Ce que H-1d ne démontre pas

- Ni le routage MOR (H-2), ni un gain moteur natif (H-3), ni un gain d'énergie.
- Ni une généralisation au-delà d'articles scientifiques peS2o.
- Ni que la tête apprise bat un mélangeur logistique continu : ce bras n'est pas testé.

Research only. TEST reste fermé. NOT_PROMOTED.
