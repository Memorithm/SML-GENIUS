# A6-C11D — Diagnostic préenregistré : crédit de trois portes ANF

**Protocole établi avant la première exécution de ce banc.**
**Historique de dépendance :** au moment du préenregistrement
initial, la PR #490 (A6-C10) était OPEN/DRAFT et cette
contribution a été créée comme PR empilée. La PR #490 a
**ensuite été fusionnée** au commit
`baf671324aa097f54d3c766e24cc4be3b2dd6c54` ;
la PR #491 a été repositionnée vers `main` avant
publication de ce résultat. Cet amendement de provenance
ne modifie aucun critère expérimental.

## Question : capacité, données ou classement des hypothèses ?

L'échec de la tâche à trois portes dans A6-C10 **ne peut pas être
attribué directement au seul budget de données ou d'énumération** :
le modèle A6-C10 permettait **au maximum deux portes cachées**.
Une simple multiplication du budget par dix avec cette même
grammaire ne pouvait pas produire une troisième porte.

Ce diagnostic élargit explicitement la grammaire **à trois portes
ANF**, dans l'anneau de délais natif à **128 bits**. Il examine
séparément :
1. **représentabilité** : une vraie réalisation native à trois
   portes (oracle du générateur) prédit-elle exactement ?
2. **signal** : des scores spectraux (Walsh de degrés 1 et 2,
   dont interaction avec la sortie passée) remontent-ils les
   trois positions et sources inconnues dans le classement ?
3. **budget de recherche** : l'augmentation **×10 des structures
   évaluées sur TRAIN complet**, de **64 à 640**, améliore-t-elle
   la découverte et les erreurs indépendantes, avec le même
   classement préalable ?
4. **budget de données** : multiplier le TRAIN par **4**,
   de 512 à 2 048 pas, apporte-t-il un gain à budget de
   recherche identique ?

## Famille de circuits et informations interdites au learner

Même anneau récurrent que les étapes A6-C7/8/9/10 :

```text
s0(t+1) = s127(t) XOR x1(t)       [FIXÉ]
sj(t+1) = f(s(j-1)(t), x_r(t))   [trois positions j<k<l]
sk(t+1) = g(s(k-1)(t), x_q(t))
sl(t+1) = h(s(l-1)(t), x_p(t))
y_hat(t) = x0(t) XOR s0(t+1)    [FIXÉ]
```

Les trois positions, trois canaux et trois tables booléennes
complètes (`16³ = 4 096` factorisations) sont inconnus
du learner. Les fonctions sont de vraies ANF natives GF(2)
via `LocalAnfNetwork`. Les autres portes sont des identités
fixées : **ce n'est pas un apprentissage libre de 128 portes**.

Pour t≥N, la **sortie finale passée sur TRAIN** fournit
`R_(t-N) = y_(t-N) XOR x0_(t-N)`, et la cible
`z_t = y_t XOR x0_t XOR x1_t`.
Les candidats de trois portes produisent
`z_t = h(g(f(R_(t-N),a_t),b_t),c_t)`, où chaque
entrée brute est décalée selon la position de sa porte.
Il s'agit d'un **teacher forcing uniquement pour
la recherche TRAIN**. Les tests et le classement final
s'effectuent en **récurrence native autonome**.

Le banc possède trois familles de tests, dont les règles ne
sont présentes que dans le **générateur de cibles** :

- **historique A6-C10**, N128 : XOR(j19,x2) →
  AND(j67,x3) → XOR(j108,x4) (trois portes).
- **non linéaire sévère**, N128 : AND(j23,x2) →
  AND(j69,x3) → AND(j104,x4) (trois portes).
- **contrôle négatif à quatre portes**, N128 :
  XOR(j17,x2) → AND(j47,x3) → XOR(j81,x4) →
  AND(j109,x1). L'apprenant étant limité à trois
  mutations, aucune solution exacte n'est attendue.

La vérité cachée n'est jamais transmise à la fonction
d'apprentissage, seulement les bitplanes INPUT et
les étiquettes finales TRAIN/VALIDATION.

## Spectre et budgets

