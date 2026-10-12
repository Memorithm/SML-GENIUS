# A6-C12d — Préenregistrement : distracteurs corrélés et identifiabilité native

**Protocole fixé le 12 octobre 2026, avant implémentation et avant les premiers résultats A6-C12d.**
Référence non modifiée : A6-C12c, PR #494 fusionnée au commit `348530a3fbf6c965355771a125995942f3157aa3`, résultats `docs/RESULTS-A6-C12C-SKETCH-MITM-508.md`.
Ne pas changer les critères ni les graines après consultation des résultats ; documenter explicitement tout amendement. Cette étape reste un diagnostic scientifique en Rust, sans promotion implicite en production.

## Hypothèses falsifiables

H1 — Avec des distracteurs de canaux **fortement corrélés mais non identiques**, le moteur hybride W2+Sketch-MITM conserve-t-il les positions, canaux et tables ANF exactes, et la même erreur que l'oracle natif, sur **au moins 2/3 graines par tâche/panneau** ?

H2 — Avec un canal **strictement dupliqué** `x1(t)=x2(t)` sur tous les temps et voies, l'origine canonique du premier gate (x2) est-elle **structurellement non identifiable** ? Il existe alors un générateur alternatif à même porte et même table lisant x1 ; les deux réseaux ont, par construction, les mêmes sorties sur tous les flux admissibles. La réussite est définie par **égalité fonctionnelle sur le TEST**, **pas** par identification exacte de l'étiquette arbitraire du canal. Vérifier la non-identifiabilité à l'aide de deux vrais réseaux natifs distincts et de leurs prédictions propres bit-à-bit.

H3 — Délimiter les gains de coût sans réutiliser la valeur « 35,06× » comme si elle restait acquise en présence de corrélations. Sur une même entrée appariée, mesurer écran W2, collisions MITM, rescoring, factorisation et coût total, puis comparer à W3 exhaustif. Toute dégradation doit être reportée.

## Grammaire et données préfixées

Topologie annulaire native **N=128**, racine et lecture fixes comme A6-C12c, **trois portes** uniquement. L'apprenant ne reçoit ni la vraie position ni les canaux ni les tables des portes. Deux générateurs représentables :

- `xor_xor_xor` : j20/x2 XOR, j68/x3 XOR, j116/x4 XOR.
- `and_xor_xor` : j20/x2 AND, j68/x3 XOR, j116/x4 XOR.

Les entrées brutes x2 et x4 sont tirées indépendamment et uniformément par SplitMix64. Modifications de la distribution d'entrées faites **avant simulation du générateur natif**, donc les cibles reflètent effectivement ces distributions :

**NEAR_ALIAS** : `x1(t)=x2(t) XOR e_1(t)`, `x3(t)=x4(t) XOR e_2(t)`, avec deux masques indépendants bit à bit Bernoulli(1/32) issus d'un générateur de corrélation séparé, graine dérivée par XOR du seed de flux avec `0xA6C1_2D00_19A5_1133`. Tous les autres canaux restent tirés comme A6-C12c. Le root fixe utilise le vrai x1, comme l'apprenant.

**EXACT_ALIAS** : `x1(t)=x2(t)` exactement sur toutes les voies et tous les temps ; x3 et x4 restent indépendants comme avant. **Les sources j20/x1 et j20/x2 sont exactement interchangeables sur le domaine d'entrées du banc.** Le fait de connaître le générateur ne permettrait pas de les distinguer à partir des sorties ; tout comptage de récupération stricte doit être secondaire et annoté « non identifiable ».

Les graines de base, la génération native, le bruit des étiquettes Bernoulli(1/16), et les stages TRAIN `0xa6c1_2b11` / VAL `0xa6c1_2b22` / TEST `0xa6c1_2b33` sont repris de C12c ; l'algorithme ne dispose pas d'informations privées du générateur. Un RNG séparé pour les masques de corrélation empêche leur dépendance aux tirages de bruit des étiquettes. Les variantes NEAR_ALIAS et EXACT_ALIAS sont **appariées par les graines brutes**, donc les 24 flux ne doivent pas être présentés comme 24 réplications indépendantes.

