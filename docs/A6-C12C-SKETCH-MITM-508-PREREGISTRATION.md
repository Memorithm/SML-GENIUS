# A6-C12c — Préenregistrement : rencontre au milieu par esquisses et vérification ANF

**Gel du protocole : 12 octobre 2026, avant toute implémentation ou mesure A6-C12c.**
Dépendance : PR #493 (A6-C12b, fusionnée sur main : `999d6963e9159e035c2712f0ab9d3c0e15f18d02`). Toute modification postérieure est un amendement daté ; les premiers logs et les échecs restent publiés.

## Question falsifiable

Peut-on retrouver le triplet de sources d'une parité récurrente bruitée sans calculer la corrélation complète de chacun des ~21,3 millions de triplets admissibles de 508 caractéristiques ? Tester l'hypothèse d'un **criblage probabiliste à rencontre au milieu** (« sketch-MITM ») suivi du **même** apprentissage des 520 fonctions ANF composées et de la même validation autonome que C12b. Comparer au classement W2, à l'union W2+MITM, et à une référence W3 exhaustive, sur les mêmes flux synthétiques.

**Ce n'est pas un algorithme sous-cubique garanti** : la construction de hachage filtre les triplets en espérance sur des données non dégénérées ; les collisions adversariales peuvent ramener le coût au cubique. Ne pas qualifier le résultat de preuve O(n²) tant que les bornes de complexité théoriques et adversariales ne sont pas démontrées.

## Données identiques à A6-C12b, sans fuite

Même anneau N128, root XOR et lecture fixées, trois mutations internes j20/x2, j68/x3, j116/x4, mais **jamais fournies au learner**. Quatre familles exactes [XOR,XOR,XOR], [AND,XOR,XOR], [XOR,AND,XOR], [AND,AND,AND] (tables ANF 6 ou 8). Même PRNG SplitMix64 et formule de graines que C12b pour TRAIN, VAL, TEST, avec séparations stages `0xa6c1_2b11/22/33`, bruit indépendant de labels 1/16 calculé par 4 AND de mots `u64`. TRAIN=512 pas ×64 lanes, VAL=512, TEST=4096 ; seules les 384 observations t≥128 servent au criblage de la récurrence teacher-forcée. Les politiques sont gelées **avant la génération des jeux TEST**.

Grilles candidates fixées : n32 (8 positions `4+16i`, x1..x4), n128 (31 positions `4+4i` plus j127, x1..x4), n508 (les **127 positions internes** j=1..127, x1..x4). Chaque vraie source est présente dans tous les espaces. Les positions véritables ne sont pas des données fournies à l'algorithme. Attention : la présence dans l'espace candidat n'est PAS la même chose qu'une connaissance des positions des trois portes.

**24 flux indépendants au niveau tâche/panneau/graine** : 4 tâches × 2 panneaux (propre, noise1/16) × 3 graines. Les mêmes flux sont utilisés aux trois largeurs de grille et pour les quatre méthodes : résultats appariés, non indépendants entre configurations.

## Méthodes préfixées (aucune utilisation de sources vraies)

**W2-PAIR** : classement des paires arbitraires `corr(p,x_i XOR x_j)` avec positions de portes distinctes, retenir top8 paires, étendre chaque paire par chacune des autres sources à position distincte, dédupliquer, puis factorisation conditionnelle de 520 fonctions composées et réévaluation autonome top16/validation. Correspond exactement à l'esprit du contrôle A6-C12b (nouveau banc identique à grille n32/n128).

**SKETCH-MITM** : cible `p=z XOR R_past` et colonnes binaires des sources retardées. Pour chaque tour r parmi **48 tours déterministes**, prendre **14 positions de bits distinctes à l'intérieur du tour**, échantillonnées uniformément avec SplitMix64 de graine de sélection `0xA6C1_2C5C_0123_4567` (indépendante des familles et des labels). Hash sur 14 bits de chaque colonne, et de p. Pour chaque paire de sources (i,j) de portes distinctes, rechercher dans les seaux les `k` vérifiant `hash(x_k)=hash(p) XOR hash(x_i) XOR hash(x_j)`. Ne retenir que les triplets à **trois positions distinctes**. Dédupliquer entre tours et permutations. **Ne pas calculer une corrélation Walsh complète pour tous les n-choose-3 triplets.**

Attribuer à chaque triplet haché un score de corrélation parité sur **les 8 premiers mots u64** des 384 observations (512 bits-lanes), puis conserver les **256 meilleurs** (tri déterministe score absolu décroissant, égalités sources lexicographiques). Réévaluer la corrélation sur toutes les 384 observations pour ces 256 seuls, garder **les 16 meilleurs triplets** et transmettre ces 16 structures au **factoriseur ANF commun** de C12b. Coûts : mesurer nombre de paires parcourues, recherches de seaux, candidats de collisions (avec et sans doublons), hypothèses rescorées 8 et 384 mots, histogrammes ANF, temps et mémoire logique (RSS non mesuré).

