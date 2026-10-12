# A6-I5 — Résultats : garde exact de support d'ordre trois et échec obligatoire à l'ordre quatre

**12 octobre 2026. Première expérience native complète réussie, et contre-exemple négatif conservé.**

Protocole gelé **avant tout code** dans [A6-I5-TRIPLE-COVERAGE-PREREGISTRATION.md](A6-I5-TRIPLE-COVERAGE-PREREGISTRATION.md), premier commit `7a2383c6e475e1525437d5d90a187091ca64d0c8`, sur base `main=11ff545df2ce4623cdf387f865e4ea302db5b514` (#506). Première tentative de CI sur source `833914369268a6d289f54a7a56c2c18e89eff54b` arrêtée avant les expériences par les seuls écarts `rustfmt` ; corrections de formatage exactes aux commits `8d947f215828e2564d3ffaccf0805b32bb39007c` et `985326691fcea033d271ee07af74dc443cfb8a85`, sans modifier scénarios, budget, graine, seuil ni code décisionnel. Premier essai complet des résultats sur SHA `985326691fcea033d271ee07af74dc443cfb8a85` : [GitHub-hosted, run 38267775965 / job 114874188411](https://github.com/Memorithm/SML-GENIUS/actions/runs/38267775965/job/114874188411), **SUCCESS** avec `cargo fmt`, `check`, Clippy strict, **4 tests de bibliothèque**, **2 tests du banc**, **90 fits** et **46 080 évaluations TEST**. Vérifier aussi les contrôles du SHA documentaire final avant merge.

## 1. Ce qui est implémenté, et ce qui ne l'est pas

Le module public Rust `sml_core::anf_triple_support::AnfTripleSupportGuard` calcule une couverture **exacte sur tous les triplets d'indices** `i<j<l` du vecteur d'entrée, sans accès aux étiquettes TEST ou aux paramètres du véritable enseignant. Il observe jusqu'à 512 vecteurs booléens TRAIN, rangés en colonnes de `u64` bit-slicées ; `seal()` explore les huit configurations de chaque triplet et retient uniquement celles qui manquent au support observé. Une prédiction OOD est autorisée uniquement si le `PassiveCommittee` existant l'accepte à confiance ≥0,90, que la banque présente sur TRAIN ≤12,5 % d'erreur minimale, et qu'aucun triplet de la requête ne prend une configuration jamais vue.

L'algorithme **ne reconstruit aucun coefficient ANF**, n'apprend aucun nouveau monôme et n'évalue pas la validité du vrai modèle hors support TRAIN. Il est strictement plus informatif qu'une mémoire de paires sur certaines censures, mais un système à seuil de couverture d'ordre fixé n'est pas une preuve de généralisation.

La mutation TRAIN et le scellement vérifient les longueurs, quotas et capacités avant de toucher aux données ; un dépassement du nombre de triplets manquants empêche d'installer tout résumé partiel. Le composant refuse `assess` avant scellement et `observe` après scellement, et exige la concordance du nombre d'observations avec `PassiveCommittee`. Aucune modification aux modèles historiques, SBG, MOR ou Delta-KV. Rust avec `#![forbid(unsafe_code)]`, environnement GitHub-hosted, SDK GPU propriétaire non requis.

## 2. Corpus et vérités cachées

Le banc couvre 5 largeurs `k=[8,16,32,64,128]`, 3 graines, 2 panneaux d'étiquettes TRAIN propres/bruitées (Bernoulli1/16), 3 scénarios. Les **90 politiques** sont toutes figées avant l'apparition du premier TEST, chaque fit utilisant 512 entrées passives TRAIN et 512 entrées TEST indépendantes ; total **46 080 comparaisons**, 15 360 par scénario. Les panels bruités partagent des trajectoires d'entrées et la structure vraie avec les propres ; il n'y a pas 90 populations indépendantes.

Dans chaque largeur, huit rôles physiques `r0..r7` sont permutés par un générateur dédié. Les quatre candidats fournis au comité sont `h0=r2 XOR r3`, puis `h0 XOR (r0 AND r1)`, `h0 XOR r0` et `h0 XOR r1`. L'enseignant caché est soit `h0` réellement présent dans la banque, soit `g3=h0 XOR (r4 AND r5 AND r6)` ou `g4=h0 XOR (r4 AND r5 AND r6 AND r7)` absents de la banque. Le garde n'accède qu'aux vecteurs d'entrées, jamais aux indices des rôles ni au nom de l'enseignant ; le comité reçoit la banque de quatre fonctions, **pas leurs monômes à découvrir**.

Dans le scénario triple, TRAIN énumère les sept configurations de `r4,r5,r6` sauf 111 ; dans le scénario quadruple les quinze configurations de `r4,r5,r6,r7` sauf 1111. Tous les patterns d'ordre strictement inférieur sont couverts. Pendant TEST, la moitié des entrées active exactement le motif caché, l'autre moitié le désactive. Les générateurs TRAIN, bruit et TEST ont des graines indépendantes ; les scénarios triple inclus/absent conservent les mêmes entrées de chaque graine.

## 3. Mesures gelées : réussite locale et échec informationnel

Les métriques primaires ont été copiées du log du premier run réussi, **sans calibration après consultation du TEST**. Une abstention n'est jamais comptée comme bonne prédiction.

| Scénario et professeur | Comité : réponses / erreurs | Garde de paires : réponses / erreurs | Garde de triplets : réponses / erreurs | Triplets OOD nouveaux | Abstentions correctes perdues |
|---|---:|---:|---:|---:|---:|
| TRIPLE_ABSENT, `g3` hors banque | 15 360 / **7 680** | 15 360 / **7 680** | **7 680 / 0** | 7 680 | 0 |
| TRIPLE_INCLUDED, `h0` dans banque | 15 360 / **0** | 15 360 / **0** | **7 680 / 0** | 7 680 | **7 680** |
| QUAD_ABSENT, `g4` hors banque | 15 360 / **7 680** | 15 360 / **7 680** | **15 360 / 7 680** | **0** | 0 |

**H1 — succès circonscrit :** la censure du motif triple 111 reste invisible au garde de paires (aucune paire nouvelle), mais est détectée par le nouveau garde exact. Les 7 680 prédictions fautives du comité dans ce scénario sont refusées, tout en conservant 7 680 réponses propres correctes.

**H2 — coût de l'abstention :** sur les mêmes entrées et le vrai `h0` dans la banque, le garde refuse 7 680 prédictions qui auraient été entièrement correctes. Un trou du support observé ne démontre pas que la véritable fonction a changé.

**H3 — échec positif obligatoire du protocole :** sous censure 1111 de `g4`, la totalité des huit combinaisons de **chaque** triplet des quatre rôles apparaît dans TRAIN. `g4=h0` sur tout le support TRAIN, y compris avec distribution de bruit identique. Le garde ne voit aucune nouveauté triple et accepte les 15 360 sorties, parmi lesquelles **7 680 erreurs confiantes**. Aucun enregistrement exhaustif des triplets observés ne peut révéler une interaction quadruple entièrement censurée sans davantage d'information.

Ces constats correspondent aux critères H1-H3 préenregistrés. Ils n'établissent ni une vraie capacité d'intuition humaine, ni une extrapolation libre, ni l'identification passive de coefficients ANF cachés.

## 4. Bornes et observations d'allocation

Le nombre de triplets possibles `C(k,3)` n'est pas approximé : pour k128, **341 376**, dans le quota `max_triple_slots=350000`; k256 **2 763 520** et k1024 **178 433 024** dépassent ce quota et sont refusés dès `new`, **avant allocation**, dans les tests natifs. La vérification exhaustive est `O(C(k,3)×8×ceil(n/64))` opérations bit à bit au scellement (borne structurelle), puis `O(nombre_de_triplets_manquants)` par interrogation. Il ne s'agit pas d'un garde à coût constant pour tous les k ni d'un certificat d'ordre trois à 1024 entrées.

À k128 et 512 observations, les colonnes bit-slicées représentent `128×8×8=8 192` **octets de payload logique**. Les logs `A6_I5_FROZEN` relèvent exactement 1 triplet manquant dans chacune des 60 politiques TRIPLE_ABSENT / TRIPLE_INCLUDED, pour 8 224 octets à k128, et 0 dans les 30 politiques QUAD_ABSENT, pour 8 192 octets à k128. Les paires manquantes sont au nombre de zéro dans les 90 fits. La différence de 32 octets par enregistrement est le `size_of<MissingTriple>` sur l'ABI de ce runner ; elle n'inclut ni les entêtes des vecteurs, ni leur capacité réservée, ni les buffers temporaires, ni l'allocateur, ni le RSS. Le rapport n'invente aucune mesure de latence CPU ou de performance Thor/GPU.

Les tests confirment aussi rejet de la construction k256/k1024 sous quota, rejet sur TRAIN vide, absence de scellement partiel, refus post-seal et des histoires de comité non synchronisées.

## 5. Traçabilité, conclusion et prochaine porte

Reproduction :
```bash
cargo fmt --all -- --check
cargo check --locked -p sml-core --all-targets
cargo clippy --locked -p sml-core --all-targets -- -D warnings
cargo test --locked -p sml-core --lib anf_triple_support
cargo test --locked -p sml-core --bin sml-epistemic-triple-support-bench
cargo run --locked --release -p sml-core --bin sml-epistemic-triple-support-bench
```

Première exécution validée : [run 38267775965 / job 114874188411](https://github.com/Memorithm/SML-GENIUS/actions/runs/38267775965/job/114874188411), SHA `985326691fcea033d271ee07af74dc443cfb8a85`, Rust 1.89.0, Ubuntu24.04 x86_64. Les contrôles vérifient strictement 90 `A6_I5_FROZEN`, 90 `A6_I5_TEST`, 3 `A6_I5_SUMMARY` et la ligne terminale `A6_I5_COMPLETE,policies=90,tests=46080,triple_absent_protected=7680,triple_included_false_abstentions=7680,quad_confident_errors=7680,oversize_rejected=true,oracle=PASS`.

**Verdict : H1 PASS, H2 PASS, H3 PASS (en reproduisant l'échec exigé), H4 PASS sur les quotas.** La frontière de non-identifiabilité se déplace d'ordre trois à ordre quatre ; elle ne disparaît pas. La future étude doit comparer des sélections adaptatives d'interventions, la couverture hiérarchique bornée et l'enrichissement réel de la banque, en conservant des contrôles positifs, des contre-exemples négatifs et un budget d'acquisition comparable. SML-25B, banque de 1 048 576 fonctions, apprentissage linguistique, GPU natif et qualité sur données naturelles restent non démontrés.
