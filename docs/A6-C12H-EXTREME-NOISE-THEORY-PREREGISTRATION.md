# A6-C12h — Préenregistrement aveugle : frontière d'identifiabilité bruit × échantillons

**Préenregistré avant toute implémentation ou tout résultat de A6-C12h, le 12 octobre 2026.** Ce protocole historique précède les PR #510 et #511 fusionnées le 12 octobre 2026 (respectivement 18:44:49 et 18:45:33 UTC). La fusion par instruction explicite ne constitue PAS une qualification expérimentale : les GitHub-hosted runs sur les heads fusionnés échouaient sans runner attribué ni étape exécutée. #510 conserve sa grille figée de 1232 politiques ; #511 apporte un banc distinct de 1872 politiques. **Aucun résultat de campagne complète indépendante n'est attesté par ces fusions.** Les hypothèses et critères scientifiques du protocole restent inchangés.

## Corrections et limite de l'approximation théorique

Le bruit de corruption i.i.d. des **étiquettes** est `η` : contrairement au modèle habituel à une seule étiquette, le proxy de cet enseignant récurrent XOR utilise `z[t] XOR z[t−128]`, donc **deux étiquettes bruitées indépendantes**. Leur XOR est inversé avec probabilité `η_proxy = 2η(1−η)`, d'où `ρ_proxy = 1−2η_proxy = (1−2η)^2`. À `η = 0.2`, `η_proxy=0.32` et `ρ_proxy=0.36`. Ceci ne constitue pas une loi universelle pour du bruit d'entrée, de l'état ou d'une tâche réelle.

L'espace effectivement admissible dans A6-C12g a **M=317440 triplets**, et non C(128,3) sans exclusion de même porte. Sous la **seule approximation exploratoire** « faux scores gaussiens à variance `1/N` et M tests comparables », le maximum absolu de bruit de fond vaut approximativement `sqrt(2 ln(2M))=5.1694` écarts-types et le crossover d'amplitude `a` est
`N* = 2 ln(2M) / a²`, avec `a_XOR≈ρ_proxy` et `a_AND≈ρ_proxy/2`. Le facteur 2, le recours à `2M`, la constante et le facteur 1/2 **ne sont pas des bornes probabilistes rigoureuses** : dépendances entre triplets, candidats partageant des colonnes, portes causales et validation/factorisation peuvent les invalider. Un écart aux chiffres ci-dessous est à publier, **pas à gommer par ajustement**.

| η label | η_proxy | ρ_proxy | crossover XOR (bits proxy) | crossover AND (bits proxy) |
|---|---:|---:|---:|---:|
| 0 | 0 | 1 | 27 | 107 |
| 1/5 | 0.32 | 0.36 | 206 | 825 |
| 3/10 | 0.42 | 0.16 | 1044 | 4175 |
| 2/5 | 0.48 | 0.04 | 16701 | 66806 |

Le dernier seuil AND est **hors champ** du budget max de 32768 bits, donc la censure est un résultat licite. Les valeurs sont des prédictions de *criblage*, pas des prédictions garanties de récupération ANF ou de succès TEST.

## Échantillons, enseignants, estimations avant TEST

Ne changer ni les architectures, ni les positions de sources, ni la grille à 128 candidats, ni les 64 voies bit-slicées, ni les hyperparamètres W3/HYBRID/Gauss/validation de C12g. Deux tâches strictes : XOR→XOR→XOR et AND→XOR→XOR. Corruption i.i.d. des étiquettes TRAIN/VAL/TEST à η = {0, 1/5, 3/10, 2/5}. Les budgets fixes de proxy sont **1, 2, 4, 8, 16, 32, 64, 128, 256, 512 mots u64** (64 à 32768 bits); TRAIN total est 128 + budget mots et les budgets sont des préfixes d'une même trajectoire, non des réplications indépendantes. VALIDATION = 640 pas; TEST = 4096 pas ×64 voies, générés *uniquement après avoir figé tous les fits*. La génération des bits de bruit à 30/40 % utilise une comparaison entière uniforme avec un seuil explicite (sans RNG floating point).

