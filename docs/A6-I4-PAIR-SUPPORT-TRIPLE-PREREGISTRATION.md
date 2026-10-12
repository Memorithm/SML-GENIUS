# A6-I4 — Préenregistrement : couverture des interactions ANF par paires, abstention et contre-exemple triple

**Gel du protocole le 12 octobre 2026, avant code, tests et première exécution A6-I4.** Source de départ `main` après fusion de [A6-I3 #505](https://github.com/Memorithm/SML-GENIUS/pull/505), SHA `aab8b49aaa14be29ae3e43d636bb530c15deb313`. Ce document est immuable au titre des hypothèses, seuils et graines ; toute correction ultérieure doit être consignée, conserver les premiers échecs et ne pas modifier les régimes après consultation des sorties TEST.

## Problème causal et falsification négative obligatoire

A6-I3 a validé une enveloppe marginale `seen-0/seen-1` par entrée, mais l'a falsifiée sur `x_i∧x_j` : chaque bit peut avoir pris les deux valeurs durant TRAIN sans que la **configuration conjointe (1,1)** ait jamais été observée. L'enveloppe n'a détecté aucune nouveauté sur ces entrées et a commis 43 008 erreurs confiantes.

I4 ajoute un **garde de support empirique pairwise** `AnfPairSupportGuard`. Il enregistre uniquement les **entrées passives TRAIN**, jamais les labels futurs ni les coefficients du vrai enseignant. Pour chaque paire d'indices `i<j`, il conserve quelles configurations `(0,0),(0,1),(1,0),(1,1)` sont apparues ensemble dans **au moins une** des 512 observations. Sur une entrée OOD, il peut refuser une prédiction si au moins une paire prend une combinaison auparavant absente. Cela est un **test de support**, non une preuve que la fonction cachée est dans les quatre candidats, ni une garantie de probabilité ou de calibration.

H1 : sur `PAIR_CENSORED_ABSENT` (enseignant absent `g=h0 ⊕ x[r6]x[r7]`; r6/r7 jamais ensemble à 1 pendant TRAIN), **42/42 ajustements** détectent toutes les **1 024** activations OOD de la paire censurée sur chacun des 2 048 TEST, éliminant les **43 008 erreurs confiantes** du comité qui seraient commises à ces activations. Le garde couvre les **43 008** entrées non censurées et ne commet aucune erreur sur les prédictions acceptées. Il faut vérifier que les deux valeurs marginales de r6 et r7 étaient observées : le bénéfice doit être **pairwise**, pas une alarme marginale triviale.

H2 : dans `PAIR_CENSORED_INCLUDED`, le professeur est vraiment `h0` présent dans la banque. Avec la même censure TRAIN, les **43 008** inputs OOD paire11 doivent être refusés malgré le fait qu'ils soient corrects. Les **43 008** autres sont acceptés sans erreur. Publier ce coût du refus inutile — la stratégie de couverture ne sait pas si la vraie fonction a changé.

H3 : construire un **contre-exemple triple obligatoire** `TRIPLE_CENSORED_ABSENT` avec professeur absent `g=h0 ⊕ x[r5]x[r6]x[r7]`, toutes les sept triplets de `{000,001,010,011,100,101,110}` vus pendant TRAIN, mais jamais 111. Chaque valeur d'une variable et **chaque paire de configurations** de r5,r6,r7 est néanmoins observée (y compris 11 pour les 3 paires). En TEST alterner 1024 contextes `r5=0` et 1024 contextes `r5=r6=r7=1` (les autres entrées Bernoulli1/2). Pour les **42/42 ajustements**, le garde ne doit détecter **aucune** nouveauté pairwise et doit laisser passer **43 008 erreurs confiantes** sur les 86 016 TEST : **un échec obligatoire**, pas un résultat à cacher. L'échec est informationnel : sous censure du triplet 111, `h0` et `g` sont exactement identiques sur le support TRAIN. Même le souvenir parfait de toutes les paires ne permet pas de constater la présence du terme d'ordre 3.

H4 : inclure des contrôles de couverture et d'adéquation de banque sans censure et sans teacher absent. Les scenarios `CENSORED_ABSENT` (r6=0 pendant TRAIN), `FULL_INCLUDED` (h0 réellement fourni) et `FULL_ABSENT` (g=h0 XOR r6&r7 observablement absent pendant TRAIN) doivent conserver leurs comportements mesurables, sans que les nouvelles métadonnées de paires simulent une reconstruction de coefficients.

## Corpus prospectif, séparation TRAIN/TEST et comparateurs

Sept largeurs `k∈{8,16,32,64,128,256,1024}` × 3 graines [0,1,2] × 2 panneaux TRAIN [clean, labels inversés Bernoulli1/16] × **6 régimes** = **252 modèles gelés**, 42 par régime. Chaque expérience TRAIN observe 512 étapes passives. **Tous les 252 modèles, supports et adéquations sont figés AVANT la création de la première entrée TEST**. TEST = **2048 contextes par modèle**, soit **516 096 prédictions** : première moitié 1024 contextes supportés, deuxième moitié 1024 contextes déclencheurs OOD, avec mêmes distributions de graines que les anciens bancs, mais régimes modifiés. Les paires propres/bruitées réutilisent les inputs TRAIN/TEST de la même configuration, donc **252 modèles ne représentent pas 252 flux de vérité indépendants**.

La banque est la même que dans A6-I3 : quatre vraies `WideAnfGate` `h0=x[r2]⊕x[r3]`, `h1=h0⊕x[r0]x[r1]`, `h2=h0⊕x[r4]x[r5]`, `h3=h0⊕x[r0]x[r1]⊕x[r4]x[r5]`. Positions `r0..r7` secrètes et permutées sur `0..k-1`; les candidats savent leurs propres positions, le garde de paires ne reçoit que **les k valeurs booléennes** de chaque pas. Les professeurs sont h0 si INCLUDED, ou g absent de la banque si ABSENT.

Six scénarios et règle TRAIN :
- `CENSORED_ABSENT` : r6=0 dans toutes les 512 observations ; professeur absent pair XOR ;
- `PAIR_CENSORED_ABSENT` : répéter `r6,r7∈{00,10,01}` sans 11 ; professeur absent pair XOR ;
- `PAIR_CENSORED_INCLUDED` : même censure des paires mais professeur h0 ;
- `TRIPLE_CENSORED_ABSENT` : `r5,r6,r7` parcourent modulo 7 tous les triplets sauf 111 ; professeur absent **triple** XOR ;
- `FULL_INCLUDED` : toutes les paires de r6/r7 présentes dans TRAIN, professeur h0 ;
- `FULL_ABSENT` : toutes les paires présentes, professeur absent pair XOR.

Entrées/labels générés par `SplitMix64`, graine `(index+1)*0xd134_2543_de82_ef95 XOR (scenario+1)*0xbf58_476d_1ce4_e5b9 XOR k XOR stage` avec `u64 wrapping`. Stages immuables : `HIDDEN=0xa614_1001_51ed_0001`, `TRAIN=0xa614_1002_51ed_0002`, `NOISE=0xa614_1003_51ed_0003`, `TEST=0xa614_1004_51ed_0004`. Le professeur absent et les TEST n'entrent jamais dans le garde ou le comité.

Le comité utilise `PassiveCommittee` natif, quatre hypothèses, prior `[4,2,2,1]/9`, flip de label supposé 1/16 et seuil de prédiction sélective **confiance ≥0,90**. Le garde ne peut répondre que si le comité accepte, que `min TRAIN mismatch ≤1/8` (`bank_adequate(0.125)`) et que **chaque combinaison de chaque paire de dimensions actives en TEST a été observée durant TRAIN**. Mesurer le comité seul, l'ancien `AnfMarginalSupportGuard` (baseline vérifiable) et le nouveau garde pairwise sur les mêmes TEST, pour chaque scénario. Un refus **ne compte jamais comme une prédiction correcte** : publier nombre de prédictions acceptées, erreurs parmi celles-ci et abstentions inutiles par scénario.

Pour les régimes pairwise, TEST alterne r6=0 (r7 aléatoire) et r6=r7=1. Pour `TRIPLE_CENSORED_ABSENT`, TEST alterne r5=0 (r6/r7 aléatoires) et r5=r6=r7=1. La censure des autres entrées n'est jamais forcée ; toutes les 4 configurations des autres paires sont largement couvertes.

## Implémentation Rust, mémoire et budget

Au TRAIN, conserver les **bits des observations** pour chaque coordonnée dans `ceil(max_train_steps/64)` mots `u64` : `512×1024` bits = **64 KiB** au k1024 pour les colonnes logiques. À `seal()`, calculer **les quatre combinaisons** de chaque paire à l'aide d'intersections de colonnes, dans des boucles Rust explicites. Ne conserver ensuite que la liste **compacte des paires dont au moins une combinaison est absente**, chaque enregistrement portant un masque de 4 bits. Ainsi, lorsque TRAIN couvre toutes les configurations, l'évaluation TEST ne parcourt aucune paire; lorsqu'une paire est censurée, elle est vérifiée. Ce n'est ni O(1) par entrée dans le pire cas ni une compression garantie sur toutes les distributions.

Budgets préfixés : `max_train=512`, `max_pair_slots=600_000` (**k1024 requiert 523776 paires**, la largeur 2048 est explicitement trop grande pour ce budget), `max_missing_pairs=8192`, `max_width=1024`. Échec fermé avant mutation si quota ou largeur incorrecte, si TRAIN vide, si `seal` échoue (ne pas installer de support incomplet), si TRAIN continue après `seal`, si nombre d'observations déphasé avec le comité, ou si une prédiction est demandée avant scellement. Le `logical_payload_bytes` doit distinguer colonnes bit-slicées, paires manquantes et scratch/transient ; aucune revendication sur RSS ou bande passante mémoire GPU.

L'API `AnfPairSupportGuard` est distincte du `AnfMarginalSupportGuard` existant. Aucun moteur d'ANF/WideSparse/MOR/Delta-KV existant n'est modifié ; aucune modification du posterior bayésien. `predict_guarded` doit retourner la prédiction du comité et les motifs d'abstention, pas seulement `None`.

## Garde scientifique de réussite

Les critères **ne demandent pas zéro erreur sur toute la population** : ils demandent H1 et H2 positifs, et **H3 négatif malgré le garde**. Les comptes primaires :
- 252 lignes `A6_I4_FROZEN`, 252 `A6_I4_TEST`, 6 `A6_I4_SUMMARY`, 1 `A6_I4_COMPLETE` ; 516 096 observations TEST ;
- PAIR_CENSORED_ABSENT : nouveau garde **43 008** acceptées, **0** erreurs, **43 008** cas de paires nouvelles ; ancien garde 86 016 acceptées / 43 008 erreurs ;
- PAIR_CENSORED_INCLUDED : nouveau garde **43 008** acceptées, **0** erreurs et **43 008 abstentions inutiles** ;
- TRIPLE_CENSORED_ABSENT : nouveau garde **86 016** acceptées, **43 008 erreurs à confiance élevée**, **0 cas de paires nouvelles** ;
- FULL_INCLUDED : nouveau garde 86 016 acceptées / 0 erreur ;
- FULL_ABSENT : nouveau garde 0 acceptées, 42/42 banques jugées inadéquates ;
- CENSORED_ABSENT : nouveau garde 43 008 acceptées / 0 erreur, 43 008 cas de nouveauté pairwise.

Conserver également les temps CPU de construction et de scoring, le nombre de paires marquées manquantes, la mémoire logique, les résultats négatifs, un contre-exemple unitaire exact où les sept triplets sont couverts et 111 invisible, et l'absence de faux claim sur l'identification d'une ANF cachée.

**Interprétation obligatoire :** l'identification de support d'ordre 2 déplace la frontière du défaut vers l'ordre 3 ; elle ne la supprime pas. Aucun garde d'ordre fixé ne prouve une bonne extrapolation sur des fonctions de degré arbitraire, et une banque fournie peut être incorrecte. Ce protocole n'est ni une expérience d'intuition humaine ni de SML-25B, ni un apprentissage passif des monômes eux-mêmes.

CI épinglée Rust1.89, runner GitHub-hosted Ubuntu24.04, checkout exact SHA, `permissions: contents:read`, `RUSTFLAGS=-D warnings`, aucun `unsafe`, service résident, runner privé ou SDK NVIDIA hors pilotes. **Ne pas fusionner tant que tous les résultats, négatifs compris, ne sont pas publiés et que `fmt`, `rust` et CI dédiée ne sont pas verts sur le SHA final.**
