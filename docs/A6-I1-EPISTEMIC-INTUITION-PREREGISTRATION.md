# A6-I1 — Préenregistrement : intuition opérationnelle sous non-identifiabilité

**Gel : 12 octobre 2026 ; avant tout code et toute mesure A6-I1.**
Base : `main` après PR #502 fusionnée (`c9a361bf0da451f8cc6e3b95ebaa4e70ccaeafd9`).
Ce protocole complète WL1–WL3 : il ne revendique ni faculté humaine ni récupération garantie de structures impossibles à identifier.

## Hypothèse principale

L'expérience teste si SML-GENIUS peut exploiter des **régularités antérieures entre tâches** pour faire de bonnes prédictions **sans pouvoir identifier la fonction cachée par les observations de la tâche courante**, tout en conservant une calibration de l'incertitude, une capacité d'abstention et un mécanisme d'intervention ciblée.

Deux portes ANF concurrentes sont réellement évaluées par le moteur `WideAnfGate` :
`f0(x) = b(x)` et `f1(x)=b(x) XOR (x_a AND x_b)`.
Le polynôme commun `b` contient quatre monômes positifs de degrés 2,3,4,5 (positions redistribuées par graines, pouvant se chevaucher). Les deux bits `a,b` sont distincts et leur position change selon la tâche ; `b` ne lit jamais ces deux bits. L'apprenant **voit les deux fonctions candidates**, comme si elles provenaient de sa banque de modèles, mais ne connaît pas l'identité de l'enseignant actuel. Il ne s'agit pas de découvrir les candidats, compétence étudiée séparément dans WL3.

Toutes les observations passives de la tâche courante respectent **`x_a AND x_b=0`**. Les sorties des deux ANF sont donc identiques pour **toute** observation admise. Cette indistinguabilité subsiste sous un bruit d'étiquette indépendant de probabilité 1/16. Elle est **structurelle et mathématiquement démontrable** : aucun algorithme ne peut identifier la porte réelle uniquement à partir de ces observations, même avec un calcul illimité. Si un agent se dit certain dans ce domaine sans information antérieure, c'est une erreur épistémique.

À l'inverse, les deux fonctions divergent sur `x_a=x_b=1`. La capacité proposée — analogue *fonctionnellement*, non cognitivement, à l'intuition — est d'apprendre à l'avance une probabilité conditionnelle de tâche `P(f1 | contexte)` à partir de **tâches historiques étiquetées**, puis de prédire avec confiance mesurée sur une **nouvelle tâche aveugle**. L'agent doit s'abstenir sur les requêtes où les deux modèles divergent si la fiabilité est insuffisante, et proposer une intervention qui tranche.

## Populations, prérequis et séparation des données

Largeurs `k={8,16,32,64,128,256,1024}`, 3 graines, **deux régimes d'observation passive** : `EXCLUSIVE` (paires `(x_a,x_b)=(0,0),(1,0),(0,1)` uniformes) et `FROZEN` (x_a=0, x_b uniforme). Deux panneaux d'étiquettes d'observation `CLEAN` et `LABEL_NOISE_1_16`, indépendants des choix du modèle. Trois régimes d'évaluation des priors sous déplacement de distribution : `MATCHED`, `REVERSED`, `UNINFORMATIVE`. Total **7×3×2×2×3=252 cellules** (appariées, pas 252 populations indépendantes).

Chaque cellule se compose de deux contextes symboliques `c0,c1` connus de l'apprenant (métadonnées de provenance comparables à une signature de contexte). **64 tâches historiques étiquetées par contexte** permettent d'apprendre un prior Beta(1,1) : `p̂_c=(n(f1,c)+1)/(n(c)+2)`. L'historique contient la **vérité de l'identité des fonctions** pour les anciennes tâches : c'est une supervision méta explicite, **pas une information obtenue à partir des observations impossibles à distinguer**. Un prior global de contrôle regroupe les deux contextes ; une référence uniformément ignorante prédit 1/2. Les mêmes priors appris sont figés **avant la création des tâches de test**, puis appliqués sans réentraînement aux trois régimes :