* **Grille principale** : graines 0–7, les 2 tâches ×4 bruits ×10 budgets ×2 méthodes W3/HYBRID = 1280 politiques ; Gauss sur bruit nul exclusivement, 2×8×10 = 160, donc **1440**.
* **Réplication ciblée théorique** : graines indépendantes 8–15 pour **trois budgets prédéterminés**, W3/HYBRID et Gauss uniquement en propre. Budgets en mots fixés **avant** la mesure : XOR η0 = {1,2,4}, η0.2 = {2,4,8}, η0.3 = {8,16,32}, η0.4 = {128,256,512} ; AND η0 = {1,2,4}, η0.2 = {8,16,32}, η0.3 = {32,64,128}, η0.4 = {128,256,512}. **384** W3/HYBRID et **48** Gauss supplémentaires.
* Total = **1872 modèles FROZEN**, **1872 TEST** sur **128 flux TEST distincts** (2 tâches×4 bruits×16 graines), **180 lignes SUMMARY** (10 budgets×[16 groupes W3/HYBRID +2 groupes Gauss]), **18 lignes THRESHOLD**. Un résultat local ou un run partiel ne saurait se substituer à ce recensement.

Les 16 graines ne sont pas 16 expériences totalement indépendantes sur tout le carré : seulement trois budgets par famille/théorie en ont 16. **Les prédictions théoriques ne doivent jamais être recalculées en fonction de TEST ni des observations A6-C12h.** Si les coûts sont trop importants, arrêter la campagne, publier ses traces comme *incomplètes*, conserver la matrice, et réduire l'implémentation/optimiser sans changer l'algorithme ni sélectionner des cellules favorables.

## Mesures et critères figés

1. Enregistrer pour chaque méthode/budget/graine les positions et vérités exactes trouvées, *proposed*, *accepted*, récupérations exactes, rang vrai top16, score vrai vs meilleur faux du top16 pour W3, marge signée, collisions/propositions Sketch, ainsi que `z_true=|r_true| sqrt(N)` et `z_false=|r_false| sqrt(N)`. Ce sont des **amplitudes normalisées par un écart-type nul de référence**, pas des scores-z calibrés ni des p-values sous dépendance. Publier aussi les valeurs de la ligne théorique `sqrt(2 ln(2M))=5.1694` et des deux `N*`.
2. Publier 3 durées brutes par fit et **médiane** des composantes screening/factorisation/total, sur chacun de trois jobs *GitHub-hosted indépendants*, jamais conclure à une supériorité matérielle sur la seule CI partagée.
3. TRAIN et VAL seuls peuvent apprendre/valider ; aucun oracle caché dans le ranker. TEST produit erreur d'étiquette et désaccord bitwise de l'état autonome, sur les mêmes entrées que l'enseignant. *Exact* + *accepted* + **zéro désaccord natif sur TEST** pour le succès strict.
4. Par groupe, publier les nombres succès/8 ou succès/16 selon la couverture programmée ; **Wilson 95 %**, aucune IC sur des observations de budgets corrélées comme si elles étaient indépendantes. Seuil empirique descriptif « au moins 7/8 » ou « au moins 14/16 », sans lui attribuer une inférence globale. Différence entre seuil théorique et premier budget franchissant ce critère est une mesure, censure si aucun franchissement. Les colonnes `n=16` sont *a priori* sélectionnées sur le seuil théorique, non sur les observations.
5. Le témoin zéro-bruit et la méthode Gauss sont des contrôles, pas une garantie : publier les contre-exemples et les performances non récupérées. Échec sur petits budgets, bruit extrême et problème AND > budget max **attendus comme possibilités**, jamais remaniés après lecture.

## Contrainte critique à `words=1/2/4`

Le sketch A6-C12g requiert actuellement au moins `SKETCH_COARSE_WORDS=8` et effectue un `assert`. Dans A6-C12h **seulement**, borner la profondeur coarse à `min(8, observed.p.len())`, garder `SKETCH_BITS=14` et exiger `64 × words ≥14`. Ajouter des tests Rust pour 1, 2 et 4 mots sans panic, absence de fuite d'oracle et recensement exact. Ne pas altérer les données ou la fenêtre C12g gelées.

## Garde de validation et limites

C12h est **une expérience nouvelle**, distincte de C12g ; en cas d'indisponibilité de runners, une mesure locale **étiquetée non indépendante** est recevable comme diagnostic, jamais comme CI/validation indépendante. Réclamer `cargo fmt --check`, `cargo check --locked`, Clippy warnings-as-errors, tests Rust et trois traces complètes GitHub-hosted au **SHA final**. Garder DRAFT et ne pas merger si aucune étape ne démarre. N'affirmer aucune performance SML-25B, généralisation linguistique, récupération LPN générique, kilobit ANF, million de fonctions ni résultat GPU.
