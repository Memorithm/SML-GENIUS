# A6-C6 — Préenregistrement du banc ANF élastique 4–256 bits

Date du protocole : 11 octobre 2026, **avant la première exécution du banc**.

## Question scientifique

Peut-on étendre l'apprentissage d'une porte ANF récurrente depuis A6-C5b
(4 bits) jusqu'à **8, 16, 32, 64, 128 et 256 bits d'état logique par
séquence**, en conservant uniquement la supervision de la sortie finale,
et reproduire une parité périodique à chaque largeur sur des séquences
indépendantes nettement plus longues que celles d'entraînement ?

**Un résultat positif n'établira pas que nous savons apprendre un graphe
de 256 portes arbitraires.** Il établira uniquement qu'un échafaudage de
délais ANF et un mécanisme de sélection train/validation s'étendent à
ces largeurs avec mémoire et temps mesurables.

## Architecture fixée et paramètres effectivement appris

Pour une largeur `N` parmi `[4, 8, 16, 32, 64, 128, 256]` :

```text
s0(t+1)  = f(s_tap(t), x_raw(t))
si(t+1)  = s_(i-1)(t)              pour i = 1..N-1
prediction(t) = x0(t) XOR s0(t+1)
```

- L'**anneau de délais identité** est fixé avant entraînement ;
  chaque registre de délai est une vraie porte ANF à une entrée,
  avec la table d'identité. Ces coefficients ne sont **pas** appris.
- Le **tap de rétroaction** (N possibilités), le canal brut
  (4 possibilités) et la **table de vérité binaire complète de f**
  (16 possibilités) sont sélectionnés exclusivement sur entraînement,
  sans étiquettes cachées.
- Toutes les fonctions sélectionnées sont converties en ANF
  (transformée de Möbius) et vérifiées par le vrai
  `LocalAnfNetwork` bit-slicé, indépendant du scorer annulaire
  utilisé pour explorer les hypothèses.
- La validation choisit la **plus petite largeur de la famille**
  avec l'erreur de validation minimale, à égalité de score,
  puis n'accepte le modèle complet qu'en cas d'amélioration
  stricte contre `x0(t)`.
- Aucune fonction n'est réglée ni aucune largeur choisie à partir
  des données de test.

## Familles, tâches et contrôles

Les neuf tâches sont :

1. Directe : `y(t) = x0(t) XOR x1(t)`, aucune mémoire
   temporelle réellement nécessaire.
2. Parité à stride **4** (comparaison A6-C5b).
3. Parité à stride **8**.
4. Parité à stride **16**.
5. Parité à stride **32**.
6. Parité à stride **64**.
7. Parité à stride **128**.
8. Parité à stride **256**.
9. Parité à stride **257**, *témoin négatif délibéré* :
   le plus grand modèle (256 bits) n'a pas la capacité
   de mémoriser 257 parités de phase indépendantes.

Chaque tâche est déclinée en panneau **propre** et avec
**bruit indépendant de label Bernoulli(1/16)**.

Contrôles systématiques : sortie `x0` sans mémoire, sept
familles fixes entraînées sur les mêmes exemples, et largeur
sélectionnée par validation. L'ancienne A6-C5b reste la
référence physique/algorithmique à 4 bits mais n'est pas
ré-entraînée ici avec sa recherche exponentielle plus riche.

**Attention :** une machine à N bits de mémoire possède au plus
2^N états logiques par séquence, mais cela ne signifie pas qu'elle
apprend une mémoire générique de N tokens. Chaque expérience
étudie des *parités de phase* et une topologie annulaire connue.

## Séparation des données et horizon

**5 graines** indépendantes par couple tâche/panneau,
soit **90 décisions train/validation**.

- Entraînement : 512 pas ×64 lanes = 32 768 observations
  par graine/panneau/tâche.
- Validation : 512 pas ×64 lanes, autre graine.
- **Après que les 90 choix auront été figés :**
  4 096 pas ×64 lanes et 16 384 pas ×64 lanes, deux
  nouveaux jeux de test indépendants.
- Les états s'initialisent à zéro pour chaque nouvelle
  séquence. Les 64 voies sont calculées simultanément par
  mots `u64` ; aucune voie n'est une information temporelle future.
- Les 256 premières positions d'un modèle à 256 bits sont une
  phase de remplissage ; au moins un second cycle de 256 pas
  est présent dans les données d'entraînement.

Sorties attendues : **90 lignes de gel**, **180 lignes de test**,
**36 résumés** (9 tâches ×2 panneaux ×2 horizons).

## Complexité et optimisation de recherche

La recherche exhaustive de toutes les 4^(N-1) fonctions de délais
serait impossible. Le présent protocole la **supprime explicitement**
en fixant les portes de délai sur l'identité.

Un seul balayage des taps 1..256, des quatre canaux d'entrée et
des 16 tables de vérité du portail fournit des scores de train
réutilisés par tous les groupes de largeur :

- 4 bits : **256 candidats distincts** admissibles.
- 8 bits : **512**.
- 16 bits : **1 024**.
- 32 bits : **2 048**.
- 64 bits : **4 096**.
- 128 bits : **8 192**.
- 256 bits : **16 384**.

Ceci représente **16 384 évaluations uniques** pour toute la
famille et non leur somme, grâce au partage des préfixes.
Le traitement d'une hypothèse coûte O(T), avec un tampon
annulaire `Vec<u64>` de 256 mots réutilisé **sans allocation
par candidat**. La construction native possède réellement N
portes d'état pour chaque famille de largeur.

Les mesures distinguent :
- le temps de recherche des candidats + validation par largeur ;
- la mémoire utile des états (`N × 8 octets` pour
  64 séquences bit-slicées), **hors overhead** ;
- le nombre de coefficients ANF et de connexions du
  modèle natif ;
- le débit de **bits de séquences évalués/seconde** du modèle
  natif en test ; **pas des tokens de langue/seconde**.
- La plateforme, le SHA exact, la version Rust,
  et les budgets effectivement consommés.

## Critères d'interprétation préenregistrés

1. **Support matériel / logiciel jusqu'à 256 bits :**
   tous les réseaux natifs doivent se construire, rester
   déterministes et réussir les tests de limites de largeur.
2. **Apprentissage de capacité :** la plus petite famille
   admissible doit être retenue pour les tâches représentables,
   et la parité doit généraliser sans bruit jusqu'à 16 384 pas.
3. **Panneau bruité :** erreur attendue voisine de 6,25 %
   si la bonne fonction est retrouvée ; ne pas se contenter
   de bons résultats sur entraînement.
4. **Témoin 257 :** ne pas annoncer une capacité réussie si
   les tests longs restent vers 50 % ; observer si la validation
   accepte abusivement une architecture inadéquate.
5. **Coût :** confronter latence, mémoire d'état et nombre
   d'évaluations aux familles 4 à 256 ; ne pas déclarer
   d'accélération matérielle GPU non mesurée.

## Limites majeures

Topologie de lignes de délais imposée, arité 2 fixe, tâche
booléenne synthétique, 64 voies, 256 bits **logiques par
séquence** et non un entier Rust scalaire `u256`.
Aucun modèle de langage n'est entraîné. Les résultats
n'indiquent pas un gain en bits/octet, en précision
d'inférence du modèle SML-25B, ni l'apprentissage libre
de banques ANF très larges. Pas de CUDA ou de logiciels
NVIDIA. Exécution sur le runner GitHub Actions tant
qu'un canal Dell contrôlé n'est pas disponible.

**La discipline expérimentale demeure** : une seule
campagne A6-C6, publication du résultat (positif ou
négatif) et arrêt avant tout banc suivant.
