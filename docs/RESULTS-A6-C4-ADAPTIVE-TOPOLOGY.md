# A6-C4 — Découverte de topologies récurrentes ANF par perte terminale

**Statut : expérience terminée ; résultats empiriques vérifiés.**
La PR #483 reste en brouillon jusqu'à restitution au demandeur.
**Aucune expérience A6-C5 n'a été lancée.**

## Hypothèse falsifiable

A6-C3 savait construire une chaîne causale ANF, mais sa forme était fixée :
trois portes en série, avec deux sauts temporels. A6-C4 retire cette
contrainte particulière et cherche, **sur la seule erreur de sortie**, à
choisir quelles connexions et quels états sont nécessaires.

Le moteur `anf_credit_topology` parcourt une **famille bornée de graphes
acycliques à trois emplacements** :

```text
s0(t+1) = f(x_i(t), x_j(t))
s1(t+1) = g( {s0(t), x(t)} : deux sources admissibles )
s2(t+1) = h( {s0(t), s1(t), x(t)} : deux sources admissibles )
y_hat(t) = x0(t) XOR s2(t+1)
```

Contraintes importantes : trois emplacements d'état maximum, arité 2,
quatre canaux bruts candidats en plus du canal baseline, sources
récentes uniquement et arêtes récurrentes dirigées vers des emplacements
ultérieurs. La recherche décide cependant elle-même si les trois étages
sont actifs, et autorise les motifs **direct**, **un saut**, **deux sauts**
et **deux branches parallèles**.

Les portes amont sont testées par tables booléennes, la dernière porte est
ajustée **exactement sur le seul entraînement** via les quatre comptes
conditionnels de sa table de vérité. Les fonctions retenues sont converties
en véritables coefficients ANF (transformée de Möbius du moteur Rust).
Le score des circuits candidats est vérifié à l'identique par simulation
complète `LocalAnfNetwork` avant l'acceptation sur validation.

## Protocole

Quatre fonctions synthétiques, inconnues du moteur apprenant :

1. **Directe, 0 saut** : `x0(t) XOR (x1(t) AND x2(t))`.
2. **Contournement, 1 saut** :
   `x0(t) XOR (x3(t) AND [x1(t-1) XOR x2(t-1)])`.
3. **Chaîne, 2 sauts** :
   `x0(t) XOR x4(t) XOR ([x1(t-2) XOR x2(t-2)] AND x3(t-1))`.
4. **Branches parallèles, 1 saut** :
   `x0(t) XOR ([x1(t-1) AND x2(t-1)] XOR
   [x3(t-1) AND x4(t-1)])`.

Chaque motif a été testé avec et sans bruit de label indépendant
Bernoulli(1/16), sur **5 graines indépendantes**, soit **40 entraînements**.
Chaque configuration utilise **3 072 observations d'entraînement**,
**1 536 de validation**, **1 536 de test** (64 lanes booléennes).
Les graines des trois splits sont distinctes.

**Barrière anti-fuite** : tous les circuits et leurs acceptations sont
figés à partir d'entraînement/validation **avant de générer le moindre
jeu de test**. Aucune cible d'une porte intermédiaire n'est fournie.

Comparaisons enregistrées sur les mêmes exemples :

- réseau à trois portes nulles ;
- meilleur circuit **direct seulement** (aucun état récurrent, 6 couples
  de sources candidates) ;
- meilleur circuit **chaîne fixée** du type A6-C3 (24 576
  ajustements de sortie sur entraînement) ;
- circuit à **topologie adaptative** (77 190 ajustements de sortie).

Le budget des trois contrôles **n'est pas égalisé**. Par ailleurs un
« ajustement de sortie » teste implicitement les 16 fonctions de la porte
finale via un comptage optimal : ces chiffres ne sont pas comparables
un pour un aux 393 216 hypothèses explicites d'A6-C3.

## Résultats réels sur les ensembles de test

Valeurs : **taux d'erreur binaire sur la tâche synthétique**, moyenne sur
5 graines, exprimée en %. Plus petit est meilleur.

| Motif / panneau | Portes nulles | Direct appris | Chaîne fixée apprise | Topologie adaptative |
|---|---:|---:|---:|---:|
| Direct, sans bruit | 25,430 % | **0,000 %** | 25,391 % | **0,000 %** |
| Direct, bruit 1/16 | 27,786 % | **6,211 %** | 28,047 % | **6,211 %** |
| Contournement, sans bruit | 24,323 % | 24,896 % | 24,023 % | **0,000 %** |
| Contournement, bruit 1/16 | 27,474 % | 28,125 % | 27,331 % | **6,615 %** |
| Chaîne, sans bruit | 49,440 % | 22,344 % | **0,000 %** | **0,000 %** |
| Chaîne, bruit 1/16 | 50,651 % | 26,016 % | **6,654 %** | **6,654 %** |
| Parallèle, sans bruit | 35,495 % | 35,495 % | 36,458 % | **0,000 %** |
| Parallèle, bruit 1/16 | 38,516 % | 38,516 % | 38,516 % | **6,680 %** |

