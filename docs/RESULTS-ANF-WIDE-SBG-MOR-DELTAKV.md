# WideSparseANF — Intégration SBG/MOR/Delta-KV et validation native

**12 octobre 2026 — Essais de la PR #499 ; résultat : tests natifs dédiés entièrement verts.**

Protocole gelé avant code : [ANF-WIDE-SBG-MOR-DELTAKV-PREREGISTRATION.md](ANF-WIDE-SBG-MOR-DELTAKV-PREREGISTRATION.md), commit `773b437c248bb558150facad7f7f5836f17214b4`. La qualification s'appuie sur le noyau WideSparseANF livré en [PR #498](https://github.com/Memorithm/SML-GENIUS/pull/498), déjà fusionnée. Le travail est une intégration **opt-in** dans l'écosystème SML et ne remplace pas les chemins PVP6 / LargeAnf existants.

## Composants livrés

- `wide_anf_selected_pages.rs` : `WideAnfPageAtlas<S>`, `WideAnfPage<S>`, `WidePageTrace`. Les pages portent une liste d'entrées `SbgCoordinate` lisibles dans les quatre champs SBG et des sorties déclarées. Les `WideAnfGate` compilées référencent cette liste d'entrées, pas une adresse `u32`; **1024 entrées** sont traitées à l'exécution. Tous les selected PageId proviennent d'un vrai `MorDecision`. Vérification avant exécution des identifiants, budget de pages, nombre de gates, mémoire de trace, scratch, sources et destinations; conflits inter-pages rejetés.
- `wide_anf_routed.rs` : `WideRoutedExperts<K,S>` utilise **l'implémentation existante** de `RoutedSemantic<K,S>` pour MOR, puis exécute uniquement les pages sélectionnées. Les comptes de gates et les traces sont cumulés sous budget. Aucune histoire ni compteur n'est commité en cas d'échec.
- `wide_anf_delta_kv.rs` : `WideAnfDeltaKvLoop<K,S,Q>` clone le routeur et l'exécuteur ainsi que le `DeltaKvDynamicRekeyKernel` natif, exécute **le cycle courant sélection → page ANF → projection de la clé de mémoire à partir du nouvel état et de la route courante**, puis ne valide les objets et leurs journaux que si toutes les étapes réussissent.
- `lib.rs` expose ces trois modules sans modification des API déjà publiées.

**Nuance importante :** `SemanticState<S>` contient un état scalaire par coordonnée; la page transforme chaque bit SBG en un mot `0` ou `u64::MAX`, applique le vrai `WideAnfGate::evaluate_lanes`, puis extrait une voie. Cela **utilise bien le backend natif WideSparseANF**, mais **ne constitue pas** une mesure de 64 états SBG indépendants exécutés en parallèle.

## Preuves Rust et résultats

