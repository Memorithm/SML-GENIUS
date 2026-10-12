# A6-I3 — Préenregistrement : enveloppe de confiance hors support et piège des interactions invisibles

**Gel du protocole le 12 octobre 2026 — avant toute ligne de code A6-I3, tout entraînement ou toute mesure.** Base : PR #504 fusionnée dans `main`, commit `5e22e38c0e402f6a6800df6a39ab911c21740f37`. A6-I2 a montré que des réseaux ANF natifs absents d'une banque de candidats peuvent créer des prédictions unanimement **fausses avec haute confiance** sous censure de TRAIN.

## Question falsifiable

Peut-on atténuer ce danger en refusant de prédire lorsqu'une coordonnée booléenne prend pour la première fois une valeur **jamais vue pendant TRAIN** ? Et que se passe-t-il quand chaque valeur individuelle a déjà été vue, mais qu'une **interaction entre deux coordonnées** ne l'a jamais été ?

Cette enveloppe est une vérification de **support univarié** O(k) mémoire, PAS une preuve de justesse sous distribution décalée. Le banc doit montrer simultanément :
- un **succès contrôlé** face à une activation nouvelle d'une dimension censurée ;
- un **échec obligatoire** face à une co-activation nouvelle de deux dimensions dont chacune a déjà pris les deux valeurs séparément ;
- un **coût de couverture** mesuré, y compris le refus de prédictions qui auraient été justes ;
- le rôle complémentaire du diagnostic de banque inadéquate lorsqu'une fonction manquante devient visible pendant TRAIN.

## Algorithme et séparation des données

Le nouveau composant `AnfMarginalSupportGuard` observe un flux TRAIN d'entrées booléennes `x_t∈{0,1}^k`. Pour chaque coordonnée `i`, mémoriser **uniquement** les deux bits `seen_zero[i],seen_one[i]` (carrier 8 bits/coordonnée pour éviter une représentation exponentielle). Un input TEST est **dans l'enveloppe** si chaque valeur de chaque coordonnée a déjà été observée au moins une fois pendant TRAIN. Une valeur nouvelle provoque l'**abstention**. Une paire nouvelle de valeurs **déjà observées séparément** n'est **pas vérifiée** par ce garde : il ne stocke ni combinaisons ni labels.

Adapter sans la réécrire la vraie API `anf_passive_committee::PassiveCommittee` de la PR #504. Après uniquement 512 observations de TRAIN appariées au garde, le comité dispose de quatre vrais `WideAnfGate`, d'un prior fixe `[4,2,2,1]`, d'un bruit de labels supposé `1/16`. L'acceptation d'une prédiction est la conjonction de **trois** conditions :
1. le vrai comité retourne une `selective_prediction` au seuil de confiance **0,90** ;
2. `committee.bank_adequate(1/8)` est vrai, selon les seules erreurs TRAIN ;
3. `guard.assess(input).novel_dimensions == 0`.

L'interface publique `guarded_predict` vérifie **avant évaluation** l'égalité des largeurs et du nombre d'observations TRAIN du garde et du comité. Erreurs de largeur, budget, état non entraîné ou confiance invalide : refus fail-closed, aucune mutation. Conserver **deux** mesures distinctes sur TEST : le prédicteur comité seul et le prédicteur avec garde+alarme, et la vérité du professeur **uniquement dans le banc post-TRAIN**.

## Générateur exact et panels figés

Sept largeurs `k={8,16,32,64,128,256,1024}`, trois graines `s=0,1,2`, deux panels de TRAIN `clean/noise1_16`, **cinq régimes** = **210 ajustements**, 42 fits par régime. Sous les 2 panels, les vérités sont appariées ; il y a **105 configurations width×seed×régime**, et non 210 structures statistiquement indépendantes.

Les huit positions physiques `r0..r7` sont obtenues par Fisher–Yates seedé depuis `HIDDEN=0xA613_1001_51ED_0001`, aucun index privilégié autre que ceux visibles dans les **quatre candidats fournis**. Banque native réelle :
- `h0=x[r2] XOR x[r3]`
- `h1=h0 XOR x[r0]x[r1]`
- `h2=h0 XOR x[r4]x[r5]`
- `h3=h0 XOR x[r0]x[r1] XOR x[r4]x[r5]`

L'enseignant soit est **h0 inclus dans la banque**, soit est **g=h0 XOR x[r6]x[r7]**, absent de la banque ; le terme absent `r6∧r7` ne fait partie d'aucun candidat du comité.

Cinq régimes fixés :
1. `CENSORED_ABSENT` : TRAIN contraint `x[r6]=0` toujours, enseignant absent g.
2. `CENSORED_INCLUDED` : même TRAIN contraint, enseignant inclus h0.
3. `PAIR_CENSORED_ABSENT` : TRAIN impose cycliquement (r6,r7)=(0,0),(1,0),(0,1), donc les deux coordonnées montrent **chacune leurs deux valeurs**, mais `x[r6]x[r7]` reste constamment 0. Enseignant absent g.
4. `FULL_INCLUDED` : TRAIN r6/r7 Bernoulli indépendants 1/2, enseignant inclus h0.
5. `FULL_ABSENT` : même TRAIN r6/r7 Bernoulli indépendants, enseignant absent g, donc les erreurs d'inadéquation doivent devenir observables.

