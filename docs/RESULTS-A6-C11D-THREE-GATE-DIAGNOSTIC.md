# A6-C11D — Résultats du diagnostic « budget ×10 » / « données ×4 »

**Statut : expérience diagnostique terminée et mesurée.**
**Verdict ciblé : les deux tâches à trois portes sont représentables et apprises
sans augmenter le budget ; l'échec A6-C10 venait d'abord de la grammaire
à deux portes et de son score de recherche, non du manque de données
sur les tâches testées.**

Le protocole `docs/A6-C11D-THREE-GATE-DIAGNOSTIC-PREREGISTRATION.md`
a été enregistré **avant la première exécution expérimentale**.
Le travail est livré dans la PR **#491**.
Au début, celle-ci était empilée sur la branche de la PR #490
encore ouverte ; après la fusion de #490 au commit
`baf671324aa097f54d3c766e24cc4be3b2dd6c54`,
la PR #491 a été repositionnée vers **main**.

## 1. Hypothèse et correction de diagnostic

Le témoin négatif d'A6-C10 possédait **trois portes cachées**,
mais l'algorithme ne pouvait en modifier que **deux**.
Le fait qu'il reste à environ 24,87 % d'erreur n'indiquait
donc **pas**, à lui seul, un déficit de recherche ou de données :
la solution exacte n'appartenait pas à la classe hypothétique.

A6-C11D autorise pour la première fois **trois** portes
intermédiaires ANF à deux entrées, dans un anneau de 128 bits
d'état par séquence. Les positions, canaux et tables de
vérité sont inconnus de l'apprenant ; le reste de l'anneau
d'identités, la racine XOR et le readout sont connus et fixés.

La cible de TRAIN est toujours **la sortie finale**, sans
supervision interne. La recherche exploite l'état racine
**observable à travers une sortie passée de TRAIN**,
mais la validation et le test sont effectués **en exécution
récurrente autonome native**, sans teacher forcing.

## 2. Méthode préenregistrée

La recherche commence par les **508** sources potentielles
`(position 1..127, raw x1..x4)`.

- Score **Walsh de degré 1**, puis interaction de degré 2
  avec la sortie racine passée ; retirer par XOR la caractéristique
  marginale dominante et recalculer ces scores.
- Conserver les **24** meilleures caractéristiques,
  construire les triplets à positions distinctes, puis les
  classer par score spectral commun.
- Comparer un budget de **64** triplets évalués sur
  TRAIN complet et un budget de **640** triplets
  évalués sur TRAIN complet (**×10 à ce stade précis**).
- Pour chaque triplet, calculer 16 histogrammes
  conditionnels de la sortie ; évaluer les
  **520 fonctions composées distinctes** provenant
  de `16³ = 4096` compositions booléennes possibles.
- Rescorer les **16** meilleurs circuits sur
  TRAIN en récurrence autonome à **quatre
  segments de mémoire** ; convertir le gagnant
  en vrai `LocalAnfNetwork` ANF GF(2) et
  imposer l'égalité exacte des deux évaluateurs.
- La validation ne sert qu'à accepter ou rejeter le
  modèle face à un anneau identité.

L'augmentation de budget **ne multiplie pas
le temps CPU total par dix**, car le filtrage
spectral, la factorisation des masques et les
16 vérifications natives demeurent communs.
Le temps mesuré ci-dessous est celui d'un
**appel ajustant les deux budgets ensemble**.

La recherche demeure **heuristique** : 24 sources
retenues, 64/640 triplets évalués, une grammaire
prédéfinie, et non un optimum global garanti.

## 3. Protocole mesuré et séparation des données

Trois tâches à largeur N=128 :

1. **Historique A6-C10** :
   XOR(j19, x2) → AND(j67, x3) → XOR(j108, x4).
2. **Non-linéarité sévère** :
   AND(j23, x2) → AND(j69, x3) → AND(j104, x4).
3. **Témoin négatif** à **quatre** portes cachées :
   XOR(j17, x2) → AND(j47, x3) →
   XOR(j81, x4) → AND(j109, x1).

