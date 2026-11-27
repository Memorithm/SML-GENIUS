# sml-tutord — superviseur permanent SML-TUTOR (P1)

Statut : **codé et testé localement / en CI**. Pas encore qualifié physiquement, pas déployé, pas en production.

## Rôle

Le moteur P1 (`crates/sml-tutor/src/engine.rs`) exécute un curriculum jusqu'au bout. `sml-tutord` est la boucle permanente au-dessus : il choisit le cycle suivant, refuse de démarrer si l'hôte est hors budget, et journalise chaque décision pour reprendre après un arrêt sans perte ni duplication.

| Élément | Fichier | Garantie |
| --- | --- | --- |
| Machine à états | `supervisor.rs` `SupervisorState::can_go` | transitions en liste blanche ; `EmergencyStopped` terminal (refus de réouverture) |
| Journal durable | `<root>/supervisor.log` (`SML_TUTORD_LOG_V1`) | une ligne par événement, `fsync`, chaîne SHA-256 (`seq`, `prev`, champs) ; queue déchirée tronquée ; toute autre anomalie = échec fermé |
| File persistante | dérivée du journal (event sourcing) | une seule source de vérité |
| Identité idempotente | `JobSpec::id` = SHA-256(kind, params, deps triées) | resoumettre = no-op ; priorité hors identité |
| Dépendances acycliques | une dépendance doit être un job déjà connu | acyclique par construction |
| Priorités, backpressure | `next_ready`, `SupervisorConfig::max_queued` | priorité puis FIFO ; soumission refusée au-delà de la borne |
| Tentatives bornées, temporisation, quarantaine | `record_failure` | backoff `base·2^(n-1)` plafonné ; quarantaine à `max_attempts` ; jamais relancé automatiquement |
| Reprise | `Supervisor::open` | verrou exclusif `supervisor.lock` (une seule instance par racine) ; un job `Running` au démarrage est remis en file immédiatement (tentative comptée) ; un job épuisé resté en backoff après un crash est mis en quarantaine ; le moteur reprend son propre journal dans le même répertoire de cycle |
| Disjoncteur | `SupervisorConfig::breaker_quarantines` (3 par défaut) | après N quarantaines consécutives sans succès : `Running` → `Maintenance`, plus aucun cycle ni planification ; compteur rejoué depuis le journal (un redémarrage ne le réarme pas) ; seul un `resume` opérateur le remet à zéro |
| Gate ressources | `host_probe.rs` `ResourceBudget::admit` | RAM disponible, charge/CPU, disque libre ; lecture inconnue = refus |
| Verrou hôte partagé | `resource_lock=PATH` | `flock` exclusif non bloquant pris pour la durée d'un seul cycle puis relâché ; occupé ⇒ tick refusé par le gate (`resource lock … busy`) puis réessayé après `idle_ms` : les autres jobs du Dell attendent au plus un cycle |
| Cadence | `cycle_gap_ms=N` | pause de N ms après chaque job réussi, interrompue dès qu'une commande opérateur attend dans `control` ; borne la croissance du store et la part d'hôte d'une exécution continue (0 = pas de pause) |
| Archivage | `archive_cycles=1` | chaque cycle réussi est empaqueté dans `archive/<job>.smlpack` (format `SML_TUTOR_PACK_V1` : SHA-256 par fichier et SHA-256 final), fsync, relu octet par octet contre le répertoire, et seulement alors le répertoire est retiré ; en cas d'échec le répertoire est conservé (`SML_TUTORD_ARCHIVE=FAIL kept_dir=true`). `sml-tutord unpack ARCHIVE DEST` restaure le cycle à l'identique. Mesure locale sur 6 cycles : 888 Kio au lieu de 2656 Kio, 1 fichier au lieu d'environ 74 par cycle |
| Mémoire bornée | `SupervisorConfig::keep_succeeded` (256) | au-delà des 256 derniers jobs réussis, un job réussi n'est plus gardé qu'à l'état d'identifiant, avec des compteurs agrégés (par type, `new_certified`, `reexamined`). Le journal reste complet et la compaction est rejouée à l'identique. `submit` reste idempotent et les dépendances restent résolues. `health.json` ajoute `compacted_succeeded`. Mesure locale sur 3000 cycles : environ 72 octets par job au lieu d'environ 1,34 Kio |
| Modes | `Mode::{Simulation, Production}` | enregistré dans chaque événement de démarrage ; `production` refusé sans `<root>/PRODUCTION_ADMITTED` |
| Pilotage | `<root>/control` : `pause`, `resume`, `maintenance`, `drain`, `stop`, `emergency` | lu puis supprimé à chaque tick |
| Santé | `<root>/health.json` (`SML_TUTORD_HEALTH_V1`), écrit atomiquement | état, mode, uptime, files, acquisitions, dernière réussite, dernière erreur, refus du gate |
| Rollback | `CycleRunner::verify` puis `rollback` | audit du store du cycle ; échec ⇒ déplacement non destructif vers `<root>/rolled-back/` ; échec du rollback ⇒ arrêt d'urgence |

## Charge actuelle

Deux types de job sont branchés :

- `anf-m2` : un curriculum ANF GF(2) (Mathématiques M2, induction native) avec une graine nouvelle à chaque cycle, dans son propre store `<root>/cycles/<job>`. Les étiquettes FR/EN/Rust internes à ce curriculum servent uniquement à tester l'équité des voies : **ce ne sont pas des leçons de langue ni de Rust**. Compté en `new_certified`.
- `l1-retention` (optionnel, arguments `l1_native`, `l1_fr`, `l1_en`, `l1_fr_sha`, `l1_en_sha`, `l1_every`) : les deux banques L1 natives déjà certifiées (français, anglais) sont réadmises par le moteur P1 dans un store neuf. Chaque cohorte doit repasser à 100 % (1 792 cas tenus à l'écart au total) et le checkpoint publié doit être identique octet pour octet à la banque parente. Compté en `reexamined`, **jamais** en `new_certified`. Un job sur `l1_every` est de ce type. La provenance (SHA des manifests parents) fait partie de l'identité du job ; une configuration différente fait échouer le job au lieu de le réinterpréter. Les fichiers de corpus sont relus à chaque cycle (fichier régulier, pas de lien symbolique, 64 KiB au plus).

Le backend R1 (#1059) n'est pas encore un type de job : son code généré doit d'abord être qualifié dans un bac à sable isolé.

## Limites connues

- `inputs` est borné à `anf_lesson::MAX_INPUTS` (12) à l'analyse des arguments et à nouveau par job.

- Un cycle est synchrone : `stop`/`emergency` sont pris en compte entre deux cycles, pas pendant.
- Un store par cycle ; pas encore de store unifié inter-cycles ni de réexamen de rétention des cycles précédents.
- Pas encore d'unité systemd, de métriques exportées ni d'alertes (P3).

## Commandes

```bash
cargo test --locked -p sml-tutor supervisor host_probe
cargo build --locked --release -p sml-tutor --bin sml-tutord
target/release/sml-tutord root=/tmp/tutord max_cycles=2 per_domain=16 inputs=10 degree=3
echo pause > /tmp/tutord/control
```

Sorties lisibles par machine : `SML_TUTORD_START`, `SML_TUTORD_CYCLE=PASS|FAIL`, `SML_TUTORD_BREAKER`, `SML_TUTORD_GATED`, `SML_TUTORD_CONTROL`, `SML_TUTORD_EXIT`.
