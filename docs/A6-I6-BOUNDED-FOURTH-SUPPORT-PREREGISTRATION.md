# A6-I6 — Préenregistrement : garde d'interactions d'ordre quatre à budget borné, bénéfice et zones aveugles

**Gel 12 octobre 2026, avant code A6-I6 et avant toute sortie expérimentale A6-I6.** Base `main` : PR #507 fusionnée, SHA `083318560b576fd07c6d32338750def589c57cd8`. Les critères restent inchangés après lecture de TRAIN/VAL/TEST ; toute correction purement technique sera traçable.

## Question et écueil de sécurité

A6-I5 a prouvé qu'un garde couvrant exactement TOUS les triplets peut accepter **7 680 prédictions fausses et confiantes** causées par un seul terme ANF d'ordre quatre invisible pendant TRAIN. Pousser le calcul exhaustif à tous les quadruplets pour `k=1024` coûterait `C(1024,4)=45 545 029 376` tuples, incompatible avec les budgets pratiques.

Une solution ciblée possible est une **mémoire d'interactions enregistrées** : un ensemble `W` de quadruplets connus *avant* l'évaluation, représentant des interactions suspectées à partir d'informations publiques antérieures, et non déduites des labels du professeur aveugle. L'algorithme retient pour chaque quadruplet les **16 configurations booléennes réellement observées** pendant TRAIN. Il s'abstient lorsque la requête présente une configuration enregistrée mais jamais observée. **Cela protège exclusivement les quadruplets déclarés** ; une fonction vraie absente de la banque peut dépendre d'un quadruplet extérieur à W (et l'échec doit rester visible).

## Composant et contrats

Nouveau module opt-in `sml_core::anf_budgeted_four_support::AnfBudgetedFourSupportGuard` :
- `new(width, watched:Vec<[usize;4]>, max_watches, max_train_samples)`. `width∈[8,1024]`, nombre de quadruplets de 1 à `max_watches≤16`, indices dans `0..width`, quatre distincts par quadruplet, tuples canonicalisés et aucun doublon. Une watch list explicite reste un **a priori fourni au garde**, jamais un support appris ni une interprétation intrinsèque de l'ANF.
- `observe(input)` pendant TRAIN enregistre pour chaque quadruplet un `u16` de configurations vues ; refuse largeur invalide, dépassement de 512 pas et toute mutation après `seal()`.
- `seal()` refuse TRAIN vide et double scellement, ne modifie pas les cartes de couverture si échec.
- `assess(input)` après gel relève le nombre de configurations de tuple encore inconnues pour la requête ; ne lit **aucun label TEST**.
- `predict_guarded(committee,input,confidence=0.90,bank_mismatch_max=1/8)` exige un vrai `PassiveCommittee` de même largeur et même historique TRAIN, et accepte uniquement si le comité lui-même fournit une prédiction sélective, la meilleure erreur empirique de banque sur TRAIN ne dépasse pas 12,5 %, et toutes les configurations surveillées sont déjà couvertes.
- Budget mémoire exact de la **charge utile logique du garde** : un tableau d'au plus 16 tuples ([usize;4], masque u16), aucun historique INPUT k×512, pas de C(k,4), et aucune variable globale/worker résident. Ne pas annoncer le `size_of` des structures comme consommation RSS.

## Plan gelé d'expérience et témoins

Sept largeurs `k∈[8,16,32,64,128,256,1024]`, trois graines 0,1,2, deux panels TRAIN (labels propres vs Bernoulli1/16 indépendamment bruités), quatre régimes : **168 fits (=84 générateurs de tâche distincts, panels appariés)**. Pour chaque largeur/graine, une permutation Fisher–Yates seedée des **huit rôles publics** `r0..r7` parmi k entrées physiques est utilisée. Le quadruplet surveillé est TOUJOURS `W=[r4,r5,r6,r7]`, fourni comme **historique de risque préalable** indépendamment du vrai choix de professeur. Le comité reçoit quatre fonctions ANF natives `h0 = r0 XOR r1`, `h1=h0 XOR r0r1`, `h2=h0 XOR r2`, `h3=h0 XOR r3`, avec prior `[4,2,2,1]/9`, bruit supposé 1/16 et au plus 512 observations. **La vraie fonction manque délibérément dans trois des quatre régimes.**

Régimes prereg :
1. `WATCHED_ABSENT` : vrai `g=h0 XOR (r4r5r6r7)`, non représenté dans le comité ; TRAIN censure `r4r5r6r7=1111`. TEST contient 256 cas activant `1111` et 256 cas ne l'activant pas. **Garde doit refuser les 256 réponses fausses** mais laisser passer les 256 réponses correctes.
2. `WATCHED_INCLUDED` : vrai `h0` figure dans le comité, TRAIN censure encore `r4r5r6r7=1111`. **Garde doit inutilement refuser 256 cas pourtant corrects**, ce qui fournit la mesure du coût d'abstention.
3. `UNWATCHED_ABSENT` : vrai `g=h0 XOR(r0r1r2r3)`, non représenté ; TRAIN censure ce quadruplet caché mais couvre **TOUS les 16 patterns** de la watch list `r4..r7`. TEST : 256 activations et 256 non-activations du quadruplet caché. **Garde doit accepter 512 cas, dont 256 erreurs confiantes**.
4. `FIFTH_UNWATCHED_ABSENT` : vrai `g=h0 XOR(r0r1r2r3r4)`, non représenté et de degré cinq ; TRAIN censure ce quintuplet mais couvre **TOUS les 16 patterns** de la watch list. TEST : 256 activations et 256 non-activations. **Garde doit accepter 512 cas dont 256 fausses prédictions**, sans revendiquer une solution générale.

**Construction de TRAIN** : pour chaque régime/seed, parcourir tous les **256** motifs des huit rôles sauf ceux qui activent la combinaison censurée (16 cas pour quadruplet, 8 pour quintuplet), puis répéter ce cycle de motifs admis jusqu'à **512** observations. Les `k−8` autres dimensions reçoivent des bits pseudo-aléatoires indépendants. Ainsi, tous les patterns de la watch list sont vus dans les régimes négatifs, tandis que 1111 est réellement absent en régimes surveillés. Ni le vrai professeur ni les bits de label ne servent à la mise à jour du garde. La différence entre le comité inclus et absent est inaccessible pendant TRAIN dans les censures.

**TEST**, généré seulement après gel des **168** ajustements du comité et des gardes, a 512 requêtes OOD par fit, soit **86 016** décisions OOD. Les cas pairs imposent la combinaison cachée désactivée (son premier bit=0) ; les cas impairs l'imposent entièrement activée. Les autres bits et la source bruit sont générés par RNGs distincts. Les deux panels d'une même largeur/graine/régime partagent les mêmes entrées TEST (84 séries uniques).

Le module fait uniquement le suivi de 1 tuple à quatre variables, pour ce banc. Une unité de couverture se rapporte au `mask u16` à l'intérieur du tuple enregistré, pas à une promesse de couverture tous les 45 milliards de quadruplets. Aucun oracle de label ni vérité de terme caché n'est fourni au garde.

## Critères primaires H1-H4

Par régime, il y a **42 fits×512 = 21 504 prédictions OOD**. Les statistiques de prédiction utilisent la véritable fonction cachée **uniquement rétrospectivement** :
- H1 `WATCHED_ABSENT`: comité répond 21504, erreurs 10752 ; garde accepte **10752, zéro erreur**, 10752 inputs OOD nouveaux.
- H2 `WATCHED_INCLUDED`: comité répond 21504 sans erreur ; garde accepte **10752, zéro erreur**, avec **10752 abstentions inutiles**.
- H3 `UNWATCHED_ABSENT`: comité répond 21504, erreurs 10752 ; garde accepte **21504, erreurs 10752**, novelty=0. Cet échec obligatoire invalide toute garantie d'intégrité de banque par watchlist.
- H4 `FIFTH_UNWATCHED_ABSENT`: mêmes **21504 acceptées, 10752 erreurs confiantes, zero novelty**, démontrant l'échec hors ordre enregistré.

Publier les résultats par width/seed/panel, erreurs, abstentions et budgets, avec une ligne `A6_I6_FROZEN` par fit (168), une ligne `A6_I6_TEST` par fit (168), quatre `A6_I6_SUMMARY`, et une `A6_I6_COMPLETE`. Le workflow exige ces effectifs et **les échecs H3/H4 exacts**, non 0 erreur universelle. Refaire chaque expérience avec les vrais modèles `PassiveCommittee`; autoriser un résultat négatif si les seuils de confiance ou de bruit cassent une équivalence et le documenter honnêtement, sans changer seeds/priors/budgets pour obtenir une histoire positive.

## Critère de sécurité et suite

Rust 1.89 sur Ubuntu 24.04 GitHub-hosted, `contents:read`, SHA PR exact, credentials non persistants, Clippy `-D warnings`, pas de runner Memorithm self-hosted, pas de SDK NVIDIA, pas d'unsafe.

La watchlist est une **politique de risque spécialisée sur quelques combinaisons**, pas une garantie distribution-free ni une véritable intuition humaine. A6-I6 ne démontre pas la découverte d'une ANF absente du comité, ni la calibration sous shift, ni la qualité SML-25B ni une banque de 1 048 576 fonctions. Une **vraie expérimentation d'interventions actives, avec budget mesuré et sans divulgation du support caché**, demeure nécessaire pour résoudre la difficulté d'une fonction que tous les candidats ignorent.