1. Évaluer toutes les **508 caractéristiques de retard
   (127 slots ×4 canaux)** au moyen de corrélations
   Walsh absolues, d'abord brutes, puis après soustraction
   XOR de la caractéristique marginale dominante.
   Inclure l'interaction avec **la sortie passée
   observable**. Cette phase est identique pour
   tous les budgets de recherche.
2. Conserver les **24 meilleurs retard/canaux** et
   construire les triplets de **positions distinctes** ;
   les classer selon le score spectral agrégé.
3. Réserver deux budgets identiques en dehors de ce
   plafond : **64** structures ou **640**
   structures du même classement ; pour chaque
   structure, apprendre la meilleure fonction
   factorisable de trois portes à deux entrées.
   Les 16³ couples de tables sont réduits à
   **520 fonctions composées distinctes**.
   L'erreur conditionnelle est évaluée sur
   **TRAIN uniquement** ; pas de tri par VAL/TEST.
4. Rejouer les **16 meilleurs circuits candidats**
   au moyen de quatre segments causaux sur tout TRAIN ;
   la meilleure hypothèse est reconstruite avec les
   128 vraies portes ANF. Exiger égalité des erreurs
   avec `LocalAnfNetwork`.
5. La VALIDATION choisit entre l'identité annulaire
   et la proposition à trois portes, sans ajuster
   ses paramètres ; aucune cible intermédiaire.

**La méthode est heuristique et peut échouer.**
La famille de 24 caractéristiques est un filtre
strict : si une vraie source n'y figure pas,
même 640 combinaisons ne suffiront pas.
La décision de capacité et l'« oracle vrai
générateur » sont des diagnostics **ex post**,
jamais des informations du moteur apprenant.

## Échantillonnage / séparation

- **Trois tâches ×2 panneaux** :
  sans bruit ou labels corrompus de manière
  indépendante avec Bernoulli(1/16).
- **5 graines** indépendantes par tâche et panneau.
- Pour chaque cas, comparer TRAIN **512 pas**
  et **2 048 pas** sur le **même flux d'entrées
  imbriqué**, et pour chaque taille les budgets
  **64 et 640**.
- VALIDATION indépendante 512 pas ×64 voies,
  commune aux quatre variantes de chaque cas.
- Toutes les décisions de 3×2×5×2×2 =
  **120 politiques** sont gelées **avant la
  génération des 30 tests** indépendants,
  chacun de 4 096 pas ×64 voies.
- Pour les comparaisons brut/long, le même
  jeu de TEST est utilisé. Un modèle oracle
  de génération est mesuré sur TEST **uniquement
  comme référence supérieure de capacité**.
- Ne pas entraîner le learner avec les paramètres
  ou labels des états cachés issus du générateur.

## Mesures préenregistrées

- Nombre d'éléments vrais parmi les **top 8, top 24
  et top 80** de caractéristiques spectrales.
- La structure vraie apparaît-elle dans les
  **64/640** structures effectivement scorées ?
- Si le triplet est présent, la table composée
  factorizable est-elle retrouvée et
  la simulation autonome reproduit-elle les labels ?
- Erreurs TRAIN/VALIDATION/TEST en pourcentage,
  modèles acceptés/rejetés sur validation,
  mémoire logique N×8 pour 64 voies,
  nombre d'évaluations par budget, temps CPU.
- Séparer strictement un **progrès de recherche**
  d'une **amélioration du signal à davantage
  d'observations**.
- Sur le contrôle quatre portes, reporter
  l'échec sans masquer d'éventuels gains partiels.

**Ce n'est pas encore le banc A6-C11 complet à
grandes familles de 256 bits, ni une preuve de
meilleure précision de prédiction de langue.**
Rust 1.89, runner GitHub-hosted x86_64, sans CUDA
ni technologie logicielle NVIDIA.

## Décision

Après publication des résultats chiffrés,
choisir entre :
A. meilleur screening spectral/interactions ;
B. faisceau plus large / backtracking ;
C. allongement des données ;
D. révision de la grammaire pour ≥4 portes.
Aucune campagne suivante avant ce diagnostic
et sa revue.
