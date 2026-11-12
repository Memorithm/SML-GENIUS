# BPB-P8-SYNTH1 — Synthèse effective de circuits booléens ADD/MUL posit8

**Recherche Rust originale, autonome et CPU-only, 12 novembre 2026.** C'est l'étape suivante de `POSIT8_LEARNED_CIRCUIT_PROTOCOL_20261112.md`.

## Problème précisément résolu

Pour chaque couple de deux posits8 `a,b`, produire la représentation exacte calculée par l'oracle de référence pour les opérations `posit8_add(a,b)` et `posit8_mul(a,b)`. L'ensemble des **65 536** couples de 8 bits est fini et entièrement énumérable. `NaR` (`0x80`), zéro, extrêmes, arrondis et saturation suivent *l'oracle déclaré*. Cela ne qualifie pas indépendamment sa conformité à l'intégralité du Posit Standard 2022.

**Ce programme synthétise réellement les huit bits de sortie de l'addition et de la multiplication**, contrairement à SYNTH0 qui ne synthétisait que la négation. Il ne mémorise pas simplement un tableau à interroger à l'exécution. Le travail est une **synthèse symbolique exacte guidée par toutes les sorties de l'oracle**, pas un apprentissage neuronal par gradient et pas une découverte libre de la décomposition arithmétique.

## Algorithme

1. Construire les deux fonctions `f_add`/`f_mul`, chacune avec **65 536** sorties 8 bits, à partir du décodeur et de l'arrondi `i128` du programme SYNTH0.
2. Entrelacer les deux mots d'entrée pour fixer une seule permutation de variables `a0,b0,a1,b1,…a7,b7`. **Aucune sélection de l'ordre des variables sur les résultats**.
3. Synthétiser les 8 fonctions booléennes de sortie par décomposition de Shannon, nœuds terminaux `0` et `1`, réduction `if low==high`, *hash-consing* global `(bit,low,high)` pour mutualiser les sous-circuits entre les 8 sorties. Budget global dur de **16 000** nœuds non terminaux, profondeur maximale de **16** décisions.
4. Générer du code Rust **sans tableau de consultation** et sans branche conditionnelle par nœud : chaque décision utilise `lo XOR ((lo XOR hi) AND input_bit)`. Il ne dépend que de `u8` et d'instructions bit à bit ; le compilateur est libre de fusionner les expressions en instructions natives plus efficaces.
5. Compiler et exécuter sur CPU hébergé, conserver les assembleurs ADD/MUL, vérifier **les 65 536 cas de chaque opération** contre le même oracle. La vérification séparée de conformité à une implémentation posit8 indépendante reste indispensable.

## Critères bloquants

- Aucun état caché ou table lookup dans le chemin d'inférence émis : seulement variables binaires, XOR/AND, décalages et OR.
- Toutes les entrées doivent réussir ; **une seule paire divergente bloque la qualification**.
- Émettre les mesures **réelles** de nombre de nœuds, taille du source Rust généré, nombre de portes MUX et profondeur maximale, plutôt que les présumer.
- Sauvegarder le code machine assemblé, tous les tests et l'identité SHA du commit.
- Ne jamais interpréter la synthèse par vérité exhaustive comme une validation de généralisation hors du domaine 8 bits, de faible complexité, de conformité standard, de vitesse ou d'efficacité énergétique.
- Ne pas copier le code nCPU, dont la licence annoncée dans README (MIT) contredit le fichier LICENSE (CC BY-NC 4.0).

## Prochain verrou scientifique

**SYNTH2 :** découvrir des sous-structures arithmétiques *composables* (régime, normalisation, retenue, arrondi) sans utiliser 100 % de la table comme exemples d'entraînement. Partition train/validation réellement disjointe, recherche d'un bon ordre de variables gelée sur DEV, équivalence exhaustive finale sur 65 536 couples. Comparer aux circuits écrits manuellement, au nombre de portes physiques estimé, à la taille binaire et au **temps réellement mesuré** face à un noyau posit8 conventionnel natif. Les nouveaux résultats ne seront promus dans SML-GENIUS ou V888-BOOLEEN qu'après leur propre validation indépendante.

Références : Robert C. Price, [nCPU, mars 2026](https://github.com/robertcprice/nCPU/blob/main/paper/ncpu_paper.md) (inspiration seulement) ; [Posit Standard 2022](https://posithub.org/docs/posit_standard-2.pdf). Mesures CPU physiques via Memorithm/RemoteOps uniquement après admission isolée.
