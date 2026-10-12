# A6-C12e — Préenregistrement : alias temporels, perturbation de l'état et classes d'équivalence

**Date du gel : 12 octobre 2026, avant le premier code et avant toute mesure A6-C12e.**
Base : A6-C12d, PR #495 fusionnée au commit `3705e29e493d6d53f1f3f1eb8ea956e5a1e53e05`. Les seuils, graines, algorithmes, coûts et modèles sont fixés ici ; tout changement après exécution sera déclaré avec provenance comme amendement, sans effacer les échecs initiaux.

## Hypothèses et portée

Le système est **toujours un anneau de 128 états avec trois portes internes inconnues, une racine XOR et une lecture XOR connues**. Le choix de sources se fait parmi **508 caractéristiques** : 127 portes internes ×4 canaux bruts. Les règles du générateur, ses positions et ses canaux ne sont pas passés aux classeurs ni au factoriseur ; ils ne sont employés que pour générer les labels natifs et comme vérité rétrospective.

H1 : des **distracteurs décalés dans le temps mais non identiques** dégradent-ils la découverte des sources ? Avec 3 graines pour chacune des 4 cellules tâche/panneau, le critère de récupération structurelle exactement canonique (3 positions, 3 canaux et 3 tables ANF) de HYBRID reste préfixé à **≥2/3 dans chaque cellule**, plus erreur TEST égale à l'oracle déterministe sur le même split (labels éventuellement bruités).

H2 : l'égalité `x3(t)=x4(t-1)` crée une **équivalence entre une mutation j68/x3 et une mutation j67/x4** lorsque j67 est identité et les séquences sont initialisées à zéro ; de même `x1(t)=x2(t-1)` peut créer d'autres ambiguïtés. Une récupération du **libellé canonique** n'est donc pas identifiable dans le cas EXACT_LAG. Le critère primaire est (i) **12/12 égalités natives sur TEST propre** entre le générateur à j68/x3 et un générateur alternatif à j67/x4 pour chacun des 12 jeux tâche×panneau×graine, et (ii) publier sans réétiquetage la réussite de prédiction fonctionnelle de HYBRID sur 3 graines/cellule. Succès du modèle : erreur TEST exactement égale à l'oracle (pas nécessairement mêmes paramètres). Les deux réseaux doivent réellement différer par position et canal.

H3 : un bruit **effectivement injecté dans la transition d'état caché**, et non une corruption de label déguisée, change les prédictions d'un enseignant déterministe exact. Le code doit comptabiliser le nombre de flips d'état réalisés et l'erreur TEST du générateur déterministe sans flip contre les cibles avec flip. Publier les erreurs des 3 méthodes et du W3 de référence, même si elles sont proches du hasard. **Ne pas imposer la récupération structurelle comme critère primaire dans ce régime hors-grammaire stochastique**, ni déclarer ce « niveau d'oracle » un plancher informationnel démontré : la stochasticité cachée rend le modèle de données mal spécifié pour notre apprenant déterministe.

## Générateurs natifs et modes

Deux familles génératrices exactement représentables (sous absence de bruit de transition), positions et tables fixes pour contrôler la capacité :

- `xor_xor_xor` : (j20,x2,XOR6), (j68,x3,XOR6), (j116,x4,XOR6).
- `and_xor_xor` : (j20,x2,AND8), (j68,x3,XOR6), (j116,x4,XOR6).

Chaque pas reçoit cinq entrées binaires bit-slicées `u64`; base SplitMix64 inchangée de C12d, graine `(seed+1)*0xd134_2543_de82_ef95 XOR (task+1)*0xbf58_476d_1ce4_e5b9 XOR panel*0x9e37_79b9_7f4a_7c15 XOR stage`. TRAIN stage `0xa6c1_2b11`, VAL `0xa6c1_2b22`, TEST `0xa6c1_2b33`. PRNG distinct et déterministe pour alias temporels `stream XOR 0xA6C1_2E11_5EED_0101` et perturbation cachée `stream XOR 0xA6C1_2E22_5EED_0202`, indépendant des tirages de données et de bruit de labels. La probabilité nominale de corruption d'étiquette reste Bernoulli(1/16) via AND de quatre mots `u64`, uniquement sur les panneaux NOISY.

