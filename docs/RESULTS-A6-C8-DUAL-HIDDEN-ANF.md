# A6-C8 — Crédit conjoint pour deux portes ANF cachées : résultats mesurés

**Statut : expérience mesurée, résultat vérifié, PR #488 en brouillon.**
Le protocole `docs/A6-C8-DUAL-HIDDEN-ANF-PREREGISTRATION.md` a été
versionné **avant la première exécution expérimentale**. Ce rapport
restitue également les témoins négatifs.

## Question scientifique

Dans A6-C7, une porte ANF cachée était apprise par sa contribution
à la sortie terminale, même 255 pas avant l'observation. Pourtant
la tâche synthétique avec **deux XOR indépendantes** restait
à environ **50 % d'erreur** : chacune avait un signal marginal
d'attribution de crédit nul.

A6-C8 teste si deux portes cachées peuvent être découvertes
**conjointement à partir de la seule cible finale**, sans
supervision explicite des états intermédiaires. Le nouvel
algorithme est une méthode **affine et bornée** :
il peut sélectionner deux portes `XOR` ou `XNOR` complètes
à deux entrées, avec positions et sources inconnues.

La topologie annulaire, la fonction racine et le readout
sont cependant fixés. Des portes `AND` non linéaires
ne font **pas** partie de l'espace appris.

## Modèle, dérivation et absence de fuite

Réseau natif sur N = 128 ou 256 bits d'état par séquence :

```text
s0(t+1) = s_(N-1)(t) XOR x1(t)
si(t+1) = s_(i-1)(t)                       [par défaut]
sj(t+1) = s_(j-1)(t) XOR x_r(t)            [porte apprise]
sk(t+1) = NOT(s_(k-1)(t) XOR x_q(t))       [variante apprise]
y_hat(t) = x0(t) XOR s0(t+1)
```

Une ou deux positions distinctes, les canaux raw `x1..x4`
et les fonctions `XOR` (table truth=6) ou `XNOR`
(truth=9) sont inconnus de l'apprenant. Il reçoit
uniquement `LaneData.inputs` et les **sorties finales**.
Le générateur de données est un véritable
`LocalAnfNetwork` indépendant.

Soit `r(t)=y(t) XOR y_ref(t)`, résidu contre la sortie
du réseau connu à délais identités. La récurrence
décalée implique, dans la classe affine :

```text
r(t) = r(t-N) XOR mutation_j(t-(N-j))
                XOR mutation_k(t-(N-k))
z(t) = r(t) XOR r(t-N)
```

L'**innovation terminale** `z(t)`, calculée
exclusivement à partir de la sortie et des entrées
observées de TRAIN, est la somme booléenne des
injections décalées. On peut donc identifier une paire
dont les contributions **ne sont pas
corrélées individuellement** avec l'étiquette finale.

Le bruit de labels crée des erreurs dans `z(t)`,
mais aucun état caché ni bit futur du générateur
n'est mis à disposition du learner. Les paramètres
de toutes les politiques sont fixés par TRAIN
et la validation choisit seulement entre 0, 1
et 2 portes. **Les 90 choix sont figés avant que
le moindre jeu de TEST ne soit généré.**

## Recherche et contrôles réellement exécutés

- Pour N=128, `127 ×4 ×2 = 1 016` candidats
  uniques ; **512 064 paires à positions distinctes**.
- Pour N=256, `255 ×4 ×2 = 2 040` candidats ;
  **2 072 640 paires distinctes**.
- Un score d'innovation sur les **32 derniers
  pas d'entraînement** (2 048 valeurs
  binaires par sonde) classe **toutes les paires**.
- Les **64 meilleures paires** sont rescannées
  sur le TRAIN complet (512 pas ×64 voies).
- La meilleure porte individuelle est également
  entraînée sur la même innovation.
- Réseaux de 0, 1 et 2 portes reconstruits dans
  `LocalAnfNetwork`. L'égalité **bit-à-bit**
  entre sortie d'innovation reconstruite et
  exécution GF(2) ANF native est requise avant
  de consulter validation.
- La validation choisit la perte de sortie
  **minimale** entre les trois réseaux et
  préfère **moins de mutations sur égalité**.
- Le contrôle **A6-C7 d'origine** est ré-entraîné
  indépendamment sur les mêmes TRAIN/VAL :
  une porte avec les **16 fonctions possibles**
  (non limitée à XOR/XNOR).
