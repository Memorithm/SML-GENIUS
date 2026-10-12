# A6-C12f — Préenregistrement : robustesse par slot/taux de bruit d'état, identité bit-à-bit et certificat temporel

**Gel du protocole : 12 octobre 2026 ; avant toute modification de code A6-C12f et avant toute mesure A6-C12f.** Dépendance : A6-C12e [PR #496 fusionnée](https://github.com/Memorithm/SML-GENIUS/pull/496), merge `193e53f40024579eefc830b92966da40df32ea54`. Les seuils, budgets, graines, méthodes et critères ci-dessous sont gelés. Si une erreur d'implémentation impose une correction, documenter les commits et conserver les premiers logs négatifs ; aucune optimisation des paramètres après lecture de TEST.

## Problème

Le banc précédent a retrouvé les trois portes d'un anneau ANF N128 sous une **seule** injection de bruit persistant `state[127] ^= Bernoulli(1/256)`. Ce résultat ne qualifie ni la dépendance au **slot perturbé**, ni la sensibilité à un bruit plus fort. Les deux bancs précédents ont utilisé l'égalité de nombres d'erreurs sur TEST comme indicateur de concordance fonctionnelle ; or **deux réseaux différents peuvent avoir le même nombre d'erreurs avec des prédictions différentes**. Le présent banc doit calculer explicitement **le Hamming bit-à-bit des prédictions** sur le même flux TEST, indépendamment des labels.

H1. À bruit de transition faible `p=1/256`, l'apprenant HYBRID récupère-t-il les vrais trois sources et tables, et présente-t-il **zéro désaccord de prédiction** avec le modèle déterministe générateur sur TEST, pour **au moins 2/3 graines dans chacune des 2 tâches×2 panneaux×3 slots** (12 cellules) ?

H2. À bruit plus fort `p=1/64`, refaire la même mesure et publier le nombre de cellules passant ce même seuil, **sans recoder les échecs comme de la simple corruption de labels**. La récupération structurelle n'est pas garantie par l'hypothèse ; publier les erreurs, qu'elles aient ou non égalité de nombre avec le modèle déterministe. Les résultats ne démontrent pas un taux minimax ni une loi générale de robustesse.

H3. Produire un **certificat d'équivalence observationnelle causal** pour le décalage temporel connu `x3(t)=x4(t-1)` avec `x3(0)=0` et l'identité de la porte à la position j67 dans le réseau d'origine. Les réseaux natifs (j20/x2, j68/x3, j116/x4) et (j20/x2, j67/x4, j116/x4) doivent donner les **mêmes sorties bit-à-bit** sur 2 tâches×3 graines de flux TEST indépendants avec cette contrainte. Sous `x3(t)=x4(t-1) XOR Bernoulli(1/32)`, les deux réseaux doivent avoir **strictement des prédictions différentes** sur les 6 flux contrôles. Le certificat analytique n'est valable que sur ce domaine d'entrées, avec état initial nul et racine/readout figés. **Ne pas appeler cela une énumération de la classe d'équivalence**.

## Modèle générateur et données gelés

Deux familles comme A6-C12e, toutes deux représentables exactement par le factoriseur à trois portes :
- XOR³ : j20/x2 (truth6), j68/x3 (truth6), j116/x4 (truth6).
- AND→XOR→XOR : j20/x2 (truth8), j68/x3 (truth6), j116/x4 (truth6).

Anneau ANF avec N=128 bits d'état et racine/readout fixés, 5 raw input lanes par pas. **508 sources candidates** sont toutes les 127 positions internes j1..127 ×4 canaux x1..x4. Les paramètres vrais ne sont pas communiqués au learner. Entrées brutes indépendantes/uniformes SplitMix64, generator/labels via modèle `LocalAnfNetwork` natif.

Fixer la grille de stress `slot∈{0,68,127}` × `p∈{1/256,1/64}` = **6 modes**. À chaque pas et pour chaque voie bit-slicée, **après** la transition `teacher.step_lanes` et **avant** la lecture du modèle, injecter au seul slot choisi `state[slot] ^= mask` ; le nouvel état perturbé est conservé pour le prochain pas. `mask` Bernoulli(1/256) via AND de huit `next_u64()` indépendants, ou Bernoulli(1/64) via AND de six `next_u64()`. Le flux RNG dédié à la transition provient de `stream XOR 0xA6C1_2E22_5EED_0202`, indépendant des inputs et du label-noise. Comptabiliser **chaque bit d'état réellement inversé**, inclure les zéros, ne jamais qualifier une simple corruption de labels d'injection d'état.

Bruit **optionnel d'étiquettes** seulement sur le panel NOISY, probabilité Bernoulli(1/16) par AND de quatre `u64` issus de `stream XOR 0xA6C1_2B66_DADA_1177`. Les deux panneaux sont appariés sur le même stream d'inputs et de flips d'état, mais leur graine de base diffère selon `panel` comme A6-C12e. Ne pas prétendre que les 72 flux sont indépendants sur toutes les dimensions.

Graines (index 0,1,2) et stages repris de A6-C12e :
`(index+1) * 0xd134_2543_de82_ef95 XOR (task_index+1) * 0xbf58_476d_1ce4_e5b9 XOR panel_index * 0x9e37_79b9_7f4a_7c15 XOR stage` avec multiplication `u64` wrapping. Stages `TRAIN 0xa6c1_2b11`, `VAL 0xa6c1_2b22`, `TEST 0xa6c1_2b33`. TRAIN512, VALIDATION512, TEST4096 pas bit-slicés ×64 voies, racines de proxy observées sur 384 pas TRAIN t≥128. Seules les données TRAIN/VAL entrent dans l'apprenant et la sélection.

## Comparateurs, effectifs et budgets

Algorithmes **strictement inchangés** de A6-C12e :
- `W2-PAIR` : corrélation sur les **128 016 paires de positions distinctes**, top8 paires, extension à chaque troisième canal admissible, factorisation ANF commune (520 fonctions) et validation native top16.
- `SKETCH-MITM` : 48 passes de hachage de 14 bits, collision par complément XOR, préscore sur huit mots u64 de chaque candidat, top256 sur 384 mots TRAIN, top16 factorisées.
- `HYBRID` : union W2 + SKETCH, même factorisation/validation, pas de paramètres oracle.
- `W3-STREAM` : scorer exhaustivement les **21 336 000 triplets** sur les 384 mots TRAIN, top16 en streaming, même factorisation/validation, seulement sur **seed0 de chaque tâche×panneau×slot×taux**.

Nombre exact de flux TRAIN/VAL/TEST : **2 tâches×2 panneaux×3 graines×3 slots×2 taux=72 flux**. Les trois méthodes W2/SKETCH/HYBRID produisent **216 politiques** ; référence W3 sur **2 tâches×2 panneaux×3 slots×2 taux=24 politiques**, soit **240 politiques gelées AVANT la première génération du TEST**, puis **240 lignes TEST** et **72 jeux TEST distincts**. W3 ne couvre qu'une graine par cellule ; les écarts de coût W3/HYBRID seront calculés exclusivement sur leurs **24 flux appariés**. Ne pas parler de 240 réplications indépendantes.

## Validation primaire et métriques obligatoires

Pour chaque politique TEST, publier :
1. égalité des trois positions/canaux et des trois tables ANF **à l'identique** ; statut accepté sur VAL ;
2. Hamming bit-à-bit `D(proposal,det_teacher)` ET `D(committed,det_teacher)` entre prédictions des deux réseaux exécutés **librement depuis état zéro** sur les mêmes inputs TEST, **sans utiliser leurs labels** ; un même nombre d'erreurs TEST n'est jamais accepté comme preuve d'identité ;
3. `E(det_teacher,stochastic_labels)`, `E(committed,stochastic_labels)`, `E(proposal,stochastic_labels)`, ratio de corruption des labels et de flips d'état observés, par graine et mode ;
4. compte de paires probées, collisions/uniques de hash, 8-word et 384-word rescoring, nombre de structures ANF factorisées, temps de criblage et de factorisation CPU et coût total ;
5. équivalence causale témoin exact-lag : sur **6 flux indépendants propres** générés *après* gel de toutes les politiques, calculer le nombre exact de désaccords en prédictions entre le vrai circuit et son alternative j67/x4. Répéter sur **6 flux NEAR_LAG** à bruit Bernoulli(1/32), qui doivent avoir un désaccord strictement positif. Les témoins ne sont pas utilisés pour choisir des méthodes ; fournir le couple canonique et la condition initiale.

**Seuil H1 fixé avant résultats :** pour chacun des 12 croisements tâche×panneau×slot au taux 1/256, le HYBRID doit retourner les trois sources et tables d'origine et `D(committed,det_teacher)=0` sur **au moins deux des trois graines**. Une même erreur moyenne sur labels stochastiques n'est pas substituable.

**H2 :** mêmes mesures au taux 1/64, rapport des 12 cellules passant ou non le seuil 2/3 ; aucun reclassement post hoc pour sauver un résultat.

**H3 :** 6/6 égalités natives exact-lag sur entrée contrainte, **6/6 désaccords positifs NEAR_LAG**, et certification logique par la relation de retard `j68/x3 ↔ j67/x4` pour le composant XOR. Aucun résultat TEST de H1/H2 ne doit être utilisé pour modifier le témoin.

## Garde-fous

Rust **1.89** natif, runner GitHub-hosted `ubuntu-24.04`, `permissions: contents:read`, checkout du SHA exact sans identifiants Git conservés. Aucun runner Memorithm/RemoteOps self-hosted ni SDK NVIDIA. Résultats publics négatifs, aucune mise à l'échelle ANF massive, pas de mesures de modèles linguistiques, de k≥4/5 portes ni de preuve de coût sous-cubique en pire cas. Les flux TEST ne sont jamais utilisés dans l'algorithme d'entraînement/factorisation/sélection.

**Garde de merge :** code, protocole, 240 traces FROZEN/240 TEST, 72 comptes de bruit d'état, 12 témoins bit-à-bit, résultats et limites publiés, CI expérimentale et générale vertes sur le SHA final.