Bilan regroupé à poids identique entre les quatre tâches :

| Panneau | Réseau nul | Direct | Chaîne fixée | Adaptatif |
|---|---:|---:|---:|---:|
| Sans bruit | 33,672 % | 20,684 % | 21,468 % | **0,000 %** |
| Bruit 1/16 | 36,107 % | 24,717 % | 25,137 % | **6,540 %** |

Les cinq validations test sans bruit de chaque topologie atteignent zéro
erreur, soit **20/20 configurations propres**. Les configurations bruitées
restent proches du plancher de corruption théorique de 6,25 %, avec
l'écart statistique attendu sur des échantillons finis.

## Découverte effective des graphes

Sur les 40 configurations, la profondeur causale **effective** est correcte :
**40/40**. La famille fonctionnelle (à permutation des emplacements
équivalents près) est correcte dans les **40/40 cas**, y compris avec bruit.
Le modèle adaptatif est accepté sur validation dans **40/40 cas**.

| Motif | Topologie retenue | Sauts | Portes actives | Monômes ANF actifs |
|---|---|---:|---:|---:|
| Direct | `direct_0_hops` | 0 | 1 | 1 |
| Contournement | `middle_1_hop` | 1 | 2 | 3 |
| Chaîne | `chain_2_hops` | 2 | 3 | 5 |
| Parallèle | `parallel_1_hop` | 1 | 3 | 4 |

**Nuance à conserver :** pour le motif « contournement », la solution
n'utilise pas littéralement le lien `s0 -> s2`; elle apprend l'équivalent
fonctionnel à un saut **`s1 -> s2`**, avec `s0` inutilisée. La
récupération des familles de graphes n'implique donc ni unicité ni
identification exacte des noms de portes cachées.

Les ablations vérifient que retirer une porte **active** augmente
l'erreur ; retirer une porte inutilisée ne détériore pas le résultat.
Ainsi, pour la tâche directe, les deux premières portes sont inutiles ;
pour le contournement, la première est inutilisée, mais les portes
médiane et de sortie sont causales. Dans le motif parallèle, chacune
des trois portes contribue à la sortie.

## Performance, mémoire et provenance

- **Révision effectivement mesurée** :
  `676a195ac639e60dfc4760f9603f728b8d02d617`.
- **Runner GitHub Actions**, Ubuntu x86_64, 4 processeurs logiques,
  Rust **1.89.0**. **Ce n'est pas une mesure Dell.**
- [Journal intégral du banc A6-C4](https://github.com/Memorithm/SML-GENIUS/actions/runs/38153831852/job/114512583192).
- Marqueurs machine : `A6C4_FROZEN` (40),
  `A6C4_ALL_FROZEN,true` (1),
  `A6C4_TEST_RESULT` (40) et `A6C4_SUMMARY` (8).
- Format, vérifications Rust/Clippy, tests de l'oracle, expérimentation
  spécialisée : **tous verts** sur la révision expérimentale.
  La suite CI complète est suivie par un contrôle séparé.
- **Temps moyen de fit sur GitHub** : environ **35,59 ms**
  par graine/motif/panneau, pour la recherche adaptative **et** ses
  contrôles. Pas un débit de tokens, ni une comparaison matérielle à A6-C3.
- **Charge utile des deux séries temporaires `u64`** :
  **768 octets** par cas de 48 pas (deux séries de 48 mots de 8 octets).
  Cela **n'est pas** la mémoire totale du processus : les entrées,
  tables, objets Rust, allocations et métadonnées en sont exclus.
- Aucun outil CUDA, cuDNN, TensorRT ou autre logiciel NVIDIA.

Rejeu Rust 1.89 :

```bash
cargo test --locked -p sml-core anf_credit_topology
cargo test --locked -p sml-core --bin sml-topology-credit-bench
cargo run --locked --release -p sml-core --bin sml-topology-credit-bench -- 5
```

## Verdict et limites de généralisation

**PASS A6-C4 : sélection de connexions et de profondeur causale au sein
d'une grammaire bornée, avec seule supervision de sortie et revalidation
hors entraînement.** Contrairement à A6-C3, la topologie de chaîne n'est
plus imposée. Le système apprend à garder des portes inactives lorsque
leur utilisation n'est pas nécessaire et découvre une combinaison
parallèle lorsque la fonction cible l'exige.

**Non démontré :** architecture libre avec cycles, latences arbitraires,
crédit causal sur des dizaines/centaines de pas, stabilité du coût avec
grandes banques ANF, amélioration de la prédiction linguistique,
généralisation sur données naturelles, ou performance du modèle SML-25B.
L'échelle du banc est délibérément minuscule et les quatre familles de
tâches sont synthétiques.

**Décision méthodologique :** communiquer ces mesures et limites au
demandeur avant toute expérience A6-C5 ; la PR #483 demeure en brouillon
jusqu'à cette communication et à sa validation.
