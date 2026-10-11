# A6-C2 — Attribution du crédit entre deux portes ANF récurrentes

**Statut : expérience réalisée, résultat mesuré ; PR #481 en brouillon.**
Les chiffres ci-dessous ont été calculés et enregistrés sur GitHub Actions.
Ils ne proviennent **pas** du Dell, dont le canal de commande RemoteOps
n'a pas été utilisé pour cette campagne.

## Question falsifiable

Est-il possible d'apprendre une représentation intermédiaire et sa consommation
par une seconde porte récurrente **sans fournir la moindre cible auxiliaire
aux portes**, uniquement par la réduction de l'erreur de sortie finale ?

A6-C1 savait entraîner chaque porte avec un résidu explicite. Ici, la
supervision ne porte que sur l'observation finale. Le réseau est initialisé
avec deux portes nulles et se met à jour de façon causale :

- `s0_(t+1) = f(x_i(t), x_j(t))` : synthèse d'un état intermédiaire,
- `s1_(t+1) = g(s0_t, x_k(t))` : consommation de cet état au pas suivant,
- `ŷ_t = x0_t XOR s1_(t+1)` : readout ANF observable, figé.

La cible observée est `y_t = x0_t XOR (x1_t AND (x2_(t-1) XOR x3_(t-1)))`.
Cette formule est employée pour générer les données **en dehors du moteur
d'apprentissage**. Le moteur ne reçoit jamais ni `x2 XOR x3` ni le résultat
`x1 AND s0` comme cibles. Il doit découvrir les sources et leurs tables de
vérité par la seule comparaison de `ŷ` et `y`.

## Méthode et contrôles

L'algorithme énumère 6 paires de sources amont × 16 fonctions booléennes
à deux entrées × 4 sources directes aval × 16 fonctions aval, soit
**6 144 couples de portes**. Il calcule la perte binaire de chaque couple sur
l'entraînement uniquement ; les tables de vérité choisies sont converties
en coefficients ANF via la transformée de Möbius existante.

Le modèle conjoint est engagé seulement si l'erreur **de validation** est
strictement plus basse que celle du modèle initial. Le groupe est proposé
comme une unité, sans crédit séparé préalable accordé à chaque porte.

Contrôles exécutés dans le même environnement : (1) deux portes nulles,
(2) descente coordonnée sur une porte à la fois avec trois passes et
acceptation des seules améliorations strictes d'entraînement,
(3) les deux portes construites conjointement, (4) mise à zéro de la
porte amont après entraînement, (5) mise à zéro de la porte aval.

Les choix de tous les modèles pour les dix couples panneau/graine sont
entièrement **figés avant même la génération** des données de test. Les
graines d'entraînement, validation et test sont différentes.

Configuration : 5 graines indépendantes par panneau ; panneau propre et
panneau avec perturbation Bernoulli 1/16 des étiquettes ; données packées sur
64 lanes ; **6 144 échantillons d'entraînement**, **3 072 de validation**,
**3 072 de test** par graine et par panneau. Les sources candidates sont
les quatre entrées binaires courantes. La profondeur temporelle observable
est **un seul pas** (l'état amont du pas précédent).

## Résultats mesurés

Taux d'erreur binaire **de la tâche synthétique locale** sur le test,
moyenne des cinq graines. Plus bas = meilleur.

| Panneau | Deux portes nulles | Descente coordonnée | Recherche conjointe | Sans porte amont | Sans porte aval |
|---|---:|---:|---:|---:|---:|
| Sans bruit | 25,30599 % | 15,61849 % | **0,00000 %** | 25,30599 % | 25,30599 % |
| Bruit 1/16 | 27,94271 % | 11,13281 % | **6,06120 %** | 27,94271 % | 27,94271 % |

La recherche conjointe est engagée dans **10 cas sur 10**. Elle retrouve
toujours les sources réelles : `gate0.inputs=[4,5]` (= x2 et x3)
et `gate1.inputs=[0,3]` (= s0 précédent et x1 courant). Le polynôme ANF
de la porte amont comporte **2 monômes** (XOR2) et celui de la porte aval
**1 monôme** (AND2).

Sans bruit, les cinq modèles conjoints ont **0 erreur** sur entraînement,
validation et test (3 072 × 5 exemples de test par panneau). Sous bruit
indépendant, l'erreur de 6,06120 % est proche du taux de corruption nominal
de 6,25 %.

La descente coordonnée **n'échoue pas systématiquement** : elle découvre une
solution utile dans **2/5 cas sans bruit** et dans **4/5 cas avec bruit**.
Dans les trois autres cas sans bruit, elle reste à l'erreur initiale.
Elle ne rejoint toutefois jamais l'erreur nulle de la recherche conjointe
dans ce protocole. Il serait faux de dire qu'un apprentissage porte par
porte est intrinsèquement impossible ; cette expérience montre surtout que
les propositions conjointes évitent la barrière du crédit initial.

Mettre à zéro **l'une ou l'autre des deux portes après entraînement**
réinstalle exactement l'erreur de référence (25,30599 % / 27,94271 %),
ce qui prouve que la sortie observée dépend causalement des deux portes
dans la configuration retenue.

Le temps local de sélection par graine/panneau est compris entre **2,55 et
2,77 ms** sur ce runner, incluant les 6 144 propositions conjointes et
les propositions coordonnées ; ce n'est pas une mesure Dell ni une
comparaison de débit de génération de tokens.

## Provenance vérifiable

- Code mesuré : `3c5700bd784ea0d03a199afa4b07dea27e6d0073`.
- Exécution : GitHub Actions Ubuntu x86_64, 4 processeurs logiques,
  Rust 1.89.0.
- Résultats : workflow A6-C2, job
  [coupled-credit](https://github.com/Memorithm/SML-GENIUS/actions/runs/38148444765).
- Source exacte de vérification : le journal du job porte les marqueurs
  `A6C2_FROZEN`, `A6C2_ALL_FROZEN`, `A6C2_TEST_RESULT` et
  `A6C2_SUMMARY`. Les validations incluent formatage, compilation,
  Clippy et tests des modules.

## Conclusion et limitations

**PASS : attribution conjointe du crédit à deux portes ANF causales par une
supervision exclusivement terminale**, sur une distribution synthétique
clairement définie. Les deux fonctions cachées sont effectivement
reconstruites sans cibles intermédiaires.

Cela ne prouve pas : la découverte d'une topologie arbitraire, une meilleure
précision linguistique, des dépendances de plusieurs dizaines de pas,
l'optimisation continue de grandes banques ANF, une mémoire acquise librement,
ni un bénéfice GPU/SIMD au niveau modèle.

Le mécanisme est expressément limité à **deux portes**, chacune d'arité 2,
et la grammaire de recherche contient déjà le bon **schéma de connexion
 temporelle** (sans révéler quelles sources ni quelles fonctions). La
recherche exhaustive croît combinatoirement avec la taille des banques.
La généralisation à un apprentissage autonome à grande échelle reste à
établir séparément.

**Discipline de recherche :** restituer les résultats de l'expérience
A6-C2 à l'utilisateur avant de lancer A6-C3 ; garder la PR en brouillon
jusqu'à révision et accord de fusion.
