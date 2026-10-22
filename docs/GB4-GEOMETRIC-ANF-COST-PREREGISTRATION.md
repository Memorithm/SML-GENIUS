# GB-4 — préenregistrement des coûts : routage géométrique vs lecture LUT vs ANF

**Date du gel : 22 octobre 2026, avant tout benchmark de GB-4.**
Dépôt d'autorité : Memorithm/SML-GENIUS. Bases qualifiées : GB-1 PR #663 et GB-2 PR #664. GB-3 PR #665 est une dépendance scientifique (ANF sur 3–6 bits) **non présumée fusionnée** dans ce document.

## Question et contraintes

Sur le même corpus de directions et les mêmes prédicats de secteurs, combien de travail et de temps faut-il pour appliquer :

A. **SCAN+LUT** : `RationalFan::classify_scan(point)`, puis lecture du bit `FanMask`.
B. **BINARY+LUT** : `RationalFan::classify(point)`, puis lecture du bit `FanMask`.
C. **BINARY+ANF** : exactement le même `RationalFan::classify(point)`, puis `FanAnf::evaluate_sector` de GB-3.

Ce protocole ne prétend pas que l'ANF possède un avantage de latence ; une lecture directe de bit peut être nettement moins coûteuse. Les chemins B et C partagent intentionnellement la géométrie. Le protocole sépare **correction sémantique**, **coût de classification**, **coût d'évaluation** et **coût complet**, et ne déduit jamais une accélération GPU d'une mesure CPU.

## Géométries, fonctions, corpus (gel)

- Quatre niveaux GB-2 `level=0,1,2,3`, soit 8,16,32,64 secteurs. Leurs largeurs angulaires sont inégales après le niveau zéro.
- Trois familles de fonction par niveau : `sparse` (un bit initial sélectionné), `alternating` (indices impairs), `pi` (les M premiers bits fractionnaires hexadécimaux de π, offset 0). Aucun apprentissage ni changement de fonction pendant la mesure.
- Ensemble de directions : grille [-16,16]^2 sans origine ; tous les rayons et sommes des deux rayons voisins ; 4096 directions de type `i64`, issues d'un SplitMix64 **spécifié**, graine `0x6b4d_2026_1022_0001`, chaque coordonnée représentée par les 32 bits bas du mot tiré convertis en entier signé `i32`, et origine rejetée/remplacée de manière explicite. Aucun véritable aléa n'est invoqué.
- L'ordre du corpus reste fixe entre chemins A/B/C. Trois valeurs d'échantillons : préfixes de longueur 1024,4096 et corpus total. S'il existe moins de 4096 points, erreur, pas de remplissage silencieux.
- Conditions limites à maintenir en contrôle de parité, **mais non incluses dans la mesure** : origine (erreur), `i64::MIN/MAX`, rayons frontières, mauvais masque, offset de π en dépassement.

## Algorithmes et comptabilité

- A : `classify_scan`, puis `((mask.bits() >> sector)&1) != 0`.
- B : `mask.contains_point(&fan, point)`, même table sans transformation.
- C : `FanAnf::from_mask(mask)` **hors chronométrage** (coût de préparation reporté séparément), `anf.evaluate_point(&fan, point)`.
- Checksum observable et identique par méthode : accumulation du bit prédicat en `u64`, en utilisant `std::hint::black_box` contre élimination à la compilation. Aucun checksum égal ne remplace une comparaison **point par point** effectuée avant la première mesure.
- Comparaison d'index : `classify_scan == classify` pour chaque point.
- Les coefficients ANF proviennent du GB-3 qualifié : aucune implémentation concurrente inventée pour « améliorer » C.

## Mesures et décisions

1. Deux modes : `deterministic` (parité et empreintes, reproductible bit à bit) et `benchmark` (temps). Pas de promotion de modèle.
2. Profil physique demandé : CPU Dell T430 (x86_64), Thor (aarch64) si admis par RemoteOps ; même SHA, Rust 1.89, mêmes flags Release, date, modèle CPU et environnement d'exécution retenus. Les résultats CI sur VM éphémère sont des diagnostics, **pas des preuves de performance native sur Thor/Dell**.
3. Warmup : 3 parcours complets par cellule ; 9 échantillons successifs chronométrés par chemin/cellule. Ordre permuté **de façon déterministe** entre répétitions pour limiter, sans éliminer, les biais de chauffe et d'ordre. Publier les 9 valeurs brutes, médiane, dispersion (min/max) ainsi que la vitesse en points/s. Ne pas agréger des machines différentes.
4. Mentionner le nombre de requêtes exécutées et la largeur du masque. Le chronométrage inclut le routage et la lecture mais exclut la création des rayons, les allocations de corpus, la compilation ANF et l'initialisation de π.
5. Aucun seuil de speedup fixé ; décider après lecture des mesures. Un gain de temps sur un micro-benchmark n'autorise **ni** un modèle plus rapide **ni** une adoption dans SML-25B. Les résultats négatifs sont conservés.

## Portée et gates

Aucun changement de pipeline SML productif, pas de bibliothèque ajoutée, pas de dépendance CUDA/TensorRT/constructeur pour les traitements CPU. Exécution GPU séparée et future, éventuellement WGPU après qualification. Exiger une suite oracle indépendante et CI `fmt`, `check`, `clippy`, `test` verte **sur SHA final** avant fusion de GB-4. Seule l'implémentation du banc scientifique optionnel est éligible à la fusion.

Références internes : `docs/GB1-GEOMETRIC-BOOLEAN-FAN-PREREGISTRATION.md`, `docs/GB2-NESTED-RATIONAL-FAN-PREREGISTRATION.md`, `docs/GB3-FAN-ANF-SIX-VARIABLES-PREREGISTRATION.md`.
