# A6-C8 — Préenregistrement de l'apprentissage conjoint de deux portes ANF cachées

**Protocole figé avant la première exécution A6-C8, le 11 octobre 2026.**

## Hypothèse scientifique falsifiable

A6-C7 apprend exactement une porte intermédiaire parmi 128 ou 256 bits
d'état récurrent, mais échoue lorsque deux portes XOR indépendantes
participent simultanément à la sortie : chacune, prise isolément,
porte un **crédit marginal nul**. A6-C8 vise à retrouver **les deux**
positions, les deux sources et les tables ANF associées en utilisant
uniquement les sorties finales, sans cibles internes.

Cette étape teste un sous-espace **affine de portes ANF** :
`f(s,x)=s XOR x` (truth=6) et `f(s,x)=NOT(s XOR x)`
(truth=9). Ces deux fonctions sont bien des portes ANF
à deux entrées et leurs positions/fonctions restent
inconnues du moteur. Le cas d'une porte AND non affine
reste **explicitement hors de cette grammaire** et sera
mesuré comme témoin négatif. Ne pas présenter A6-C8
comme une résolution générale des réseaux ANF non linéaires.

## Architecture / supervision réellement utilisées

```text
s0(t+1) = s_(N-1)(t) XOR x1(t)             [racine fixée]
si(t+1) = s_(i-1)(t)                       [par défaut identité]
sj(t+1) = s_(j-1)(t) XOR x_r(t)            [porte XOR cachée]
sk(t+1) = NOT(s_(k-1)(t) XOR x_q(t))      [autre variante possible]
y_hat(t) = x0(t) XOR s0(t+1)              [readout fixé]
```

N vaut 128 ou 256. Le moteur peut choisir **zéro, une ou
deux portes cachées** et sélectionne, depuis la seule erreur
terminale, leurs positions distinctes `j`, leurs canaux
bruts parmi `x1..x4`, et leurs tables complètes dans
la classe `{6,9}`.

**La disposition circulaire, les portes intermédiaires
inactives et la racine sont fixées**, ainsi que la
limite supérieure de deux mutations. Les formes XOR/XNOR
forment une sous-classe affine, à crédit calculable par
superposition. Il n'est pas prétendu que des graphes
arbitraires ou tous les coefficients des 256 portes
seraient entraînés.

### Déconvolution causale de l'erreur finale

Soit `r_t = y_t XOR y_t^(référence)` le résidu observé
contre le réseau de référence (racine XOR, délais identités).
Avec N bits récurrents et une ou deux injections XOR/XNOR,
le résidu vérifie exactement :

```text
r_t = r_(t-N) XOR injection_j(t-(N-j))
                 XOR injection_k(t-(N-k))
z_t = r_t XOR r_(t-N)
    = injection_j(t-(N-j)) XOR injection_k(t-(N-k))
```

Les termes de temps négatif valent zéro. `z_t` est une
**innovation observable calculée à partir des étiquettes
terminales et de la référence**, pas une étiquette de
porte cachée fournie par le générateur. Avec bruit de
labels, l'innovation incorpore deux perturbations de
labels et reste mesurable.

Cette égalité permet de tester **des paires d'interactions
qui seraient individuellement non corrélées à la cible**.

### Méthode de recherche préenregistrée

Pour chaque largeur N :

- `8 × (N−1)` candidats individuels : tous les slots
  internes, quatre canaux, deux fonctions `{XOR,XNOR}`.
  Donc 1 016 pour N128 et 2 040 pour N256.
- Paires à **deux slots physiques distincts** :
  `C(N−1,2) × 8²`, soit **512 064** paires pour N128
  et **2 072 640** pour N256.
- Score de **toutes les paires** sur une **sonde fixée
  de 32 pas terminaux de TRAIN** (2 048 exemples
  binaires par sonde, 64 voies). Aucun score de VAL/TEST
  n'intervient dans ce classement.
- Les **64 meilleures paires** d'après leur erreur
  de sonde sont ensuite rescannées sur la totalité
  du TRAIN pour sélectionner une paire. C'est une
  **recherche heuristique à faisceau**, pas une preuve
  de minimum global de perte TRAIN sur les 2M paires.
