# GB-3 — géométrie rationnelle imbriquée, 6 variables booléennes et ANF exactes

**Préenregistrement : 22 octobre 2026, avant toute mesure GB-3.**
Protocole de **correction sémantique** uniquement. Base SML-GENIUS : GB-2 PR #664 intégrée dans `main` au commit `97805aac562dc9bd7dc1f3d26ba9de3cbc1d4645`.
Aucune modification de l'inférence ni de l'apprentissage productif n'est autorisée dans ce lot.

## Domaine, définition, représentation

La partition rationnelle GB-2 définit M=2^(3+level) cellules angulaires **non équiprobables**, niveaux 0..3, M=8,16,32,64. Le numéro de secteur, attribué par `RationalFan::classify`, sert d'adresse binaire `s in [0,M)` dans une LUT de M bits.

Chaque union de cellules est une fonction `f:{0,1}^d -> {0,1}` avec `d=log2(M)=3..6` ; le bit numéro `s` du masque `FanMask` vaut `f(s)`. Son ANF est :

`f(x) = XOR_{m ⊆ x} a_m`

avec `a_m = XOR_{u ⊆ m} f(u)`, sur GF(2). `a_0` correspond au terme constant et le bit `m` du mot u64 indique le monôme associé aux variables dont les bits sont à 1 dans `m`. Les bits de poids faible des identifiants sont les variables de plus petit rang.

Représentation : `u64` pour la LUT et pour les coefficients ANF, largeur typée dans {8,16,32,64}. Extraction par Möbius en `d` étapes de XOR. La transformée appliquée deux fois est l'identité ; une reconstruction explicite de la LUT est disponible.

## Trois chemins de contrôle indépendants

1. **LUT** : sélection directe `FanMask.contains_point(fan,point)` après classification logarithmique des rayons.
2. **Oraculaire linéaire** : `RationalFan.classify_scan(point)` suivi du même bit LUT (mais angle déterminé par intersections exactes).
3. **ANF** : `FanAnf.evaluate_point(fan,point)`, classification géométrique puis évaluation indépendante de tous les monômes actifs, avec `(m & s) == m`.

Ces trois chemins ne permettent pas de conclure à un gain physique ou modèle. L'évaluation ANF partage volontairement le classificateur géométrique avec le chemin 1 ; le chemin 2 sert à contrôler la classification indépendante. Ce banc n'établit pas une indépendance causale totale entre les chemins.

## Corpus préfixé

- M=8 : les **256** masques possibles et les 8 adresses, comparés aussi avec GB-1 `SectorMask::anf_coefficients`.
- M=16,32,64 : vide, plein, alternance (poids faible), blocs, terme `x0`, terme `x1`, un singleton à chacun des M indices, et masques déterministes `π` à offsets 0 et M si la source est disponible.
- États géométriques : grille entière [-8,8]^2 sans origine, les rayons de chaque niveau, les sommes des rayons voisins, coordonnées signées extrêmes. Tests de frontières et dimensions invalides.
- Conservation : soulever de 8 à 64 bits au moyen de `FanMask.refine_once` trois fois ; la LUT et l'ANF du masque raffiné doivent produire la même appartenance pour tout point apparié.
- Échec explicite pour secteurs hors bornes, fan de mauvaise largeur et origine non orientable.
- Tests de métrologie : compter `popcount(coefficients)` et vérifier les témoins constants et littéraux exactement ; ce compte n'est ni une latence ni une estimation d'énergie.

## Décisions

Le **seul résultat qualifiant** ce lot est une parité exacte, zéro mismatch de la LUT reconstruite et absence de panique sur tous les cas préenregistrés. Une comparaison de vitesse CPU (dichotomie vs balayage vs ANF), GPU portable, compression effective des fonctions ou amélioration de la sélection d'experts SML sera préenregistrée et mesurée séparément sur appareil réel. Un test de correction réussi ne qualifie pas automatiquement l'ANF comme plus économique qu'une lecture de bit LUT (souvent moins coûteuse).

Rust 1.89 sans dépendance supplémentaire ; `cargo fmt --all -- --check`, `cargo check --locked --workspace --all-targets`, `cargo clippy --locked --workspace --all-targets -- -D warnings` et `cargo test --locked --workspace` verts sur le SHA PR exact. Les modifications restent opt-in dans `sml-core`, sans bibliothèque externe obligatoire, pilotes GPU uniquement si des benchmarks matériels sont ultérieurement autorisés.
