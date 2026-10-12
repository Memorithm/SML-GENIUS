# A6-WL1 — Résultats natifs : identification active de monômes ANF sparse jusqu'à 1024 entrées

**12 octobre 2026 — expérience préenregistrée, exécutée, qualifiée sur ses propres limites.**

Le [préenregistrement](ANF-WIDE-ACTIVE-SPARSE-LEARNING-PREREGISTRATION.md) a été commité **avant le code** au SHA `9b7de5132dd8be3109f7d7f368d4d0b5fe6bd1a9`. Le premier run complet du protocole a passé la CI dédiée [GitHub Actions `wide-learning`, run 38226041262 / job 114733673678](https://github.com/Memorithm/SML-GENIUS/actions/runs/38226041262/job/114733673678), SHA code `cd356b4a0448e1442084aaf0d1e62317b5280df1`. Il a vérifié formatage, compilation, Clippy strict, tests du module d'apprentissage et du binaire, ainsi que les 42 fits et les contrôles négatifs. La CI générale et la CI dédiée doivent également être vertes sur le **SHA documentaire final** avant fusion.

## 1. Nouveauté réelle et grammaire délibérément bornée

Les PR #498 et #499 savaient **exécuter** un `WideAnfGate` à 1024 entrées et l'installer dans des pages SBG sélectionnées par MOR, avec un cycle Delta-KV. Elles ne savaient **pas découvrir** ses monômes. La PR actuelle ajoute dans `crates/sml-core/src/anf_wide_active_learn.rs` un ajusteur natif `fit_selector_anchored_sparse` à entrée `FnMut(&[bool]) -> Result<bool,...>` et sortie **`WideAnchorFit {gate, learned_monomials, logical_queries, physical_queries, ...}`**.

Il s'agit d'**apprentissage actif avec requêtes d'appartenance** (membership queries) d'une famille bien déterminée :

`f(x) = XOR_{a=0..3} [x_a AND AND_{j ∈ H_a} x_j]`.

Les variables **x0, x1, x2, x3 sont des sélecteurs publics connus**. Chacun porte exactement **un** monôme de variables positives `H_a`, pris dans `x4..x(k−1)`. Le support caché, son degré exact et ses positions ne sont pas fournis à l'apprenant. La construction du générateur est séparée, et le fitter ne reçoit que la réponse à **ses propres requêtes TRAIN**. Aucun gradient, aucune énumération des `2^k` configurations, et aucun accès aux masques cachés, à VAL ou à TEST pendant l'ajustement.

**Mécanisme de découverte :** fixer un seul sélecteur à 1, les autres à zéro et toutes les données à 1 ; puis basculer à zéro des groupes de variables non sélectrices. Le signal indique si le groupe contient au moins un membre du monôme positif caché. Une bissection récursive apprend les positions actives avec des requêtes proportionnelles au nombre de variables actives multiplié par un facteur logarithmique de la largeur, **uniquement sous cette grammaire**. Une fonction ANF arbitraire ne garantit ni ce signal, ni ce coût.

**TRAIN bruité** : l'oracle retourne une mauvaise étiquette avec une probabilité indépendante `1/16` par observation physique ; la procédure emploie exactement **15 votes de majorité** par requête logique, contre **1** vote sur le panel propre. La moyenne des erreurs réelles injectées augmente avec le nombre de requêtes; aucune observation bruitée n'est réutilisée comme vérité de VAL ou de TEST. Les limites du learner sont `max_fan_in=2048`, `max_selectors=4`, `max_physical_queries=20 000`, `max_hidden_literals_per_term=8`, et un `WideAnfLimits` natif explicite.

## 2. Recensement exhaustif du protocole, sans confusion statistique

**7 largeurs × 3 graines × 2 panels de TRAIN = 42 modèles gelés** (4 monômes par modèle). Les panels propre/bruité d'une largeur/graine partagent exactement le **même générateur**, donc il y a **21 supports cachés distincts**, pas 42 tâches structurelles indépendantes. Les tailles cachées des quatre conjonctions sont `[2,3,4,5]`, plafonnées à `k−4` pour `k=8`; positions tirées sans remise avec SplitMix64 seedé, cachées de l'apprenant.

Chaque fit est sélectionné uniquement sur **544 contrôles VAL** : les 16 combinaisons des quatre sélecteurs avec les autres bits tous à 1, les mêmes avec les autres bits tous à 0, et 512 entrées aléatoires indépendantes de TRAIN. Le critère est **zéro erreur de validation**, sans raffinement par VAL. Les 42 décisions ont été **gelées avant la première génération des TEST**, qui comportent **4096 requêtes aléatoires par modèle** (les mêmes flux pour les deux panels d'une largeur/graine, ce qui signifie **21 flux TEST distincts**). Le cas `k=8` ajoute un contrôle exhaustif des 256 configurations possibles, pour six modèles.

Le banc a aussi appris un modèle volontairement **hors grammaire**, quatre monômes supportés PLUS `x0 ∧ x1`. Cet ajout ne change pas les sorties quand un seul sélecteur est actif ; il est donc **invisible dans le TRAIN one-hot** mais devient observable lors de la validation sur plusieurs sélecteurs actifs. Cette expérience falsifie une hypothèse abusive de généralité, sans promettre de détecter toutes les fonctions hors prior.

## 3. Résultats scientifiques et critères préenregistrés

| Contrôle | Résultat |
|---|---:|
| Modèles réellement gelés avant TEST | **42/42** |
| Récupération exacte des quatre supports et monômes | **42/42** |
| Validation indépendante à zéro erreur (544 labels/fit) | **42/42** |
| Désaccords sur les 4096 entrées TEST/fit | **0/172 032** |
| Contrôles exhaustifs k8, 256 entrées/fit | **6/6 modèles, 0/1536 désaccord** |
| Contre-exemples `x0∧x1` effectivement rejetés par VAL | **7/7** |
| Porte k1024 apprise réellement installée dans `WideAnfPageAtlas<1024>` | **16/16 sorties exactes** |

Les sept témoins négatifs donnent **121, 133, 134, 141, 126, 134 et 144 désaccords de validation sur 544** respectivement pour `k=8,16,32,64,128,256,1024`. Ils sont **tous rejetés**, ce qui démontre que l'acceptation n'est pas tautologique après la recherche par groupes.

Les contraintes sur la mémoire physique et les limites d'API ont également été vérifiées : rejets fail-closed de sélecteurs invalides ou répétés, largeur zéro, répétitions incorrectes, budgets de requêtes épuisés et supports plus larges que la limite de degré. Aucun mode d'extension dense automatique en `2^{1024}` n'est utilisé ; toutes les portes apprises utilisent le backend `sparse`.

### Coût d'apprentissage observé (TRAIN seulement)

Moyennes sur **3 graines pour chaque ligne** ; le temps inclut l'appel **local et immédiat** de l'oracle natif Rust et la construction de la porte. Ce n'est **pas** la latence d'un oracle distant, d'un jeu de données collecté en production ni d'un système SML-25B.

| Fan-in `k` | Requêtes logiques | Observations physiques, TRAIN bruité | Temps de fit propre | Temps de fit bruité |
|---:|---:|---:|---:|---:|
| 8 | 32 | 480 | 0,006 ms | 0,022 ms |
| 16 | 70 | 1050 | 0,011 ms | 0,038 ms |
| 32 | 103,33 | 1550 | 0,014 ms | 0,041 ms |
| 64 | 132,67 | 1990 | 0,014 ms | 0,067 ms |
| 128 | 154 | 2310 | 0,019 ms | 0,057 ms |
| 256 | 186 | 2790 | 0,026 ms | 0,075 ms |
| **1024** | **240** | **3600** | **0,065 ms** | **0,186 ms** |

Les appels physiques sur le panel propre sont égaux aux requêtes logiques ; sur le panel bruité ils valent exactement **15 fois** ces dernières. À `k=1024`, la porte compilée contient **quatre monômes ANF**, avec **9024 octets de payload logique déclaré** et un scratch de **19 mots `u64`**, excluant allocations auxiliaires de recherche, capacité résiduelle des vecteurs, headers et mémoire du modèle global.

Ces chiffres sont cohérents avec la parcimonie et la priorisation par sélecteur, mais **ils ne bornent pas le coût d'identification d'une ANF à 1024 entrées sans cette structure**, notamment les termes d'interaction entre plusieurs sélecteurs, les négations, les monômes multiples par sélecteur et le bruit structuré non indépendant.

## 4. Raccordement réel au chemin des pages SBG sélectionnées

Le banc choisit le modèle **déjà appris et gelé** `k1024, seed0, TRAIN propre` et fournit sa `WideAnfGate` à une vraie `WideAnfPageAtlas<1024>`. L'atlas est évalué par le backend natif de `WideAnfGate::evaluate_lanes`, avec sources `SbgCoordinate::Value[0..1023]`, destination `Meaning[0]`, et un `MorDecision` qui sélectionne `PageId(17)`. Sur **16 états SBG** couvrant les configurations des sélecteurs et un bit non sélecteur forcé à zéro sur certains cas, les sorties exactes coïncident toutes avec l'enseignant indépendant.

Ce contrôle **montre que les monômes récupérés servent effectivement à une porte SBG sélectionnée**, et non à un objet d'entraînement uniquement. La décision MOR utilisée dans ce banc est **choisie explicitement** ; l'algorithme MOR ne découvre pas encore seul des sélecteurs ni des pages, et cette expérience ne constitue pas un apprentissage complet end-to-end avec Delta-KV. Le fonctionnement natif de ce dernier circuit a été qualifié séparément en PR #499.

## 5. Reproductibilité, sécurité et décision

Première exécution complète : [GitHub Actions — 38226041262/job/114733673678](https://github.com/Memorithm/SML-GENIUS/actions/runs/38226041262/job/114733673678), sur le commit `cd356b4a0448e1442084aaf0d1e62317b5280df1`. Les journaux contiennent exactement **42 `WIDE_LEARN_FROZEN`**, **42 `WIDE_LEARN_TEST`**, **7 `WIDE_LEARN_NEGATIVE`**, **1 `WIDE_LEARN_SBG`**, et la ligne `WIDE_LEARN_COMPLETE` exigeant les comptes exacts. Tous les tests natifs dédiés, ainsi que Rust fmt, check et Clippy `-D warnings`, sont verts sur ce run.

```bash
cargo fmt --all -- --check
cargo check --locked -p sml-core --all-targets
cargo clippy --locked -p sml-core --all-targets -- -D warnings
cargo test --locked -p sml-core --lib anf_wide_active_learn
cargo test --locked -p sml-core --bin sml-wide-active-learn-bench
cargo run --locked --release -p sml-core --bin sml-wide-active-learn-bench
```

Le premier CI a révélé des écarts de formatage, puis un échec de conversion d'erreur `WidePageError` dans le binaire ; corrections de formatage et conversion **strictement localisées au banc**, sans changer la grammaire, les graines, les seuils, les tests VAL/TEST ni les paramètres d'apprentissage préenregistrés. Les résultats négatifs restent publiés, ainsi que toutes les limites. Runner GitHub-hosted Ubuntu 24.04, Rust 1.89 épinglé, permissions `contents:read`, checkout exact sans secrets persistants, aucun runner Memorithm/RemoteOps ni SDK propriétaire NVIDIA.

**Décision : H1 PASS 42/42, H2 PASS 21/21 panels bruités et 21/21 propres, H3 PASS 7/7 rejets**, uniquement dans le sous-espace de quatre sélecteurs fixes avec un monôme positif par sélecteur et interrogations actives. Le résultat *ne prouve pas* l'identification d'ANF génériques, la qualité linguistique SML-25B, un RAG supérieur, des banques de 1 048 576 monômes ou un débit en tokens/s.

**Prochains obstacles scientifiques** : (i) inconnue des sélecteurs, (ii) plusieurs monômes par sélecteur et degrés variables avec interactions, (iii) apprentissage sur trajectoires passives et bruit temporel, (iv) coût de projection/identification et choix automatique du nombre de termes, (v) persistance et activation des fonctions à grande échelle dans les pages MOR/Delta-KV avec budgets d'état réels. Ils nécessitent chacun un protocole réfutable avant expansion.