## Méthodes et budgets conservés

Rejouer **inchangés** les trois méthodes W2-PAIR, SKETCH-MITM et HYBRID sur **toutes les 508 caractéristiques possibles** : j1..127 × quatre canaux. SKETCH = 48 tours, 14 bits par tour, 8 mots u64 de présélection, top256 de correlation complète puis top16 de factorisation. W2 = toutes les **128 016 paires admissibles**, top8 puis extension par toutes les sources de troisième porte, même facteur ANF 520 masques et top16 exécuté en autonomie. HYBRID = union de ces deux propositions avant factorisation/validation. W3-STREAM exhaustif = **21 336 000 triplets entièrement scorés** ; échantillon de référence fixé à **seed 0 par tâche × panneau × mode**, soit 8 flux, avec règles/temps et erreurs publiés. Aucun ajustement post hoc de top8/top16, des 48 tours ou du nombre de bits de l'esquisse.

## Séparation, mesures et critères

Deux tâches × deux panneaux clean/noise1/16 × trois graines × deux modes = **24 configurations TRAIN/VAL**, appariées 2×2 au niveau des sources aléatoires. TRAIN512, VAL512, TEST4096 pas ×64 voies ; observations de criblage t≥128.

W2+SKETCH+HYBRID sur les 24 configurations = **72 politiques**, W3 exhaustif sur les 8 seeds0 déclarées = **8 politiques**, soit **80 décisions gelées avant génération des TEST**, puis 80 lignes de résultats sur 24 flux TEST mode/tâche/panneau/graine. Toutes les comparaisons de sélection sont faites sur VALIDATION sans accès au TEST ; TEST n'entre jamais dans le score ni dans les seuils. Ne pas prétendre à 80 essais indépendants.

Pour chaque configuration publier : vrais paramètres retrouvés (même s'ils ne sont pas uniques sous EXACT_ALIAS), proposition native, retenue sur validation, erreur autonome TEST et oracle; preuve par seconde réalisation native à règles `j20/x1` versus `j20/x2` appliquée aux mêmes **entrées propres** (sans effet du bruit des étiquettes), son erreur de discordance, nombre de collisions, triplets uniques, scores courts, scores longs, nombre d'histogrammes ANF, CPU temps écran et factorisation.

**Critère principal H1 NEAR_ALIAS** : HYBRID doit obtenir positions+sources+trois vérités exactes et TEST identique à l'oracle natif sur **≥2/3 graines par tâche et panneau** (quatre cellules de trois). Aucune modification de cette règle après RUN.

**Critère principal H2 EXACT_ALIAS** : 12/12 couples de générateurs natifs vrai/alternatif doivent produire zéro différence sur le TEST propre. Une éventuelle erreur de récupération des étiquettes de sources n'est **pas** considérée comme échec lorsque deux modèles sont observablement équivalents. Reporter séparément succès/faille de prédiction TEST HYBRID dans chaque cellule de trois graines ; on ne présume pas qu'il saura retrouver une représentation équivalente.

**Critère coût H3** : publier moyenne et dispersion par méthode et mode, calculer ratio apparié de W3/HYBRID uniquement sur les huit mêmes données déclarées. Les coûts sont du fit TRAIN+VAL, pas des tokens/s ni des performances d'inférence. Tout échec sous modes corrélés est un résultat scientifique, pas un prétexte pour écarter des graines.

## Sécurité et limites

CI isolée GitHub-hosted Ubuntu 24.04, Rust **1.89** pin, checkout SHA exact sans persist-credentials, `permissions: contents:read`. Pas de runners privés, RemoteOps restant plan de contrôle de référence mais non-exécuteur du code PR non approuvé. Aucun SDK NVIDIA ni logiciel GPU propriétaire. Les résultats demeurent un anneau connu à trois portes avec 508 sources **candidates**, pas un graphe libre à 508 états, ni k=4/5, ni une preuve d'une banque ANF d'un million de fonctions. Pas d'affirmation sur SML-25B/perplexité/GPU.

**Garde de fusion :** aucune fusion avant CI dédiée et générale vertes, résultat positif ou négatif documenté avec provenance, et traitement explicite de la non-identifiabilité.