# SYNTH-LEARN1G — Découverte active d'un circuit de négation posit8 complet

**Objet :** franchir le stade de la classification de statut : apprendre et compiler une véritable opération sur les huit bits de la valeur posit8, ici la **négation unaire**. Le produit `Posit8(raw) × Posit8(0xc0)` (−1) est le professeur arithmétique. Le nouveau compilateur doit générer la même transformation binaire sur **tous les 256 encodages**, sans consulter l'oracle pendant l'exécution.

## Hypothèse compositionnelle préenregistrée

On impose une architecture récurrente avec deux signaux binaires par étage :
`current = bit_i(raw)` et `prefix = OR des bits d'entrée inférieurs à i`.

Les deux fonctions à apprendre sont :
1. **Propagation du préfixe** : à partir de `(current,prefix)`, prédire le préfixe pour l'étage suivant. Pendant l'entraînement seulement, son étiquette est le OU des `i+1` bits les moins significatifs de l'entrée.
2. **Bit négatif** : à partir des mêmes `(current,prefix)`, prédire le bit `i` de `Posit8(raw)×(-1)` calculé par le professeur.

Le moteur ne reçoit pas de nom de porte cible. Il choisit **par interrogations actives** parmi neuf portes : constantes, fils directs, inversions, AND, OR, XOR. À chaque étape, sur les 2048 couples `(raw, position)` potentiels, il sélectionne **la paire qui sépare le mieux les hypothèses encore compatibles**, interroge son enseignant **uniquement pour cet exemple**, puis supprime les hypothèses contradictoires. Plafond : huit requêtes par fonction, seize au total. Les consultations de l'oracle pendant l'audit final sont strictement postérieures au gel des portes, jamais utilisées pour l'apprentissage.

L'interprétation de `prefix` et le découpage récurrent sont **des biais architecturaux fournis**, pas une topologie découverte librement. Cependant, les **opérateurs de chaque étage sont sélectionnés à partir des observations** et leurs compositions doivent calculer une transformation complète, pas seulement un prédicat.

## Critères exacts

- Exactitude bit à bit de chaque opérateur sur les couples non interrogés `(raw, bit)`.
- Reconstruction récurrente de la négation sur les 256 valeurs, y compris zéro et NaR, **0 erreur** face au produit par −1 de notre oracle. La valeur NaR n'est pas considérée comme un nombre ; ce test vérifie uniquement sa préservation en tant que sentinelle.
- Génération complémentaire d'un **noyau de magnitude finie positive** : pour une valeur finie négative, appliquer le circuit appris ; pour une valeur finie positive, conserver l'encodage ; si NaR, retourner la sentinelle. Ce sélecteur de signe est encore une règle fournie.
- Génération réelle de deux fonctions GNU GAS x86-64 System V avec huit étages OR/XOR compilés en instructions natives, sans tableau des 256 réponses.
- Assemblage `.s → .o`, désassemblage `objdump`, liaison FFI Rust ; comparaisons exhaustives des **256 réponses machine** aux circuits appris et à l'oracle. Tests Rust 1.89 et CI générale sur le SHA exact, aucune fusion anticipée.

Les résultats ne démontrent ni une norme posit externe, ni un additionneur/multiplicateur posit complet, ni une vitesse supérieure à l'instruction x86 `neg` ou `abs`, ni une application directe à l'inférence SML-GENIUS. Le circuit récurrent est une primitive de **normalisation de magnitude et de comparaison** pour les futurs sous-circuits ADD et MUL. Aucun SDK GPU ni machine privée n'est mobilisé.

Fichiers : `research/boolean-posit/posit8_active_neg8.rs`, `research/boolean-posit/posit8_active_neg8_native_test.rs`.
