# A6-I2 — Préenregistrement : quatre hypothèses ANF, mémoire contextuelle passive, oracle incomplet et shifts

**Gel : 12 octobre 2026, avant l'implémentation et la première mesure A6-I2.** Base `main` SHA `1178e6001388504cae6746c0f0807ab5378cac08`, A6-I1 PR #503 déjà fusionnée. Préenregistrement immuable ; si un défaut de code est découvert, publier le correctif et conserver les premières traces négatives, sans changer graines, budgets, seuils ou population après TEST.

## Ce qui doit devenir plus difficile que A6-I1

A6-I1 a mesuré un a priori analogique sous **deux** hypothèses, avec **un enseignant garanti dans la paire** et des contextes passifs où l'identité n'était jamais observable. Sa réussite (~87,64% quand le prior historique correspond à la distribution) échouait catastrophiquement (~11,82%) lorsque l'environnement inversait la distribution. A6-I2 ne répète pas ce banc : il impose **quatre vraies fonctions concurrentes**, trois régimes de visibilité passive, des entrées **corrélées par récurrence Markov**, des décalages de distribution des contextes au TEST, et **deux enseignants absents de la banque** distincts, l'un détectable pendant TRAIN et l'autre mathématiquement indétectable.

La question est celle d'une *inférence intuitive avec honnêteté épistémique* : le système doit séparer probabilité de prédiction, prédiction forcée, confiance et abstention et savoir déclarer une inadéquation observable, sans jamais revendiquer qu'il retrouve une fonction impossible à distinguer.

**La banque de quatre candidats est fournie et contient l'enseignant dans les expériences positives** ; le système ne découvre pas encore les monômes depuis les flux passifs. Les négatifs vérifient ce qui arrive quand cette hypothèse est fausse.

## Fonctions, prior et attribution

Largeurs `k∈{8,16,32,64,128,256,1024}`. Pour chacune des **3 graines**, choisir 8 indices physiques distincts `r0..r7` parmi `0..k-1` à l'aide de la permutation Fisher–Yates seedée `ROLES=0xA6B1_5001_51ED_0001`. Les candidats de la banque sont :
- `h0 = x[r2] XOR x[r3]`
- `h1 = h0 XOR (x[r0] AND x[r1])`
- `h2 = h0 XOR (x[r4] AND x[r5])`
- `h3 = h0 XOR (x[r0] AND x[r1]) XOR (x[r4] AND x[r5])`.

Tous sont de **vrais `WideAnfGate`** sparse de fan-in k ; les rôles physiques sont connus lors de la construction de la **banque candidate**, mais le professeur secret choisit un des quatre h sans donner son ID à l'algorithme. Cette banque est un a priori structurel très fort, non une découverte de structure. Les roles `r6,r7` sont réservés aux contrôles hors banque et tirés de la même permutation.

A priori **délibérément biaisé et gelé** `P(h0..h3)=[4,2,2,1]/9`, comme un biais de simplicité. Référence uniforme `[1,1,1,1]/4`. La population évaluée est **équilibrée en quatre enseignants**, et ne suit donc pas ce prior. La pénalisation ou l'avantage du prior est rapporté honnêtement, sans promettre une calibration universelle.

Le nouveau module `anf_passive_committee` prend exclusivement ces `WideAnfGate`, un prior fourni, des observations passives `(x,y)`, le paramètre explicite de bruit supposé `p=1/16`, et des limites. Il met à jour la masse log-posterior par vraisemblance Bernoulli, avec normalisation log-sum-exp. Il retourne probabilité de y=1, confiance, MAP forcé, décision `Some(bool)` seulement si confiance≥**0,90**, sinon abstention, et un compteur exact de classes d'équivalence des **signatures de prédiction sur tous les x vus**. Aucun oracle TEST ou identifiant du vrai enseignant n'est accessible au comité. Le classificateur d'adéquation compare `min_h erreur_train(h)/512` au seuil préfixé **1/8** : c'est un contrôle de l'inadéquation **observable** de la banque, non une garantie hors-support.

## Processus de données passives réellement corrélées

512 pas TRAIN pour chaque expérience, étiquettes propres ou flips Bernoulli(1/16) **indépendants** (`PANEL_CLEAN`, `PANEL_NOISY`). L'apprenant **ne choisit jamais ses entrées**. Les deux « triggers » observés `x[r0]` et `x[r4]` sont des états persistants, générés par une transition **Markov** : quand un trigger vaut 0, il passe à 1 avec probabilité 1/14 ; quand il vaut 1, il revient à 0 avec probabilité 1/2, sans remise à zéro à chaque pas. Les autres composantes sont renouvelées indépendamment, et `x[r1]=1` ou `x[r5]=1` lorsque le trigger correspondant est actif pour que la distinction soit observable. Les modèles voient la **suite ordonnée des x et y**, sans accès au choix du régime.