- MATCHED : `P(f1|c0)=1/8`, `P(f1|c1)=7/8` dans histoire ET tâches de test ;
- REVERSED : même histoire, mais `P(f1|c0)=7/8`, `P(f1|c1)=1/8` pendant TEST ;
- UNINFORMATIVE : même histoire, mais `P(f1|c0)=P(f1|c1)=1/2` pendant TEST.

Chaque régime utilise **32 tâches de test aveugles par contexte**, donc **64 enseignants inconnus par cellule**. Pour chaque tâche de test, créer **16 observations TRAIN passives** puis **16 observations VAL passives** selon le régime EXCLUSIVE/FROZEN. L'apprenant peut calculer des prédictions sur ces observations, mais **n'accède jamais à l'identité cachée** de cette tâche. Les deux fonctions doivent avoir **exactement zéro désaccord** sur toutes ces observations — toute divergence signale un échec de protocole. Aucun de ces échantillons ne sert à ajuster le prior appris sur les anciennes tâches.

Après gel du prior et des observations passives de chaque tâche, générer **48 requêtes TEST par tâche** sur de nouveaux flux indépendants : 32 entrées à bits uniformes Bernoulli(1/2), et 16 entrées OOD ciblées avec `x_a=x_b=1` (autres bits uniformes). Cibles : sorties propres de la véritable ANF +, uniquement pour le panneau bruité, une inversion de label Bernoulli1/16 indépendante. Comparer les sorties ANF candidates **bit à bit**. Les 16 requêtes ciblées garantissent une évaluation du désaccord, évitant la réussite artificielle sur une distribution qui ne différencie jamais les fonctions.

Prévoir en parallèle un **bras interventionnel**, indépendant du modèle intuitif passif : une seule requête déterminée par l'écart des deux candidats, `x_a=x_b=1` et toutes les autres entrées à zéro. Sur les données propres, une réponse suffit à sélectionner exactement f0 ou f1. Sur les données bruitées, **21 observations indépendantes** de cette requête sont votées à majorité stricte. Ne pas confondre la sélection obtenue **après intervention** avec l'inférence intuitive passive.

## Les modèles et leurs sorties

La nouvelle API Rust `EpistemicAnfPair` contient les deux vraies `WideAnfGate` natives. Elle retourne séparément leurs prédictions, leur accord/désaccord et la probabilité prédictive conditionnelle `q=(1−p̂_c)f0(x)+p̂_c f1(x)`. La décision forcée choisit `q≥1/2`, et une politique **sélective** s'abstient exactement lorsque les deux candidats divergent ; elle ne prétend donc pas être correcte sur les requêtes refusées. La sortie peut inclure un `needs_intervention`, qu'un moteur autonome ultérieur pourrait transformer en plan d'acquisition d'information. Pas de réétiquetage rétroactif après TEST.

Baselines fixes :
1. `contextual prior` appris sur les 64 tâches méta de chaque contexte ;
2. `pooled prior` appris sur les 128 tâches historiques sans métadonnées ;
3. `uniform posterior` fixé à 1/2 ;
4. `selective consensus` (accepte seulement si f0=f1, couverture évaluée) ;
5. `active 1-or-21-query` (oracle interventionnel, bras **séparé** et compté).

Calculer la **précision conditionnelle de choix f0/f1 uniquement sur les cas OOD en désaccord**, la couverture et l'erreur des prédictions acceptées, et le **Brier score** `(q−y_clean)^2` ainsi que l'écart confiance observée versus attendue sur les requêtes en désaccord, en séparant les panels d'étiquettes. Les métriques de sortie propres et bruitées sont distinctes : une confusion entre bruit aléatoire d'étiquette et incertitude épistémique sera considérée comme échec de l'analyse. Rapporter coût CPU d'inférence et méta-fit, nombre de tâches passives, de lectures d'observation, coût d'intervention et budgets de payload/scratch ; aucune lecture des identités TEST n'est autorisée dans les décisions, uniquement dans l'évaluation rétrospective.

## Critères préfixés et contrôles négatifs

