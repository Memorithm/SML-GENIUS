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

## bmx-v3 : contextes à masques booléens

Un **masque booléen** `ByteMask = [u8; 8]` sélectionne explicitement des bits
des 8 derniers octets (`mask[j]` est appliqué par ET à l'octet `j+1` positions
en arrière) ; les octets masqués sont hachés pour former un contexte. C'est un
sous-cube booléen du contexte : contextes à saut, séquences repliées en casse
ou de classes de caractères. Le prédicteur accepte jusqu'à 16 masques comme
entrées supplémentaires du mélangeur.

`sml-bmx-mask-search` choisit les masques de façon gloutonne dans un réservoir
structuré de 25 candidats : à chaque tour, le candidat qui réduit le plus la
longueur de code adaptative totale est gardé s'il fait gagner au moins
200 bits. Recherche lancée **uniquement** sur les ensembles de réglage
(DEV d'*Opticks* amorcé par son TRAIN, extrait de `version7.txt`) ; elle
s'est arrêtée au troisième tour (meilleur gain 195,8 bits). Masques figés
(`V3_MASKS`) :

| Masque (octets t-1 … t-8) | Lecture | Gain sur le réglage |
|---|---|---:|
| `e0 e0 e0 e0 e0 e0 e0 e0` | 3 bits de poids fort des 8 octets : suite des classes (minuscule, majuscule/symbole, chiffre/ponctuation, octet UTF-8) | 883 bits |
| `ff e0 e0 e0 e0 e0 00 00` | octet précédent exact et classes des 5 octets antérieurs | 317 bits |

Les deux masques retenus portent sur les **bits 7-5**, c'est-à-dire la
structure de classes, et non sur l'identité exacte des octets : c'est
l'information que les contextes d'ordre fixe n'isolent pas.

### Évaluation sur textes non utilisés pour le réglage

Tailles en octets du flux pour 65 536 octets codés ; « amorcé » : avec 340 Kio
de texte précédent (coût conditionnel pour `bzip2`). *Opticks* TEST avait
déjà servi à l'unique mesure de `bmx-v2` ; les deux autres textes sont neufs
pour les deux versions.

| Texte | Mode | `bzip2 -9` | `xz -9e` | `bmx-v2` | `bmx-v3` | v3 vs bzip2 |
|---|---|---:|---:|---:|---:|---:|
| *Opticks* TEST (anglais, XVIIIe) | seul | 18 775 | 19 916 | 16 104 | 15 985 | −14,9 % |
| | amorcé | 15 530 | 16 840 | 13 680 | 13 587 | −12,5 % |
| Documentation SML (`docs/*.md`, FR/EN technique) | seul | 24 691 | 25 336 | 21 043 | 20 916 | −15,3 % |
| | amorcé | 20 393 | 21 204 | 17 331 | 17 259 | −15,4 % |
| Manuel utilisateur Vim (`usr_*.txt`, anglais technique) | seul | 19 420 | 20 632 | 16 804 | 16 655 | −14,2 % |
| | amorcé | 15 433 | 16 976 | 13 773 | 13 692 | −11,3 % |

Valeurs `bmx` = longueur de code idéale ; les flux réels mesurés par
`sml-bool-codec` sur *Opticks* et la documentation SML sont de 0 à 5 octets
au-dessus et se décodent à l'identique (SHA-256 vérifié). Les masques
booléens améliorent les six cas, de 0,4 à 0,9 %. Débit : 120 à 190 Kio/s.

Limites : réservoir de candidats conçu à la main, recherche gloutonne sur
deux textes, trois textes d'évaluation choisis par disponibilité (pas un
panel), aucun texte littéraire français parmi eux. Ce n'est pas une
revendication générale ; la confirmation passe par un panel préregistré.
