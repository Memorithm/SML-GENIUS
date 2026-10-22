# GB-2 — partitions angulaires imbriquées, arithmétique entière et masques booléens

**Préenregistrement : 22 octobre 2026, avant toute qualification GB-2.**
Autorité scientifique : `Memorithm/SML-GENIUS`. Base : GB-1 fusionné dans `main` par PR #663, commit `92d7b5c29a2846fe40a91ee09b23a3041ce3e015`.
Limite : candidat expérimental Rust pur, sans mutation du runtime d'inférence, de l'entraînement ou des poids.

## Hypothèse gelée

Un faisceau **imbriqué** de directions rationnelles peut représenter une partition angulaire sur 8, 16, 32 ou 64 secteurs par comparaison entière, avec conservation exacte des décisions de ses prédicats lorsqu'on passe au niveau suivant. Le routage par recherche binaire dans les rayons ordonnés doit être exactement équivalent à un oracle linéaire indépendant en termes d'appartenance à une cellule. Ce pilote est un contrôle de correction sémantique et de composabilité, pas une affirmation d'optimisation.

## Construction des quatre géométries

- Niveau 0 : huit rayons orientés (1,0), (1,1), (0,1), (-1,1), (-1,0), (-1,-1), (0,-1), (1,-1).
- De `level` à `level+1` : pour chaque paire consécutive `(u,v)` sur le cercle, conserver `u` puis insérer le rayon `u+v`. Par construction tous les secteurs initiaux ont une ouverture de 45° et les sommes sont des directions intérieures exactes, non nulles. Chaque secteur ancien devient deux secteurs enfants d'indices `2i` et `2i+1`.
- N=4, 8, 16, 32 droites non orientées donnant M=2N=8, 16, 32, 64 secteurs. Les nouvelles divisions **ne sont pas uniformément espacées en angle** : un compte de cellules n'est donc pas une mesure angulaire. Ne pas prétendre qu'un secteur vaut 360°/M au-delà du niveau zéro.
- Frontières : chaque secteur est semi-ouvert, rayon de début inclus et rayon de fin exclu, dans l'ordre anti-horaire à partir de l'axe des abscisses positif. Origine refusée explicitement.
- Entrées : vecteurs `i64` quelconques, incl. `i64::MIN`; produit vectoriel entier en `i128` avec petits rayons constants. Les quatre niveaux sont bornés : pas de somme de rayons arbitraire ou de dépassement.
- Algorithme candidat : ordre angulaire par demi-plan supérieur/inférieur puis signe du produit vectoriel ; partition `partition_point` (recherche binaire).
- Oracle indépendant : balayage exhaustif de tous les wedges `cross(start,p)>=0 && cross(p,end)>0`, jamais de calcul trigonométrique flottant, avec rapport d'échec invariant explicite.

## Algèbre et pont π

- Masques de M bits empaquetés en `u64`, largeur M explicite et validée. Membres appartenant au même niveau : ET/OU/NON/XOR sur les bits utiles seulement, conservation des grands indices.
- Raffinement du masque : `bit(i)` parent → `bit(2i)` et `bit(2i+1)` enfants ; préservation exacte du prédicat sur tout point à direction définie.
- Premier niveau : identité binaire de chaque code et de chaque prédicat avec `geometric_boolean::sector_of` et `SectorMask`.
- Option π : consommer sans rebouclage M bits consécutifs de `pi_boolean::PiBitStream` ; ne pas confondre flux déterministe et aléa indépendant.

## Plan de validation fixé avant code

1. Dimensions exactes : niveaux 0..3 → 8,16,32,64 ; niveau 4 refusé.
2. Contrôle parité : pour tous les points de la grille entière `[-16,16]^2\{(0,0)}`, comparaison recherche binaire vs oracle linéaire, pour chacun des niveaux. Ajouter tous les rayons et leurs sommes adjacentes, les valeurs extrêmes signées et points limites.
3. Raffinement : pour tous les points de la grille, `child_sector / 2 == parent_sector` et sélection inchangée pour les masques levés. Masques de contrôle : vide, plein, premiers/derniers bits, alternance, échantillons issus de π.
4. Premier niveau : même résultat pour toutes ces directions que le module GB-1, sans changement à ce dernier.
5. Opérations de masques : complémentaires bit-justes aux 4 largeurs, refus des bits hors domaine et erreurs de largeur.
6. π : parité de la version N=4 sur les 8 premiers bits `00100100`, erreurs explicites à épuisement.
7. Cargo : `cargo fmt --all -- --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings`, `cargo test --locked --workspace` sur SHA exact.

Ne pas utiliser un résultat de correction comme preuve de gain de latence, d'efficacité mémoire, de performance Jetson, de qualité du modèle ni de remplacement d'attention. La mesure des temps (scan/binaire), la vectorisation/WGPU et un essai modèle aligné devront faire l'objet d'une préinscription distincte avec témoins et budget fixe.

## Reproductibilité et souveraineté

Rust 1.89, zéro nouvelle dépendance externe, aucun accès aux données utilisateurs, aucune dépendance obligatoire à CUDA/TensorRT et aucun recours runtime à SciRust/FLAT/NNIS. Le module est autonome et purement opt-in. Tout échec CI bloque la fusion.
