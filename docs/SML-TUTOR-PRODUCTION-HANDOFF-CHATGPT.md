# SML-TUTOR — passation production vers ChatGPT

Date : 2026-11-27, mise à jour vers 16 h 30 UTC (première version vers 11 h 40 UTC). Rédigé par Claude à partir des dépôts `Memorithm/SML-GENIUS` (`main` = `1b636c6`) et `Memorithm/RemoteOps` (`main` = `5c38f14`, après #2093), et des journaux GitHub Actions relus le jour même.

Règles de lecture :
- **PHYSIQUE** signifie qu'un job a tourné sur `dell-t430-remoteops-x64-01` via RemoteOps et que sa sortie a été relue.
- **CI** signifie des tests automatiques sur GitHub.
- **CONTENEUR** signifie une exécution dans le conteneur cloud de Claude. Ce n'est jamais une preuve de performance.
- Une durée citée est une durée qui s'est réellement écoulée. Aucun réexamen n'est compté comme nouvelle connaissance.
- L'inventaire exhaustif de l'API est dans [`SML-TUTOR-API-INVENTORY.md`](SML-TUTOR-API-INVENTORY.md) : 290 items `pub`, formats sur disque, lignes machine, tests.

---

## A. Revue scientifique et architecturale : état réel de chaque brique

Colonnes : conçu / codé / testé en CI / fusionné / qualifié physiquement / déployé / actif en production.

| Brique | Conçu | Codé | CI | Fusionné | Physique | Déployé | Production | Preuve |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Moteur P1 (registre, DAG, ordonnanceur 9 voies, exécuteur, examinateur 100 %, store transactionnel, journal, reprise) | oui | oui | oui | oui | oui | non | non | SML-GENIUS #1056 ; bench Dell run 45971988952 |
| Sélection d'outillage Rust (rustc, rustdoc et cargo 1.89.0 épinglés ensemble, fin de E0514) | oui | oui | oui | oui | oui | sans objet | sans objet | RemoteOps #2084 ; runs 45971988952 et suivants |
| Apprenant L1 FR/EN natif branché sur le moteur | oui | oui | oui | oui | oui | non | non | #1058 (ChatGPT) ; run 45963415854 (fr/en 896/896, V9 inchangé) |
| Apprenant Rust R1 (synthèse d'expressions, examen exhaustif par le compilateur) | oui | oui | oui | oui | **non vérifié** | non | non | #1059 (ChatGPT) ; aucun workflow physique trouvé |
| Curriculum mixte FR + EN + maths | oui | oui | oui | oui | **non vérifié** | non | non | #1060 (ChatGPT, fusionnée 09:49Z) |
| Dialogue bilingue C1 (10 tours) | oui | oui | oui | oui | oui | non | non | #1054 ; run 45882986028 : fr 160/160, en 160/160, `teacher_inference=none`, store inchangé |
| C2 à C4 | C2 précurseur seulement (#1055) | partiel | partiel | #1055 | non | non | non | — |
| Superviseur permanent `sml-tutord` | oui | oui | oui | oui | oui (mode simulation) | non | non | #1061 et #1063 ; run 45982603559 |
| Gate ressources hôte (RAM, charge par CPU, disque ; échec fermé) | oui | oui | oui | oui | oui (inclus dans la simulation) | non | non | `host_probe.rs` ; run 45982603559 |
| Disjoncteur de quarantaines consécutives | oui | oui | oui | oui | non (testé en unitaire uniquement) | non | non | #1063 |
| Commit groupé du store (fsync parallèles, un fsync de répertoire par commit) | oui | oui | oui | oui | oui (mesure A/B) | sans objet | non | #1064 ; run 45985324296 |
| Job `l1-retention` dans le superviseur (réexamen des banques L1 fr et en) | oui | oui | oui | oui | oui (mode simulation) | non | non | #1066 ; runs 45999833595 (phase E) et 46004010532 (33 réexamens) |
| Jobs R1 et mixtes dans le superviseur | oui | **non** | non | non | non | non | non | seuls `anf-m2` et `l1-retention` sont branchés |
| Archivage vérifié des cycles (`archive_cycles=1`, `unpack`) | oui | oui | oui | oui | oui (endurance 1 h) | non | non | #1068 ; RemoteOps #2093 |
| Professeur LFM3-1.7B CUDA (RTX 4060) à la demande, non résident | oui | oui (hors moteur) | oui | oui | oui (acquisition L1) | non | non | run 45864449915 ; **pas encore appelé par `sml-tutord`** |
| Cohortes TRAIN_RECALL, DEV, TEST, TESTTEST, RELOAD, RETENTION | oui | oui | oui | oui | oui (via le moteur) | non | non | `mastery.rs::Cohort` |
| Cohortes TRANSFER et ROBUSTNESS | voie `Transfer` seulement | **non** | non | non | non | non | non | aucune variante `Cohort` correspondante |
| Unité systemd (modèle durci, redémarrage sur échec, drain à l'arrêt) | oui | oui | oui | oui | oui (unité transitoire) | **non installée** | non | RemoteOps #2092 ; run 46014329451 |
| Endurance 1 h | oui | oui | oui | oui | oui (simulation) | sans objet | non | RemoteOps #2091 ; run 46004010532 (`wall_s=3605`) |
| Endurance 6 h | oui | oui | oui | oui | oui (simulation) | sans objet | non | RemoteOps #2104 ; run 46049084161 (`wall_s=21611`, RSS en plateau) |
| Endurance 24 h, 72 h, 7 j | oui | oui (`minutes=1440` pour 24 h) | oui | oui | **non exécutée** | non | non | — |
| Tableau de bord versionné JSON et Markdown | `health.json` seulement | partiel | oui (`health.json`) | oui | oui (`health.json`) | non | non | `SML_TUTORD_HEALTH_V1` |
| Mémoire V9 de référence (9 compétences) | — | — | — | — | vérifiée inchangée avant et après chaque run cité | — | — | `SML_TUTOR_DURABLE_VERIFY=PASS head=1142ff9d94d363f27471bdcbe986df790a66c9bb344794694d03a315971eb3f2 depth=9` |

Conclusion : **rien n'est déployé ni actif en production.** Le superviseur est qualifié en simulation sur deux types de job (`anf-m2` et `l1-retention`), sous systemd (unité transitoire) et sur 1 h d'endurance. La bascule en production exige une admission explicite, matérialisée par le fichier `<root>/PRODUCTION_ADMITTED`, qui n'a jamais été créé.

### Jugement scientifique

- Les leçons `anf-m2` sont toutes de l'algèbre booléenne (induction ANF sur GF(2)). Leurs étiquettes FR, EN et Rust servent seulement à tester l'équité entre les voies : **ce ne sont pas des compétences linguistiques ni Rust.**
- La séparation des rôles tient par les types :
  - l'ordonnanceur ne voit ni l'examinateur ni le store ;
  - `mastery::certify` est le seul constructeur de `Certificate` ;
  - le store n'accepte que des entrées certifiées ;
  - le professeur n'est jamais digne de confiance : un oracle indépendant vérifie chaque proposition.
- La cognition de l'élève n'est pas déléguée au professeur. Le professeur étiquette des exemples TRAIN, l'apprenant natif induit, et l'examinateur juge sur des cohortes disjointes.
- La règle 100 % est fixe, sans paramètre de seuil ; une abstention compte comme un échec.

---

## B. Carte technique

### B.1 Dépôts, chemins, branches

| Élément | Chemin |
| --- | --- |
| Crate | `SML-GENIUS/crates/sml-tutor/` (std uniquement, `#![forbid(unsafe_code)]`, dépendance `sml-core` pour SHA-256) |
| Binaire superviseur | `crates/sml-tutor/src/bin/sml-tutord.rs` |
| Exemples | `examples/sml_tutor_engine_bench.rs`, `examples/sml_tutor_bilingual_l1.rs`, `examples/sml_tutor_mixed_l1_anf.rs` |
| Docs | `docs/SML-TUTOR-ENGINE-P1.md`, `docs/SML-TUTOR-L1-ENGINE-BRIDGE.md`, `docs/SML-TUTORD-SUPERVISOR.md`, ce document, `docs/SML-TUTOR-API-INVENTORY.md` |
| Scripts physiques | `RemoteOps/ops/sml-tutor/` : `rust-toolchain.sh`, `run-engine-bench-p1.sh`, `run-engine-l1-physical.sh`, `run-supervisor-sim.sh`, `run-engine-bench-ab.sh`, `checkpoint-store.py`, `run-c1-dialogue.sh` |
| Workflows | `RemoteOps/.github/workflows/` : `sml-tutor-engine-bench.yml`, `sml-tutor-engine-l1.yml`, `sml-tutor-supervisor-sim.yml`, `sml-tutor-engine-bench-ab.yml`, `sml-tutor-c1-dialogue.yml` (environ 50 autres workflows `sml-tutor-*` existent pour V1 à V9, LFM3, dictionnaires) |
| Store V9 durable (lecture seule pour tout ce qui précède) | `$HOME/.local/share/memorithm/sml-tutor/experiment-store-v1` sur le Dell |

### B.2 Modules

| Module | Rôle | Types et fonctions clés |
| --- | --- | --- |
| `domain.rs` | disciplines, voies, états | `Domain` (4), `Lane` (9 : french, english, mathematics, rust, revision, validation, gap_detection, consolidation, transfer), `KnowledgeState` (10) et `can_transition` (liste blanche) |
| `lesson.rs`, `registry.rs` | leçons à identité SHA-256, registre idempotent | `LessonSpec`, `LessonId`, `LessonRegistry` ; acquisition refusée si les prérequis ne sont pas `Certified` ou `Consolidated` ; invalidation transitive |
| `graph.rs` | DAG interdomaines | `DependencyGraph::add_node`, `add_edge` (rejet des cycles), `prerequisites`, `dependents` |
| `scheduler.rs` | ordonnancement par voie | files de priorité, round robin à déficit pondéré, backpressure, déduplication, AIMD |
| `executor.rs` | parallélisme | `run_parallel` (vol de tâches, `catch_unwind`), `CancellationToken`, `JobOutcome` |
| `resources.rs` | baux machines | seules les machines `AiCompute` mesurées et en ligne sont éligibles ; `IosBuildReserved` (Mac mini) et `ControlPlane` ne sont jamais choisies |
| `teacher.rs` | professeurs et oracle | traits `Teacher` et `Oracle`, `Residency::NonResident`, `TeacherLedger` |
| `mastery.rs` | examinateur | `Cohort { TrainRecall, Dev, Test, TestTest, Reload, Retention(Digest) }`, `certify`, `Certificate`, `ExamReport`, `check_partitions` |
| `store.rs` | mémoire cumulative transactionnelle | `KnowledgeStore::{open, commit, commit_rebasing, snapshot, get, history, restore, audit}` ; `write_objects`, privée (commit groupé, #1064) |
| `journal.rs` | journal de transitions | append-only, séquence stricte, queue déchirée tronquée, corruption = échec fermé |
| `engine.rs` (+ `linguistic_backend.rs` via `#[path]`) | boucle continue | `Engine::{open, open_bilingual_l1, open_mixed_l1_anf, run}`, `anf_curriculum`, `EngineConfig`, `RunSummary`, `ResumeReport`, `knowledge_key`, `linguistic_l1_key` |
| `anf_lesson.rs` | apprenant ANF GF(2) | `MAX_INPUTS = 12`, `learn`, `exam`, `encode_checkpoint`, `decode_checkpoint` |
| `telemetry.rs` | compteurs | par domaine et par machine, lignes JSON |
| `supervisor.rs` | superviseur permanent | voir B.3 |
| `host_probe.rs` | gate hôte | `HostSample`, `sample`, `ResourceBudget::admit` (valeurs par défaut : 4 GiB de RAM disponible, charge ≤ 0,85 par CPU, 2 GiB de disque libre ; lecture inconnue = refus) |

### B.3 Superviseur `sml-tutord` (`supervisor.rs`)

- **États :** `SupervisorState { Starting, Running, Paused, Maintenance, Draining, Stopped, EmergencyStopped }`. `can_go` est une liste blanche et `EmergencyStopped` est terminal : le superviseur refuse de rouvrir dans cet état.
- **Modes :** `Mode { Simulation, Production }`.
- **Commandes :** `Command { Pause, Resume, Maintenance, Drain, Stop, Emergency }`, lues dans le fichier `<root>/control`, qui est supprimé après lecture.
- **Jobs :**
  - `JobSpec { kind, params, deps, priority, max_attempts }` ;
  - `JobId = SHA-256("sml-tutord-job-v1", kind, params, deps triées)` : resoumettre le même job ne fait rien, et la priorité ne fait pas partie de l'identité.
- **Exécution d'un cycle :** traits `CycleRunner { run, verify, rollback }` et `Gate { admit }`.
  - Un échec de vérification provoque un rollback non destructif vers `<root>/rolled-back/`.
  - Un échec du rollback provoque un arrêt d'urgence.
- **Configuration :** `SupervisorConfig { max_queued: 256, backoff_base_ms: 30000, backoff_max_ms: 3600000, breaker_quarantines: 3 }`.
  - Backoff `base·2^(n-1)`, plafonné, compté depuis la fin du cycle échoué.
  - Quarantaine à `max_attempts`.
  - Disjoncteur : passage `Running → Maintenance`. Le compteur est rejoué depuis le journal, et seul un `resume` de l'opérateur le remet à zéro.
- **Reprise :**
  - verrou exclusif `supervisor.lock` (`File::try_lock`) ;
  - un job `Running` trouvé à l'ouverture est remis en file immédiatement (sa tentative compte) ;
  - un job épuisé est mis en quarantaine à l'ouverture.
- **API publique :** `open`, `start`, `submit`, `command`, `tick`, `poll_control`, `health_json`, `write_health`, `pending`, `jobs`, `job`, `log_seq`, `log_head`, `consecutive_quarantines`, `breaker_tripped`.
- **Binaire :** arguments `root= mode= workers= per_domain=16 inputs=10 (4..=12) degree=3 max_cycles=0 idle_ms=5000 cycle_gap_ms=0 archive_cycles=0 plan_ahead=2 min_mem_gib min_disk_gib max_load_per_cpu resource_lock=`, plus `l1_native= l1_fr= l1_en= l1_fr_sha= l1_en_sha= l1_every=` pour les jobs L1. Codes de sortie : 0 arrêt, 1 erreur, 2 arguments invalides ou production refusée, 3 arrêt d'urgence.
- **Types de jobs :** `anf-m2` (apprentissage de nouvelles leçons ANF) et `l1-retention` (réexamen des deux banques L1 natives fr et en, sans apprentissage). Un job L1 est planifié tous les `l1_every` jobs ANF. Il est compté en `reexamined`, jamais en `new_certified`. Il vérifie que le checkpoint republié est identique octet pour octet à la banque parente. Une tentative interrompue est déplacée dans `rolled-back/`, puis l'examen repart de zéro (#1066).
- **Cadence :** `cycle_gap_ms=N` fait une pause de N ms après chaque succès. La pause est interrompue dès qu'une commande attend dans `control` et n'a pas lieu pendant un drain (#1067).
- **Verrou partagé :** `resource_lock=` prend le verrou Dell pour un cycle seulement. Les autres jobs appariés attendent donc au plus un cycle.
- **Archivage :** avec `archive_cycles=1`, chaque cycle réussi est empaqueté dans `archive/<job>.smlpack`, au format `SML_TUTOR_PACK_V1`, sans dépendance. Le répertoire n'est retiré qu'après les étapes suivantes :
  - fsync de l'archive, puis renommage ;
  - fsync des répertoires ;
  - relecture de l'archive octet par octet contre le répertoire.
  
  Tout échec conserve le répertoire. Au démarrage, les archivages restés inachevés sont repris. `sml-tutord unpack ARCHIVE DEST` restaure un cycle à l'identique (#1068).

### B.4 Formats de checkpoints et schémas de données

| Fichier | En-tête | Disposition |
| --- | --- | --- |
| `<engine_root>/journal.log` | `SML_TUTOR_JOURNAL_V1` | `T <lesson_hex> <from> <to> <seq>` |
| `<engine_root>/store/HEAD` | `SML_TUTOR_HEAD_V1` | `<version> <manifest_hex\|none>`, renommage atomique |
| `store/manifests/<sha256>` | `SML_TUTOR_MANIFEST_V1` | `version n`, `parent hex\|none`, puis `entry <key> <lesson> <certificate> <object>` triées par clé |
| `store/objects/<sha256>` | aucun (adressé par contenu) | checkpoint ANF ou banque L1 native |
| checkpoint ANF | `SMLANF1\0` | octet 8 `inputs`, octet 9 `max_degree`, octets 10 à 13 `count` (u32 LE), puis `ceil(count/64)` u64 LE ; les bits au-delà de `count` sont nuls |
| `store/LOCK` | `SML_TUTOR_LOCK_V1 <pid> <boot_id>` | un verrou d'un processus mort sur le même hôte est récupéré ; tout autre verrou échoue fermé |
| `<root>/supervisor.log` | `SML_TUTORD_LOG_V1` | `E\t<seq>\t<prev>\t<champs>\t<hash>`, chaîne SHA-256 (`sml-tutord-log-v1`) ; événements `submit`, `start`, `succeed`, `fail`, `quarantine`, `state` ; échappement `%` |
| `<root>/health.json` | `"schema":"SML_TUTORD_HEALTH_V1"` | état, mode, uptime, `log_seq`, `log_head`, jobs par statut, `new_certified_total`, `reexamined_total`, dernière réussite, dernière erreur, refus du gate, `interrupted_at_open`, `torn_tail_at_open`, `consecutive_quarantines`, `breaker_tripped` |
| `<root>/cycles/<job_hex>/` | — | racine du moteur pour un cycle (`store/` et `journal.log`) |
| `<root>/archive/<job_hex>.smlpack` | `SMLPACK1` | `u64` nombre de membres, puis par membre : `u64` longueur du chemin, chemin UTF-8 relatif, `u64` taille, SHA-256, octets. Membres triés par chemin. Se termine par un SHA-256 de tout ce qui précède. Liens symboliques, fichiers spéciaux et chemins non sûrs sont refusés. |

### B.5 Workflows physiques et déclenchement

- **Garde commune :** dépôt `Memorithm/RemoteOps`, acteur `CHECKUPAUTO`, `refs/heads/main`, hôte `debian`, utilisateur `remoteops`, `flock -w 300 /tmp/memorithm-bpb-dell-resource.lock`, groupe de concurrence `remoteops-dell-boolean-cuda-paired`.
- **Isolation :** `CUDA_VISIBLE_DEVICES=""` pour les jobs CPU, `TMPDIR` dans un répertoire jetable, store V9 vérifié avant et après.
- **Déclenchement :** `workflow_dispatch` pour `sml-tutor-engine-bench.yml`, `sml-tutor-supervisor-sim.yml` et `sml-tutor-engine-bench-ab.yml`. Les autres se déclenchent par un commentaire exact de CHECKUPAUTO sur la PR #2053 (par exemple `/remoteops sml-tutor engine-bench-p1`).
- **Contrats :** sur `checkupauto-dell-ci-isolated-*` : `bash -n`, et refus de toute épingle `_PENDING`.
- **Épingles :** chaque script fixe une révision SML-GENIUS et les SHA de blob des fichiers clés. Pour tester une nouvelle révision, il faut une PR RemoteOps qui change ces épingles.

---

## C. Résultats mesurés (PHYSIQUE, Dell T430, 64 threads)

**Bench moteur P1, avant #1064** (run 45971988952, 3 répétitions, ANF M2, 256 leçons, `inputs=10`, `degree=3`) : 256/256 consolidées et store identique pour chaque nombre de workers.

| workers | médiane | min–max | accélération de la phase d'apprentissage |
| --- | --- | --- | --- |
| 1 | 4378 ms | 4375–4442 | 1,00 |
| 2 | 3799 ms | 3766–3809 | 2,12 |
| 4 | 3517 ms | 3512–3527 | 3,97 |
| 8 | 3406 ms | 3397–3414 | 6,93 |
| 16 | 3338 ms | 3333–3343 | 10,57 |

**A/B du commit groupé** (run 45985324296, A = `86d1db2`, B = `fdbf1e3`, exécutions alternées A, B ×3, 256/256 partout, store identique, V9 inchangé) :

| workers | A médiane [min–max] | B médiane [min–max] | B/A |
| --- | --- | --- | --- |
| 1 | 4438,6 [4420,2–4443,0] | 2064,3 [2049,5–2077,3] | 2,150 |
| 2 | 3845,3 [3778,4–3896,7] | 1660,5 [1656,3–1668,0] | 2,316 |
| 4 | 3537,7 [3514,7–3537,8] | 1342,8 [1320,3–1388,6] | 2,635 |
| 8 | 3425,0 [3321,3–3455,9] | 1223,6 [1192,5–1300,1] | 2,799 |
| 16 | 3342,7 [3330,7–3363,1] | 1122,8 [1081,4–1189,6] | 2,977 |

Le gain de bout en bout entre 1 et 16 workers passe de 1,31× à 1,84×. Le passage à 32 ou 64 workers n'a pas été mesuré et n'est pas encore justifié : il reste environ 1 s de partie série (fsync par objet, journal, manifeste).

**Simulation du superviseur** (run 45982603559, durée réelle environ 1 min 20) :
- 3 cycles de 256/256 leçons, de 3,3 à 3,4 s chacun (avant #1064) ;
- `kill -9` envoyé quand le journal du cycle comptait 321 lignes, suivi au redémarrage de `interrupted=1` et `resumed_records=320`, avec 256/256 à la fin ;
- deuxième instance refusée avec le code 1 ;
- pause, reprise et arrêt fonctionnent ;
- `mode=production` sans admission refusé avec le code 2, `inputs=13` refusé avec le code 2 ;
- santé finale : 0 quarantaine, disjoncteur non déclenché, V9 inchangé.

**Autres résultats physiques relus :**
- L1 sur moteur : run 45963415854, fr et en 896/896.
- C1 : run 45882986028, 160/160 par langue.
- Acquisition L1 avec le professeur LFM3 CUDA : run 45864449915, 48/48, 896/896, reload 896/896.

**Échec honnête à signaler :** le premier run de simulation (45981962996) a échoué à cause d'un défaut du script de test (il visait le journal d'un cycle déjà terminé). Le défaut est corrigé par #2087.

**Rétention L1 dans le superviseur** (phase E de `run-supervisor-sim.sh`, run 45999833595, SML-GENIUS `1eedf8a`) :
- 2 réexamens L1, chacun `banks=2 held_out=1792` ;
- banques parentes et V9 inchangés.

**Endurance 1 h** (run 46004010532, SML-GENIUS `e2f0d8e`, `workers=8 per_domain=16 l1_every=10 cycle_gap_ms=10000`, verrou par cycle) :
- `wall_s=3605` mesuré ;
- 299 cycles ANF et 33 réexamens L1 ;
- `new_certified_total=19136` et `reexamined_total=66` ;
- 0 échec, 0 quarantaine, 0 refus du gate ;
- chaîne du journal contiguë (`log_seq=1001`) ;
- V9 inchangé.

Ce run a aussi mesuré deux croissances :
- **Mémoire :** la RSS passe de 6116 à 11792 Kio. L'analyse locale montre un plateau d'environ 11,6 Mio dès le premier job L1, puis environ 1,7 Kio par job, ce qui correspond à la table des jobs en mémoire. Ce n'est pas une fuite, mais la croissance est linéaire.
- **Disque :** la racine atteint 135204 Kio. Un cycle occupe environ 440 Kio sur disque pour 133 Kio de données réelles, répartis en environ 74 fichiers. C'est ce qui a motivé l'archivage de #1068.

**Qualification systemd** (run 46014329451, unité transitoire, gestionnaire système) :
- les propriétés de `ops/sml-tutor/systemd/sml-tutord-sim.service` sont appliquées : `CPUQuota=800%`, `MemoryMax=4G`, `TasksMax=256`, `ProtectSystem=strict`, `PrivateNetwork=yes`, capacités vides ;
- un `kill -9` du processus principal, envoyé pendant un cycle quand son journal comptait 489 lignes, donne `NRestarts=1`, puis une reprise du cycle depuis son journal ;
- le drain se termine proprement avec `Result=success` et le code 0, sans nouveau redémarrage ;
- 4 cycles, `new_certified_total=1024`, V9 inchangé.

L'unité n'est **pas installée** de façon permanente : seul le modèle est versionné.

**Endurance 1 h avec archivage** (run 46023151716, SML-GENIUS `1b636c6`, mêmes réglages plus `archive_cycles=1`) :
- `wall_s=3602` mesuré ;
- 298 cycles ANF et 33 réexamens L1 ;
- `new_certified_total=19072` et `reexamined_total=66` ;
- 0 échec et 0 refus du gate ;
- `log_seq=998` ;
- V9 inchangé.

Archivage :
- 331 archives, une par cycle réussi, sans aucun `ARCHIVE=FAIL` ;
- aucun répertoire de cycle restant ;
- `unpack` de la dernière archive : `PASS files=75`.

Racine : **45352 Kio après 1 h, contre 135204 Kio sans archivage**, soit 3,0 fois moins. La croissance est linéaire, environ 3,7 Mio toutes les 5 minutes, soit environ 44 Mio/h à cette cadence de test.

Trajectoire RSS (Kio) :

| t (s) | 300 | 600 | 900 | 1200 | 1800 | 2400 | 3000 | 3601 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| RSS | 9148 | 10192 | 10864 | 11048 | 11112 | 11708 | 11820 | 11968 |
| racine | 3848 | 7704 | 11524 | 15316 | 23036 | 30576 | 38076 | 45352 |

La RSS monte vite pendant les 20 premières minutes (montée en charge des banques L1). Elle augmente ensuite d'environ 0,9 Mio en 40 minutes, ce qui correspond à la table des jobs en mémoire et confirme l'analyse locale. Ce n'est pas un plateau strict.

**Endurance 6 h avec compaction de la table des jobs** (run 46049084161, SML-GENIUS `8ad4959` (#1071), `minutes=360`, mêmes réglages, du 2026-11-27 à 18 h 24 UTC au 2026-11-28 à 00 h 24 UTC) :
- `wall_s=21611` mesuré ;
- 1762 cycles ANF et 195 réexamens L1 ;
- `new_certified_total=112768` et `reexamined_total=390` ;
- 0 échec et 0 refus du gate ;
- `log_seq=5876` ;
- V9 inchangé.

Archivage et compaction :
- 1957 archives, toutes vérifiées ;
- `unpack` de la dernière archive : `PASS files=75` ;
- 1701 jobs compactés.

**RSS : plateau.** La RSS atteint 11996 Kio à 1 h, puis 12184 Kio à 3 h et 12272 Kio à 6 h, soit environ +55 Kio/h après la première heure, contre environ +1,3 Mio/h avant #1071.

**Disque : linéaire.** La racine atteint 268648 Kio en 6 h, soit environ 44 Mio/h à cette cadence de test. Ce n'est pas une fuite : c'est le volume des archives de cycles, qui sont la mémoire conservée.

**Durées non exécutées :** 24 h, 72 h et 7 j. Aucun chiffre n'existe pour ces durées.

---

## D. Invariants à ne jamais casser

1. Toute opération physique passe par RemoteOps, sous le verrou et le groupe de concurrence ci-dessus. Le Mac mini n'est jamais utilisé.
2. Ne jamais affaiblir les contrôles de sécurité, de certification ou RSI. La règle 100 % n'a pas de seuil réglable, et les doctests ne doivent pas être désactivés.
3. Ne jamais détruire une mémoire antérieure. Le store V9 est vérifié avant et après chaque run ; les rollbacks déplacent au lieu de supprimer ; les objets et les manifestes sont immuables.
4. Ne jamais fabriquer de résultats. Un chiffre de conteneur ne doit jamais être présenté comme physique.
5. Le code généré (par exemple Rust R1) s'exécute isolé, sans secrets, réseau ni accès aux stores de production, avec des limites de CPU, de RAM, de disque, de processus et de durée.
6. Aucune modification autonome des règles de sécurité ou de certification en production. La production n'est atteignable qu'avec `PRODUCTION_ADMITTED`, posé par l'opérateur après admission explicite des critères.
7. LFM3-1.7B CUDA sur la RTX 4060 reste non résident entre deux lots (`Residency::NonResident`).
8. La boolean/ANF/SBG reste l'architecture. Pas de Transformer pour l'élève.

---

## E. Limites et blocages connus

- Un cycle est synchrone : `stop` et `emergency` ne sont pris en compte qu'entre deux cycles.
- Chaque cycle a son propre store (`cycles/<job>/store`, ou son archive). Il n'existe pas encore de store unifié entre cycles et aucun lien vers le store V9. La rétention est réexaminée pour les banques L1, mais pas encore pour les leçons ANF des cycles précédents.
- La table des jobs est compactée au-delà des 256 derniers jobs réussis (#1071). Sur 6 h physiques, la RSS est en plateau à environ 12 Mio. Le disque reste linéaire, environ 44 Mio/h à la cadence de test et environ 6 fois moins à celle de l'unité. Avant 72 h ou 7 j, il faudra une politique de rétention, ou une consolidation des archives dans un store unifié.
- Le superviseur lance `anf-m2` et `l1-retention`. R1 et les jobs mixtes ne sont pas branchés, et le professeur LFM3 n'est pas appelé par le superviseur.
- Les cohortes TRANSFER et ROBUSTNESS n'existent pas.
- R1 et le curriculum mixte (#1059, #1060) n'ont pas de qualification physique connue.
- L'unité systemd est qualifiée mais n'est pas installée. Il n'y a ni métriques exportées, ni alertes, ni tableau de bord versionné au-delà de `health.json`.
- L'hôte des artefacts GitHub (`productionresultssa8.blob.core.windows.net`) n'est pas joignable depuis la session de l'agent. Les chiffres utiles sont donc imprimés dans le log du job (`SML_TUTORD_ENDURANCE_SAMPLE`).
- La partie série résiduelle est d'environ 1 s par cycle de 256 leçons sur le Dell.

---

## F. Prochaines étapes recommandées (par ordre)

1. **Endurance longue.** Lancer 24 h (`minutes=1440`), puis 72 h et 7 j une fois le disque traité. Ne rapporter que les durées réellement écoulées.
2. **Croissance disque.** Consolider les archives de cycles dans un store cumulatif, ou définir une rétention explicite décidée par l'opérateur, sans jamais détruire de mémoire certifiée.
3. **Store unifié et rétention ANF.** Consolider les cycles dans un store cumulatif par `commit_rebasing`. Ajouter des réexamens périodiques des leçons ANF déjà certifiées, comptés en `reexamined`.
4. **R1 Rust.** Lancer d'abord une qualification physique isolée (bac à sable sans réseau, avec `ulimit` et `timeout`), avant tout branchement dans le superviseur.
5. **Installation de l'unité systemd** en simulation sur le Dell, par une PR RemoteOps explicite, sur décision de l'opérateur.
6. **Optimisation.** Pipeliner la consolidation avec la vague suivante et grouper les fsync du journal. Mesurer avec `sml-tutor-engine-bench-ab.yml` en changeant les épingles A et B.
7. **Production.** Seulement après les étapes 1 à 5, et sur admission explicite de l'opérateur (`PRODUCTION_ADMITTED`).

---

## G. Reproduire

```bash
# SML-GENIUS
cargo test --locked -p sml-tutor                 # 131 tests unitaires + 1 d'exemple + doctests
cargo clippy --locked -p sml-tutor --all-targets -- -D warnings
cargo run --release -p sml-tutor --example sml_tutor_engine_bench -- per_domain=64 inputs=10 degree=3 workers=1,2,4,8,16 lie_every=0
cargo build --locked --release -p sml-tutor --bin sml-tutord
target/release/sml-tutord root=/tmp/tutord max_cycles=2 per_domain=16 inputs=10 degree=3 archive_cycles=1
echo pause > /tmp/tutord/control; cat /tmp/tutord/health.json
target/release/sml-tutord unpack /tmp/tutord/archive/<job>.smlpack /tmp/cycle-restored

# RemoteOps (CHECKUPAUTO, depuis main)
#   Actions → "SML TUTOR sml-tutord physical simulation qualification" → Run workflow
#   Actions → "SML TUTOR engine A/B physical bench (store group commit)" → Run workflow
#   Actions → "SML TUTOR sml-tutord endurance (simulation)" (entrée `minutes`) → Run workflow
#   Actions → "SML TUTOR sml-tutord systemd service qualification (simulation)" → Run workflow
```

## H. PR de cette phase

| Dépôt | PR | Objet | Auteur |
| --- | --- | --- | --- |
| RemoteOps | #2084 | outillage Rust épinglé (E0514), `workflow_dispatch` du bench | Claude |
| SML-GENIUS | #1061 | superviseur `sml-tutord`, gate hôte | Claude |
| SML-GENIUS | #1063 | `inputs ≤ 12`, disjoncteur | Claude |
| RemoteOps | #2086, #2087 | qualification physique du superviseur | Claude |
| SML-GENIUS | #1064 | commit groupé du store | Claude |
| RemoteOps | #2089 | bench A/B physique | Claude |
| SML-GENIUS | #1066 | jobs `l1-retention` dans le superviseur | Claude |
| SML-GENIUS | #1067 | cadence `cycle_gap_ms` | Claude |
| SML-GENIUS | #1068 | archivage vérifié des cycles | Claude |
| RemoteOps | #2090 | phase E (rétention L1 physique) | Claude |
| RemoteOps | #2091 | endurance 1 h | Claude |
| RemoteOps | #2092 | qualification systemd, modèle d'unité | Claude |
| RemoteOps | #2093 | endurance avec archivage | Claude |
| SML-GENIUS | #1071 | compaction de la table des jobs | Claude |
| RemoteOps | #2104 | durée d'endurance au choix (60, 360 ou 1440 min) | Claude |
| SML-GENIUS | #1058, #1059, #1060 | L1 sur moteur, R1 Rust, curriculum mixte | automatisation ChatGPT |
