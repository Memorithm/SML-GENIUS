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

## Entrée ANF de SML dans le mélangeur : résultat négatif

Question : le modèle ANF récurrent de SML (`anf_char` : réseau de portes
ANF locales à fan-in 6, couche d'historique, 8 lectures chaînées choisies
par sélection gloutonne) apporte-t-il de l'information au-delà de
`bmx-v3` ?

Outil : `anf_side::AnfCharSide` rejoue le modèle de façon causale, octet
par octet, sur une seule voie. Un test vérifie que la somme de ses
`-log2 p` égale exactement (à 1e-9) le score de `anf_char::evaluate`, donc
que le rejeu est fidèle. Le trait `bit_mixer::SideModel` branche ce
prédicteur comme entrées du mélangeur : probabilité KT calibrée par
position de bit, et/ou compteur adaptatif indexé par l'adresse de lecture
ANF (`SideInputs::{Both, AddressOnly, ProbabilityOnly}`). Le modèle ANF est
entraîné sur le texte d'amorçage (90 % ajustement, 10 % arrêt anticipé) ;
l'identité du conteneur inclut l'empreinte SHA-256 du modèle entraîné.

Mesure (`sml-bmx-anf-eval`, ensembles de réglage uniquement) :

| Ensemble | `bmx-v3` | + ANF (deux entrées) | adresse seule | probabilité seule |
|---|---:|---:|---:|---:|
| *Opticks* DEV (amorcé par TRAIN) | 6 765,2 o | +0,55 % | +0,02 % | +0,22 % |
| Vim `version7.txt` (amorcé) | 9 636,4 o | +0,91 % | −0,06 % | +0,24 % |

