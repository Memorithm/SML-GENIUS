# A6-C12g — État vérifié au 12 octobre 2026 : protocole et code créés, exécution GitHub-hosted bloquée AVANT les étapes

## Livrables effectivement enregistrés

- Préenregistrement intégral (avant tout nouveau code) : commit `fed44e4300b3f4cc16a600d4fb614607ad9c9a6a`, `docs/A6-C12G-NOISE-SAMPLE-PHASE-PREREGISTRATION.md`.
- Nouveau banc Rust natif : `crates/sml-core/src/bin/sml-noise-sample-phase-bench.rs`, commit `0dfabb00cddb5e7edda5f65ffc7f432cec8a607c`. Prévu pour 2 familles×5 taux d'étiquettes×7 budgets×8 graines, soit **1232 politiques** (1120 W3/HYBRID +112 GF2 propre) et **80 TEST appariés** seulement après gel.
- Workflow dédié : `.github/workflows/a6-c12g-noise-sample-phase.yml`, trois jobs ubuntu-24.04 GitHub-hosted, SHA exact, Rust 1.89, `contents:read`, interdiction des runners self-hosted.
- PR **#510 DRAFT**, non fusionnée. Les résultats scientifiques, la compilation et le formatage ne sont **pas encore validés**.

## Preuve d'échec de provisionnement des runners (distinct d'un échec cargo)

Sur le commit `b3fa5eacf2829894f151dc429b04943f3a3e38ee`, les jobs suivants ont été marqués **failure** après ~2 s, avec **zéro étape enregistrée** (`steps: null` dans la collection des jobs, `steps: []` au diagnostic détaillé) :

| Workflow / run | Jobs exemples | Observation |
|---|---|---|
| CI / [38284234097](https://github.com/Memorithm/SML-GENIUS/actions/runs/38284234097) | fmt 114928636709 | conclusion failure, ~2 secondes, zéro étape |
| A6-C12g / [38284234103](https://github.com/Memorithm/SML-GENIUS/actions/runs/38284234103) | phase-transition (1),(2),(3) | trois conclusions failure, chacune ~2–3 secondes, zéro étape |
| Mise en forme temporaire / [38284234168](https://github.com/Memorithm/SML-GENIUS/actions/runs/38284234168) | format-once 114928637381 | failure en ~2 secondes, zéro étape |

L'API de téléchargement des logs pour ces jobs retourne `HTTP 404 BlobNotFound`. Les logs n'existent pas lorsque le runner ne parvient pas à s'initialiser. **Ces résultats ne prouvent aucun échec de `cargo fmt`, `cargo check`, Clippy ou du modèle** : aucune de ces commandes n'a été exécutée dans les jobs cités.

**Cause exacte non accessible dans ce dossier** : le schéma est compatible avec une impossibilité de provisionnement / quota de minutes GitHub Actions / plafond de dépenses / incident de runner. Ne pas annoncer une cause billing comme établie sans annotation GitHub explicite ni accès aux réglages de facturation. La discussion GitHub [#209031](https://github.com/orgs/community/discussions/209031) décrit ce même triptyque `0 steps + 2 s + BlobNotFound`.

Un workflow transitoire `contents:write` avait été ajouté pour tenter un rustfmt automatique, mais n'a **jamais démarré** et a été **supprimé** du contenu final de la branche au commit `b3578628a17f38aad4ad57550143d91174a694f5`. Le workflow scientifique restant a les seuls droits `contents:read`.

## Nouvelle vérification / deuxième tentative

Sur le SHA documentaire final `f5fefc6b0d00b6600eb6267cbda661301c4e368e`, [CI 38284481784](https://github.com/Memorithm/SML-GENIUS/actions/runs/38284481784) et [banc A6-C12g 38284481727](https://github.com/Memorithm/SML-GENIUS/actions/runs/38284481727) échouent également sans étapes (`steps:null`), y compris les trois copies du job scientifique. Un **rerun explicite des jobs en échec** sur 38284481727 a été accepté par l'API GitHub, puis s'est terminé avec `run_attempt=2`, `conclusion=failure`, encore `steps:null` sur les trois jobs. Le blocage est ainsi reproduit sans modification du code. Aucun résultat/erreur Rust n'est disponible. La PR reste en draft et aucun merge n'est autorisé.

## Garde avant merge

Maintenir la PR en **draft**. Lorsque l'exécution des GitHub-hosted runners redevient possible, demander le contrôle format/check/Clippy/tests puis le banc réel sur les trois runs, vérifier **1232 FROZEN/1232 TEST/154 SUMMARY/22 THRESHOLD par runner** et **80 TEST distincts appariés**, publier les résultats et marges complètes y compris les échecs et intervalle Wilson. Ne pas modifier les taux, budgets, graines, critères de récupération ou Sketch/ANF sur la base des performances de TEST. Ne pas basculer sur un runner privé RemoteOps pour contourner ce blocage du protocole.

## Mise à jour du 12 octobre 2026 — source compilée et testée de façon isolée, expérience non exécutée

Le diagnostic initial ci-dessus décrit correctement les échecs pré-runner GitHub-hosted, mais **ne décrit plus l'état de compilation du code**. Deux erreurs Rust ont depuis été réellement identifiées par un job isolé RemoteOps puis corrigées sur la branche #510 : la construction de `Frozen` utilisait `fit` hors portée au lieu de `first_fit` (Rust E0425), et la fonction `freeze` à huit arguments déclenchait `clippy::too_many_arguments` sous `-D warnings`. La mesure du seuil de récupération utilise maintenant les résultats du TEST natif autonome, en distinguant les modèles seulement exacts et acceptés sur TRAIN/VAL.

Preuve de qualité du **code de `sml-core` uniquement** : [RemoteOps run 38292372909](https://github.com/Memorithm/RemoteOps/actions/runs/38292372909), succès sur source C12g au SHA `aa3161ad36dc1aeca896312de23d4143fe361f77`, image officielle Rust 1.89 épinglée, composants rustfmt + Clippy installés **avant** le checkout de la source privée, conteneur non privilégié sans accès réseau ni identifiants. Le job a réellement exécuté `cargo fmt --all -- --check` sans formatage préalable, `cargo check`, les **trois tests** du bin, puis Clippy `-D warnings` — tous réussis. Limite incontournable : le workspace a été **réduit à `crates/sml-core` dans une copie jetable** avec un lockfile régénéré hors ligne pour éviter `mmap-guard` d'un autre crate ; la commande n'a donc pas qualifié l'intégralité du workspace et de son lockfile original. Le code dans GitHub n'a pas été réduit ni son Cargo.lock changé.

Tests scientifiques A6-C12g : **toujours aucun des 1232 modèles FROZEN et 1232 TEST du banc complet mesuré** ; aucune marge spectrale ni courbe de seuil mesurée. La CI indépendante GitHub-hosted reste bloquée avant checkout : les sondes temporaires `ubuntu-22.04` et `macos-15` ont aussi échoué avec zéro étape, puis les workflows de sondage ont été supprimés de la branche. L'attribution causale précise (facturation, quota, incident fournisseur) reste inconnue. Le passage d'un banc compilable à une démonstration expérimentale **n'est pas acquis** ; aucun résultat de performance SML-25B ou de langage n'en découle.

Toute fusion du **code expérimental** alors que la garde scientifique initiale exige trois exécutions GitHub-hosted devra être consignée comme une **dérogation de livraison du code demandée par l'utilisateur**, non comme une qualification scientifique réussie. Les observations restent NON QUALIFIÉES jusqu'à exécution de la matrice complète et vérifications indépendantes ; aucune prétention à une courbe de transition observée.