- Les modèles témoin sans mutation, meilleur
  candidat unique et meilleur couple de portes
  sont ensuite recompilés comme de véritables
  `LocalAnfNetwork` GF(2), exécutant les N portes,
  avec **égalité exacte** des scores causal/réseau
  sur TRAIN. La validation choisit lequel des
  trois modèles conserver, avec préférence pour
  moins de mutations en cas d'égalité.
- Aucun test ne sert à choisir un paramètre, une
  source, une vérité ANF, un faisceau ou une
  largeur. L'ensemble des 90 politiques doit
  être gelé **avant génération de TEST**.

Le contrôle A6-C7 original à **une porte libre avec
les 16 fonctions à deux entrées** est ré-entraîné
sur les mêmes TRAIN/VAL et évalué en TEST.
Cela préserve un contrôle direct du mécanisme
précédent, plus expressif localement que la
classe XOR/XNOR.

**Biais méthodologique à déclarer :** la recherche
A6-C8 utilise le résidu d'innovation issu d'une
linéarisation connue et bénéficie d'un budget
de propositions beaucoup plus élevé que A6-C7.
Une performance supérieure de cette recherche
sur les deux injections n'est pas automatiquement
un gain général de calcul ou de sample efficiency.

## Sept problèmes positifs/négatifs et contrôle

Neuf cas synthétiques définis avant exécution :

| ID | N | Générateur de vérité (strictement hors API entraînement) |
|---|---:|---|
| A | 128 | anneau sans mutation (baseline) |
| B | 128 | une XOR `j=45,x2` |
| C | 128 | deux XOR `j=31,x2` et `j=88,x3` (ancien FAIL A6-C7) |
| D | 256 | deux XOR `j=1,x2` et `j=191,x3` |
| E | 256 | deux XOR `j=57,x2` et `j=127,x3` |
| F | 128 | XOR `j=37,x2` et XNOR `j=93,x3` |
| G | 256 | XOR `j=1,x2` et XNOR `j=190,x3` |
| H | 128 | **négatif** : trois XOR distinctes `j=19,x2`, `j=67,x3`, `j=108,x4` |
| I | 128 | **négatif** : XOR `j=31,x2` et AND non affine `j=88,x3` |

Pour toutes les tâches, les **cibles internes ne sont
jamais communiquées** au moteur d'apprentissage :
le générateur natif utilise ses fonctions seulement
pour produire une paire `(séquence d'entrées,
séquence d'étiquettes finales)`.

Deux panneaux : propre et corruption de labels
indépendante `Bernoulli(1/16)`.
**5 graines indépendantes** par tâche/panneau.

Entraînement : 512 pas ×64 voies ; validation :
512 pas ×64 voies avec autre graine. Les **90
décisions** sont figées intégralement avant création
des données de TEST. Tests : deux flux indépendants
à 4 096 et 16 384 pas ×64 voies :
**180 résultats et 36 synthèses**.

## Mesures et critères préenregistrés

- Exactitude des **deux** positions, canaux et tables
  ANF cachés (hors ambiguïté mathématique
  éventuelle, explicitée si observée).
- Taux d'erreur sur sortie TEST à 16 384 pas :
  attendu **0 % sans bruit** pour A, B et C..G,
  proche du taux de corruption 6,25 % avec bruit.
- Contrôle précédent A6-C7 à une porte :
  sur C..G, échec attendu de généralisation complète
  car les deux signaux sont indépendants.
- Les témoins H et I doivent rester visibles,
  même si la validation accepte un modèle inadapté.
- Ablations : retirer une mutation d'un modèle
  à deux portes doit dégrader son score TEST,
  y compris pour le cas profond `j=1,N256`.
- Temps CPU du fit et du test natif, mémoire utile
  du tampon circulaire et empreinte des caractéristiques
  sondées, nombre de paires classées, sélection
  du meilleur faisceau sur TRAIN.
- Exécution Rust 1.89 sur runner GitHub-hosted
  x86_64 ; **pas de mesure Dell / GPU**.
- Ne pas confondre **bit-lanes/s** avec débit
  linguistique tokens/s ; ne pas annoncer de
  progression de précision du modèle SML-25B.

**Décision scientifique** : présenter les résultats
de cette seule expérience A6-C8 avant d'ouvrir
toute A6-C9. Les échecs devront être conservés.
