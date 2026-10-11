# A6-C12b — Résultats : récupération GF(2), Walsh de paires et référence exhaustive d'ordre 3

**Statut : expérience exécutée sur runner GitHub Actions, succès des contrôles préenregistrés.**
Protocole figé avant tout code et résultat : `docs/A6-C12B-GF2-WALSH23-PREREGISTRATION.md`, commit `cdfb8bae8c4004c028b3fc1907949c0a4b77eadc`.
Première exécution complète sur la révision Rust `a3fa6310e96aff5d57aece361fe810a05f90d1f1` : [job A6-C12b](https://github.com/Memorithm/SML-GENIUS/actions/runs/38183477438/job/114600763480). Contrôles format, check, Clippy, tests unitaires et **216/216 tentatives** du banc : réussis. CI générale `rust` vérifiée séparément (ne pas la supposer verte sur une révision documentaire ultérieure).

## 1. Question scientifique et distinction de capacité

Les quatre circuits à **trois portes** sont **tous représentables exactement** dans la grammaire. Les positions j20/k68/l116 ont été choisies à l'avance sur une grille de portes 4-alignées ; l'apprenant connaît uniquement la grille de recherche, **pas les vraies mutations**, les sources ou les tables. Le banc n'utilise donc **pas** la même géométrie que A6-C12a (j19/k67/l108) et ne prétend pas à une comparaison quantitative directe de leurs taux d'erreur.

Les 24 flux TRAIN/VAL sont issus de 4 tâches ×2 panneaux ×3 graines, 512 pas TRAIN, 512 VAL, 64 lanes. Les **24 TEST indépendants**, 4096 pas chacun, sont générés uniquement après gel des **216 tentatives**, partagées sur n=32,64,128 et trois méthodes. Les 216 lignes ne sont pas indépendantes ; chaque n réutilise le même flux et chaque algorithme est comparé sur le même TEST.

## 2. Résultats de récupération à n=128

Chaque cellule est un **nombre de modèles aux paramètres exactement récupérés** (positions, canaux ET trois tables ANF), parmi trois graines ; les résultats propres/bruités sont indiqués séparément. Le contrôle natif indépendant est sans erreur propre et atteint environ 6,2–6,3 % sur labels bruités.

| Fonction vraie | Gauss exact propre | Gauss bruité | W2+paires+factorisation propre/bruité | W3 exhaustif+factorisation propre/bruité |
|---|---:|---|---:|---:|
| **XOR→XOR→XOR** | **3/3** | non exécuté (défini ainsi) | **0/3 ; 0/3** | **3/3 ; 3/3** |
| **AND→XOR→XOR** | 0/3 (pas d'ajustement affine exact) | non exécuté | **3/3 ; 3/3** | **3/3 ; 3/3** |
| XOR→AND→XOR | 0/3 | non exécuté | **3/3 ; 3/3** | **3/3 ; 3/3** |
| AND→AND→AND | 0/3 | non exécuté | **3/3 ; 3/3** | **3/3 ; 3/3** |

La **récupération exacte est identique à n=32,64,128 sur les graines étudiées**. Tous les modèles aux règles retrouvées reproduisent le TEST de l'oracle natif en autonomie, sans teacher forcing. Gauss est appliqué uniquement aux données propres ; il retrouve les trois XOR, les factorise ensuite par la même banque ANF que les autres méthodes, et produit **0 % d'erreur TEST**. Il ne traite pas les labels bruités.

**Le contre-exemple recherché à A6-C12a est donc surmonté sur cette grille bornée**, mais de manières différentes selon la famille : Gauss résout l'affine propre, W2+factorisation récupère le mixte non linéaire, W3 retrouve même la parité avec bruit sans utiliser un algorithme LPN général. W2 échoue encore sur XOR pur : ses réseaux sélectionnés restent proches de 50 % d'erreur TEST ; Gauss et W3 atteignent l'oracle sur leurs panels admissibles.

## 3. Valeur des classements et coût de recherche

- **W3** : la vraie structure occupe le **rang 1 de tous les 24 classements n=128** (4 tâches ×2 panneaux ×3 graines) ; chaque classement explore toutes les **317 440 structures admissibles**.
- **W2** : la vraie paire b,c de AND→XOR→XOR se classe dans le **top8 pour 6/6 panneaux×graines**, puis la recherche conditionnelle de la troisième source et des 520 fonctions composées récupère les trois tables exactes. Cela résout le cas où la W1+peeling antérieure n'identifiait aucune structure.
- **Gauss** : 1024 équations binaires (16 pas ×64 lanes), solution GF(2) vérifiée sur **tous les 384 pas utilisables** de TRAIN. Récupération exacte de la parité propre **3/3** à chaque taille n ; rejet de l'hypothèse d'ajustement exactement affine sur les tâches non affines (3/3 par tâche).

Les temps ci-dessous sont des **moyennes de 24 appels par taille et méthode**, en millisecondes, sur le runner x86_64 GitHub-hosted (Rust 1.89 RELEASE) ; les opérations W2/W3 incluent 512 pas TRAIN et la même banque de factorisation 520 masques, mais **pas un budget égal**. `W2 factorisation` inclut jusqu'à ≈960 structures complétées après top8 paires, contre 16 pour W3. `Gauss` inclut les cas bruités non exécutés et les cas non affines rejetés ; son temps moyen total dans ce tableau n'est donc **pas** une mesure de coût pour résoudre la parité propre.

| n | Paires admissibles | Triplets admissibles | W2 : écran / factorisation / total | W3 : écran / factorisation / total |
|---:|---:|---:|---:|---:|
| 32 | 448 | 3 584 | 0,214 / 4,767 / **4,981 ms** | 1,319 / 1,261 / **2,580 ms** |
| 64 | 1 920 | 35 840 | 0,858 / 9,820 / **10,677 ms** | 11,917 / 1,255 / **13,172 ms** |
| 128 | 7 936 | **317 440** | 3,003 / 19,883 / **22,886 ms** | 105,940 / 1,280 / **107,220 ms** |

Pour la **parité propre seule à n128**, Gauss a mesuré **0,075 ms de résolution/contrôle + 0,997 ms de factorisation native en moyenne**, soit ≈**1,072 ms**. Cette durée vient de trois graines et d'un espace de recherche borné connu ; elle ne représente ni un décodeur LPN bruité, ni une propriété de vitesse générale.

**Combinatoire :** C(128,3)=341 376 triplets sans contrainte ; 317 440 sont autorisés par les positions distinctes. À 508 sources, C(508,3)=21 720 556 et le domaine admissible natif contient **21 336 000** triplets : **il n'a pas été exploré**. Toute projection du coût temporel mesuré de n128 vers n508 ou vers des millions de fonctions reste une **extrapolation interdite** par le protocole.

Le seuil de bascule mesuré est informatif : W3 est plus rapide *de bout en bout* que W2 à n32 (2,580 contre 4,981 ms), mais W2 l'emporte à n64 (10,677 contre 13,172 ms) et surtout n128 (22,886 contre 107,220 ms). On ne peut donc attribuer un « gain » à W2 sans inclure sa factorisation extensive.

## 4. Bruit présent et faux raccourci « corrélation 0,875 »

Dans les données de TRAIN, `p_t=z_t XOR R_(t-128)` dépend de **deux labels**, celui du pas t et celui du pas t-128. Pour deux bruits indépendants de probabilité 1/16, l'erreur proxy effective est

```text
eta_proxy = 2 × (1/16) × (15/16) = 15/128 = 0,1171875
rho_proxy = (1-2/16)^2 = 49/64 = 0,765625
```

La corrélation **0,875** serait correcte si l'observation récurrente passée était propre et seule la sortie présente bruitée, ce qui n'est pas le cas du protocole. La réussite du classement W3 sur ces panels **ne résout pas l'apprentissage de parité bruitée pour k et n quelconques** ; elle résout une énumération exhaustive de k=3 dans une grille volontairement limitée.

Les erreurs TEST de l'oracle, moyennées sur 3 graines à n128 (identiques pour les méthodes puisque TEST partagés) : XOR³ **6,1897 %**, AND→XOR→XOR **6,2719 %**, XOR→AND→XOR **6,2565 %**, AND³ **6,2620 %**. Les modèles exacts retrouvent chacune de ces erreurs à l'unité de bit.

## 5. Reproductibilité et contrôles

Les sorties de chaque tentative `A6C12B_FROZEN` publient statut, rang, hypothèses proposées, nombre de structures factorisées, temps de filtrage et de factorisation. Les sorties `A6C12B_TEST` donnent oracle, proposition, sélection, reconstruction paramétrique et erreurs individuelles. La CI exige **72 lignes de grille, 216 tentatives gelées, 216 comparaisons TEST et 24 TEST distincts**, sans aucun tuning après TEST.

```bash
cargo fmt --all -- --check
cargo check --locked -p sml-core --all-targets
cargo clippy --locked -p sml-core --all-targets -- -D warnings
cargo test --locked -p sml-core anf_three_gate_diagnostic
cargo test --locked -p sml-core --bin sml-gf2-walsh23-bench
cargo run --locked --release -p sml-core --bin sml-gf2-walsh23-bench -- 3
```

Workflow dédié : [A6-C12b GF2/Walsh23](https://github.com/Memorithm/SML-GENIUS/actions/runs/38183477438). Les garde-fous restent : runner GitHub-hosted sans autorisations d'écriture, pas de runner privé RemoteOps, Rust natif uniquement, aucun logiciel NVIDIA.

## 6. Décision et limites

**PASS expérimental borné pour les comparateurs GF(2), W2 et W3 ; FAIL maintenu de W2 seul pour XOR pur.** Le résultat A6-C12a n'était pas une limite de représentation ; cette campagne montre qu'une détection d'ordre supérieur peut récupérer les mêmes types de fonctions dans un espace de recherche délimité. Elle ne valide **ni un algorithme sous-cubique général**, ni une sélection efficace à n508, ni un apprentissage de quatre/cinq sources avec bruit, ni un gain d'inférence ou de qualité de langage SML-25B.

Prochaine hypothèse à préenregistrer **avant** lancement : généralisation à des positions de portes libres parmi 508 sources, introduire un mécanisme sub-exhaustif de génération de triplets (p.ex. regroupement par paires, factorisation spectrale/syndrome) et comparer sa sensibilité et son coût au W3 de référence sur les mêmes familles, sans donner le vrai triplet. Les cas à variables masquées nécessitent d'abord un contrôle théorique d'identifiabilité, et les k≥4/5 exigent des apprenants de capacité adéquate.