- Ablations : retirer la première ou la deuxième
  porte du réseau sélectionné, et mesurer de
  nouveau la sortie du réseau natif sur TEST.

**Précision fondamentale :** la sonde et le
faisceau sont un **heuristique train-only**.
Les 2 millions de paires N256 sont comparées
sur la sonde, **pas toutes sur l'intégralité
de l'entraînement**. Aucune optimalité
globale sur l'espace de toutes les fonctions
ANF ou toutes les paires n'est établie.
Le budget n'est pas égalisé avec A6-C7.

## Données et qualification

Neuf tâches préenregistrées, cinq graines par
tâche et par panneau propre/bruité
(Bernoulli1/16 sur les étiquettes) :
**90 entraînements/validations**.
Chaque split TRAIN et VAL est un flux
indépendant de **512 pas ×64 lanes**.

Après gel de ces 90 choix, deux nouveaux flux
TEST indépendants de **4 096 et 16 384 pas
×64 lanes**, soit **180 mesures, 36 synthèses**.

Sept tâches **représentables** : référence
identité128, une XOR128, double XOR128,
deux dispositions double XOR256,
XOR+XNOR128 et XOR+XNOR256.
Deux tâches **volontairement hors grammaire** :
triple XOR128 et XOR+AND128.

## Résultats : TEST indépendant de 16 384 pas

Taux d'erreur binaire, **en %** (moyenne
des 5 graines) ; plus faible = meilleur.
Sur la ligne « avec bruit », seule
l'étiquette finale est corrompue.

| Tâche | Contrôle A6-C7 une porte, propre | A6-C8 sélectionné, propre | A6-C8 sélectionné, bruité |
|---|---:|---:|---:|
| Identité128 | 0,0000 | **0,0000** | 6,2492 |
| XOR128 unique j45 | **0,0000** | **0,0000** | 6,2463 |
| **Deux XOR128 j31+j88** | 49,7051 | **0,0000** | **6,2532** |
| **Deux XOR256 j1+j191** | 49,2314 | **0,0000** | **6,2792** |
| **Deux XOR256 j57+j127** | 49,3774 | **0,0000** | **6,2423** |
| **XOR+XNOR128 j37+j93** | 49,7527 | **0,0000** | **6,2467** |
| **XOR+XNOR256 j1+j190** | 49,2241 | **0,0000** | **6,2534** |
| **Triple XOR128 (négatif)** | 49,8705 | **49,8305** | **49,8288** |
| **XOR+AND128 (négatif)** | **24,8305** | **49,2788** | **49,4118** |

Pour les **cinq tâches à deux portes
XOR/XNOR**, le réseau nouvellement entraîné
retrouve **exactement les deux positions,
les deux canaux et les deux tables de vérité**
sur **50/50 cas** (5 tâches ×2 panneaux
×5 graines). Leurs tests longs sans bruit
ont zéro erreur et les tests bruités restent
vers **6,25 %**, soit le plancher attendu
de corruption de labels.

Sur la tâche à **une seule porte**, la
sélection de validation conserve **une
mutation dans 10/10 cas** ; sur la tâche
identité, elle conserve **zéro mutation dans
10/10 cas**. Total :
**70/70 architectures et fonctions
correctement retrouvées parmi les sept
tâches représentables**.

Ces mesures incluent les cas de deux
portes dont **l'une est au slot 1**
d'un anneau de 256 bits : son effet
met 255 pas à atteindre la sortie
terminale, mais il est identifié
malgré l'autre injection indépendante.

### Ablation causale

Pour chaque modèle à deux portes apprises
et étiquettes propres :

- si la première porte est retirée,
  les erreurs reviennent autour de
  **49–50 %** ;
- si la deuxième porte est retirée,
  le même effondrement se produit.

Exemple précis sur la tâche historique
double XOR128 :

| Test | Erreur |
|---|---:|
| A6-C8, deux portes | **0 %** |
| Supprimer la première | **49,7046 %** |
| Supprimer la deuxième | **49,8782 %** |

Aucune simple mémorisation des étiquettes
ne suffit à expliquer le résultat :
la sortie dépend causalement des
**deux fonctions cachées**.

### Négatifs : ne pas les minimiser

