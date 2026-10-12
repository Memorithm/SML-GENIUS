# A6-I2 — Résultats : inférence probabiliste à quatre fonctions ANF, récurrence passive et fausse confiance hors banque

**12 octobre 2026 — expérience complète, Rust natif, préenregistrée, avec témoin négatif impossible à détecter.**

Protocole principal gelé **avant code et mesures** : [A6-I2-FOUR-HYPOTHESIS-PASSIVE-PREREGISTRATION.md](A6-I2-FOUR-HYPOTHESIS-PASSIVE-PREREGISTRATION.md), premier commit `93233ca9dfc48cc006396955c7982aa70b1e7b51`. Premier run scientifique complet avant diagnostic ajouté : [CI `passive-committee` job 114811920996](https://github.com/Memorithm/SML-GENIUS/actions/runs/38249522352/job/114811920996), SHA code `2781d0b56c6df92fa148f91744fe3d9f8f610ca7`, **SUCCESS**. Un [amendement prospectif transparent](A6-I2-FAIR-BASELINE-AMENDMENT.md) a ensuite été commité **avant** d'ajouter un **comparateur bayésien uniforme actualisé**. Second run complet : [job 114813702118](https://github.com/Memorithm/SML-GENIUS/actions/runs/38250054077/job/114813702118), SHA `a200c2811bb53762c4bc6ecca89b177846f485de`, **SUCCESS**. Les effectifs, les 504 apprentissages, les seuils, les graines, les labels et tous les résultats primaires sont identiques entre les deux runs.

## 1. Ce que nous avons réellement donné au système

La nouveauté est un `sml_core::anf_passive_committee::PassiveCommittee` probabiliste, au-dessus de **quatre vrais `WideAnfGate` natifs**. Ce n'est **pas** un apprenant découvrant les monômes. Le système reçoit **une banque de candidats fournie**, une distribution a priori explicite et de vraies observations passives successives `(x_t,y_t)`. Le code n'a aucun argument enseignant caché ; la véritable fonction est utilisée **uniquement par le générateur pour établir les labels et rétrospectivement par le banc pour mesurer les erreurs**. La mise à jour d'hypothèse est le produit de vraisemblances Bernoulli traité en log-espace, avec hypothèse de bruit d'étiquettes `p=1/16`.

Pour chaque largeur `k=8,16,32,64,128,256,1024`, huit indices physiques distincts `r0..r7` sont mélangés dans l'intégralité de `0..k−1` indépendamment du professeur. Les quatre hypothèses sont :
- `h0=b(x)=x[r2] XOR x[r3]` ;
- `h1=b(x) XOR x[r0]x[r1]` ;
- `h2=b(x) XOR x[r4]x[r5]` ;
- `h3=b(x) XOR x[r0]x[r1] XOR x[r4]x[r5]`.

Les huit indices sont **déclarés dans les candidats** et les quatre fonctions sont **présentes dès le départ**. La variable secrète du professeur, parmi h0..h3, **n'est pas transmise**. Les autres `k−8` entrées ne modifient pas ces quatre fonctions, même si les vecteurs et les positions physiques atteignent bien k1024. L'expérience n'est donc ni une recherche passive de fonctions à 1024 variables actives, ni une recherche de monômes arbitraires.

Le prior `[4,2,2,1]/9` favorise le circuit le plus simple h0 ; il est **délibérément inadapté à la population test équilibrée** des quatre professeurs. Le module conserve les **signatures exactes de toutes les prédictions candidates sur les 512 échantillons TRAIN**, sans supposer qu'un hash de 64 bits suffirait à certifier l'équivalence observationnelle. Il expose une prédiction forcée MAP, une probabilité binaire, un niveau de confiance, l'option **abstention si confiance <0,90**, et une alarme `min_train_error_h > 1/8` sur inadéquation observée à la banque. Une classe de signature identique ne signifie pas identité mathématique hors du support observé.

## 2. Trois régimes récurrents de collecte, puis transfert OOD

Trois distributions d'entrées **Markov-corrélées dans le temps** sur 512 pas TRAIN, sans possibilité d'intervention par le comité :
- **BLIND** : deux signaux déclencheurs `x[r0]` et `x[r4]` sont forcés à zéro. Les quatre vrais réseaux natifs sont **identiques sur la totalité de ce support**, même en présence de bruit de labels indépendant. Les **quatre traces TRAIN et leurs postérieurs sont identiques**, à même graine et panel : l'identité du professeur est mathématiquement non identifiable.
- **PARTIAL** : un trigger varie selon une transition Markov (activation depuis zéro 1/14, sortie depuis un 1/2), l'autre reste à zéro. Il existe exactement les **deux classes de signatures** `{h0,h2}` et `{h1,h3}`.
- **FULL** : les deux triggers évoluent selon le même mécanisme Markov. Tous les quatre h deviennent distinguables sur les traces TRAIN seedées.

Une fois **tous les 504 comités** entraînés et leurs **126 strates d'équivalence** vérifiées, seulement alors les entrées TEST sont générées. Les entrées OOD ont des déclencheurs persistants de transition 1/4 entrée et 1/4 sortie et des cofacteurs non forcés, créant un **déplacement de distribution des contextes**. Il y a **21 flux TEST d'entrées distincts**, 2048 pas pour chaque (k,graine), réutilisés volontairement sur les différents professeurs et panels. Les **504×2048=1 032 192** évaluations de prédictions ne représentent pas 1 032 192 séquences indépendantes. Le générateur n'est **pas** encore le vrai runtime de transition récurrente SBG/MOR/Delta-KV : il s'agit d'une source passive Markov persistante.

## 3. H1–H3 : identité impossible, sélection probabiliste et coût d'un prior biaisé

**Résultat conforme aux seuils préfixés :**

- **126/126 contrôles d'équivalence** : pour chacun des 42 groupes largeur×graine×panel, BLIND a **une classe** et les postérieurs sont identiques entre tous les professeurs ; PARTIAL en a **deux** et les paires correspondantes ont des historiques identiques ; FULL en a **quatre**.
- **168/168 comités FULL** attribuent au vrai professeur une masse postérieure **≥0,99** avec seulement 512 observations passives (avec ou sans labels bruités).
- Les trois modes ont **zéro erreur propre** sur les prédictions retenues à **90% de confiance**, *tant que l'enseignant est bien dans la banque de quatre candidats* ; sur BLIND, **aucun désaccord entre candidats n'est accepté**.

| Visibilité TRAIN | Évaluations OOD | Couverture de la prédiction sélective ≥90% | Erreurs sur prédictions acceptées | Erreurs du MAP forcé |
|---|---:|---:|---:|---:|
| BLIND | 344 064 | **56,1942 %** (193 344 acceptées) | **0** | **75 360** (21,90%) |
| PARTIAL | 344 064 | **74,9419 %** (257 848 acceptées) | **0** | **43 108** (12,53%) |
| FULL | 344 064 | **100 %** | **0** | **0** |

La progression n'est **pas** celle d'une fonction devenue connaissable grâce à une « intuition miraculeuse ». Quand les données deviennent informatives, un posterior conditionnel devient identifiable ; quand elles restent ambiguës, le comité évite la prédiction sur les questions contestées.

### Le piège du prior historique et les deux baselines

Les Brier ci-dessous sont des pertes de probabilité sur les mêmes TEST propres. La référence **uniforme statique** n'apprend rien ; la référence **uniforme actualisée** incorpore la **même vraisemblance TRAIN** que le prior biaisé. Cette seconde référence a été instrumentée **après** le premier run, dans le respect de l'amendement conservé, et n'a modifié aucun résultat primaire.

| Visibilité | Prior biaisé [4,2,2,1], actualisé | Prior uniforme, actualisé | Mélange uniforme, statique |
|---|---:|---:|---:|
| BLIND | **0,120071** | **0,109515** | 0,109515 |
| PARTIAL | **0,069606** | **0,062645** | 0,109515 |
| FULL | **0,000000** | **0,000000** | 0,109515 |

Sur les professeurs BLIND équilibrés, le prior biaisé **aggrave** le Brier par rapport au vrai comparateur bayésien uniforme. Il aide quand **h0** est le vrai professeur (`Brier=0,054313` contre 0,109515 uniforme), mais nuit quand **h3** est vrai (`Brier=0,178577` contre 0,109515). Sous PARTIAL, le même effet subsiste sur la dimension latente non observable. Sous FULL, toutes les masses a posteriori convergent et les deux méthodes réussissent. **Aucun avantage universel d'un a priori de simplicité n'est établi**.

## 4. H4 — deux enseignants absents de la banque, opposés informationnellement

Dans **42 cas négatifs**, le véritable professeur est `g=h0 XOR x[r6]x[r7]` ; il **n'existe dans aucun des quatre candidats**. La tâche est différente de la sélection du bon h0..h3.

**VISIBLE (21 largeurs/graines)** : pendant TRAIN BLIND, les bits r6 et r7 évoluent librement. Le terme absent produit de nombreuses divergences observables. Le meilleur des quatre candidats a en moyenne **27,7251 %** d'erreur TRAIN, au-dessus du seuil d'inadéquation fixé **12,5 %**. L'alarme **rejette 21/21 cas**. Si l'on ignorait cette alarme, le comité continuerait néanmoins à faire des prédictions incorrectes hors support ; celles-ci sont consignées comme un contre-factuel et non comme une politique recommandée.

**CENSORED (21 autres cas)** : forcer **r6=0 pour tous les pas TRAIN**, avec le même générateur de bruit d'étiquettes. Dès lors `g=h0` exactement sur **l'intégralité des données accessibles** : même un learner parfait ne peut soupçonner la présence du terme caché à partir des réponses passives seules. L'alarme ne signale rien, **21/21 cas acceptés**, avec une erreur TRAIN moyenne de **6,4174 %**, compatible avec le bruit nominal 1/16. Sur les nouveaux contextes OOD r6,r7 deviennent libres, et le comité, parfois certain que les quatre candidats sont unanimes, se trompe **5 981 fois parmi 24 168 prédictions à confiance ≥90 %** (≈**24,75 % de risque sélectif**). **Tous les 21 épisodes CENSORED** manifestent au moins une erreur propre à confiance élevée.

La conséquence est fondamentale : **l'abstention sur les désaccords connus ne suffit pas si toutes les hypothèses disponibles omettent la vraie fonction**. Ce cas n'est ni une faute de calcul ni une incapacité à optimiser le prior ; c'est une limite de couverture du modèle et des observations. Sans interventions de découverte ou classes `UNKNOWN`, une confiance de 100 % peut être fictive hors de la banque.

## 5. Métrologie, reproductibilité et limites

Les deux runs complets ont reproduit exactement les mêmes counts primaires :
`A6_I2_FROZEN=504`, `A6_I2_STRATUM=126`, `A6_I2_TEST=504`, `A6_I2_NEGATIVE=42`, `A6_I2_REGIME=3`, puis `A6_I2_COMPLETE ... oracle=PASS`. Premier job [114811920996](https://github.com/Memorithm/SML-GENIUS/actions/runs/38249522352/job/114811920996), run corrigé avec baseline uniforme actualisée [114813702118](https://github.com/Memorithm/SML-GENIUS/actions/runs/38250054077/job/114813702118). La première tentative de compilation nécessitait les seuls ajustements `rustfmt` de trois fichiers ; l'ajout diagnostique ultérieur a entraîné deux ajustements `rustfmt` supplémentaires. Ni l'algorithme d'actualisation ni les paramètres des familles et des seuils n'ont changé après la première mesure.

```bash
cargo fmt --all -- --check
cargo check --locked -p sml-core --all-targets
cargo clippy --locked -p sml-core --all-targets -- -D warnings
cargo test --locked -p sml-core --lib anf_passive_committee
cargo test --locked -p sml-core --bin sml-passive-committee-bench
cargo run --locked --release -p sml-core --bin sml-passive-committee-bench
```

Le pipeline est **Rust natif sans `unsafe`**, CI GitHub-hosted Ubuntu24.04, toolchain 1.89 pinned, SHA exact, `permissions:contents:read`, checkout sans credentials persistants, aucun runner Memorithm/RemoteOps ni SDK NVIDIA propriétaire. Les `WideAnfGate` sont de vraies fonctions natives, mais le calcul actuel ne modifie que les **poids de quatre hypothèses déjà fournies** ; il n'apprend ni leurs monômes, ni les règles MOR, ni la mémoire Delta-KV.

**Conclusion, à conserver sans excès :** H1 **PASS** sur 126 strates, H2 **PASS** sur les 168 comités FULL, H3 **PASS mais seulement si la vérité est dans la banque**, H4 **PASS** en démontrant qu'une inadéquation observable est détectable (21/21), tandis qu'une inadéquation censurée ne l'est pas (21/21) et rend la prédiction confiante dangereuse (5981/24168). Aucun argument de type « intuition humaine », SML-25B, apprentissage de fonctions booléennes arbitraires, banque à 1 million de fonctions, vitesse GPU ou généralisation linguistique ne peut être dérivé de ces résultats.

**Priorité suivante indépendante :** permettre à un modèle de décider *quand enrichir la banque de fonctions*, via une petite quantité d'interventions ciblées ou de contrôles de couverture OOD, et mesurer le **coût en observations et le délai de détection** du changement de support. Un autre axe est de raccorder cette gestion épistémique à de véritables cycles SBG/MOR/Delta-KV plutôt qu'aux seuls déclencheurs Markov simulés.