Chaque tâche possède 5 graines et deux panneaux :
sans bruit ou bruit de labels indépendant Bernoulli(1/16).

Chaque combinaison est entraînée deux fois sur **le même
flux strictement imbriqué**, de **512** ou **2048 pas**,
64 voies bit-slicées. Chaque entraînement apprend
deux politiques indépendantes de budget **64/640**.
La validation indépendante contient 512 pas.

**120 politiques distinctes ont été gelées avant
la génération de tout jeu de TEST.**
Il existe ensuite **30 jeux de TEST indépendants**,
chacun de 4096 pas ×64 voies, réutilisé sans modification
pour les quatre variantes de chaque cas :
**120 résultats, 24 synthèses**.

Les règles du générateur ne passent jamais à l'API du learner.
Elles sont utilisées seulement **après l'entraînement** pour
mesurer le classement des véritables sources et la capacité
maximale du circuit.

## 4. Résultats sur TEST autonome de 4096 pas

Erreurs binaires **en %**, moyenne sur cinq graines.
La colonne « 64 » vaut aussi, effectivement, pour
640 triplets évalués, et 512 vaut aussi pour
2048 pas de TRAIN : **aucun de ces changements
n'a modifié les erreurs, sélections ou règles**.

| Tâche | Panneau | TRAIN 512, budget 64 | TRAIN 2048, budget 640 | Oracle natif du générateur |
|---|---|---:|---:|---:|
| XOR → AND → XOR | propre | **0,000 %** | **0,000 %** | 0,000 % |
| XOR → AND → XOR | bruit 1/16 | **6,262 %** | **6,262 %** | 6,262 % |
| AND → AND → AND | propre | **0,000 %** | **0,000 %** | 0,000 % |
| AND → AND → AND | bruit 1/16 | **6,253 %** | **6,253 %** | 6,253 % |
| **Quatre portes — négatif** | propre | **12,176 %** | **12,176 %** | 0,000 % |
| **Quatre portes — négatif** | bruit 1/16 | **16,898 %** | **16,898 %** | 6,244 % |

L'« oracle » utilise les **vraies quatre portes du
générateur** uniquement pour vérifier que les jeux
de données sont réalisables par un circuit plus grand.
Il n'est **pas** un résultat d'apprentissage ni
une comparaison de budget équitable.

**Sur les deux tâches à trois portes, toutes les
80 politiques (2 tâches ×2 panneaux ×5 graines
×2 longueurs TRAIN ×2 budgets) sont exactes en
positions, sources et tables de vérité.** Les
40 politiques propres atteignent zéro erreur TEST,
et les bruitées retrouvent exactement le plancher
de corruption du jeu de test.

**Le contrôle à quatre portes reste en échec
sur les 40 politiques correspondantes**, malgré
des gains partiels et l'acceptation sur validation.
Son erreur (12,176 % / 16,898 %) est
supérieure au niveau du bruit seul : les 3
portes autorisées n'expliquent pas toute
la dynamique du générateur à 4 portes.

## 5. Diagnostic des hypothèses

### A. Insuffisance de données : non observée ici

À **512 pas**, les 3 positions et 3 sources
de chacune des deux tâches représentables
sont déjà correctement remontées dans le
**top 8 spectral pour les 5/5 graines**,
sans bruit comme avec bruit.

Ceci représente **40/40 classements TRAIN**
(2 tâches ×2 panneaux ×2 tailles TRAIN ×5 graines),
avec les **trois véritables sources dans le top 8**.
Le passage **512 → 2048 pas** n'améliore aucun résultat
de recherche ni aucune sortie TEST sur ces cas.

Cela n'établit **pas** que 512 pas suffiraient
sur des tâches moins structurées, avec
davantage de portes, d'autres bruits ou
des canaux de données corrélés.

### B. Budget de recherche : le ×10 n'est pas décisif ici

La structure à trois portes correcte figure déjà
dans les **64 premiers** triplets classés
par le score spectral dans **40/40 cas
positifs**. Passer de **64 à 640**
évaluations complètes ne change pas le
gagnant ni les résultats sur ce banc.

