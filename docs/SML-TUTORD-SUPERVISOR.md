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
| Reprise | `Supervisor::open` | un job `Running` au démarrage est remis en file immédiatement (tentative comptée) ; le moteur reprend son propre journal dans le même répertoire de cycle |
| Gate ressources | `host_probe.rs` `ResourceBudget::admit` | RAM disponible, charge/CPU, disque libre ; lecture inconnue = refus |
| Modes | `Mode::{Simulation, Production}` | enregistré dans chaque événement de démarrage ; `production` refusé sans `<root>/PRODUCTION_ADMITTED` |
| Pilotage | `<root>/control` : `pause`, `resume`, `maintenance`, `drain`, `stop`, `emergency` | lu puis supprimé à chaque tick |
| Santé | `<root>/health.json` (`SML_TUTORD_HEALTH_V1`), écrit atomiquement | état, mode, uptime, files, acquisitions, dernière réussite, dernière erreur, refus du gate |
| Rollback | `CycleRunner::verify` puis `rollback` | audit du store du cycle ; échec ⇒ déplacement non destructif vers `<root>/rolled-back/` ; échec du rollback ⇒ arrêt d'urgence |

## Charge actuelle

Le seul type de job branché est `anf-m2` : un curriculum ANF GF(2) (Mathématiques M2, induction native) avec une graine nouvelle à chaque cycle, dans son propre store `<root>/cycles/<job>`. Les étiquettes FR/EN/Rust internes à ce curriculum servent uniquement à tester l'équité des voies : **ce ne sont pas des leçons de langue ni de Rust**. Les backends L1 (#1058) et R1 (#1059) sont les prochains types de job.

## Limites connues

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

Sorties lisibles par machine : `SML_TUTORD_START`, `SML_TUTORD_CYCLE=PASS|FAIL`, `SML_TUTORD_GATED`, `SML_TUTORD_CONTROL`, `SML_TUTORD_EXIT`.