**NEAR_LAG :** pour t≥1, `x1(t)=x2(t-1) XOR e1(t)` et `x3(t)=x4(t-1) XOR e2(t)`, e1,e2 tirés indépendamment bit à bit Bernoulli(1/32) par cinq AND de mots. À t=0, l'entrée antérieure virtuelle est 0 et x1/x3 sont égaux à leur masque e correspondant. Les entrées sont corrélées sur un **retard temporel**, pas sur le même pas. Elles sont construites **avant** toute simulation du modèle natif. Comparer la véritable source j68/x3 à la substitution native j67/x4 : elle doit produire au moins une prédiction différente sur TEST propre dans chaque flux, attestant l'observabilité pratique (non une preuve générale d'unicité).

**EXACT_LAG :** pour t≥1, `x1(t)=x2(t-1)` et `x3(t)=x4(t-1)` ; t=0 x1=x3=0. Les réseaux natifs à mutations (j20/x2,j68/x3,j116/x4) et (j20/x2,j67/x4,j116/x4) doivent donner des sorties strictement identiques sur les mêmes flux propres : la substitution est une équivalence observationnelle d'une **position et d'un canal**, pas une réussite d'exactitude structurale. Les deux modèles sont indépendamment exécutés en récurrence complète.

**STATE_FLIP :** les cinq entrées restent indépendantes et uniformes. Après chaque appel de `teacher.step_lanes(...)` et **avant** la lecture native, appliquer `state[127] ^= mask` avec `mask` Bernoulli(1/256) lane-par-lane généré par AND de huit mots indépendants du PRNG `stream XOR 0xA6C1_2E22_5EED_0202`. L'état corrompu est porté à l'instant suivant : c'est une perturbation **persistante de la trajectoire cachée**, pas du readout. Mesurer le compte exact des flips injectés, les erreurs du modèle natif déterministe sur ces mêmes inputs/labels, la sélection autonome et les différences entre panels. Ni injection ni PRNG de perturbation ne sont passés à l'apprenant. La comparaison avec oracle propre sert d'**ancre**, pas de score de succès structurel obligatoire.

## Algorithmes et budgets inchangés

Réutiliser les trois classes de recherche de C12d **sans changement de paramètres** : W2 top8 paires parmi **128 016** paires admissibles et toutes les sources de troisième porte, SKETCH-MITM à **48 tours ×14 bits**, présélection sur 8 mots u64, top256 rescoring sur les 384 mots TRAIN et top16 factorisation, et HYBRID union des hypothèses W2+SKETCH. En aval, mêmes 520 fonctions ANF composées, validation native top16. Tous les classements/factorisations sont calculés sur TRAIN uniquement, tous les choix de retenue faits sur VAL.

**W3-STREAM** parcourt les **21 336 000** triplets admissibles au score complet (384 mots TRAIN), retient top16 en mémoire. Exécuter uniquement sur seed=0 de chaque tâche×panneau×mode : 2×2×3=**12** références. Ne jamais confondre les 12 références avec 36.

## Effectifs, séparation et décision

Trois modes ×2 tâches ×2 panneaux ×3 graines = **36 jeux TRAIN/VAL**. Trois méthodes W2/SKETCH/HYBRID sur les 36 jeux = **108 ajustements**, W3 sur les 12 seeds=0 = **12 ajustements**, soit **120 politiques gelées**. Après gel des 120 résultats TRAIN/VAL, générer **36 TEST** de 4096 pas×64 voies chacun ; les 120 lignes TEST réutilisent ces 36 flux. Les méthodes et modes restent appariés par familles/graines. L'unité expérimentale réelle n'est donc pas 120 répétitions indépendantes.

Pour chaque méthode×mode×tâche×panneau×seed, publier sources/règles exactes, retenue validation, erreur du réseau proposé, erreur du réseau sélectionné, erreur du générateur déterministe, flip-count d'état de la cible, valeur TEST oracle et preuve native de substitution j68/x3 versus j67/x4 sur entrée commune. Séparer explicitement :
1. identité de paramètres cachés ;
2. appartenance à la même **classe d'équivalence sur le support d'entrées** ;
3. erreurs prédictives sur TEST ;
4. modèle mal spécifié par une perturbation cachée.

Coûts : nombres de probes, collisions, candidats uniques, scores courts, scores longs, histogrammes ANF, temps écran, temps factorisation, total. Comparer HYBRID/W3 uniquement sur les 12 TEST **strictement appariés**, mais rapporter séparément 4 témoins W3 par mode ; toute perte de vitesse ou de précision est publiable. Aucun ajustement après lecture de TEST.

## Garde de sécurité et généralisation

Rust 1.89 pin, `ubuntu-24.04` GitHub-hosted, `permissions: contents:read`, checkout exact SHA sans `persist-credentials`; aucun runner Memorithm/RemoteOps self-hosted, aucun logiciel NVIDIA au-delà des pilotes GPU. Pas de modifications de moteurs SML productifs ; banc scientifique autonome et fichiers docs. Ne conclure ni à un algorithme sous-cubique dans le pire cas, ni à une récupération de graphes libres, k=4/5 portes, un million d'ANF, un gain GPU, la qualité linguistique ou la performance SML-25B.

**Fusion interdite avant CI générale et CI de banc vertes sur SHA final et publication honnête des résultats positifs/négatifs.**
