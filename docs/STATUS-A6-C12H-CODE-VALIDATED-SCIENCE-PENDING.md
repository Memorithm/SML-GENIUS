# A6-C12h — Qualification du code Rust et absence de résultats scientifiques

**État établi le 12 octobre 2026.** Le protocole avant code `docs/A6-C12H-EXTREME-NOISE-THEORY-PREREGISTRATION.md` (premier commit `62da8067f0442705dac143647704162f98d2d075`) reste inchangé : il gèle deux familles d'enseignants, 128 candidats sources, η = 0 / 20 / 30 / 40 %, 10 budgets de 64 à 32 768 bits, une grille de 8 graines plus 8 graines dans les fenêtres préfixées par le seuil théorique, soit 1872 politiques et 128 TEST distincts.

## Code effectivement vérifié

- Corrigé : `Frozen` stocke `first_fit` et la signature de `freeze` regroupe `(seed,words)` pour satisfaire Clippy strict, sans modifier les calculs. Le banc C12g parent sur cette branche correspond à la version compilée et rustfmt de #510.
- Corrigé : Sketch sous 1, 2 et 4 mots, masque de bruit i.i.d. 30/40 % sur comparaison `u128`, population de graines ciblée, amplitude de corrélation référée à `sqrt(2ln(2M))`, test natif autonome et Wilson selon n=8/16.
- [RemoteOps run 38292537194](https://github.com/Memorithm/RemoteOps/actions/runs/38292537194) : compilation, cinq tests Rust et Clippy strict **réussis** sur copie locale formatée automatiquement à partir du SHA `af7b4028aa1a934f9cd270262dd11b14964b52b9`. La sortie rustfmt a ensuite été appliquée dans GitHub, commit `0080abd5a065da492a1fb24086b367577dfa5bf8`.
- [RemoteOps run 38292684370](https://github.com/Memorithm/RemoteOps/actions/runs/38292684370) : **succès** du contrôle `cargo fmt --all -- --check` sans modification, `cargo check --locked --offline -p sml-core --all-targets`, **cinq tests binaires**, Clippy `-D warnings` sur le SHA réellement enregistré `0080abd5a065da492a1fb24086b367577dfa5bf8`. Rust 1.89, Clippy et rustfmt dans une image conteneur construite à partir d'une image officielle SHA256-épinglée, avant récupération de code privé ; test ensuite sans réseau, sans jeton ni montages de secrets, utilisateur non privilégié.

**Limite de qualification technique :** la copie de validation rétrécit temporairement le Cargo workspace à `sml-core` et régénère un lockfile local hors ligne, car le workspace complet résout `mmap-guard` depuis `sml-store`. Aucun manifeste ou lockfile suivi par Git n'a été changé. Le test n'est donc **pas** une CI complète du workspace GitHub, ni une mesure indépendante multi-runner.

## Résultats scientifiques : toujours inconnus

**Zéro résultat qualifié pour les 1872 politiques FROZEN / 1872 évaluations TEST / 128 jeux TEST.** Aucun tableau de succès par seuil, intervalle Wilson observé, marge spectrale observée ou latence matérielle n'a encore été obtenu. Les chiffres `N*=2 ln(2M)/a²` sont exclusivement des hypothèses **préenregistrées** avant code, avec constantes et hypothèses de null non démontrées.

Les jobs GitHub-hosted ont échoué **avant** exécution des étapes (absence de runner) sur la branche #511. La cause exacte n'est pas connue. Aucune fusion de code ne doit être présentée comme ayant satisfait à la qualification scientifique indépendante définie dans la préinscription. En cas de livraison du code malgré cette indisponibilité par demande explicite de l'utilisateur, ce sera une **dérogation administrative aux conditions de fusion**, non une mesure scientifique ou un succès du modèle. Le résultat négatif A6-E1 sur données réelles reste STOP.

**Usage permis :** lecture, compilation et exécution explicite du banc de recherche. **Usage non permis :** promotions SML-2/25B, promesse de récupération universelle ou d'inférence linguistique, chiffre de performance de banque million-fonctions ou de GPU.