Premier run dédié complet : commit `9998480d6f210f1f618acc072dcefefc8f8345f8`, [GitHub Actions — wide-coupled job 114725320933](https://github.com/Memorithm/SML-GENIUS/actions/runs/38223518560/job/114725320933). Résultat **SUCCESS** après vérification `cargo fmt --all -- --check`, `cargo check --locked -p sml-core --all-targets`, `cargo clippy --locked -p sml-core --all-targets -- -D warnings`.

Les tests dédiés sont **4/4** (`wide_anf_selected_pages`) et **3/3** (`wide_anf_delta_kv`); les **7/7** tests du cœur `anf_wide_sparse` ont aussi été rejoués sans échec. Le module `wide_anf_routed` n'a pas de test isolé supplémentaire (**0 test** sur le filtre dédié), mais il est traversé effectivement par les trois tests de la boucle couplée. Le workflow a émis `WIDE_SBG_COMPLETE` et tous les tests pertinents étaient verts.

### H1 — Vraie porte k1024 dans des pages sélectionnées

Un atlas `S=1024` contient une première page à **une porte AND de 1024 entrées**, et une seconde page inactive. Une `MorDecision` sélectionne uniquement la première; les 1024 sources sont projetées depuis `state.value`, leur conjonction est évaluée par `WideAnfGate::evaluate_lanes`, et le bit de résultat est écrit dans `state.meaning_mask`. Résultat vrai quand **1024/1024** entrées sont à 1, et faux lorsque les bits 0, 512 ou 1023 sont individuellement annulés. Les traces confirment **1024 entrées, scratch 1025 mots et 1 résultat packed**. Les sorties de la page inactive ne changent pas.

Deux pages sélectionnées simultanément produisent des résultats à partir du **même instantané SBG pré-exécution** : la seconde page lit l'ancienne valeur d'un bit que la première écrira. Le test confirme l'isolation au niveau des données d'entrée et la simultanéité des écritures.

### H2 — Refus de mutation partielle, bornes et validation

Le test `conflict_missing_duplicate_and_all_budgets_fail_closed` rejette un conflit de destination, un PageId répété, un PageId inconnu, un budget de gates insuffisant, un budget de trace insuffisant et un budget de scratch insuffisant. Les objets `SemanticState` et `WideAnfPageAtlas` sont inchangés. Le constructeur refuse ID de page dupliqué, coordonnées d'entrée dupliquées et destination hors domaine. Les portes conservent les contrôles de largeur/monômes/doublons/budgets compilés par `anf_wide_sparse.rs`.

### H3 — Boucle couplée MOR, pages SBG k1024 et Delta-KV

Sur un état SBG `S=1024`, un contexte de routeurs MOR natifs sélectionne PageId(0) ou PageId(1). Un kernel `WideAnfDeltaKvLoop` exécute **deux cycles complets**. À chaque profondeur, son état post-expert, son état post-mémoire, la sélection MOR, la clé projetée `key_q12` et les compteurs de gate-évaluations/trace coïncident **à l'identique** avec le résultat obtenu en exécutant indépendamment les trois mêmes composants Rust sur le même état. Les deux cycles sont effectivement consignés, et l'appel avec une profondeur réutilisée est refusé.

Lorsqu'on fournit une provenance Delta-KV invalide, l'exécution échoue **après l'étape de page et avant tout commit** : le routeur, la sélection, les traces d'ANF, les cycles de mémoire et ses compteurs de lecture conservent leurs valeurs antérieures. Un dépassement de budget de gates au second cycle confirme la même absence de mutation des étages déjà validés.

## Protocole de reproduction

```bash
cargo fmt --all -- --check
cargo check --locked -p sml-core --all-targets
cargo clippy --locked -p sml-core --all-targets -- -D warnings
cargo test --locked -p sml-core --lib wide_anf_selected_pages -- --nocapture
cargo test --locked -p sml-core --lib wide_anf_delta_kv -- --nocapture
cargo test --locked -p sml-core --lib anf_wide_sparse -- --nocapture
```

GitHub-hosted Ubuntu 24.04, Rust 1.89 explicitement épinglé, checkout du SHA exact, `permissions: contents:read`, pas de runner privé ni logiciel NVIDIA propriétaire. Le premier échec CI provenait uniquement du formatage des nouveaux fichiers et de `lib.rs`; la correction reproduit strictement les diffs `rustfmt` officiels, **sans changer le comportement**. Vérifier aussi la CI générale au dernier SHA documentaire avant fusion.

## Portée et limites

**H1 PASS, H2 PASS, H3 PASS** dans le périmètre déclaré, avec tests natifs exacts. Le pont relie désormais réellement un exécuteur WideSparseANF à MOR, SBG et Delta-KV dans une **API opt-in**, sans activer cette API partout dans l'architecture ni entraîner les coefficients ANF. Le test k1024 ne constitue pas un apprentissage d'une porte k1024, une vraie banque de 1 048 576 fonctions, une mesure multi-SBG SIMD, ni un benchmark sur Jetson Thor.

Les prochains jalons restent séparés : apprentissage de monômes sparse à fan-in variable, banque de fonctions massive avec routage parcimonieux, qualification GPU et qualité linguistique de SML-25B. Éviter de confondre 1024 entrées par porte, 1024 états, une page ANF et le nombre de portes activées par token.