- **Non-identifiabilité vérifiée à 100 %** : zéro désaccord f0/f1 sur toutes les 16 TRAIN +16 VAL observations passives des 64 tâches de chaque cellule (252 cellules). Sur les 16 requêtes TEST OOD forcées, f0/f1 doivent **différer à chaque fois**.
- **MATCHED** : précision moyenne (sur tous les désaccords et cellules, mesurée sur labels **propres**) de la décision contextuelle **≥80 %**, et Brier contextuel **strictement inférieur** au Brier uniforme et au prior global pooled. La confiance mesurée doit être à moins de **0,10** de la précision empirique sur l'ensemble des désaccords.
- **REVERSED** (témoin adversarial de prior périmé) : précision de la même stratégie contextuelle **≤25 %**, Brier contextuel **supérieur** au Brier uniforme. Ne jamais requalifier cette dégradation comme succès intuitif.
- **UNINFORMATIVE** : précision de la stratégie contextuelle **entre 45 et 55 %**, et Brier contextuel **supérieur ou égal** au Brier uniforme. Un « succès » systématique dans ce régime serait une fuite d'information ou une sélection non aveugle.
- **Sélectif** : zéro erreur sur les cibles propres des requêtes où f0=f1, abstention sur **tous** les désaccords, couverture OOD publiée (ne pas comptabiliser l'abstention comme prédiction juste). La précision sélective sous bruit d'étiquette doit baisser conformément à celui-ci.
- **Intervention** : bras propre exact **100 %**, bras bruité **≥99 %** sur l'agrégat, avec coûts honnêtes 1 ou 21 requêtes par tâche, et sans que l'oracle soit consulté par la prédiction passive.

Les probabilités sont testées sur un corpus synthétique de générateurs variés ; les critères statistiques portent sur des agrégats par cellules corrélées, et **pas** sur des probabilités universelles pour toute ANF. Inclure un test unitaire où deux générateurs contradictoires ont des observations TRAIN identiques et divergent hors domaine, un cas de prior de contexte inversé, un cas d'abstention correcte et un contrôle fail-closed d'un prior vide / d'entrées hors domaine.

## Graines, ressources et garde-fous

Chaque séparation reçoit un `SplitMix64` indépendamment seedé, avec `base=(index+1)*0xD134_2543_DE82_EF95 XOR width XOR mode*​0xBF58_476D_1CE4_E5B9 XOR panel*​0x9E37_79B9_7F4A_7C15`, et sels distincts `META=0xA6B1_4001_51ED_0001`, `PASSIVE=0xA6B1_4002_51ED_0002`, `TEST=0xA6B1_4003_51ED_0003`, `INTERVENTION=0xA6B1_4004_51ED_0004`, `CANDIDATES=0xA6B1_4005_51ED_0005`. Pour les deux panels et les trois régimes, réutiliser les mêmes **prior counts** au sein d'un bloc largeur×seed×mode passif×panel ; le régime TEST n'est pas utilisé pour entraîner le prior.

Paramètres finaux : 7 largeurs ×3 graines ×2 modes passifs ×2 panels ×3 prior tests = **252 cellules** ; 128 historiques étiquetés par bloc, 64 blind tasks par cellule, 16+16 observations passives par tâche, 48 sorties OOD par tâche et une intervention séparée par tâche (21 répétitions pour panel bruité). **Ne pas publier de métriques sur 252 cellules sans dédupliquer les observations partagées** ou sans signaler leur structure appariée. Tous les objets de fonction sont exécutés en Rust sûr, avec `WideAnfLimits::new(2048,256,8192,8192,8192)` et sans table dense à 1024 entrées.

CI : Rust 1.89 pin, GitHub-hosted Ubuntu 24.04, droits `contents:read`, checkout SHA exact sans identifiants persistants, zéro runner privé Memorithm/RemoteOps, zéro SDK NVIDIA hors pilotes, aucun `unsafe` ni service résident.

**Conclusions interdites avant validation :** preuve d'intuition humaine, choix certain d'une vérité intrinsèquement non identifiable, solution générale à la sous-détermination, prédiction OOD garanties sous changement de distribution, qualité linguistique SML-25B, débit GPU/tokens ou million de fonctions ANF entraînées. **Ne fusionner qu'après publication honnête des résultats, CI dédiée et générale toutes vertes au HEAD documentaire final.**
