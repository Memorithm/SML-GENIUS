# P8-BS256 — bitplanes booléens Rust quatre mots (256 couples)

Programme de recherche Memorithm CPU-only, indépendant, sans activation du modèle.

## Hypothèse et implémentation

Le circuit posit8 à arêtes complémentaires est exécuté sur **256 couples d'opérandes** en traversant une seule fois ses nœuds, chaque signal étant représenté par **quatre mots `u64`**. Seules les opérations entières XOR, AND, NOT, décalages et tests d'indice sont nécessaires. Les 16 plans d'entrée sont reconstruits par lot ; les huit plans de sortie sont décodés vers 256 octets. Les buffers sont réutilisables entre les appels, les lots incomplets sont comblés localement et les longueurs de sortie incohérentes sont rejetées avant toute modification.

Le chemin de calcul est écrit en **Rust sûr**. Son comportement est indépendant de la présence d'AVX2. Le compilateur peut utiliser des instructions SIMD, mais aucune accélération SIMD n'est affirmée sans inspection du code machine et mesure explicite.

## Critères d'exactitude

- Oracle : posit8 ADD/MUL à entiers exacts du dépôt, dont la conformité normative extérieure reste non certifiée.
- Domaine : exactement 65 536 couples pour ADD et 65 536 couples pour MUL, soit **256 groupes de 256** par opération, aucune combinaison écartée.
- Cas supplémentaires : tailles 0, 1, 64, 127, 255, 256, 257, 513 et 1 024 ; répétitions, NaR, valeurs extrêmes et erreurs de dimensionnement.
- Mémoire de travail : `16 × 4 × 8 + (N+1) × 4 × 8` octets pour N nœuds. La mémoire est propre à un graphe, ne contient ni pointeurs externes ni exécutable.
- Compilateur : Rust 1.89 épinglé. Les deux opérations doivent obtenir une égalité bit-à-bit, zéro erreur, sans recours à un produit tiers.

## Comparaison prospective

Le comparateur est **notre noyau bitplane64**, sur le **même graphe** et la même séquence gelée de 65 536 paires pseudo-mélangées. La mesure inclut les transformations entrées/sorties et le temps du noyau. Deux passages d'échauffement, sept répétitions, ordre alterné, médianes ns par couple et checksums bit-à-bit identiques. Une amélioration face au noyau `u64` de référence ne démontrerait pas un avantage face à une ALU native standard, à l'assembleur scalaire qualifié, à un modèle complet ou au Dell T430.

Les mesures du processeur GitHub sont exploratoires et ne constituent pas une preuve de performance industrielle. Aucune charge sur Dell/Thor/Mac et aucune modification de services via RemoteOps ne sont nécessaires à ce protocole.
