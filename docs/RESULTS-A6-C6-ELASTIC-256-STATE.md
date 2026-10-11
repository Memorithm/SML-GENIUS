# A6-C6 — Résultats de la montée en capacité ANF élastique à 256 bits

**Statut : expérience A6-C6 effectivement terminée, mesures vérifiées,
premier commit expérimental avec CI spécialisée verte.**
La PR #486 demeure en brouillon ; aucune campagne ultérieure n'est lancée.

Le protocole détaillé, publié **avant exécution du banc**, est
`docs/A6-C6-ELASTIC-256-PRE-REGISTRATION.md`.

## Question scientifique

Après la validation A6-C5b jusqu'à quatre bits d'état, vérifier
dans le vrai exécuteur natif SML-GENIUS si des fonctions récurrentes
ANF peuvent être entraînées avec **4, 8, 16, 32, 64, 128 et 256 bits
d'état logique par séquence**, et généraliser sur des séquences
de **4 096 et 16 384 pas** après un apprentissage sur **512 pas**.

Le banc doit aussi montrer une limite de capacité à **257 phases**
lorsque le plafond matériel/logique est de **256 bits**.

## Méthode — architecture annulaire restreinte

Un modèle de largeur N exécute synchroniquement :

```text
s0(t+1) = f(s_tap(t), x_raw(t))
si(t+1) = s_(i-1)(t)         pour i = 1..N-1
y_hat(t) = x0(t) XOR s0(t+1)
```

**Appris sur entraînement exclusivement** : l'indice `tap`
de rétroaction parmi N positions, le canal brut `x_raw` parmi
quatre candidats, et les quatre valeurs de la table booléenne de
la porte racine `f`. Le système ne reçoit **aucune étiquette
pour les portes internes**, seulement la sortie cible observée.

**Fixé par conception, non appris :** tous les registres
intermédiaires ont des portes ANF unaires à table d'identité.
Le readout XOR et la disposition annulaire sont également fixés.
Ce point est indispensable : il rend l'essai exploitable jusqu'à
256 bits, sans prétendre résoudre la recherche combinatoire des
4^(N−1) tables possibles des registres de délai.

La meilleure porte de chaque famille est classée exclusivement
par l'erreur d'entraînement, avec déterminisme sur les égalités.
La validation sélectionne la **plus petite famille obtenant la
perte minimale**. Le choix est engagé seulement si la validation
améliore strictement la référence `y_hat=x0`.

Les 7 familles partagent une **unique recherche préfixe**.
Les 16 384 combinaisons `256 taps ×4 raw ×16 fonctions`
sont évaluées chacune une fois par configuration. Chaque famille
retient le meilleur candidat dont l'indice de retour entre dans
ses bits d'état.

Les fonctions sélectionnées sont compilées en coefficients ANF
GF(2) avec `LocalAnfGate::from_truth_table`. Les erreurs
train/validation de chaque famille sont **recalculées dans
l'exécuteur natif `LocalAnfNetwork`** et comparées exactement
au scorer circulaire `u64`. En test, chaque famille est
évaluée par ce scorer circulaire et le modèle sélectionné est
également évalué par un **exécuteur ANF natif sans allocation
de trajectoire** ; les deux résultats doivent être
**bit-identiques**.

Les tests couvrent aussi les frontières logiques
des champs packés `SemanticBitField<128>` et
`SemanticBitField<256>`, aux indices 63,64,127,128 et255.

## Protocole effectivement exécuté

- Familles d'état `[4,8,16,32,64,128,256]`.
- Neuf tâches pré-déclarées : calcul direct, parité avec
  strides 4,8,16,32,64,128,256 et témoin négatif à **257**.
- Deux panneaux : étiquettes intactes et bruit indépendant
  des étiquettes Bernoulli(1/16).
- Cinq graines par couple tâche/panneau :
  **9 ×2 ×5 = 90 entraînements/validations**.
- Entraînement : **512 pas ×64 voies**, soit 32 768 exemples
  binaires par configuration ; validation indépendante
  de même taille.
- **Après le gel intégral des 90 choix** seulement,
  génération de deux nouveaux jeux de test à
  **4 096 et 16 384 pas** ×64 voies.
- Le banc a produit et contrôlé **90 enregistrements gelés**,
  **180 résultats de test** et **36 synthèses**. Tous
  les tests de cohérence/train-val/test et oracles ont réussi.

