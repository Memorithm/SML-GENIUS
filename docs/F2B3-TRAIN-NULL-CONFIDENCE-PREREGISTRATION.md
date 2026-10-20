# F2b-3 — Filtre de confiance TRAIN-only contre les admissions aléatoires

**Préenregistré le 2026-10-20, avant tout nouveau calcul F2b-3.** Réparation motivée par le **FAIL** de F2b-2 (RemoteOps #626) : sous l'ancienne limite d'erreur 500000 ppm, un contrôle IID a été routé à tort dans 2/4 cas n32/n508 (reproduits sur Dell/Thor mais corrélés entre grilles). Conserver intégralement les anciens résultats et la CLI `--f2b`/`--f2b2`. Les graines et le nouveau seuil ne peuvent être ajustés après l'exécution F2b-3.

## Choix explicite de seuil, dérivé d'un modèle nul

Un modèle sans signal qui prédit des bits indépendants équilibrés est proche d'une erreur de 50%. Pour l'hypothèse **IID Bernoulli(0.5)**, l'inégalité de Hoeffding donne
`P(p_err <= 0.5 - e) <= exp(-2 N e²)`.
Pour N=640 pas ×64 voies = **40960 bits** par partition et e=0.01 (un point de pourcentage), cette borne est exp(-8.192)=**≈0.000276** *par partition*, sous indépendance. Le seuil d'admission est donc **au maximum 490000 erreurs par million de cibles (49%) sur chacune des deux partitions TRAIN_cal** pour toute méthode. Le minimum de gain W3 contre Hybrid reste **10000 ppm** sur chacune des moitiés, et les budgets CPU restent **500000 ns/cible Hybrid, 1000000 ns/cible W3**, avec plancher de 1024 cibles par partition. Aucun poids ne s'ajuste sur les cibles de calibration.

**Limite mathématique :** une borne binomiale/Hoeffding sur des voies IID ne vaut pas automatiquement pour des bits corrélés dans le temps, des fonctions apprises après sélection adaptative sur la même calibration, ou plusieurs modèles comparés sans correction. La sélection de 49% a été *motivée par les données F2b-2* ; la nouvelle campagne utilise de nouvelles graines mais n'est pas une évaluation externe d'auteur indépendant. Elle n'établit pas un taux universel de fausses découvertes.

## Nouveau flux et grille fixés

Conserver la même classe d'enseignants synthétiques que F2b-2, mais imposer un troisième espace de sels RNG et les graines nouvelles **41,53,67,79** (indices 401,403,407,409), jamais réutilisées dans C12h ni F2b-1/2. Fonctions XOR³, AND→XOR→XOR, et **null** dont les cibles sont remplacées par des labels uniformes indépendants dans **toutes les phases**. Deux niveaux de bruit pour les fonctions causales, 0% et 40%, null uniquement 0%. Source candidate n32/n508. Au total **40 lignes par hôte** : (2 fonctions ×2 bruits +1 null) ×4 graines ×2 grilles. Les observations n32/n508 d'une même graine peuvent partager leurs flux : ne pas présenter ces 80 lignes sur deux hôtes comme 80 jeux indépendants.

TRAIN_fit (128 ou 512 mots de 64 bits utiles après 128 pas d'halo), validation interne 640×64, deux TRAIN_cal séparés de 640×64, DEV de 4096×64 **généré après freeze** ; moteurs réels C12h W3 et W2+Sketch/MITM, mêmes poids, faux positifs contrôlés sans oracle source. Sur n508, W3 **non exécuté** car >1 million de triplets ; drapeau explicite, pas une précision DEV implicite. Recherche/fit et évaluation TRAIN facturés comme auparavant. Aucune évaluation de qualité TEST avant le choix final.

## Critères de décision et preuves

Introduire uniquement un nouveau mode opt-in `--f2b3 xor|mixed|null 0|40 41|53|67|79 32|508`. Anciennes commandes et leurs logs doivent rester compatibles. Les 29 colonnes de F2B_ROUTE_V2 sont conservées, avec une trentième colonne **max_error_ppm=490000** dans `F2B_ROUTE_V3`, pour empêcher toute confusion avec l'ancienne politique 500000 ppm. Tests de politique explicites utilisant les observations négatives archivées F2b-2 *comme test de régression*, et des cas synthétiques autorisés à 40% d'erreur, mais **aucun réglage après l'exécution F2b-3**.

Succès d'ingénierie : CI Rustfmt, Clippy strict, tests originaux C12h et F2b1/2 inchangés, 40 lignes de 30 colonnes sur chacun des Dell/Thor, n508 sans exécution W3, abstention sans prédictions et source SHA/artefacts. **Succès scientifique provisoire** seulement si les **8 observations null par hôte** (4 graines ×2 grilles, non indépendantes entre grilles) ont **0 admission**, sans masquer les cas bruités qui deviennent abstentions. L'absence de faux positif sur ces petits échantillons ne prouve PAS une garantie générale : fournir intervalles et répétitions futures indépendantes.

STOP et retour à une politique de refus si un contrôle nul est promu ; ne pas contourner par baisse de la marge W3, surajustement DEV ou exclusion de la graine. Ne jamais fusionner une politique de déploiement « validée » uniquement sur ces tâches synthétiques.

SEC G0 NOT_ADMITTED, TEST 2024 UNOPENED, aucune donnée confidentielle, GPU, pile logicielle NVIDIA ou entraînement SML25B.
