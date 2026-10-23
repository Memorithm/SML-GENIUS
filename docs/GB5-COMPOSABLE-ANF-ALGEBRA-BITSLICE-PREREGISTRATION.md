# GB-5 — évaluation distincte des circuits ANF, transformations algébriques et bit-slicing

**Statut : préenregistrement avant implémentation et mesures GB-5, 23 octobre 2026.**
Dépôt d'autorité : `Memorithm/SML-GENIUS`. Dépendances qualifiées : GB-1..GB-4, y compris GB-4 PR #667 au commit `c8613098ec6cf3cf59b784f1d425e12f8c710283`. Module expérimental, non branché sur l'inférence ou l'entraînement SML-25B.

## Point de départ (observation, pas extrapolation)

La campagne x86_64 transmise par l'opérateur (source GB-4 exacte, Rust 1.89.0) rapporte, en lecture directe par direction, une ANF scalaire plus lente que le bit LUT sur les 12 configurations `(8,16,32,64) × (sparse,alternating,pi)`. Ce résultat reste propre au chemin scalaire : il ne dit rien, seul, sur le coût de **composer une fois** des circuits, de **transformer leurs coefficients**, ou d'**évaluer jusqu'à 64 directions simultanément**.

Les trois expériences qui suivent sont **indépendantes** : chacune est jugée selon son propre oracle et son propre coût, sans mélanger débit en requêtes, coût de compilation, stockage et modèle final.

## GB-5A — composition algébrique de circuits

Représentation : `AnfCircuit64`, largeur typée `M ∈ {8,16,32,64}`, `n=log2(M)`, mot `u64` des monômes ANF en indices petit-boutistes. Une fonction d'entrée est la LUT sectorielle `FanMask` qualifiée GB-2 et convertie par GB-3.

Opérateurs exacts `GF(2)` : `f XOR g` : XOR des coefficients ; `NOT f` : toggle du coefficient constant `a_0` ; `f AND g` : produit polynomial avec réduction `x_i²=x_i`, donc `h_m = XOR_{i | j = m} (a_i AND b_j)` ; `f OR g = f XOR g XOR (f AND g)`. Tous nécessitent les mêmes variables et rejettent les largeurs différentes. Le nombre de monômes est une métrique structurelle et **pas** un coût d'exécution assimilable à une lecture LUT.

Témoins : `FanMask` avec opérations binaires natives bit à bit, oracle d'évaluation indépendante sur les M adresses. Exhaustif `256²` paires à M=8 et plusieurs familles gélées à M=16/32/64 (vide, plein, alternance, pi, chaque singleton). Les compositions doivent retrouver *exactement* la LUT oracle, y compris constantes, idempotence, De Morgan.

## GB-5B — substitutions et changements de variables

`restrict_var(v,c)` remplace `x_v` par `c∈{0,1}` et retourne un polynôme sur le **même domaine typé** ; les coefficients des monômes contenant `x_v` disparaissent. Oracle : `f(x_v:=c)` évalué par lecture directe dans la LUT originale avec bit d'adresse ajusté.

`permute_variables` remplace chaque variable ancienne `x_i` par `x_{p(i)}` pour une permutation bijective de `[0,n)`. Oracle indépendant : lecture de la LUT d'origine à l'adresse recomposée. Permutation invalidée si longueur incorrecte, doublon ou index hors domaine. Aucune promesse que l'ANF compressée fournisse un gain sur une LUT transformée.

Tests : toutes les 256 fonctions M=8 × toutes les variables × valeurs 0/1 ; familles gelées à 16/32/64, permutations identité et inversion, compositions et identité de reconstruction.

## GB-5C — représentation matérielle `u64` bit-slicée

Protocole CPU **portable**, sans intrinsics ni runtime propriétaire : une entrée batch ≤64 adresses de secteurs est transposée en `n` bitplanes `u64` (un bit par requête), avec `lane_mask` pour éviter de compter les lanes inactives. Chaque monôme est une réduction AND des bitplanes requis ; l'ANF est leur XOR. Ce traitement partage les **mêmes** coefficients de GB-3/GB-5A.

Témoins : évaluation ANF scalaire `FanAnf.evaluate_sector` et lecture bit LUT, **sur les mêmes 0..64 requêtes**. Exhaustif sur les 256 masques à M=8 et corpus gelé à M=16/32/64. Contrôles explicites pour batch vide, 1, 63, 64 et >64, adresse hors domaine, variables inutilisées, `lane_mask` et constance sur 64 lanes.

La preuve de correction est une parité bit à bit ; la preuve d'un gain CPU SIMD, ARM NEON ou GPU WGPU nécessite des mesures natives et un travail distinct. Un `u64` n'implique pas automatiquement une vectorisation SIMD physique. Ne pas déduire un meilleur coût pour l'évaluation d'une direction individuelle du débit en batch.

## Mesure future de coût (banc séparé)

- A : coût de construire `f∧g` en LUT vs en coefficients ANF, mesurer *construction* et *réutilisation* séparément.
- B : coût de `restrict_var`, de `permute_variables` et équivalence contre une LUT recomposée.
- C : coût de `64 × LUT`, `64 × ANF scalaire`, et `u64` bit-slicé **sur les mêmes adresses** ; le coût de transposition des adresses doit être reporté **avec et sans prépackaging**.
- Première étape : oracles et tests Rust exacts. Avant toute revendication de débit : banc Release avec entrées gelées, `std::hint::black_box`, warmup et valeurs brutes sur machines physiques, protocoles de mémoire/cache/frequence documentés. Comparateurs équitables à compiler avec les mêmes flags. Mesures sur Dell x86 et Thor ARM64 non agrégées.
- Aucun seuil de gain fixé a posteriori ; conserver les résultats négatifs, indiquer la largeur, la densité ANF, le nombre de batchs et la provenance du code/outillage.
- Aucun changement de SML-25B, aucune prétention de remplacement de l'attention, aucun raccourci vers l'entraînement, et aucune dépendance obligatoire CUDA/TensorRT.

## Promotion

Exact-head CI verte : `fmt`, `check`, `clippy -D warnings`, tests workspace. Qualifier seulement la sémantique scalaire portable GB-5 ; les avantages d'implémentation matériels n'existent qu'après mesure appariée. Le programme PVP garde le principe d'une référence indépendante, de provenance et d'une souveraineté logicielle.
