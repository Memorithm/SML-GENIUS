# A6-WL3 — Préenregistrement : monômes ANF chevauchants, nombre inconnu et certificats de Möbius

**Gel : 12 octobre 2026, avant tout code A6-WL3 et toute mesure de ses configurations.**
Base : `main` après fusion de [A6-WL2 PR #501](https://github.com/Memorithm/SML-GENIUS/pull/501), commit `ec610b027af5300fadbca73ad22eb8f8b6321040`.

## Objectif : supprimer deux hypothèses de WL2 sans prétendre résoudre le problème général

Le prototype A6-WL2 apprenait exactement **quatre** monômes ANF positifs sur des supports **disjoints**, sans sélecteurs connus. Le nouvel apprenant reçoit seulement `k`, `max_terms=4`, `min_degree=2`, `max_degree=5` et des budgets ; **ni la cardinalité réelle (2,3 ou 4), ni les positions, ni la topologie des chevauchements** ne sont communiquées.

Modèle de données : `f(x)=⊕_{S ∈ T} ∏_{i∈S} x_i`, avec `T` ensemble inconnu de 2..4 monômes **positifs distincts** de degrés 2..5. Supports chevauchants, voire emboîtés, permis. Les quatre termes sont **un plafond et non la cardinalité véritable**. Toutes les données d'entraînement sont obtenues par interventions booléennes `membership(&[bool])`; pas d'accès au générateur, aux ensembles T, ni à VAL/TEST. La sortie est l'ensemble canonique **non ordonné** des supports complets et un vrai `WideAnfGate` utilisable dans les pages SBG sélectionnées. Cela reste de l'**apprentissage actif avec requêtes**, pas de l'apprentissage passif ou une solution de la recherche ANF arbitraire.

## Hypothèse mathématique et preuve de refus des faux monômes

À partir d'un témoin `f_res(x)=1`, l'algorithme de suppression de groupes peut se bloquer sur une coalition XOR de **trois monômes chevauchants**, de degré total ≤5. Exemple : `x0x1 ⊕ x0x2 ⊕ x0x3` est positif quand les quatre variables sont 1, mais supprimer individuellement l'un de x0,x1,x2,x3 rend la sortie nulle. Par conséquent, la seule **minimalité sous suppression** n'est plus une preuve suffisante qu'un support proposé est un monôme réel. WL3 ajoute un **certificat algébrique exact par la transformée de Möbius GF(2)** sur les `2^d` sous-ensembles d'un candidat de degré `d≤5` : la coefficient ANF calculé `c_U=⊕_{V⊆U} f_res(1_V)` vaut 1 si et seulement si le support `U` est un monôme du polynôme résiduel, sous oracle correct. Un support de coalition est **rejeté/corrigé** en faveur d'un coefficient exact de degré 2..5, jamais admis sans certificat.

Algorithme fixé avant TEST :
1. soustraire au label du même oracle les monômes déjà **certifiés** par XOR local pour former un résidu de requête, sans consulter T ;
2. générer des affectations uniformes Bernoulli(1/2) de largeur k avec un PRNG de proposition indépendant du générateur, rechercher un résidu positif, au plus **512** essais par monôme recherché ;
3. réduire l'affectation par suppression adaptative de groupes puis leurs sous-groupes, sans jamais accepter une modification où le résidu n'est pas 1 ; récursions au plus k variables avec une **seule** passe complète des groupes et une passe de stabilisation, sans paramètres sélectionnés par VAL/TEST ;
4. si le support restant contient 2..5 bits, interroger **tous ses sous-ensembles** et appliquer la transformée de Möbius booléenne ; retenir le **premier** monôme certifié de degré 2..5, trié par degré puis ordre lexicographique, absent des termes connus ; les candidats de degré hors plage, les certificats sans monôme, ou les supports déjà identifiés sont rejetés et l'algorithme tente un autre témoin ;
5. si 512 candidats uniformes n'ont pas donné de résidu positif, **déclarer arrêt probabiliste**, y compris avant les 4 termes maximaux ; si quatre termes sont déjà appris, il faut **encore** effectuer ce test résiduel : une réponse positive à ce stade provoque une erreur **`TooManyTerms`**, jamais une troncature silencieuse de l'ANF.
6. toutes les requêtes comptent dans un budget **max_physical_queries=160000**, y compris les répétitions, les certificats et les tests d'arrêt. Une réponse de callback Err est propagée **sans la remplacer par false/true** ; l'assignation temporairement réduite doit être restaurée si la requête échoue.

**Condition explicite du certificat d'arrêt** : pour un résidu GF(2) **non nul de degré ≤5**, la distance minimale du code Reed–Muller donne `P_{x~Bern(1/2)^k}[f_res(x)=1]≥2^−5`. Donc avec 512 témoins uniformes indépendants et un oracle exact, la probabilité conditionnelle de faux arrêt est au plus `(31/32)^512 ≈8.72×10^−8`. C'est une **borne probabiliste conditionnelle**, pas une preuve d'absence universelle de monômes : elle cesse de valoir si le bruit de l'oracle corrompt les décisions, si le degré réel dépasse cinq ou si les entrées ne sont pas uniformes et indépendantes.

## Populations figées

Sept fan-in `k∈{8,16,32,64,128,256,1024}`, trois familles de chevauchements, trois graines, deux panels de TRAIN :
- `STAR4` : 4 monômes avec **hub partagé**. Sur k8 : `{r0,r1},{r0,r2},{r0,r3},{r0,r4}`. Sur k≥16 : `{r0,r1},{r0,r2,r3},{r0,r4,r5,r6},{r0,r7,r8,r9,r10}`.
- `CHAIN3` : 3 monômes en **chaîne**. Sur k8 : `{r0,r1},{r1,r2},{r2,r3}`. Sur k≥16 : `{r0,r1},{r1,r2,r3},{r3,r4,r5,r6}`.
- `NESTED2` : 2 monômes **emboîtés** `{r0,r1}` et `{r0,r1,r2,r3,r4}` sur toutes largeurs.

Les positions `r0..r_{k-1}` proviennent d'une permutation Fisher–Yates uniforme pseudo-aléatoire seedée parmi les k entrées physiques, construite **dans le banc hors du learner**. Ainsi aucun `r_i` n'a une identité connue de l'apprenant ; le modèle est donné comme XOR canonique de véritables monômes ANF, pas comme une table dense. Les trois familles sont des instances délibérément structurées, et **ne représentent pas** une distribution naturelle de fonctions booléennes.

Total : **7 largeurs × 3 familles × 3 graines ×2 panels =126 fits** à geler **avant la première génération TEST**, sur **63 enseignants réellement distincts** (les panels propre/bruité partagent le même enseignant). Au k8 : degrés autorisés dans la grammaire fixée ; le nombre de monômes est inconnu mais sous plafond 4.

Graines indépendantes par (k,famille,seed) et stage :
- `HIDDEN=0xA6B1_3001_51ED_0001` ; `NOISE=0xA6B1_3002_51ED_0002` ;
- `VAL=0xA6B1_3003_51ED_0003` ; `TEST=0xA6B1_3004_51ED_0004` ; `PROPOSE=0xA6B1_3005_51ED_0005` ; `SBG=0xA6B1_3006_51ED_0006`.
- Mélange : `(seed+1).wrapping_mul(0xd134_2543_de82_ef95) XOR (family+1).wrapping_mul(0xbf58_476d_1ce4_e5b9) XOR k XOR stage`. La graine de PROPOSE est fournie au learner ; elle est indépendante du support caché.

Panel propre : une lecture par requête logique. Panel bruité : flip indépendant de label Bernoulli(1/16) par observation, **21** votes de majorité stricte par requête logique. Les 512 essais d'arrêt restent **512 requêtes logiques distinctes**, et consomment 10 752 requêtes physiques en panel bruité. Garder un budget maximum 160 000 sans reparamétrage après l'essai. Les tentatives de minimisation peuvent échouer ou coûter davantage ; publier le coût exact et les refus.

## VALIDATION, négatifs, TEST et intégration SBG

Après chaque fit mais sans raffinement, exécuter **544** observations VAL : 512 uniformes Bernoulli(1/2) à graine VAL distincte, **32** contrôles ciblés sur les vrais supports et leurs combinaisons (16 masques de sélection de supports en arrière-plan tous zéro, puis 16 en arrière-plan tous un). Les supports du générateur sont utilisés **uniquement par le banc pour VAL**, jamais par le fitter. Seul `val_errors=0` peut être accepté.

Contrôle négatif par largeur (**7 témoins**), enseignant `STAR4` seed0 avec un **cinquième monôme** `AND` de degré **6**, défini sur les six premières positions permutées. La borne de degré 5 est violée. Un refus d'apprentissage explicite ou **une erreur VAL strictement positive**, notamment le témoin structuré tout-un si observable, constitue un rejet. Publier si la cinquième fonction a été détectée en TRAIN ou par VALIDATION ; ne pas ajuster les échantillons après coup. Les supports négatifs doivent rester différents et ne pas simplifier algébriquement l'enseignant en polynôme de ≤4 termes de degré ≤5.

Après gel des 126 décisions VAL, produire **4096 entrées TEST indépendantes** (Bernoulli 1/2) par fit, soit 126 sorties `A6_WL3_TEST` et comparer **bit à bit** les prédictions de la `WideAnfGate` apprise à l'enseignant. Les deux panels par (k,famille,seed) partagent intentionnellement le même TEST ; les 126 lignes ne représentent pas 126 distributions indépendantes. Contrôle **exhaustif k8 : 18 fits ×256 configurations**.

Faire exécuter la **porte réellement apprise** du k1024 STAR4 seed0 TRAIN propre dans `WideAnfPageAtlas<1024>`, avec `MorDecision` explicite et **16 entrées SBG** comparées aux sorties de l'enseignant natif, sans confondre cette sélection fixe avec l'apprentissage de la route MOR.

**Critères primaires préenregistrés :** **126/126 monômes exacts et cardinalité exacte**, **126/126 VAL zéro erreur**, **126/126 TEST zéro désaccord**, **18/18 k8 exhaustifs**, **7/7 négatifs rejetés**, **16/16 sorties de page SBG**. Quel que soit le résultat, tous les échecs et temps de CPU sont rapportés. Les erreurs de code peuvent être corrigées avec provenance, sans changer d'algorithme, de graines ou de seuil à la lumière de TEST.

## Sécurité et limites

Rust 1.89 sur runner GitHub-hosted Ubuntu 24.04, `contents:read`, checkout exact SHA sans identifiants persistants, `RUSTFLAGS="-D warnings"`, fmt/check/Clippy/tests et binaire RELEASE. Aucun runner privé Memorithm, aucun SDK NVIDIA, aucune bibliothèque propriétaire autre que pilotes GPU autorisés, aucun code `unsafe`. Les anciens learners WL1/WL2, les pages MOR/Delta-KV et leur CI doivent rester verts.

Cette expérience **ne valide pas** des ANF à monômes négatifs, des interactions arbitraires de degré non borné, une estimation certaine de l'ordre sans hypothèse probabiliste, l'apprentissage depuis des séquences passives, ni un modèle SML-25B ou une banque d'un million de fonctions apprises. L'entraînement sur trajectoires autonomes/Delta-KV et les monômes de degré >5 sont des gates scientifiques séparés.

**Merge interdit** avant validation des 126 fits, rapport complet positifs/négatifs, et CI dédiée + CI générale vertes sur SHA final.