Les bits d'état s'initialisent à zéro au début de chaque
séquence. Le jeu de test n'entre jamais dans l'API
d'entraînement/sélection.

## Résultats de capacité à 16 384 pas

Taux d'erreur binaire moyen sur cinq graines, %. Plus bas
= meilleur. Les fonctions sans bruit représentables sont
retrouvées exactement par le moteur à largeur adaptée.

| Tâche | Bits sélectionnés | Propre | Bruit 1/16 |
|---|---:|---:|---:|
| Directe (mémoire non requise) | 4 alloués, 0 utiles | **0,0000 %** | 6,2429 % |
| Parité stride4 | 4 | **0,0000 %** | 6,2366 % |
| Parité stride8 | 8 | **0,0000 %** | 6,2485 % |
| Parité stride16 | 16 | **0,0000 %** | 6,2497 % |
| Parité stride32 | 32 | **0,0000 %** | 6,2468 % |
| Parité stride64 | 64 | **0,0000 %** | 6,2465 % |
| Parité stride128 | **128** | **0,0000 %** | **6,2451 %** |
| Parité stride256 | **256** | **0,0000 %** | **6,2277 %** |
| Parité stride257 (témoin négatif) | Non stable (4 ou 256) | **49,2077 %** | **49,3056 %** |

**80/80 sélections correctes de largeur** sur les huit
tâches représentables ×deux panneaux ×cinq graines.
Toutes ont été acceptées sur validation.

Pour chaque tâche stride=N avec N=4..256, la porte racine
apprend le **XOR à deux entrées**, table booléenne `0b0110`,
avec canal brut `x1` et tap `s_(N-1)`. Les délais natifs
intermédiaires sont des portes ANF unaires identité,
explicitement fixées par le protocole.

À 16 384 pas, **tous les exemples propres des 40 modèles
de parité périodique ont zéro erreur**, soit **32 fois
les 512 pas de leurs séquences d'entraînement**.
Le panneau bruité a l'erreur attendue ≈6,25 %.

### Contrôle des largeurs inadéquates

Les familles plus petites qu'une période utile ne trouvent
pas de solution exacte. Résultats du jeu de test propre
à 16 384 pas :

| Tâche | Famille insuffisante | Erreur famille insuffisante | Famille suffisante | Erreur |
|---|---:|---:|---:|---:|
| Stride8 | 4 | 49,972 % | 8 | **0,000 %** |
| Stride16 | 8 | 49,935 % | 16 | **0,000 %** |
| Stride32 | 16 | 49,888 % | 32 | **0,000 %** |
| Stride64 | 32 | 49,797 % | 64 | **0,000 %** |
| Stride128 | 64 | 49,616 % | 128 | **0,000 %** |
| Stride256 | 128 | 49,218 % | 256 | **0,000 %** |
| Stride257 négatif | 256 | 49,194 % | Non disponible | — |

La famille 128 bits est donc effectivement nécessaire au
test stride128 dans le protocole ; la famille 256 bits
est nécessaire au stride256. Le moteur a **sélectionné
la profondeur en utilisant la seule validation**, sans
fixer artificiellement le bon tap pour chaque tâche.

### Échec délibéré : le témoin à 257 phases

Le témoin stride257 reste autour de **49,2 % d'erreur
sur 16 384 pas**, et environ **46,8 % sur 4 096 pas**.
La différence tient à la phase initiale de **257 pas**,
pendant laquelle la cible ne dépend pas encore des
éléments antérieurs à cette distance. Sur un très long
flux, la part de ce préfixe devient négligeable et
l'erreur revient vers 50 %.

**10/10 modèles négatifs ont pourtant été acceptés
sur validation**, car des signaux courts donnent
de petites améliorations apparentes. La réussite
train/validation ne suffit donc toujours pas
pour confirmer une capacité de mémoire adéquate.

## Mesures physiques des familles

Runner : **GitHub Actions Ubuntu x86_64, 4 cœurs logiques,
Rust 1.89.0**. Ce n'est **pas** le Dell, ni la
mesure d'un GPU NVIDIA.

Débit mesuré : **millions de valeurs booléennes de
séquence évaluées par seconde**, agrégé sur cinq
modèles propres de chaque tâche, en exécution
ANF native de flux 16 384 pas ×64 voies.
Ce n'est **pas un débit de tokens linguistiques**.

