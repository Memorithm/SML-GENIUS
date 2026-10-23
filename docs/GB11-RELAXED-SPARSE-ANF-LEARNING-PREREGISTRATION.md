# GB-11 — apprendre des banques ANF parcimonieuses par relaxation, sans perdre le gain du bit-slicing

**Préenregistrement gelé le 23 octobre 2026, avant tout entraînement GB-11.** Ce document fixe la méthode, la matrice, les métriques et les critères de décision du pilote P0. Les phases P1 (corpus réel contre le témoin de 15 M paramètres) et P2 (mesure native Dell/Thor des banques apprises) sont seulement esquissées : elles demandent l'accord du propriétaire avant lancement.

Numérotation : GB-10 est réservé par GB-6 à l'architecture sans attention et GB-8B à l'étude de qualité modèle. GB-11 fournit la méthode d'apprentissage dont GB-8B a besoin ; il ne qualifie pas GB-8B.

## Question

Les décisions booléennes ne sont pas différentiables. Peut-on apprendre ensemble **la représentation** (quels bits d'entrée forment l'adresse d'une banque), **le routage** (quelle banque traite l'exemple) et **les experts** (les fonctions ANF de la banque) par une relaxation continue, puis binariser, en obtenant un modèle booléen final qui :

1. garde la qualité du modèle relaxé ;
2. reste **parcimonieux en ANF** (k ≤ 8 monômes par porte, régime où GB-9 mesure un gain moteur) ;
3. garde le **partage d'entrées** (R portes d'une banque lisent la même adresse, donc une seule transposition) ;
4. s'exécute **exactement** dans `Pvp6Bank` (bras `Selected` et `Bitsliced`) ?

## État des lieux du code existant (constat, 23 octobre 2026)

| Composant | Ce qu'il apprend | Limite pour GB-11 |
|---|---|---|
| `train.rs` (SML-0.3) | softmax sur les 16 tables d'une porte à 2 entrées | 2^(2^n) tables : impossible au-delà de n = 2 ; aucune notion de densité ANF |
| `circuit.rs` (SML-0.4) | identités de portes, câblage fixe | câblage non appris |
| `routing_train.rs` (SML-0.6) | bits de route par sigmoïde, cibles de page fournies | routage supervisé par une cible, pas appris par la tâche |
| `boolean_integral.rs`, `boolean_learning.rs`, `anf_local_learn.rs` | flips de bits durs par interventions XOR / recherche locale, lecture par Möbius | sans gradient ; la lecture par Möbius d'une table ajustée est dense en général |
| `learned_mor_anf_recurrent.rs`, `sparse_mor_anf_recurrent.rs` | poids flottants sur monômes **fixés**, porte MOR souple | les monômes ne sont pas appris, la sortie n'est pas un circuit booléen |
| `sparse_gradient_atlas.rs` (témoin S5h, ≈15 M paramètres) | poids scalaires f32 routés par hachage fixe | modèle non booléen ; routeur non appris |
| `pvp6_executor.rs` (`Pvp6Bank`) | — (exécution) | **densité uniforme exigée par banque** et **porte à 0 monôme refusée** |

Conséquence : aucun chemin actuel ne produit une `Pvp6Bank` à partir d'un apprentissage, et aucun ne mesure la densité ANF après binarisation. La contrainte de densité uniforme oblige à découper une banque apprise en sous-banques de même k, chacune avec sa propre transposition ; GB-11 compte ces sous-banques au lieu de modifier le contrat.

## Méthode

Entrée x ∈ {0,1}^d. Chaque banque possède n créneaux d'adresse ; chaque porte est une fonction de ces n bits (D = 2^n).