Une configuration ANF plus forte (128 portes libres, 256 propositions par
époque, 30 époques, 176 s d'entraînement) donne aussi +0,66 % sur
*Opticks* DEV. Le modèle ANF seul atteint 2,66 bit/octet en validation, loin
du mélangeur.

Conclusion pour cette configuration : **le modèle ANF de caractères actuel
n'apporte pas d'information exploitable par le mélangeur** ; sa probabilité
figée dégrade le code, et ses adresses n'aident pas au-delà du bruit
(±0,06 %). Aucune évaluation sur les textes réservés n'a été faite, faute de
gain sur le réglage. L'outil reste disponible pour tester de futurs
modèles ANF (`SideModel`) sans toucher au codeur.

## Plafond d'une adaptation en ligne des masques

Le compteur adaptatif d'un contexte masqué apprend déjà en ligne la table de
vérité complète (donc la forme ANF complète) de la fonction sur les bits
retenus. Une « ANF en ligne » ne peut plus choisir que l'ensemble de bits.
Plafond mesuré par un oracle qui choisit les masques sur le texte même
(`sml-bmx-mask-search 22 3 0`), sur un domaine nouveau : du code Rust de SML
(`crates/sml-core/src/anf_*.rs`, 340 000 octets d'amorçage puis 65 536
octets codés ; ensemble de réglage, jamais d'évaluation).

| Contextes | Octets | vs sans masque |
|---|---:|---:|
| sans masque (`bmx-v2`) | 10 051,8 | — |
| `V3_MASKS` figés | 9 969,1 | −0,82 % |
| oracle (3 masques choisis sur ce texte) | 9 950,2 | −1,01 % |

L'oracle reprend les deux masques de `V3` et n'ajoute que 0,19 %. Une
sélection en ligne, qui ne peut faire mieux que l'oracle, n'est pas
construite.

Référence PPMd var. H (`pyppmd`, meilleur ordre parmi 4 à 16 par texte, sans
réglage) : *Opticks* TEST 16 600, documentation SML 21 859, manuel Vim
17 178 octets seuls. `bmx-v3` était déjà de 3,0 à 4,3 % en dessous.

## bmx-v4 : cases contrôlées et trois jeux de poids

Diagnostic : passer les tables de 2^22 à 2^24 cases gagne 0,67 % sur le
réglage. Les collisions de hachage coûtaient donc, mais ×4 en mémoire
n'est pas acceptable.

`Architecture::V4` (configuration par défaut, identité `bmx-v4:…`) :

- **cases contrôlées** : chaque case garde 5 bits de contrôle tirés du haut
  du hachage (probabilité 20 bits, compte 7 bits). Une case dont le contrôle
  diffère se lit comme jamais vue et est remplacée à la mise à jour, au lieu
  de prêter les statistiques d'un autre contexte. Mémoire inchangée.
- **trois jeux de poids** du mélangeur, choisis par l'octet partiel (comme
  avant), par l'octet précédent × l'état de la correspondance (aucune,
  longueur < 16, < 32, ≥ 32), et par les classes (bits 7-5) des deux octets
  précédents × la position du bit. Un petit mélangeur final appris (choisi
  par l'octet partiel, initialisé à la moyenne) combine les trois sorties.
- La compatibilité est conservée : `MixerConfig::released()` liste v4, v3 et
  v2. `decode-mix` choisit la configuration dont l'identité est stockée, et
  deux fichiers de référence, écrits par les binaires publiés v2 et v3, se
  décodent toujours et se réencodent octet pour octet.

Choix faits sur les trois ensembles de réglage seulement (*Opticks* DEV
amorcé par TRAIN, Vim `version7.txt`, code Rust ci-dessus) ; total de
`bmx-v3` : 26 370,7 octets.

| Variante (une seule passe d'essais) | Total | vs v3 |
|---|---:|---:|
| tables 2^24 (référence de coût des collisions) | 26 193,1 | −0,67 % |
| deuxième entrée « confiance » par contexte | 26 477,6 | +0,41 % (écartée) |
| cases contrôlées seules | 26 161,1 | −0,79 % |
| deuxième jeu de poids seul | 25 760,8 | −2,31 % |
| cases contrôlées + 2 jeux + moyenne | 25 583,1 | −2,99 % |
| **bmx-v4** : + 3e jeu + mélangeur final appris | **25 465,5** | **−3,43 %** |

### Évaluation sur textes réservés

Mêmes textes et même méthode que pour v3 (`sml-bmx-eval`, longueur idéale
pour 65 536 octets codés ; les valeurs v2/v3 se reproduisent à l'octet près).
C'est un deuxième regard sur la documentation SML et le manuel Vim, un
troisième sur *Opticks* TEST ; aucun choix de v4 n'a été fait sur eux.

| Texte | Mode | `bzip2 -9` | `bmx-v3` | `bmx-v4` | v4 vs v3 | v4 vs bzip2 |
|---|---|---:|---:|---:|---:|---:|
| *Opticks* TEST | seul | 18 775 | 15 985 | 15 672 | −1,96 % | −16,5 % |
| | amorcé | 15 530 | 13 587 | 13 237 | −2,57 % | −14,8 % |
| Documentation SML | seul | 24 691 | 20 916 | 20 546 | −1,77 % | −16,8 % |
| | amorcé | 20 393 | 17 259 | 16 766 | −2,85 % | −17,8 % |
| Manuel Vim | seul | 19 420 | 16 655 | 16 306 | −2,09 % | −16,0 % |
| | amorcé | 15 433 | 13 692 | 13 252 | −3,22 % | −14,1 % |

Les six flux réels de `sml-bool-codec` (charge utile à 5 octets au plus de la
valeur idéale) se décodent à l'identique, SHA-256 vérifié. Débit mesuré
séquentiellement sur *Opticks* : 150 Kio/s pour v4 contre 174 pour v3.

Limites : sélecteurs des jeux de poids choisis à la main, une seule passe
d'essais ; mêmes trois textes d'évaluation, choisis par disponibilité (pas
un panel, aucun texte littéraire français). La confirmation passe par un
panel préregistré.

## Projection de Pascal GF(2) des contextes : résultat négatif

Question : des contextes pris sur la transformée de Pascal modulo 2
(`pascal_gf2::pascal_gf2_in_place`, `y[n] = XOR x[k]` sur les sous-masques
`k` de `n`) des 64 bits des 8 derniers octets apportent-ils de
l'information au-delà des masques sur bits bruts ?

Structure : l'octet `k` de la sortie vaut Pascal₈ du XOR des octets
passés dont la position est un sous-masque de `k` (par exemple
`t-1 ⊕ t-2 ⊕ t-3 ⊕ t-4` pour `k = 3`). La transformée étant inversible,
une table de compteurs indexée par tous ses bits apprend exactement la
même chose que sur les bits bruts ; seule une **projection** (ne garder
qu'une partie des bits de sortie) peut créer des contextes nouveaux.

Mesure (essai jetable, non intégré) : 19 projections, chacune ajoutée
comme contexte à `bmx-v4` + `V3_MASKS`, comparées au même masque appliqué
aux bits bruts, sur les trois ensembles de réglage (référence 25 465,5 o).

| Projection | Bits bruts | Pascal |
|---|---:|---:|
| bits 7-5 des 8 octets | −0,01 % | −0,10 % (meilleur cas) |
| bits 7-6 des 8 octets | −0,05 % | −0,09 % |
| bit 7 des 8 octets | −0,03 % | −0,06 % |
| bit 0 des 8 octets | −0,05 % | −0,05 % |
| octets 0 et 1 entiers (bijection de `t-1, t-2`) | +0,00 % | +0,01 % |
| octets 0 et 2 entiers (bijection de `t-1, t-3`) | −0,05 % | −0,05 % |
| les 19 projections | — | de +0,01 % à −0,10 % |

Aucune projection ne gagne plus de 0,10 % ; le meilleur cas (bits 7-5 des
8 octets de sortie) fait 0,09 point de mieux que le même masque sur les
bits bruts, déjà présent dans `V3`. Des masques bruts simples (bit 0 ou
octets `t-1, t-3`) gagnent autant que la plupart des projections, environ
0,05 %. Ces écarts, mesurés une seule fois sans témoin aléatoire, sont trop
petits pour être distingués de l'effet d'une entrée supplémentaire
quelconque, et ne paieraient de toute façon ni une table de plus (16 Mio)
ni le temps de calcul. Les projections qui gardent des octets entiers
égalent les octets bruts correspondants, comme prévu. Conclusion : sur ces
ensembles, la projection de Pascal n'apporte pas de gain exploitable au
mélangeur ; elle n'est pas intégrée.