Trois régimes de visibilité :
- `BLIND` : forcer `x[r0]=x[r4]=0` à tous les pas, mais conserver les autres bits et leur générateur. **Tous les quatre h sont exactement identiques sur toute séquence TRAIN possible**, même avec des labels bruités. Exiger 1 classe de signature et **même postérieur pour les quatre enseignants** soumis aux mêmes x et au même flux de bruit, sans fuite d'ID professeur.
- `PARTIAL` : déclencher seulement `x[r0]` selon Markov, `x[r4]=0`. Deux classes observationnelles : `{h0,h2}` et `{h1,h3}`, tandis que les différences sur le deuxième monôme sont impossibles à observer.
- `FULL` : les deux triggers Markov évoluent. Quatre classes observables avec les 512 données ; la mise à jour doit rendre le vrai professeur **postérieurement ≥0,99** sous les réponses propres ET bruitées, pour chaque enseignant et chaque graine (soit **168 cas FULL**).

Les entrées TRAIN sont **identiques pour les quatre enseignants** à largeur/graine/régime/panel donnés, et les flips de label bruité utilisent le même flux, afin de rendre les égalités observationnelles testables par simple comparaison des traces. Les essais restent **corrélés entre enseignants et entre panels** : ne pas annoncer 504 distributions indépendantes. L'échantillonnage Markov est un vrai processus récurrent d'inputs, mais **pas encore la boucle de propagation SBG/MOR/Delta-KV**.

### TEST et décalage préenregistré

Seulement après gel de **tous les 504 comités**, générer des flux OOD de **2048 pas par (largeur,graine)** avec un second processus Markov : taux symétriques activation 1/4 et désactivation 1/4 pour les deux triggers (marginale stationnaire environ 1/2), et **x[r1],x[r5] restent aléatoires même si leur trigger est actif**. Les autres variables sont aléatoires ; aucun label bruité TEST pour les mesures de la vraie fonction. Chaque modèle est évalué sur le **même** flux OOD de sa largeur/graine, puis son professeur caché h0..h3 définit la vérité **rétrospectivement**. Il n'existe que **21 flux d'inputs TEST distincts**, délibérément partagés entre modèles, et **504×2048=1 032 192 sorties OOD comptées**.

Mesurer pour chaque expérience : vrais états « identifiable/non », classes de signature passives, masse a posteriori du vrai professeur (mesure rétrospective seulement), exactitude du MAP forcé, Brier `(p_y−y)^2` pour le prior biaisé et le prior uniforme, confiance, couverture à seuil0,90 et taux d'erreurs sur les prédictions acceptées, disagreement non couvert, temps CPU d'ajustement et de prédiction. Distinguer *couverture* et *précision sur couverture*; rapporter par largeur, professeur, régime et panel.

## H1–H4, seuils préfixés

**H1 : non-identifiabilité prouvée dans BLIND.** Pour les 7×3×2=42 groupes largeur/graine/panel, tous les quatre enseignants ont rigoureusement la **même suite de 512 labels TRAIN** et **le même postérieur**, et il existe **une classe de prédictions** parmi les quatre fonctions. Pour PARTIAL, exactement 2 classes ; pour FULL, exactement 4. Ces nombres sont vérifiés à partir des **512 signatures complètes**, sans hachage faible.

**H2 : apprentissage lorsque l'information devient disponible.** Les 168 comités FULL doivent donner `P(teacher)≥0,99` avec 512 observations TRAIN, sous les deux panneaux, sans consulter TEST. Exiger 168/168 ; si le modèle n'y parvient pas, le seuil scientifique est en échec (ne pas masquer).

**H3 : honnêteté sélective sous banque complète.** En BLIND, toute divergence entre candidats qui subsiste au TEST doit entraîner une **abstention au seuil0,90** ; les points d'unanimité peuvent être prédits. Exiger zéro erreur propre parmi les prédictions acceptées BLIND et couverture strictement comprise entre 0 et 1. FULL : sous le seuil de posterior H2, exigence d'au moins 99% de couverture OOD avec zéro erreur propre. Le prior biaisé doit **être moins bon en Brier moyen** que l'uniforme sur la population BLIND équilibrée des quatre enseignants, malgré son avantage rétrospectif spécifique lorsque h0 est vrai. Ce contrôle réfute une « intuition toujours meilleure » sous prior incorrect.

