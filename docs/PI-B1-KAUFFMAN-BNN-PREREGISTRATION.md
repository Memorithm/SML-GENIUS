# PI-B1 — réseaux booléens de Kauffman et initialisation BNN à partir de π

**Préenregistrement : 22 octobre 2026, antérieur à toute mesure de cette expérience.**
Statut : candidat scientifique non promu. Dépôt d'autorité : Memorithm/SML-GENIUS.
Langage : Rust 1.89, sans dépendance ajoutée, sans API NVIDIA, sans mutation de l'entraînement ni des poids de production.

## Deux hypothèses indépendantes

- **H4 (Kauffman)** : des tables de vérité et des connexions issues de chiffres **décimaux successifs** de π produisent un réseau déterministe dont les perturbations peuvent être mesurées. Cela ne présume pas une dynamique supérieure à celle d'autres générateurs.
- **H5 (BNN)** : des bits de la représentation **hexadécimale fractionnaire** de π, parcourus du bit fort vers le bit faible de chaque chiffre, donnent une suite exacte de poids dans {-1,+1}, avec reproduction bit à bit d'une machine à l'autre sur un préfixe fixé. Cela **ne garantit pas** la reproductibilité d'un entraînement flottant multi-plateforme.

Ces deux codages (décimal H4, hexadécimal H5) sont intentionnellement distincts et correspondent aux formulations respectives des hypothèses. Aucun recyclage cyclique du préfixe n'est autorisé : dépassement = erreur explicite. Les offsets sont exprimés en chiffres décimaux pour H4 et en bits pour H5.

## Source et algorithmes gelés

Pour cette qualification limitée, les préfixes **fractionnaires** de π sont embarqués et versionnés :
- 1 600 chiffres décimaux, premiers chiffres `141592653589793238462643383279`.
- 1 024 chiffres hexadécimaux (4 096 bits), premiers chiffres `243f6a8885a308d313198a2e03707344`.

Les préfixes sont produits avec la formule de Machin, en arithmétique entière et avec 1 700 décimales de précision de travail. Ce protocole n'établit aucune propriété de normalité ou d'entropie : la source est publique et parfaitement prévisible.

H4 : chaque indice d'entrée est tiré à partir de quatre décimales consécutives représentant un nombre de 0 à 9 999, avec rejet de toute valeur au-delà du plus grand multiple de N inférieur ou égal à 10 000, afin d'éviter le biais du modulo. Des indices distincts sont retenus pour chaque nœud ; les candidats rejetés consomment effectivement des décimales. Chaque case de table de vérité lit ensuite une décimale et retient sa parité. Les états sont synchrones, les tables restent figées pendant toute la trajectoire. N est borné à 256, K à min(N, 8), et les demandes excédant le préfixe échouent explicitement.

H5 : chaque bit lu vaut poids -1 si zéro, +1 si un. Pas de flottants, pas d'auto-expansion ou de condensat cryptographique implicitement présenté comme « bits de π ». Le format des poids est `i8` et leur ordre est celui du flux.

## Contrôles, sorties et décision

Le pilote de conformité teste la connaissance des premiers chiffres/hexadécimales, les offsets, l'épuisement sans rebouclage, l'unicité des entrées, les règles synchrones et l'effet causal d'un flip d'entrée sur une petite référence.

La campagne scientifique ultérieure, **non exécutée dans cette PR**, opposera sous budget identique : π, générateur à graine publiée de type SplitMix64/PCG et témoin constant (à présenter séparément, non assimilé à un tirage aléatoire).

- H4 : N = 8, 16, 32 ; K = 1, 2, 3 sous réserve de capacité effective du préfixe ; états initiaux communs appariés, fraction de nœuds actifs, distance de Hamming après un pas, pente empirique des courbes de Derrida, distribution des longueurs de cycles avec plafond explicite de pas.
- H5 : taille = 64, 256, 1 024, 4 096 poids (au plus la capacité du préfixe) ; parité binaire x86_64/aarch64 ; équilibre -1/+1 et différences éventuelles avec une initialisation témoin. Toute comparaison de perte de validation, vitesse de convergence ou précision exige **un entraînement BNN distinct**, mêmes données/architecture/budget, graines appariées et un split test indépendant.

**Critère de ce pilote uniquement :** exactitude des contrats et tests sur SHA exact. Aucun seuil de performance, accélération, amélioration de convergence, découverte de chaos ou de propriété cryptographique n'est préjugé. Tous les échecs et limites de corpus sont publiables ; pas de promotion automatique en SML-25B.

## Gouvernance

Le code reste un module expérimental autonome de `sml-core`, sans dépendances de production sur des bibliothèques externes ni intégration silencieuse à l'entraîneur. CI nécessaire : `cargo fmt --all -- --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings`, `cargo test --locked --workspace`. Fusion uniquement avec ces contrôles verts sur le **head final**. Exécution sur Thor et qualification CUDA/WGPU expressément **hors de la portée**.

Références : Derrida et Pomeau (1986), DOI 10.1209/0295-5075/1/2/001 ; Bailey, Borwein et Plouffe (1997), *Mathematics of Computation* 66, 903–913 ; David H. Bailey (2006), *The BBP Algorithm for Pi*, DOI 10.2172/983322.