L'important est donc **la qualité du score
de classement** (peeling XOR, interactions
Walsh avec la racine observée) et
**la présence explicite d'une
troisième porte dans la grammaire**.
Un simple agrandissement aveugle du faisceau
n'aurait pas été nécessaire pour ces signaux.

Le témoin à quatre portes donne un contre-exemple :
les quatre sources ne sont généralement **pas
toutes** dans le top 24, et la classe de
trois portes ne peut de toute manière
représenter sa fonction entière.

### C. Coût observé

Exécution sur **GitHub Actions Ubuntu x86_64,
4 processeurs logiques, Rust 1.89.0** ;
**pas le Dell, pas de GPU**.

| Longueur TRAIN | Temps moyen du fit des DEUX budgets (64 et 640 ensemble) | Charge utile du ring de 128 bits pour 64 voies |
|---|---:|---:|
| 512 pas | ≈ **12,7–12,9 ms** | **1 024 octets** |
| 2048 pas | ≈ **55,0–55,6 ms** | **1 024 octets** |

Le facteur de durée observé suit approximativement
la longueur du TRAIN. Ce calcul ne donne **pas**
un ratio de vitesse entre « K64 » et « K640 »,
puisque la fonction unique calcule les
deux budgets avant de rendre son résultat.

Les 1 024 octets représentent seulement les
128 mots `u64` nécessaires à 64 séquences
parallèles dans le scoreur, et **pas**
le RSS total, les coefficients ANF,
les observations temporelles, le tas des candidats
ou les allocations du programme.

## 6. Provenance et reproductibilité

- **Révision mesurée :**
  `671d092230e44eb979edd216f0a93b3fe7f0396d`.
- **Job GitHub-hosted vert :**
  [A6-C11D, recherche trois portes et diagnostic budgétaire](https://github.com/Memorithm/SML-GENIUS/actions/runs/38179852548/job/114589906811).
- Contrôles Rust 1.89 sur ce SHA :
  `cargo fmt --check`, `cargo check --all-targets`,
  Clippy strict, tests de toutes les variantes du scoreur
  quatre-segments vs ANF native, factorisation
  `4096 → 520` fonctions, tests d'imbrication TRAIN,
  puis campagne complète **120/120/24 : SUCCESS**.
- **CI générale sur la branche principale :**
  non revendiquée comme verte pour ce SHA tant
  que le changement de base de la PR #491
  vers `main` n'a pas été requalifié.

Reproduire avec Rust 1.89 :

```bash
cargo fmt --all -- --check
cargo test --locked -p sml-core anf_three_gate_diagnostic
cargo test --locked -p sml-core --bin sml-three-gate-diagnostic-bench
cargo run --release --locked -p sml-core --bin sml-three-gate-diagnostic-bench -- 5
```

## 7. Verdict et décision

**PASS diagnostique A6-C11D**, mais ne pas exagérer :

- **Le défaut initial était d'abord une
  restriction de capacité à deux portes**, et
  non la démonstration d'un manque de données.
- Une **grammaire à trois portes** et un bon
  classement spectral par racine passée/peeling
  suffisent à résoudre exactement **deux familles
  contrôlées**, avec et sans bruit de labels.
- Ni **×10 de budget**, ni **×4 de TRAIN** ne
  produit de gain supplémentaire sur ces tâches.
- Le contrôle à **quatre portes** échoue, et
  le moteur reste une **architecture annulaire
  fixée**, limitée à N128 et trois mutations.
  Aucune preuve de recherche libre dans
  une banque ANF de taille massive ni de
  qualité de langage SML-25B.

**Direction scientifique issue des mesures :**
prioriser une généralisation robuste du
classement spectral/ANF à **quatre portes**,
aux signaux masqués et aux banques larges,
avec faisceaux adaptatifs et évaluation
à budget égal. Ne pas investir
d'abord dans davantage de données
sur ces deux tâches déjà résolues.

**Stop méthodologique :** PR #491 en brouillon,
résultats présentés à l'utilisateur avant tout
banc A6-C11 complémentaire.