**Trois XOR indépendantes.** Le learner
autorise au maximum deux injections.
Le meilleur modèle sélectionné
contient une seule mutation dans les
**10/10 cas** de cette tâche et obtient
**49,83 % d'erreur propre**.
L'entraînement et la validation
peuvent néanmoins admettre de faibles
améliorations fortuites ; cela
n'est pas une récupération utile.

**XOR + AND non affine.** La classe
XOR/XNOR ne sait pas représenter une
porte `AND(s,x)`. Le modèle A6-C8
retient une seule mutation et
obtient environ **49,28 % propre /
49,41 % bruité** à 16 384 pas.
Le contrôle A6-C7 avec une porte
libre sur **16 tables** obtient
**24,83 % propre / 27,97 % bruité** :
il extrait un signal partiel
mais ne résout pas la composition.
**A6-C8 n'est donc pas supérieur
à A6-C7 pour toutes les tâches**.

Ces deux négatifs confirment la
limitation de la grammaire affine
et de l'apprentissage à au plus
deux mutations.

## Coût expérimental et mémoire

Exécution de la révision
`5e3696777fccf590c2b9fec7b44b7ffbd4bf4a4a`
sur un **runner GitHub-hosted Ubuntu x86_64,
quatre processeurs logiques, Rust 1.89.0**.
**Aucune mesure Dell ni GPU.**

| Largeur | Paires comparées sur la sonde | Rescans complets | Charge utile des états de 64 séquences | Charge utile des caractéristiques de sonde | Temps moyen train+VAL |
|---|---:|---:|---:|---:|---:|
| 128 bits | 512 064 | 64 | 1 024 octets | **260 096 octets** | **≈18,3 ms** |
| 256 bits | 2 072 640 | 64 | 2 048 octets | **522 240 octets** | **≈67 ms** |

Le contrôle A6-C7 à une porte prend
≈28–30 ms à N128 et ≈57–60 ms
à N256 sur le même runner.
Ces différences ne forment **pas un facteur
d'accélération comparable**, car
les moteurs utilisent des grammaires
et des budgets très différents.

Les charges utiles ci-dessus n'incluent
pas l'ensemble du heap, les `Vec`,
les coefficients ANF, les données
d'entrée et les allocations du programme.
Chaque porte est effectivement exécutée
dans le moteur natif N128/N256,
pas simulée avec un faux registre `u256`.

## Audit / reproductibilité

- Code du banc réellement mesuré :
  `5e3696777fccf590c2b9fec7b44b7ffbd4bf4a4a`.
- [Workflow A6-C8 et logs bruts réussis](https://github.com/Memorithm/SML-GENIUS/actions/runs/38172898844/job/114569071829).
- [CI générale de la révision expérimentale](https://github.com/Memorithm/SML-GENIUS/actions/runs/38172898839).
- Contrôles Rust 1.89 : formatage,
  compilation stricte, Clippy et
  unit tests de séparation causale,
  exactitude d'innovation et
  recherche jointe tous réussis.
- Signatures de sortie :
  `A6C8_FROZEN` (90),
  `A6C8_ALL_FROZEN,true` (1),
  `A6C8_TEST_RESULT` (180),
  `A6C8_SUMMARY` (36).

Pour reproduire :

```bash
cargo fmt --all -- --check
cargo test --locked -p sml-core anf_dual_hidden_credit
cargo test --locked -p sml-core --bin sml-dual-hidden-credit-bench
cargo run --release --locked -p sml-core --bin sml-dual-hidden-credit-bench -- 5
```

## Verdict scientifique et décision

**PASS A6-C8** pour la découverte conjointe
de deux portes ANF XOR/XNOR cachées dans
des états récurrents de 128 ou 256 bits,
à partir de la sortie finale seulement,
même quand une seule porte n'a pas de
crédit marginal exploitable.

**FAIL contrôlé** pour trois mutations
et pour les interactions AND non affines :
un crédit multi-porte non linéaire
général reste à démontrer.

Les architectures étudiées utilisent
un anneau récurrent entièrement
défini à l'avance, seulement
deux fonctions affines libres et
une recherche à faisceau borné.
Le résultat ne prouve ni
l'apprentissage libre de centaines
de portes, ni la génération de
langage plus précise, ni une
amélioration du modèle SML-25B,
ni une performance GPU.

**Discipline de campagne :**
publier ces résultats avant toute
A6-C9 et garder #488 en brouillon
jusqu'à accord utilisateur.
