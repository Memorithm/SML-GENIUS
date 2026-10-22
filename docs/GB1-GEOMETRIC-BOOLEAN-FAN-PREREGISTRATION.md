# GB-1 — faisceau géométrique exact → prédicats booléens → ANF

**Préenregistrement : 22 octobre 2026, avant toute mesure GB-1.**
Portée : SML-GENIUS, référence scalaire Rust `sml-core`, isolée des poids de production et du routage productif.

## Énoncé corrigé et frontières

Pour N droites **distinctes, concourantes en un point**, le complémentaire de leur union dans le plan comporte 2N secteurs ouverts. Dans le cas de N droites régulièrement espacées de π/N, chaque secteur occupe π/N radians. « Théorème des phares » n'est pas utilisé ici comme intitulé de théorème académique vérifié. Les 2N secteurs ne sont pas N « bandes ». Le point d'intersection commun n'a pas de direction et doit être rejeté.

Une partition finie d'un espace directionnel en M=2N secteurs donne l'algèbre booléenne des 2^M unions de cellules (ET=intersection, OU=union, NON=complément *dans l'espace directionnel*). Les frontières de secteur sont attribuées par convention demi-ouverte afin de rendre la fonction totale hors origine. La puissance booléenne de cette représentation ne prouve ni supériorité expressive sur une LUT M-bit ni accélération par rapport aux réalisations classiques.

Quand N augmente dans un schéma de raffinements cohérents de partitions, les indicatrices de cellules angulaires ne deviennent pas automatiquement des deltas de Dirac. Les unions finies d'arcs et leurs raffinements peuvent engendrer une tribu borélienne sous fermeture dénombrable (hypothèses sur le système de partitions requises) ; elles ne deviennent pas « de la logique floue » par seule application d'une mesure. La probabilité angulaire ne décrit une distribution de données que lorsque les orientations sont distribuées uniformément.

## GB-1 : objet exact et codage gelé

- N=4 droites : axes x=0, y=0 et diagonales y=x, y=-x. Huit secteurs numérotés en sens trigonométrique, débutant à l'axe des abscisses positif ; chaque secteur comprend son rayon d'ouverture et exclut le rayon suivant.
- Type des coordonnées d'entrée : `i64`, calculs de comparaison sur `i128` afin d'éviter les inversions de signe sur `i64::MIN`. Origine explicitement non admissible. Toutes les autres coordonnées possèdent un code secteur unique de 0 à 7.
- Prédicat sectoriel : masque `u8`, bit i sélectionne le secteur i. Algèbre ET/OU/NON/XOR sur masques ; appartenance d'un point évaluée sur le secteur qui le contient.
- Angle uniformément distribué, **uniquement dans cette géométrie à huit secteurs égaux** : mesure de l'union = `popcount(mask)/8` du tour complet. Pas une probabilité empirique sur un jeu de données.
- ANF : décomposer la fonction de vérité `f(a,b,c)` où `a,b,c` sont les trois bits de l'indice de secteur, par transformée zêta/Möbius sur GF(2). Contrôler exhaustivement `eval_ANF == eval_LUT` pour les 256 masques × 8 positions.
- Pont π (optionnel, recherche seulement) : huit bits consécutifs de `sml_core::pi_boolean::PiBitStream`, bit lu en premier → bit du secteur zéro. Aucun PRNG, recyclage de bits ou mutation du réseau Kauffman. Les mêmes prédicats peuvent préparer l'état initial d'un réseau existant, sans intervenir dans son algorithme interne.

## Oracles et tests gelés

- Points sur les huit rayons entiers cardinaux/diagonaux → indices 0..7 exacts ; points strictement intérieurs → mêmes indices ; origine refusée ; limites `i64::MIN` traitées sans débordement.
- Propriétés d'algèbre booléenne complètes sur les 256 masques au moins (identité, complément, union/intersection, XOR) ; tests d'indicatrices à rayon ouvert et fermeture du coin opposé.
- Contrôle causal : sélectionner secteur 0, une direction à l'intérieur doit être acceptée et une direction du secteur 1 rejetée ; intersection des demi-plans `x >= 0`, `y >= 0`, `x-y > 0` contrôlée sur un point strictement intérieur pour éviter les ambiguïtés de frontière.
- Exhaustivité ANF : 256×8, inversion de la transformée et oracles construits sans dépendance à l'implémentation de sélection.
- Flux π : premiers huit bits `00100100`, sélection des secteurs 2 et 5 ; dépassement du préfixe erreur.
- Liaison Kauffman : production du vecteur booléen d'état à partir de points ; simulation d'un pas et répétition exactement égale, sans prétendre à une qualité de modèle.

## Suites de recherche à préenregistrer séparément

1. Raffinements N=8,16,... avec orientations représentées par un contrat stable, erreurs d'approximation des frontières et oracle géométrique indépendant.
2. Projections bit-slicées, SIMD et WGPU : mesures en matériel réel uniquement et parité complète contre la référence scalaire. Pas d'obligation de dépendance NVIDIA autre que pilote.
3. Benchmark sur données séquentielles : routeur géométrique témoin vs routeur ANF vs baselines routage classique, budgets appariés, split TRAIN/VAL/TEST et mesure de couverture, calibration, coût et qualité.
4. **Ne pas revendiquer LSH** à partir des droites déterministes : le théorème de hachage angulaire de Charikar (2002) s'appuie sur un tirage aléatoire d'hyperplans et ne s'applique pas directement à GB-1.

## Condition de promotion

Formatage, Clippy strict et tests workspace verts sur SHA de PR exact. Mesures modèle/hardware absentes = aucun gain revendiqué. Fusion du module opt-in autorisée uniquement si CI requise verte ; aucun changement automatique de sémantique ANF/SML ou de l'exécution d'inférence.

Sources : M. S. Charikar, *Similarity Estimation Techniques from Rounding Algorithms*, STOC 2002, DOI:10.1145/509907.509965 ; documentation interne `docs/SML-PASCAL-PVP-HARDWARE-ALIGNMENT-BOOTSTRAP.md` ; `docs/PI-B1-KAUFFMAN-BNN-PREREGISTRATION.md`.