Tous les autres bits d'inputs TRAIN sont Bernoulli(1/2) via un PRNG natif indépendant, `TRAIN=0xA613_1002_51ED_0002`. Chaque séquence est de **512 pas**. Label noise clean=0, noisy flip1/16 avec `NOISE=0xA613_1003_51ED_0003`. Seule la callback de l'enseignant qui produit les labels a accès à g/h0 ; garde et comité ne reçoivent jamais la classe vraie.

**TEST généré uniquement après gel des 210 fit/diagnostics** : `TEST=0xA613_1004_51ED_0004`, 2 048 entrées par fit, toujours **1 024** entrées `x[r6]=0` et `x[r7]` aléatoire (prédiction h0 et g identique), plus **1 024** entrées `x[r6]=x[r7]=1` (g est l'inverse de h0). Les autres variables suivent un flux Bernoulli 1/2 indépendant du TRAIN. Cette fenêtre équilibrée isole explicitement le coût d'abstention et les erreurs lors de la co-activation manquante. Les panels ont des seeds TEST partagés par (width,seed,régime), à signaler.

## Hypothèses et verdicts préfixés

- H1, sur les 42 `CENSORED_ABSENT` : le garde doit refuser les 1 024 entrées par fit à `r6=1`, accepter les 1 024 entrées `r6=0`, et avoir **zéro prédiction acceptée fausse** sur les 42. Le comité sans garde doit produire **au moins une erreur confiante** par fit sur les nouveaux contextes.
- H2, sur les 42 `PAIR_CENSORED_ABSENT` : le garde marginal doit **rater** la co-activation des deux caractéristiques, c.-à-d. accepter les 2 048 contextes par fit, et produire **au moins une erreur confiante** par fit ; ne pas cacher ce résultat négatif. Le témoin mathématique est que g≡h0 sur *toutes* les observations TRAIN possibles dans ce régime, pas seulement les 512 tirages.
- H3, les 42 `CENSORED_INCLUDED` : 1 024 prédictions `r6=1` doivent être rejetées **même si h0 est la vraie fonction partout**, pour mesurer la perte de couverture (faux refus). Les 42 `FULL_INCLUDED` : aucune nouveauté marginale et aucune erreur acceptée sur les 2 048 contextes/fits.
- H4, les 42 `FULL_ABSENT` : `bank_adequate(1/8)=false` sur les 42, et l'enveloppe doit refuser toutes les prédictions sous l'alarme de banque ; c'est un diagnostic de qualité TRAIN, pas une détection magique des fonctions manquantes à partir des inputs seuls.

Enregistrer 210 lignes `A6_I3_FROZEN` **avant de générer TEST**, 210 `A6_I3_TEST`, 5 `A6_I3_SUMMARY`, un unique `A6_I3_COMPLETE`. Chaque ligne publie nombre de contextes TEST, couverture comité seule, couverture garde, erreurs acceptées pour les deux, nombre de nouveautés marginales, adéquation de banque, taux d'inversion TRAIN observé, nombre de classes exactes des candidats, et largeur. Les erreurs que le garde ne peut détecter sont autant une part de l'expérience que les succès.

**Pas d'ajustement du seuil 0,90, 1/8, des séquences, des taux de bruit, des cinq régimes ni des seeds après lecture de TEST.** Si un test préfixé échoue, publier l'échec et corriger uniquement un défaut de code indépendant des données (avec commit et logs traçables), ou laisser la PR ouverte en explicitant la limite.

## Sécurité et portée

Rust pur sur GitHub-hosted `ubuntu-24.04`, Rust 1.89 épinglé, `permissions: contents: read`, checkout SHA exact sans identifiants persistants, `#![forbid(unsafe_code)]`, ni exécution privée Memorithm/RemoteOps, ni pile logicielle NVIDIA propriétaire. Formatage, check, Clippy `-D warnings`, tests unitaires et binaire RELEASE dédiés, et CI générale doivent être verts sur HEAD final avant fusion.

**Limite non négociable :** aucune surveillance marginale ne peut démontrer l'identité de l'enseignant ni un risque OOD universellement borné. Le contrôle PAIR_CENSORED_ABSENT est un **contre-exemple exact** à la prétention contraire. Ce travail n'ajoute ni monôme appris depuis corpus, ni banque d'un million de fonctions, ni évaluation SML-25B linguistique, ni intelligence consciente. Il renforce la séparation entre **confiance conditionnelle à la banque**, **support empirique des entrées**, **inadéquation mesurable** et **abstention**.
