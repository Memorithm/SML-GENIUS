# GB-5D — protocole de métrologie indépendante de la composition et du bit-slicing

**Gel : 23 octobre 2026, avant la première exécution du banc GB-5D.** Ce banc est dépendant de GB-5A/B/C (PR #668), et reste expérimental. Les résultats GB-4 x86_64 de l'utilisateur ne sont **pas** des mesures GB-5D.

## Matrice de référence

Domaines 8/16/32/64 cellules ; familles `sparse` (bit 0 sélectionné), `alternating` (0xAA...), `pi` (deux masques distincts à offsets 0 et width). La LUT GB-2, le circuit GB-5A et ses substituts sont calculés sur les mêmes mots, indépendamment de la géométrie, car cette expérience mesure le **calcul une fois l'adresse connue**.

### A — coût de composer deux fonctions

`lut_and` : `a & b` comme mot de vérité ; `anf_and` : produit quotient-ring de coefficients déjà préparés. Mesure la construction d'un circuit composé et non l'évaluation de celui-ci. L'ANF a un coût d'encodage `FanMask → FanAnf` exclu de ces deux comparaisons ; toute analyse précise devra l'inclure comme coût d'installation séparé.

### B — coût de transformer les variables

`lut_restrict` : reconstruire une LUT en remplaçant x0 par 1 ; `anf_restrict` : substitution x0=1 dans les monômes. `lut_reverse` : reconstruire les adresses avec variables renversées ; `anf_reverse` : permuter les supports monomiaux. Toutes les fonctions sont comparées **sur toutes les adresses** avant les mesures. Pas de réduction de largeur implicite.

### C — coût de 64 requêtes connues

64 secteurs gelés `[i % width for i in 0..64]`. `scalar_lut` : 64 lectures de bit ; `scalar_anf` : 64 évaluations monomiales ; `bitslice_prepacked` : 1 mot de 64 résultats à partir de bitplanes prétransposés ; `bitslice_including_pack` : conversion vers bitplanes + calcul. La transposition des adresses est hors coût dans l'arm `prepacked` mais incluse dans `including_pack`, ce qui empêche de présenter un résultat de prépacking comme coût de service complet.

## Protocole de timing

- 3 répétitions de préchauffage puis 9 répétitions avec `std::time::Instant`; mêmes nombres d'itérations pour les méthodes d'une famille et d'une expérience. A et B : 1024 transformations par répétition. C : 256 batchs de 64 adresses (=16 384 requêtes) par répétition.
- Même entrée gelée et mêmes itérations pour chaque méthode ; permutations d'ordre déterministes par répétition afin de réduire le biais d'ordre.
- Préparations de masques et coefficients, allocations et construction du corpus hors boucles chronométrées. `std::hint::black_box` sur les opérandes et les résultats pour éviter des optimisations triviales. 9 durées brutes par combinaison largeur×famille×méthode, pas uniquement la meilleure.
- Le retour d'une méthode A/B est un mot `u64`/coefficients ; la vérification de parité est **effectuée préalablement**, et une accumulation sur les itérations est contrôlée pendant la mesure. L'égalité des checksums ne remplace jamais l'oracle.
- Publication JSON/CSV des résultats, machine physique, SHA SML exact, rustc 1.89.0, variables d'environnement, modèle CPU, version OS, CPU affinity si disponible et hashes fichiers. Les temps ci-dessous ne seront mesurés qu'au moment d'une exécution réelle.
- Un gain en débit de 64 requêtes ne prouve pas un gain sur une requête scalaire. Le groupe C ne doit jamais être mélangé aux métriques de GB-4 dont le classificateur géométrique fait partie du temps. Une amélioration de microbench est seulement une condition préalable à une expérience modèle.

## Gates

`cargo fmt --all -- --check`, `cargo check --locked --workspace --all-targets`, `cargo clippy --locked --workspace --all-targets -- -D warnings`, `cargo test --locked --workspace` sur dernier SHA. Lancement sur Dell/Thor seulement en mode CPU natif, dans le périmètre RemoteOps avec admission des runners ; résultats isolés par machine. Pas de GPU, pas de comparaison absolue cross-architecture, pas de déploiement productif.