| Famille utilisée | Charge utile des états pour 64 voies | Coefficients ANF logiques | Entrées lues par pas | Débit natif mesuré |
|---|---:|---:|---:|---:|
| 4 bits | 32 octets | 14 | 7 | ≈2 675 M bit-lanes/s |
| 8 bits | 64 octets | 22 | 11 | ≈1 914 M bit-lanes/s |
| 16 bits | 128 octets | 38 | 19 | ≈1 259 M bit-lanes/s |
| 32 bits | 256 octets | 70 | 35 | ≈708 M bit-lanes/s |
| 64 bits | 512 octets | 134 | 67 | ≈384 M bit-lanes/s |
| **128 bits** | **1 024 octets** | **262** | **131** | **≈195 M bit-lanes/s** |
| **256 bits** | **2 048 octets** | **518** | **259** | **≈99 M bit-lanes/s** |

**À interpréter précisément :**
- La charge de state logique ci-dessus compte **N mots u64**
  pour **64 flux en parallèle**, soit N bits d'état par flux.
- Les « coefficients ANF logiques » comptent les emplacements
  de monômes du moteur, pas les seuls coefficients égaux à 1.
  Les `LocalAnfGate` actuelles stockent également des
  `Vec` et métadonnées ; ces nombres ne mesurent donc
  **pas la mémoire résidente totale**.
- Le débit natif diminue à mesure que le nombre de
  portes mises à jour augmente. Aucun résultat
  d'accélération GPU/SIMD n'est revendiqué.
- Un nombre de bits d'état de **256** n'exige pas
  le type entier Rust `u256` : le modèle conserve un
  vecteur dynamique de mots `u64` bit-slicés,
  exécuté par les portes de `LocalAnfNetwork`.
- Le banc exerce les deux domaines `SemanticBitField<128>`
  et `SemanticBitField<256>` pour les opérations
  packées simples, mais le réseau natif récurrent
  utilise **son propre stockage dynamique**, plutôt
  qu'un `SemanticBitField<256>` unique.
- L'entraînement complet des sept familles + choix
  validation, à budget constant **16 384 candidats**
  par cas, dure en moyenne **environ 42 ms** sur ce
  runner ; cela comprend les oracles ANF natifs et
  les scores de validation. La recherche est bien
  moins exhaustive qu'A6-C5b sur l'espace
  de toutes les fonctions de délai : elle a
  changé l'hypothèse structurelle.

## Provenance scientifique

- Révision des mesures :
  `e0f13a29c51e20331fa38f292bc6d6c48ad00526`.
- [Workflow Rust A6-C6 et logs complets](https://github.com/Memorithm/SML-GENIUS/actions/runs/38168343485/job/114555490999).
- CI du bench sur la révision mesurée :
  **formatage, compilation, Clippy, tests de capacités
  128/256, égalité scorer-réseau et benchmark réussis**.
  CI générale suivie par workflow distinct.
- Marqueurs de logs :
  `A6C6_FROZEN` (90),
  `A6C6_ALL_FROZEN,true` (1),
  `A6C6_TEST_RESULT` (180),
  `A6C6_SUMMARY` (36).

Rejouer sur le code de la PR #486 :

```bash
cargo fmt --all -- --check
cargo test --locked -p sml-core anf_elastic_capacity
cargo test --locked -p sml-core --bin sml-elastic-capacity-bench
cargo run --locked --release -p sml-core --bin sml-elastic-capacity-bench -- 5
```

## Verdict, limites et suite méthodologique

**PASS A6-C6 dans le périmètre défini** :
- la représentation native supporte les familles **128 et 256 bits** ;
- les taps et fonctions ANF nécessaires sont retrouvés
  sans supervision intermédiaire ;
- le choix de famille sur validation donne la capacité
  minimale attendue dans **80/80 cas représentables** ;
- les fonctions exactes extrapolent à 16 384 pas ;
- les coûts temps/stockage sont mesurés ;
- le témoin stride257 échoue délibérément.

**Non démontré :** apprentissage libre de toutes les
256 portes et de leurs connexions, cycles arbitraires,
extraction de mémoire sur des textes, prédiction de
langage, débit de tokens/s en production, mémoire
occupée totale par un modèle 25B, ou gains GPU.

Le progrès important porte sur **l'échelle du
nombre de bits d'état**, mais il faut conserver
le fait que la base des retards est **fixée**.
Les expériences suivantes pourraient aborder
l'apprentissage des tables de délai, l'adaptation
de topologies non annulaires ou la projection SIMD
du moteur. **Aucune expérience suivante n'est
lancée avant communication et décision de l'utilisateur.**
