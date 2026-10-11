# A6-C3 — Attribution du crédit sur trois portes ANF et deux sauts temporels

**Statut : expérience A6-C3 terminée, résultats observés.** La PR #482 reste en
brouillon jusqu'à communication des résultats. Aucune A6-C4 n'est lancée.

## Hypothèse vérifiée

Peut-on apprendre des fonctions booléennes distribuées dans **trois portes
d'état récurrentes en chaîne**, dont l'information circule sur **deux décalages
temporels**, uniquement à partir de l'erreur de la **sortie finale**,
sans cibles supervisées pour les portes cachées ?

Modèle natif utilisé :

```text
s0(t+1) = f(x_i(t), x_j(t))
s1(t+1) = g(s0(t), x_k(t))
s2(t+1) = h(s1(t), x_l(t))
ŷ(t) = x0(t) XOR s2(t+1)
```

Les trois fonctions à deux entrées sont converties en coefficients ANF par
la transformée de Möbius existante et exécutées via `LocalAnfNetwork`.
La cible observée, utilisée seulement par le générateur synthétique, est :

```text
y(t) = x0(t) XOR x4(t)
       XOR (x3(t-1) AND (x1(t-2) XOR x2(t-2)))
```

Toutes les entrées précédant le début d'une séquence sont nulles.
L'algorithme d'apprentissage ne reçoit que les entrées brutes et y(t),
**jamais les cibles intermédiaires** des trois portes.

## Dispositif

- **5 graines** par panneau : propre et bruit de label Bernoulli(1/16)
  indépendant par bit, soit dix cas d'entraînement.
- **6 144** observations d'entraînement, **3 072** de validation et **3 072**
  de test par graine/panneau, en registres `u64` bit-slicés (64 voies).
- Entrées `x0..x4` i.i.d. Bernoulli(1/2), avec `x0` fixé comme baseline.
- Les quatre autres entrées sont candidates, les deux sources de la première
  porte et la source supplémentaire de chacune des deux suivantes sont
  **à retrouver**.
- 16 tables de vérité par porte à deux entrées. Recherche conjointe :
  `C(4,2) × 16 × 4 × 16 × 4 × 16 = 393 216` hypothèses entraînement.
- Tous les modèles et décisions d'acceptation sont figés exclusivement sur
  entraînement/validation **avant la génération** du jeu de test.
- Vérification oracle : le score train obtenu par masques booléens `u64` est
  égal exactement à la simulation complète du réseau ANF natif pour les
  modèles retenus. Les modèles complets sont engagés uniquement si la
  validation s'améliore strictement.

Contrôles : réseau de portes nulles ; descente coordonnée monotone, une porte
à la fois ; descente coordonnée avec **redémarrages aléatoires**, autorisée à
évaluer au maximum les mêmes **393 216 hypothèses finales** que la recherche
conjointe ; ablation successive de chacune des trois portes du modèle appris.

Attention : même si le **plafond** d'évaluations est égalisé, les contrôles
à redémarrages peuvent arrêter tôt après avoir trouvé une solution parfaite.
Les nombres réels de candidats évalués sont donc présentés ci-dessous. La
recherche conjointe énumère toujours toute la grammaire. Le contrôle monotone
sans redémarrage utilise quant à lui très peu d'évaluations.

## Résultats observés sur séquences de test inédites

**Taux d'erreur binaire de la tâche synthétique**, moyenne de cinq graines,
en %. Plus faible = meilleur.

| Panneau | Portes nulles | Coordonnée stricte | Coordonnée + redémarrages | Conjointe 3 portes |
|---|---:|---:|---:|---:|
| **Sans bruit** | 49,96745 % | 23,88021 % | **0,00000 %** | **0,00000 %** |
| **Bruit 1/16** | 49,69401 % | 23,05339 % | **6,06120 %** | **6,06120 %** |

Nombre total d'erreurs sur les cinq tests (15 360 observations par panneau) :

| Panneau | Portes nulles | Coordonnée stricte | Redémarrages | Conjointe |
|---|---:|---:|---:|---:|
| Sans bruit | 7 675 | 3 668 | **0** | **0** |
| Bruit 1/16 | 7 633 | 3 541 | **931** | **931** |

La recherche conjointe et le contrôle par redémarrages construisent chacun
**10/10 modèles acceptés** sur validation, puis obtiennent des scores
identiques sur les deux panneaux. Les cinq réseaux du panneau sans bruit
généralisent avec zéro erreur. En présence de bruit, le taux observé est
proche des 6,25 % de corruption nominale.

Les dix modèles conjoints ont identifié les bons emplacements de sources :