**Représentation (sélection d'entrées partagée).** Le créneau j d'une banque porte des logits L_j ∈ ℝ^d. En entraînement, a_j = Σ_i softmax(L_j/τ)_i · x_i, avec τ recuit géométriquement. En inférence, a_j = x_{argmax L_j}. Toutes les portes d'une banque lisent les mêmes a_j : **le partage d'entrées est imposé par construction**, il n'est pas espéré.

**Experts, relaxation ANF native (« XOR probabiliste »).** Chaque porte a un logit θ_m par monôme m ⊆ {1..n}. Les valeurs de monôme sont v_m = Π_{j∈m} a_j. Avec p_m = σ(θ_m), la sortie relaxée est

  P(y = 1 | a) = ½ · (1 − Π_m (1 − 2 p_m v_m)),

qui est la probabilité exacte que le XOR de bits indépendants Bernoulli(p_m v_m) vaille 1 (lemme d'empilement). Pour a et c = 1[p ≥ ½] binaires, elle coïncide avec l'ANF dure. Les gradients sont analytiques (produits exclusifs préfixe/suffixe). L'espérance du nombre de monômes est Σ_m p_m : la densité est donc régularisable **dans la base même où se mesure le coût matériel**.

**Coût régularisé.** λ · moyenne_portes Σ_m p_m · (1 + |m|), soit le nombre attendu de monômes pondéré par le coût d'un monôme dans le noyau bit-slicé (|m| − 1 AND et un XOR, arrondi à 1 + |m|).

**Binarisation.** Les derniers 25 % des époques utilisent l'estimateur straight-through : propagation avant avec c_m dur, rétropropagation par σ′(θ_m). La sélection d'entrées reste douce mais τ atteint 0,05.

**Routage.** Le routeur est lui-même une petite banque booléenne : ⌈log2 B⌉ portes sur sa propre adresse apprise, relaxées comme les experts. π_b(x) = Π_j r_j^{b_j} (1 − r_j)^{1−b_j}. La sortie relaxée est le mélange Σ_b π_b P_b ; en inférence, la banque choisie est celle des bits durs du routeur (top-1, une seule banque évaluée). Un terme d'équilibrage β · Σ_b π̄_b log π̄_b (π̄ moyenne du mini-lot) évite l'effondrement.

## Bras (tous avec la même sélection d'entrées et le même routage)

| Bras | Experts | Densité |
|---|---|---|
| `lut_ste` | LUT apprise : logit par entrée de table, extension multilinéaire, STE, puis Möbius GF(2) → ANF | non contrôlée (contrôle « LUT apprises puis binarisées ») |
| `anf_xor` | relaxation ANF native | λ = 0 |
| `anf_cost_s` / `anf_cost_m` / `anf_cost_l` | relaxation ANF native | λ ∈ {3·10⁻⁴, 10⁻³, 3·10⁻³} |
| `anf_concrete` | relaxation ANF native avec bruit logistique (Concrete binaire / Gumbel-sigmoïde, température 1 → 0,3) puis STE | λ = 10⁻³ |

Le routeur suit le bras de ses experts (LUT pour `lut_ste`, ANF sinon).

## Matrice fixe du pilote P0

- d = 32 bits iid uniformes, n = 6 (D = 64, plus petit domaine `Pvp6Bank`), B = 4 banques, R = 8 portes par banque, routeur à 2 portes.
- Deux professeurs figés (graines `0x6B11`, `0x6B12`), même structure (4 banques, adresses de 6 bits distincts tirés parmi 32, routeur à 2 portes x_a ⊕ x_b·x_c) :
  - `T-sparse` : k ∈ U{1..6} monômes distincts par porte, degré 1 à 3 ;
  - `T-dense` : k ∈ U{24..40} monômes distincts uniformes sur 0..63.
- TRAIN 16 384, VAL 4 096 (étiquettes bruitées, η = 0,05), TEST 8 192 (étiquettes **non bruitées** du professeur). Flux pseudo-aléatoires disjoints par partition.
- Adam (lr 0,05, β 0,9/0,999), mini-lot 256, 40 époques, β d'équilibrage 0,1, init θ = −3 ± 0,1, φ (LUT) = 0 ± 0,1, logits d'adresse 0 ± 0,1.
- 6 bras × 2 professeurs × 5 graines (1..5) = **60 entraînements**, aucun retiré a posteriori. TEST n'est lu qu'une fois, à la fin de chaque entraînement.

## Métriques

- Qualité : exactitude par bit et exactitude des 8 bits sur TEST, pour le modèle **dur** (inférence booléenne) et pour le modèle relaxé à la température finale ; écart relaxé − dur.
- Parcimonie : k par porte (médiane, p95, max), degré moyen, portes constantes nulles, monômes totaux.
- Partage : nombre de bits d'entrée distincts par banque ; nombre de sous-banques de densité uniforme (= transpositions) par banque sous le contrat actuel.
- Routage : pureté de la partition dure contre celle du professeur, charge minimale d'une banque.
- Exécution : chaque porte apprise non nulle est compilée en `Pvp6Gate<64>` ; sur tout TEST, les sorties de `Pvp6Bank::evaluate_on(Selected)` et `(Bitsliced)` doivent égaler l'inférence dure du modèle, bit à bit.
- Coût analytique par bloc de 64 requêtes (modèle, pas une mesure) : 6 · 64 pour la transposition, Σ (|m| − 1) AND et Σ (k − 1) XOR par porte.

## Critères de décision (T-sparse, médiane sur 5 graines)

Le bras ANF régularisé retenu est celui de plus grand λ dont l'exactitude **VAL** dure reste à 1 point de `anf_xor` (sélection sur VAL, jamais sur TEST).

- **C1 exactitude d'exécution** : 0 écart entre modèle dur, `Selected` et `Bitsliced`, sur tous les entraînements.
- **C2 qualité** : exactitude TEST dure par bit du bras retenu ≥ celle de `lut_ste` − 1 point.
- **C3 parcimonie** : bras retenu avec k médian ≤ 8 et p95 ≤ 16, et au moins 2 fois moins de monômes totaux que `lut_ste`.
- **C4 binarisation** : écart relaxé − dur ≤ 1 point pour le bras retenu.
- **C5 routage** : pureté dure ≥ 0,9 et charge minimale ≥ 10 %.

C1–C5 vrais : la méthode est démontrée **sur données synthétiques** ; P1 peut être proposé. C2 faux : la relaxation ANF native est rejetée à cette échelle. C3 faux : la régularisation ne suffit pas à garder le régime de gain matériel. Sur T-dense, on rapporte le compromis qualité/densité sans critère : il délimite les tâches où la LUT doit rester le bon choix.

## Ce que P0 ne démontre pas

Aucune qualité de modèle de langage, aucune généralisation hors distribution, aucun gain de temps natif (le coût est analytique), aucune énergie (GB-7 : non mesurée). Des données iid avec un professeur de la même famille que l'élève favorisent l'élève ; T-dense sert de contrôle partiel.

## Suites conditionnelles (accord du propriétaire requis)

- **P1 — corpus réel.** Prédiction du prochain octet (8 portes par banque = 8 bits de sortie) sur le corpus S5h, contre le témoin `--stream-50m-msb` (≈15 M paramètres, BPC 3,66 sur le DEV S5h), à budget de paramètres et de tokens égal, même DEV, TEST fermé. L'adresse d'une banque est tirée des bits des octets de contexte et d'un état récurrent ; protocole à préenregistrer séparément.
- **P2 — mesure native.** Banques apprises compilées en `Pvp6Bank`, chronométrées sur Dell et Thor via RemoteOps selon le protocole GB-9 (moteur complet et noyau), contre la LUT résidente.
- **Amendement de contrat possible** : accepter dans `Pvp6Bank` une densité déclarée comme borne supérieure, pour éviter le découpage en sous-banques. Ce serait une PR séparée avec ses propres tests de parité.
