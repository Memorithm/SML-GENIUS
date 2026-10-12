# A6-C12g — Préenregistrement : transition de phase bruit × échantillons

**Gel du protocole : 12 octobre 2026, avant toute implémentation ou mesure A6-C12g.** Dépendances vérifiées sur `main` : A6-C12c PR #494, A6-C12d PR #495, A6-C12e PR #496, A6-C12f PR #497 fusionnées. Aucun réglage après lecture des résultats de cette campagne ; conserver et expliquer les échecs de CI et scientifiques.

## Question et hypothèses

La récupération 24/24 de C12c n'identifie ni la taille d'échantillon nécessaire, ni le niveau de bruit qui fait échouer Walsh-3 ou W2+SKETCH-MITM. Le nouveau banc mesure explicitement **le nombre de réseaux exacts**, la **marge Walsh du vrai triplet contre le meilleur faux**, la récupération bit-à-bit autonome sur TEST, et les coûts de détection/factorisation **séparés**.

- **H1 (témoin favorable)** : à bruit zéro et au moins 128 mots utilisables, W3 et HYBRID récupèrent les trois sources et trois tables sur ≥7/8 graines pour chacune des deux tâches. Le résultat reste mesuré, pas forcé.
- **H2 (transition)** : rechercher, sans fixer le point de rupture a priori, les cellules avec récupérations entre 1/8 et 7/8 lorsque bruit croît et que le nombre de mots diminue. Publier toutes les cellules y compris 0/8 ou 8/8, sans optimisation des méthodes.
- **H3 (discrimination)** : pour chaque cellule, mesurer le coefficient absolu `|1−2 erreurs/N|` du vrai triplet (rétrospectivement seulement) et celui du meilleur triplet faux réellement évalué ; marge signée (vrai−faux), rang du vrai si top16, et rappel des propositions. Une marge négative ne peut être cachée par la réussite du factoriseur.
- **H4 (temps)** : mesurer le criblage et le fit ANF/validation sur trois répétitions identiques par cellule/méthode, publier les médianes et les valeurs brutes dans les traces ; réexécuter sur trois jobs de runners hébergés distincts. Les temps de CI partagés sont descriptifs et ne constituent pas une loi asymptotique.

## Matrice, effectifs et sources

**Architecture strictement inchangée** : anneau natif N128 avec trois portes cachées, root/readout fixes, 5 entrées, 64 voies bit-slicées ; **n=128** caractéristiques source candidates (32 positions×4 canaux), grille C12c contenant les trois sources j20/x2, j68/x3, j116/x4 *par construction*, sans divulgation des sources vraies au learner. Ne pas confondre n128 sources et taille de la banque ANF SML-25B.

Deux tâches représentables : `XOR→XOR→XOR` (truths 6/6/6) et `AND→XOR→XOR` (8/6/6). **Huit graines** indexées 0..7. Cinq taux i.i.d. de corruption d'étiquettes (non pas d'état interne) : **0, 1/32, 1/16, 1/8, 1/5**. Sept quantités d'observations utilisables pour le proxy récurrent t≥128 : **8, 16, 32, 64, 128, 256, 512 mots u64**, soit **512, 1024, 2048, 4096, 8192, 16384, 32768 bits** ; TRAIN total =128+nombre de mots, donc **136..640 pas**. L'ordre des budgets est celui des préfixes imbriqués d'un TRAIN maximal unique de 640 pas, par graine/tâche/bruit. VALIDATION =640 pas, TEST autonome=4096 pas chacun×64 voies, sur stages PRNG disjoints ; seule VALIDATION choisit les modèles, jamais TEST.

