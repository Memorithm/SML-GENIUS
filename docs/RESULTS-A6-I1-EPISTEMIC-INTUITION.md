# A6-I1 — Résultats : inférence intuitive sous non-identifiabilité et décalage OOD

**12 octobre 2026 — première expérience complète exécutée, explicitement falsifiable et préenregistrée.**
Le protocole [A6-I1](A6-I1-EPISTEMIC-INTUITION-PREREGISTRATION.md) a été commité **avant le code et les résultats** au SHA `dae29ee57a58e4e503608b52f89279a013a0a268`. Première reproduction native complète sur source SHA `515e723a5565d919b9bf2b0e1f490c15d2cc4969` : [GitHub Actions A6-I1, job 114798125973](https://github.com/Memorithm/SML-GENIUS/actions/runs/38245454519/job/114798125973), **SUCCESS** avec tests Rust, `fmt`/`check`/`clippy -D warnings`, 84 historiques méta, 252 cellules de test et trois régimes de décision.

## 1. Question scientifique, résultat et garde de vocabulaire

**Question :** peut-on prédire utilement le comportement d'une fonction cachée lorsque les données passives de la tâche **ne permettent mathématiquement pas** d'en établir l'identité ? Une analogie fonctionnelle avec l'intuition humaine serait de choisir une hypothèse sur la base d'expériences précédentes, garder l'incertitude explicite, s'abstenir sur un désaccord non résolu et rechercher une intervention informative.

L'expérience démontre trois faits **dans un cadre synthétique contrôlé** :
1. des tâches dont la fonction est indiscernable à partir des observations passives bénéficient d'une **mémoire de corrélations entre tâches antérieures** : dans le régime où ces corrélations sont stationnaires, la précision sur les prédictions ambiguës hors domaine atteint **87,6379 %**, au lieu d'environ **50 %** pour une référence uniforme ou un prior global ;
2. la même procédure devient **très mauvaise sous décalage de distribution** : **11,8151 %** d'exactitude après inversion des corrélations historiques, et **49,6503 %** lorsque le contexte ne donne plus aucun indice ;
3. la gestion **épistémique** de l'incertitude fonctionne indépendamment de ce prior : les deux hypothèses sont **identiques sur 100 % des observations passives**, l'abstention ne commet **aucune erreur sur les sorties propres acceptées**, et une requête d'intervention séparée identifie correctement le candidat sur **100 % des tâches aveugles du corpus** (1 lecture propre ou 21 votes sous bruit).

**Ne pas appeler ces résultats la découverte certaine de la « véritable fonction » à partir de données impossibles à identifier.** La réussite provient précisément d'une information **supplémentaire** : la vérité de tâches **historiques étiquetées** et une métadonnée de contexte. Cette information méta est disponible lors de la prédiction, alors que la vérité de la **tâche courante** reste cachée. Il s'agit d'une capacité d'**inférence a priori transférée**, pas d'une intuition humaine ou d'une résolution surnaturelle de la non-identifiabilité.

## 2. Preuve constructive d'impossibilité locale

Pour chaque nouvel épisode, deux `WideAnfGate` réelles sont composées :
`f0(x)=b(x)` et `f1(x)=b(x) XOR (x_a AND x_b)`,
où `b` comprend quatre monômes positifs ANF de degrés 2 à 5 et dont les supports sont générés par permutation ; `a` et `b` désignent deux **positions physiques randomisées**, distinctes de celles des termes de base.

Deux mécanismes d'échantillonnage passif distincts ont été essayés :
- **EXCLUSIVE** : seules les paires `(0,0),(1,0),(0,1)` sont possibles sur les deux bits de distinction ;
- **FROZEN_ZERO** : le premier bit de distinction est toujours 0, le second est libre.

Dans les deux cas, `x_a AND x_b = 0` sur toutes les observations. Ainsi `f0(x)=f1(x)` **identiquement sur tout le support observable**, quels que soient le nombre d'observations, les autres entrées, la récurrence de l'échantillonnage ou le calcul du learner. Avec bruit d'étiquette Bernoulli(1/16) indépendant et identiquement distribué entre les hypothèses, **les distributions de labels observables sont également identiques**. Aucun estimateur sans information latérale ne peut améliorer l'inférence de l'identité de la tâche par rapport au prior de cette identité.

**Contraste OOD :** lorsqu'on force `x_a=x_b=1`, `f0(x)≠f1(x)` pour toute configuration des autres entrées. Chaque cellule a donc 32 requêtes OOD uniformes plus 16 **requêtes OOD forcées** par tâche. Le test ne peut pas réussir en n'examinant que le domaine où les hypothèses étaient déjà équivalentes.

**Mesure réellement effectuée :** pour chaque régime de déploiement, sur **5376 tâches aveugles** et **172 032 observations passives TRAIN/VAL**, le nombre de divergences natives `f0 vs f1` est **0**. Sur **86 016 requêtes OOD forcées**, les deux hypothèses divergent **86 016 fois**. Sur l'ensemble de 258 048 requêtes OOD, il existe **128 700 désaccords**.

## 3. Ce qui est appris et ce qui reste caché

Le nouveau module Rust public `anf_epistemic_intuition` fournit :

- `ContextualPrior` : apprend un prior conditionnel Beta(1,1) `P(f1|tag)=(n_{f1,tag}+1)/(n_{tag}+2)` à partir de **64 épisodes historiques pleinement étiquetés pour chacun des deux tags**. Ces étiquettes **révèlent** quelle hypothèse était correcte **dans les anciennes tâches**. Le modèle n'est donc pas supposé inférer cette information à partir d'observations locales indiscernables.
- `EpistemicAnfPair` : contient **les deux hypothèses natives données comme candidates** par la banque ANF ; n'apprend pas dans cette expérience leurs coefficients. Pour toute entrée, il évalue les deux portes, retourne une probabilité de sortie, une décision forcée et `consensus_only=None` si elles diffèrent. Une alerte `needs_intervention` accompagne le désaccord.
- `classify_intervention` : un bras **distinct** interroge une entrée dont les candidats prédisent des bits différents, et utilise une observation propre ou 21 observations bruitées à majorité pour trancher la tâche. Ces réponses **ne sont jamais transmises** à la décision passive.

La séparation est verrouillée : **84 ajustements de prior méta** (`k×graines×type de censure passive×panel de bruit`) sont gelés, chacun utilisant 128 épisodes historiques, avant de créer les épisodes aveugles ; **les mêmes priors méta** sont évalués sur les trois régimes de déploiement sans raffinement. Total **252 cellules de déploiement**, 7 largeurs `[8,16,32,64,128,256,1024]`, trois graines, deux censures, deux panels de bruit d'étiquette et trois régimes `MATCHED/REVERSED/UNINFORMATIVE`. Chaque cellule a **64 nouvelles tâches aveugles** (32 par tag) avec 16 TRAIN+16 VAL observations passives et 48 sorties OOD. Les corpus et priors sont appariés et réutilisés : **252 cellules ≠ 252 expériences statistiques totalement indépendantes**.

### Baselines

La variante **contextuelle** peut exploiter le tag. La référence **pooled** utilise toutes les tâches passées mais ignore ce tag, et la référence **uniforme** garde `P(f1)=0,5`. La référence **sélective** n'émet une sortie que quand les deux candidats s'accordent ; ses abstentions ne sont **pas comptées comme bonnes prédictions**. Enfin, la référence **interventionnelle** peut poser une question supplémentaire et possède donc un budget d'information différent. La précision est mesurée contre les sorties propres de l'enseignant **uniquement sur les requêtes où les hypothèses divergent**. Le Brier est calculé contre la sortie **propre**, séparément des erreurs provenant du bruit d'étiquette.

## 4. Résultats chiffrés : réussite, renversement et ignorance

| Évaluation OOD | Méta contextualisée | Prior global pooled | Référence uniforme | Brier contextualisé | Brier uniforme |
|---|---:|---:|---:|---:|---:|
| **MATCHED** : même régularité que le passé | **87,6379 %** | 49,9596 % | 50,2347 % | **0,109700** | 0,250000 |
| **REVERSED** : régularité historique inversée | **11,8151 %** | 49,5711 % | 50,0389 % | **0,659025** | 0,250000 |
| **UNINFORMATIVE** : tag sans information | **49,6503 %** | 51,0054 % | 49,8213 % | **0,385161** | 0,250000 |

Le Brier du prior global vaut **0,250943** dans MATCHED, **0,251017** dans REVERSED et **0,250508** dans UNINFORMATIVE. L'approche contextuelle améliore donc nettement les deux baselines **uniquement** lorsque la relation entre contexte historique et enseignant actuel est stable. Sous inversion ou disparition du lien, elle peut être **pire qu'un système qui reconnaît son ignorance**.

Confiance contextuelle moyenne dans sa propre décision : **86,1877 %** dans les trois régimes, car les priors gelés sont les mêmes. Calibration observée contre les résultats : écart **1,4502 points** en MATCHED, mais **74,3726 points** en REVERSED et **36,5374 points** en UNINFORMATIVE. Le modèle **ne détecte pas automatiquement** un changement de prior sur une tâche dont les observations demeurent non identifiantes. Ce défaut est **le principal résultat négatif à retenir** pour le futur mécanisme « intuitif » : une confiance élevée sans vérification de la stabilité du contexte est dangereuse.

### Résultat par largeur (précision contextuelle, contrôles appariés)

| Largeur `k` | MATCHED | REVERSED | UNINFORMATIVE |
|---:|---:|---:|---:|
| 8 | 87,896 % | 13,792 % | 48,203 % |
| 16 | 87,197 % | 12,411 % | 53,956 % |
| 32 | 88,153 % | 11,199 % | 46,589 % |
| 64 | 87,516 % | 10,217 % | 48,652 % |
| 128 | 87,237 % | 9,089 % | 49,778 % |
| 256 | 87,591 % | 13,515 % | 51,103 % |
| 1024 | **87,876 %** | **12,488 %** | **49,278 %** |

La précision MATCHED varie donc entre **87,197 % et 88,153 %** pour les sept largeurs : le résultat n'est pas porté par la seule largeur k1024 ni par k8. Un bootstrap illustratif à 4000 rééchantillonnages de **21 blocs largeur×graine**, en conservant leurs panels/modes ensemble, situe la précision synthétique MATCHED environ entre **86,75 % et 88,50 %** ; il ne s'agit **pas** d'un intervalle de confiance externe pour d'autres environnements ou enseignants ANF.

## 5. Abstention et intervention : deux comportements essentiels

Pour chaque régime, **129 348 des 258 048 requêtes OOD** sont sur un sous-domaine où les deux candidats s'accordent. La politique de consensus y répond et **s'abstient sur les 128 700 requêtes restantes**. Couverture OOD = **50,1256 %**. L'erreur par rapport à l'enseignant **propre** sur les prédictions effectivement acceptées est **0/129 348** : il s'agit d'une conséquence de l'inclusion du vrai enseignant dans les deux candidats et de leur accord, pas d'une propriété universelle d'ensemble de modèles.

Sous le panel bruité Bernoulli(1/16), **3988 erreurs sur 64 696 prédictions acceptées** ont été observées, soit **6,1642 %**, proche des 6,25 % de perturbation attendus. Ce bruit **aléatoire** ne doit pas être imputé à une erreur d'identification du candidat. Les 64 652 prédictions acceptées du panel propre sont sans erreur.

Le bras **interventionnel séparé** fixe une entrée `x_a=x_b=1`, les autres bits à zéro ; `f0=0, f1=1` sur cette entrée. Chaque décision demandant une seule vraie réponse, ou **21 votes** si elle est bruitée, identifie correctement le générateur sur **5376/5376 tâches par régime**, avec **59 136 requêtes physiques** consommées par régime. Cette intervention est utile mais **n'est pas une réussite de l'intuition passive** : elle ajoute précisément une information qui n'était pas présente dans les données insuffisantes.

## 6. Contrats reproduisibles, sécurité et limites

Première expérience complète [A6-I1 GitHub Actions, run 38245454519](https://github.com/Memorithm/SML-GENIUS/actions/runs/38245454519/job/114798125973), code commit `515e723a5565d919b9bf2b0e1f490c15d2cc4969`. Sorties auditées : **84 `A6_I1_META`**, **252 `A6_I1_CELL`**, **3 `A6_I1_RESULT`** et `A6_I1_COMPLETE` aux cinq clauses `matched_pass`, `reversed_guard_pass`, `uninformative_guard_pass`, `consensus_pass`, `intervention_pass` toutes vraies. La CI dédiée inclut compilation `--all-targets`, Clippy `-D warnings`, tests unitaires Rust et benchmark RELEASE. Premier échec CI lié uniquement au formatage, corrigé à l'identique des diffs Rust 1.89 avant les résultats. Exiger requalification du SHA **final documentaire** ainsi que la CI générale avant fusion.

```bash
cargo fmt --all -- --check
cargo check --locked -p sml-core --all-targets
cargo clippy --locked -p sml-core --all-targets -- -D warnings
cargo test --locked -p sml-core --lib anf_epistemic_intuition
cargo test --locked -p sml-core --bin sml-epistemic-intuition-bench
cargo run --locked --release -p sml-core --bin sml-epistemic-intuition-bench
```

Tous les workflows utilisent Ubuntu 24.04 GitHub-hosted, Rust 1.89 épinglé, `permissions:contents:read`, checkout SHA exact et sans authentification persistante. Aucun SDK NVIDIA ni runner RemoteOps privé n'intervient dans cette expérience. L'API ne lance aucun service résident et ne change aucune route SBG/MOR/Delta-KV existante.

**Limites centrales :** deux fonctions candidates connues **incluant la vraie par construction**, une seule interaction occultée de degré deux, historique méta **supervisé** donnant les étiquettes des enseignants passés, deux tags symboliques préexistants, censure de l'observation construite et non découverte, générateur synthétique, bruit IID et domaines OOD contrôlés. Ni l'émergence d'une intuition humaine, ni un raisonnement analogique non supervisé, ni la découverte de candidats inconnus, ni des données récurrentes réelles, ni la robustesse sous changements de distribution non observés, ni la performance d'un modèle 25B ne sont établis.

## 7. Décision pour la suite de SML-GENIUS

**Résultat expérimental : H1 PASS sous MATCHED ; témoins négatifs REVERSED et UNINFORMATIVE PASS en tant qu'échecs prévus ; politique sélective et intervention PASS.** La démonstration est forte **dans le dispositif**, car elle montre une information impossible à obtenir d'une source locale seule, une véritable amélioration obtenue par transfert entre tâches, et des contre-exemples qui détruisent cet avantage.

Pour transformer ce noyau en **intuition utile et sûre**, les prochaines expériences doivent franchir au moins quatre frontières indépendantes :

1. **Plus de deux candidats, vrai enseignant parfois absent** de l'ensemble de fonctions, et calibrage de la confiance sous modèle misspécifié ; une abstention par consensus sur deux hypothèses ne protège pas contre une banque incomplète.
2. **Historique méta sans étiquette d'enseignant**, analogies apprises à partir de succès/échecs, mémoire SBG/MOR/Delta-KV et contraintes de coût ; vérifier si un signal robuste apparaît sans accès explicite aux anciennes vérités.
3. **Détection d'un contexte devenu trompeur** par retour d'expérience et sélection adaptative d'interventions, avec regret/couverture/coût, et recalibration empirique.
4. **Traces récurrentes passives et contraintes de temporalité**, dont sources observationnellement équivalentes et perturbations persistantes d'état, ainsi que des tests hors distribution avec agents et enseignants non synthétiques.

Ne pas annoncer une capacité d'intuition acquise pour SML-25B avant ces tests. Le résultat A6-I1 donne un mécanisme opérationnel et un protocole falsifiable, **pas** une équivalence cognitive avec un humain.