**H4 : deux types de fonctions absentes de la banque.** Pour chacune des **21 largeurs/graines**, un professeur externe `g = h0 XOR x[r6]x[r7]` est testé avec :
- `VISIBLE` : pendant TRAIN BLIND, r6/r7 restent aléatoires, donc la composante hors banque est observée ≈1/4 des pas. Exiger que le seuil d'inadéquation 1/8 refuse **21/21**.
- `CENSORED` : pendant TRAIN BLIND, r6 est forcé à 0 ; les labels sont exactement ceux de h0 avec le même bruit, alors que sur OOD r6/r7 sont libres. Exiger que le contrôle d'adéquation **accepte 21/21** (il ne peut détecter le manque d'information), **et** au moins **une erreur propre avec confiance ≥0,90** sur les 2048 entrées OOD de chacun des 21 cas. Il s'agit d'un **contre-exemple requis** : l'absence du vrai modèle de la banque rend même l'abstention consensuelle potentiellement fausse. Réutiliser les vrais candidats de la banque, aucune « branche magiquement informée » du résultat caché.

**H5 : adéquation et contrôle des entrées.** L'API rejette avant toute mutation les candidats vides, largeurs incompatibles, priors non positifs/non finis, observations trop courtes, plus de 512 mises à jour, bruit invalide, seuil de confiance invalide, et collisions de candidats. Le nombre de modèles en comité est borné `2..=8`. Tous les budgets de `WideAnfLimits` restent explicitement ceux du code de PR #498 ; aucune table dense de `2^k` fonctions. Des tests indépendants doivent exposer le défaut de prior et l'égalité exacte des histoires BLIND.

## Population, seeds, fichiers et limites

- Population positive : **7 largeurs ×3 graines ×4 professeurs ×3 régimes ×2 panels =504 comités** entraînés, gelés puis TEST. Aucune intervention choisie par le comité et aucun label TEST ne modifie les masses a posteriori.
- Négatifs : **21 VISIBLE +21 CENSORED** ; tous les modèles de la banque sont identiques à ceux des positifs correspondants.
- Séquences : TRAIN512, TEST OOD2048, mêmes inputs et bruit de TRAIN entre professeurs d'une même strate. Seeds `ROLES=0xA6B1_5001_51ED_0001`, `TRAIN=0xA6B1_5002_51ED_0002`, `LABEL=0xA6B1_5003_51ED_0003`, `TEST=0xA6B1_5004_51ED_0004`, `NEGATIVE=0xA6B1_5005_51ED_0005`, mélangés avec `(seed+1)×0xd1342543de82ef95 XOR width XOR régime` par `u64 wrapping`. Les seeds effectives et les requêtes sont auditables dans les logs.
- L'observateur ne peut pas mettre à jour les coefficients ANF ni rechercher une fonction absente : il ne fait qu'ordonner un **ensemble fourni de quatre candidats**. C'est un point central du contrat. Les quatre candidats sont distincts et calculés par `WideAnfGate` au fan-in déclaré, avec des indices de rôles physiques tirés dans tout le domaine (k1024 inclus).
- Publier `A6_I2_FROZEN` **504**, `A6_I2_TEST` **504**, `A6_I2_NEGATIVE` **42**, `A6_I2_COMPLETE` **1** ; les 42 invariants « hist identiques » et les tailles de classes par régime vérifiés **avant TEST** ; données négatives, calibrage/Brier, couverture, risques et temps CPU sans suppression des mauvais cas.
- CI Rust 1.89 épinglé, GitHub-hosted Ubuntu24.04, read-only checkout de SHA exact sans credentials persistants, aucun runner privé, `#![forbid(unsafe_code)]`, aucun SDK NVIDIA hors pilotes matériel. Écrire docs/rapport/roadmap/bootstrap et fusionner uniquement après `fmt`, `rust`, CI dédiée et tous checks verts sur le dernier SHA.

**Ce que cela ne démontre pas :** une capacité de conscience ou une reproduction de l'intuition humaine, une garantie de confiance si l'enseignant sort de la banque, découverte autonome de monômes, apprentissage d'un routeur MOR, un SML-25B entraîné, des performances GPU ou une banque d'un million de fonctions. Le processus d'inputs est Markov-corrélé mais n'est pas encore le vrai graphe récurrent SBG/Delta-KV ; cette intégration sera un autre banc.