Cela fait **2×8×5×7=560 cellules de datasets TRAIN/VAL**. Dans chaque cellule : W3-STREAM exhaustif et HYBRID inchangés (48 tours, hash 14 bits, top256 court/top16 complet ; W2 paires top8 ; factorisation ANF 520 masques), donc **1120 fits**. Gauss exact GF(2) n'est admissible que sur les **112 cellules propres** (2×8×7) : base sur au plus 16 mots de TRAIN et vérification du préfixe complet ; un ajustement non affine/rejet reste un résultat. Les **448 cellules bruitées** ne sont *pas* une tentative de LPN et n'exécutent pas Gauss. Au total **1232 politiques FROZEN**, trois répétitions par politique pour métriques CPU, mais **1232 politiques distinctes seulement**. Évaluation sur **80 jeux TEST bruités/propre appariés** (2 tâches×8 graines×5 taux), après gel de *tous* les modèles, **1232 comparaisons TEST**. Les différents taux partagent les mêmes entrées de base, et les budgets leurs préfixes : ne pas prétendre à 560 observations statistiquement indépendantes.

## Critères observés et sorties obligatoires

Pour chaque politique FROZEN enregistrer tâche, seed, taux bruit, mots utilisables, nombre de bits proxy, méthode, rang top16 du vrai triplet, score vrai et meilleur faux **normalisés par le même N**, marge signée, rang éventuel de la paire b/c, statut `proposed`, position/canaux et vérités exacts, nombre de structures proposées, statistiques de collisions Sketch, temps `screen/factor/total` (3 valeurs et médiane). Les métriques rétrospectives de vérité ne doivent intervenir **ni dans l'appariement Sketch ni dans le classement/factorisation**.

Pour chaque TEST (jamais introduit avant le gel) comparer séparément l'erreur contre labels observés **et** le Hamming de **prédiction native bit-à-bit** modèle proposé/committed versus enseignant déterministe sur les mêmes entrées depuis états initiaux nuls ; mesurer exactement les paramètres récupérés et l'acceptation sur VALIDATION. Une égalité du *nombre* d'erreurs ne suffit pas à conclure à l'équivalence fonctionnelle.

Publier des tableaux réussite/8 par tâche×taux×budget×méthode, un **intervalle Wilson 95 %** sur les 8 graines (et avertissement d'IC exploratoire sous corrélations), le plus petit budget atteignant ≥7/8 empiriquement par cellule, la distribution des marges et les médianes CPU avec séparation screen/factor. Aucun changement d'hyperparamètres entre les cellules.

## Échelle théorique : à tester, pas à proclamer

Le proxy TRAIN utilise la cible courante et la cible d'il y a 128 pas, toutes deux bruitées. Pour le taux d'étiquette `η`, `η_proxy=2η(1−η)`, `ρ_proxy=(1−2η)^2`, non `1−2η`. La corrélation pure du vrai XOR³ attendue est `ρ_proxy`, tandis que `AND→XOR→XOR` possède dans cette géométrie une amplitude spectrale plus faible (environ `ρ_proxy/2` si les variables pertinentes sont uniformes indépendantes). La formule `ln(M)/signal²` avec M=317440 triplets est une **échelle heuristique à constante inconnue**, pas une borne démontrée garantissant le classement lorsque les proxies se recouvrent, ni la taille d'échantillon rigoureuse d'un modèle récurrent.

## Reproductibilité, limites et garde merge

Rust natif 1.89, no SDK CUDA/NVIDIA, GitHub-hosted ubuntu-24.04, checkout SHA exact, token read-only, pas de runners RemoteOps self-hosted. Figer les 1232 sorties, publier toutes les 1232 évaluations TEST après le gel et un tableau complet machine-readable ; trois jobs CI isolés pour les mesures de durée. `cargo fmt --check`, `cargo check --locked`, `cargo clippy -D warnings` et tests binaires obligatoires. Fusion seulement si les jobs expérimentaux et la CI générale sont verts sur le SHA final. Les échecs sous bruit et faibles échantillons sont des **résultats**, pas des raisons d'ajuster le seuil.

Cette étude n'utilise que deux enseignants synthétiques, trois portes, 128 caractéristiques proposées, sources brutes uniformes, bruit i.i.d. et une topologie imposée. Elle ne prouve aucune récupération LPN générique, aucune extrapolation à 508 sources, k≥4/5, données linguistiques, banque K=1 048 576 ou gains GPU/inférence SML-25B.
