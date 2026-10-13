# ENG-1d — mélange booléen adaptatif (BMX)

Statut : **outil et mesure exploratoire**, aucune revendication générale.

## Ce que fait le prédicteur

`crates/sml-core/src/bit_mixer.rs` prédit chaque bit (poids fort d'abord)
et apprend pendant le codage ; aucune table TRAIN figée.

- Entrées : contextes hachés des 1, 2, 3, 4, 6 et 8 octets précédents, un
  contexte « mot » (lettres, casse repliée), l'ordre 0 (octet partiel) et un
  modèle de correspondance (plus longue répétition retrouvée par hachage des
  6 derniers octets ; prédit le bit de l'octet qui suivait).
- Compteurs adaptatifs par contexte (pas 1/(n+1,5), limite 127), puis un
  **adaptateur de confiance** par entrée : la probabilité est corrigée selon
  le nombre d'observations de son contexte. Sans lui, un contexte vu une
  fois sur des données bruitées était cru à 83 % et le codage d'octets
  aléatoires coûtait 8,85 bit/octet ; avec lui, < 8,15 (test).
- Mélange logistique des entrées (poids appris en ligne, un jeu par octet
  partiel), puis deux cartes de probabilité adaptatives (contextes ordre 1
  et ordre 2 haché) moyennées avec la sortie du mélangeur.
- **Arithmétique entière uniquement** dans la prédiction et l'apprentissage :
  encodeur et décodeur restent identiques bit à bit sur toute cible.
- Amorçage optionnel (`prime`) : le prédicteur apprend d'abord un texte
  fourni, exactement comme s'il l'avait codé ; le décodage exige le même
  texte (SHA-256 dans le conteneur).

`bool_coder` expose `BitPredictor`, `encode_with_predictor` et
`decode_with_predictor`, avec les mêmes garanties d'échec fermé que le
codage par distribution.

```bash
cargo build --locked --release -p sml-core --bin sml-bool-codec
sml-bool-codec encode-mix PRIME|- INPUT NEW_OUTPUT
sml-bool-codec decode-mix PRIME|- INPUT NEW_OUTPUT
```

## Réglage et mesure

Réglage (taux d'apprentissage du mélangeur, limite des compteurs, tailles
de tables) **uniquement** sur deux ensembles de développement : DEV
d'*Opticks* (32 Kio à 60 %, amorcé par le TRAIN) et un extrait de
`version7.txt` de la documentation Vim. Le levier dominant a été le taux
du mélangeur (division par 16 du pas initial : environ −13 % sur DEV).
Configuration figée ensuite (`bmx-v2`, tables 2^22, ~120 Mo de mémoire).

Évaluation unique sur le TEST d'*Opticks* (65 536 octets à 80 %, jamais
utilisé pour régler). Aller-retour identique et SHA-256 vérifié dans les
deux cas.

| Méthode | Sans amorçage (octets) | Avec le TRAIN (octets) |
|---|---:|---:|
| `gzip -9` | 21 617 | 20 700 |
| `xz -9e` | 19 916 | 16 840 |
| `bzip2 -9` | 18 775 | 15 530 |
| n-gramme paginé SML, prof. 12, `swb:16` | — | 17 116 |
| **BMX (`bmx-v2`)** | **16 109** (1,966 bit/octet) | **13 684** (1,670 bit/octet) |

« Avec le TRAIN » : coût conditionnel des compresseurs standard (taille de
TRAIN+TEST moins taille de TRAIN). Débit mesuré : environ 180 Kio/s en
codage et en décodage (conteneur Linux partagé, version release), contre
2 à 4 Kio/s pour le n-gramme paginé.

## Limites

Un seul texte d'évaluation, en anglais, en dehors du panel TESTTEST ; le
réglage a vu un autre extrait du même livre. Aucun compresseur de la
famille PAQ/cmix n'est installé ici pour comparaison, et ils sont attendus
nettement meilleurs. Toute affirmation de performance sur le français ou
sur un panel exige une évaluation préenregistrée sur des œuvres non
exposées. Le prédicteur n'utilise pas encore les fonctions booléennes ANF
de SML : c'est le point d'insertion naturel (nouvelles entrées du mélangeur).
