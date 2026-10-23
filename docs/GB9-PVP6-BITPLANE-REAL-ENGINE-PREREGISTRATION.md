# GB-9 — témoin PVP6 réel contre évaluation par bitplanes

**Préenregistrement : 23 octobre 2026, avant toute mesure GB-9.**
Question restée ouverte après GB-6 : la parité mathématique et l'amortissement CPU du banc synthétique peuvent-ils être retrouvés lorsqu'on utilise les **vraies structures de banque PVP6** déjà présentes dans SML-GENIUS ? C'est une étape de correction, pas de performance modèle.

## Source et portée

GB-6 `sml-gb6-reused-bitplanes` (SHA `8f367ff163db98a47695a9025e35073f1276af8a`) a montré un gain conditionnel à R experts distincts, sur un banc synthétique (RemoteOps #39977860338, Dell/Thor). `Pvp6Bank<64>`, `Pvp6Gate<64>` et `Pvp6Workload` constituent la vraie interface actuelle du moteur PVP6 avec ses trois architectures (Pascal, Sparse ANF, Submask). Le nouveau contrôle ne modifie ni le sélecteur d'architecture ni `Pvp6Bank::evaluate`.

## Corpus et chemins figés

- Domaine 64 adresses, 64 lanes exactement ; R∈{1,4,8,16,32} portes, chacune à quatre monômes distincts `[g+1,g+2,g+3,g+4]`, avec `g∈[0,R)`. Les portes sont structurées et déterministes, pas des poids entraînés.
- 2 régimes d'adresses : `cyclic` `i mod 64` et `clustered` `floor(i/4)`, `i∈[0,64)`. Réutilisation native `reuse∈{1,2,4}` sans mutation du batch.
- Trois profils de population : `AffineUniform`, `LowPopcount`, `HighPopcount`; nombre total de cellules `5×2×3×3 = 90`.
- **Référence indépendante** : pour chaque porte `g` et adresse `x`, XOR direct de la proposition `(monomial & x)==monomial` pour les quatre monômes. Reconstruire à partir de ces résultats sa LUT de 64 bits, puis convertir par Möbius GB-3/5 afin de vérifier que les coefficients concordent **exactement** avec les bits stockés dans `Pvp6Gate::coefficients`.
- **Chemin existant** : `Pvp6Bank<64>::evaluate(Pvp6Workload{density=4,query_count=64,reuse,profile}, &queries)`, avec les représentations de sortie gate-major et query-major.
- **Chemin bitplanes portable** : `pack_sectors(&queries_u8,64)` (64 lanes, 6 bitplanes), puis `AnfCircuit64::evaluate_bitplanes` pour chaque porte.
- Chaque sortie des trois chemins est comparée bit à bit, pour chaque gate, chaque lane et chaque répétition. Toute erreur est bloquante. Vérifier aussi que le selecteur natif reste `diagnostic_only` lorsqu'il l'est déjà, sans lui attribuer une accélération GB-6.
- Aucune vitesse n'est mesurée dans cette première PR. Elle vérifie la **compatibilité avec les données du moteur PVP6 réel**, pas un expert entraîné, MOR, SBG ou un modèle en langage naturel.

## Gates avant extension de performance

CI sur SHA final, cargo fmt/check/clippy -D warnings et tous tests workspace. Ensuite et seulement ensuite : étude GB-9B appariée de `Pvp6Bank::evaluate` vs packed, **en incluant** construction des sorties gate-major et query-major, préparation de la banque, allocation, classification SBG/MOR et instrumentation RSS. Pas de branchement à l'inférence productive sans attribution de gain de qualité, régression, mémoire et coût de routage sur cas représentatifs.

Limites : les 4 monômes fixes assurent une comparaison contrôlée mais ne représentent ni les distributions de monômes d'une banque réellement entraînée ni la structure des tâches linguistiques. Le compromis qualité/vitesse et le remplacement de l'attention restent non démontrés.