- porte 0 : `[4, 5]` = `x1, x2`, 2 monômes ANF (XOR) ;
- porte 1 : `[0, 6]` = état précédent de la porte 0 et `x3`,
  1 monôme ANF (AND) ;
- porte 2 : `[1, 7]` = état précédent de la porte 1 et `x4`,
  2 monômes ANF (XOR).

Ces représentations sont identifiées à équivalence de codage interne près :
des inversions complémentaires pourraient représenter la même fonction
observée dans d'autres recherches, donc on ne revendique pas une unicité
mathématique générale des fonctions cachées.

## Attribution causale : ablations

Mettre à zéro une seule des portes apprises modifie fortement la tâche :

| Panneau | Sans porte 0 | Sans porte 1 | Sans porte 2 |
|---|---:|---:|---:|
| Sans bruit | 23,88021 % | 23,88021 % | 49,96745 % |
| Bruit 1/16 | 27,29167 % | 27,29167 % | 49,69401 % |

Les trois portes sont ainsi nécessaires à la solution retenue, pour cette
topologie et cette tâche.

## Contrôle crucial : avantage des redémarrages

La descente coordonnée stricte depuis l'état nul plafonne vers 23–24 %
d'erreur, bien qu'elle améliore la baseline. Cependant, **la recherche
conjointe n'est PAS supérieure à un contrôle de recherche locale suffisamment
diversifié** : la descente coordonnée avec redémarrages retrouve le même
résultat sur les dix cas.

Budget réel d'évaluations de l'objectif final :

- recherche conjointe : **393 216** pour chacun des dix cas ;
- descente monotone sans redémarrage : **449** propositions pour les cas
  sans bruit, et **449 à 673** pour les cas bruités ;
- redémarrages, panneau sans bruit : **673 à 7 857 évaluations**
  (1 à 17 redémarrages) avant convergence exacte ;
- redémarrages, panneau bruité : **393 216 évaluations** dans chacun des
  cinq cas (812 à 870 redémarrages), avec le même taux d'erreur test final
  que la recherche conjointe.

**Conclusion comparative :** le résultat valide l'apprentissage à trois
portes par supervision terminale et montre que **sortir des plateaux
d'attribution du crédit grâce aux redémarrages** est une piste crédible,
potentiellement plus économique sur les tâches exactes que l'énumération
conjointe. Les données ne justifient pas d'affirmer que l'exhaustivité
conjointe est intrinsèquement meilleure.

## Ressources et reproductibilité

Exécution sur un **runner GitHub Actions Ubuntu x86_64, 4 processeurs
logiques, Rust 1.89.0**, et non sur le Dell.

- SHA exact des mesures : `a1a828e225c5b2a70e8c69c4779cf1ffb8e2d1ca`.
- Contrôle expérimentateur : `A6C3_ALL_FROZEN,true`.
- Journal brut : [A6-C3 GitHub Actions #38151775397](https://github.com/Memorithm/SML-GENIUS/actions/runs/38151775397/job/114506532125).
- Compilation, Clippy, formatage et tests ciblés : **verts** sur le SHA
  mesuré. La CI générale complète possède un contrôle séparé.
- Cache des réponses du deuxième étage : **4 718 592 octets** de charge
  utile (4,50 Mio), hors métadonnées Vec et allocations.
- Durée de recherche et contrôles sur le runner : moyenne **159,7 ms**
  par cas sans bruit et **323,2 ms** par cas bruité, mesurée pour le
  protocole entier, pas pour les modèles isolés. Il n'existe pas encore
  de mesure Dell ou de débit de tokens associée.

Rejeu : `cargo run --locked --release -p sml-core --bin
sml-three-hop-credit-bench -- 5` sur Rust 1.89.0. Seuls les temps
physiques peuvent varier entre matériels.

## Conclusion et limites

**PASS : attribution de crédit terminale à trois portes récurrentes
et deux pas de dépendance temporelle** sur un banc booléen synthétique
contrôlé et sans supervision intermédiaire.

**Précision essentielle :** le résultat démontre la viabilité du mécanisme
dans une **grammaire connue de trois étages**, pas l'apprentissage libre de
la topologie, ni une mémoire spontanément acquise sur de longues durées.
Le nombre d'hypothèses énumérées croît combinatoirement avec les sources,
les fonctions et la profondeur. Le résultat ne prouve ni amélioration de
la précision linguistique, ni entraînement de grandes banques ANF, ni débit
GPU/SIMD supérieur, ni performance d'un SML-25B.

La prochaine question scientifique serait de réduire le coût de recherche
et de tester la récupération du crédit sans fixer la topologie causale.
**Aucune expérience suivante n'est ouverte ici : restitution et décision
de l'utilisateur d'abord.**