**HYBRID-W2+MITM** : union déterministe des candidats W2 et des 16 structures finales MITM, factorisation unique des 520 masques pour toutes les structures dans l'union (limite actuelle de 8192), vérification native top16, sélection VALIDATION. Le but est de résoudre à la fois XOR³ et les compositions AND/XOR. Il n'est pas supposé que l'union améliore la vitesse : publier temps de W2+MITM ET coût du facteur.

**W3-EXHAUSTIVE-STREAMING** : référence non probabiliste de corrélation d'ordre 3 `|corr(p,x_i XOR x_j XOR x_k)|` sur **tous les triplets admissibles** ; ne conserver en mémoire que les 16 meilleurs (même ordre déterministe que C12b) puis factorisation commune, top16 native. Contrairement au tri intégral utilisé dans C12b, c'est une référence exhaustive en calcul des corrélations mais **optimisée en mémoire par top16 en flux** ; ne pas comparer naïvement ses millisecondes aux anciens tris complets. Aucun paramètre vrai dans le calcul du score.

## Plan expérimental et coût

Toutes méthodes W2, MITM, HYBRID pour n32, n128, n508, tous 24 flux : **216 ajustements**. W3 aux n32/n128 sur 24 flux chacun (**48 ajustements**), et W3 n508 sur **la graine #0 uniquement de chaque tâche/panneau** (8 ajustements ; borne mémoire et temps). **272 politiques gelées**, **24 jeux TEST**, puis **272 lignes TEST**. Les 8 comparateurs W3 n508 sont appariés aux 8 méthodes hybrides de la même graine, mais **pas** aux 16 autres ; signaler ce déséquilibre.

Valeurs exactes de la combinatoire : n32 = C(8,3)×4³ = 3584 structures admissibles, n128 = C(32,3)×4³ = 317440 ; n508 = C(127,3)×4³ = 21 336 000. Éviter toute extrapolation à k≥4 ou aux banques de millions de fonctions. Sur un runner x86_64 GitHub-hosted Rust 1.89 RELEASE, mesurer temps d'écran, factorisation, coût total, propositions, erreurs TEST et récupération paramétrique.

## Critères de succès gelés AVANT tests

- **Principal parité bruitée à n508** : présence de la vraie structure parmi les 16 candidats MITM (rétrospective), récupération exacte des trois positions, canaux et tables par HYBRID, et TEST autonome identique bit à bit à l'oracle **pour au moins 2 graines sur 3**. Publier systématiquement 3/3, 2/3, 1/3 ou 0/3, sans requalifier après observation.
- **Principal mixte à n508** : AND→XOR→XOR, récupération paramétrique exacte par HYBRID pour **au moins 2/3 par panneau** ; vérifier le rang de la paire b,c parmi top8 W2.
- **Non-régression** : XOR→AND→XOR et AND³ atteignent l'erreur oracle dans HYBRID sur **au moins 2/3 par panneau** à n508 ; toute dégradation doit être publiée.
- **Comparateur W3** : publier séparément toutes les erreurs et temps aux n32/128 sur 3 graines, et n508 sur 1 graine uniquement. Mesurer *ratio des triplets entièrement scorés* : le filtre probabiliste ne peut revendiquer un gain que si son nombre d'évaluations complètes est strictement inférieur aux 21 336 000 tests W3 et qu'il réussit le critère primaire. Les collisions de hachage ne sont pas des corrélations complètes : les compter séparément.
- **Ablation scientifique** : rapporter chacune des méthodes W2, MITM, HYBRID, W3. Aucune affirmation « l'hybride surpasse W3 en vitesse » sans comparer coûts totaux mesurés sur les **mêmes graines**, en incluant histogrammes, factorisation, validation.
- **Échec négatif** : si aucun triplet MITM ne survit, la factorisation est explicitement impossible pour cette méthode ; pas de fallback caché. La validation ne sélectionne pas à l'aide de TEST.
- Les masques AND sur les données mixtes ne sont pas des parités pures : **MITM seul peut échouer** ; mesurer sans le réétiqueter « hors capacité ».
- Aucune prétention de résolution LPN générale : étiquettes courantes ET sortie passée bruitées, donc `eta_proxy=15/128`, `rho_proxy=49/64` pour une parité correcte, non 7/8.

## Validation et protection

Tests Rust : exactitude des hashes et collisions en mode sans bruit ; rejets des mêmes positions ; reproductibilité de la sélection des bits ; vérification de l'oracle natif ; cardinalité du plan et traitement `None` sans succès inventé. Pinned Rust 1.89, workflow isolated ubuntu-24.04 read-only, checkout SHA exact et sans credentials persistants, pas de self-hosted Memorithm/RemoteOps, pas de CUDA/vendor GPU. Contrôle full fmt/check/Clippy/tests + mesure n508, publications des échecs. **Rester draft/unmerged jusqu'aux résultats chiffrés et CI verte.**

**Hors périmètre** : graphes libres, largeurs >128 pour l'état, attention GPU, 25B tokens/s, qualité linguistique et sélection réelle de millions de fonctions ANF ; ce sont des hypothèses à tester plus tard.
