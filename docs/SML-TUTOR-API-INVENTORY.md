# Inventaire technique du crate `sml-tutor`

Source : `crates/sml-tutor` à `main` = `fdbf1e3` (2026-11-27). Généré par lecture du code ; à régénérer après toute modification de l'API.
Chemins de section relatifs à la racine du crate. Descriptions tirées des commentaires `///` et `//!` du code, traduites en français ; `(non documenté)` quand l'item n'a pas de commentaire.

**Méthode de comptage**

- « Item pub » = déclaration `pub struct`, `pub enum`, `pub trait`, `pub type`, `pub const` (dont `pub const fn`), `pub fn` (libre ou méthode d'`impl`). Total : **290** (57 struct, 28 enum, 4 trait, 1 type, 14 const dont 7 `const fn`, 186 fn).
- Non comptés dans les 290 : 17 `pub mod` et 1 `pub use` de `lib.rs` / `engine.rs` ; variantes d'enum et méthodes de trait (listées sur la ligne de leur parent) ; champs `pub` ; `pub(crate)` et `pub(super)` (exclus, sauf mention explicite).
- Le module `linguistic_backend.rs` est inclus dans `engine.rs` par `#[path]` (module privé `engine::linguistic`) ; ses items publics sont exposés par `pub use` dans `engine`.
- Crate : `#![forbid(unsafe_code)]`, dépendance unique `sml-core` (SHA-256).

---

## src/lib.rs

Racine du crate : déclare les 17 modules publics, interdit `unsafe` et énonce la séparation des quatre rôles (ordonnanceur, enseignants, examinateur, consolidateur).

- `pub mod` anf_lesson, domain, engine, executor, graph, hash, host_probe, journal, lesson, mastery, registry, resources, scheduler, store, supervisor, teacher, telemetry (17 déclarations, non comptées dans les 290).

## src/anf_lesson.rs

Apprenant ANF GF(2) natif (induction de fonctions booléennes par élimination de Gauss sur GF(2)), utilisé comme charge de qualification de la voie Mathématiques, avec encodage des checkpoints.

- `MAX_INPUTS: u8 = 12` — Nombre maximal de variables d'entrée acceptées par les points d'entrée validés.
- `CHECKPOINT_MAGIC: &[u8; 8] = b"SMLANF1\0"` — Préfixe magique d'un checkpoint encodé.
- `enum AnfLessonError { InputsOutOfRange(u8), TooManySamples { requested, available }, BadMagic, Truncated, TrailingBytes(usize), CountMismatch { expected, found }, StrayBits }` — Erreurs du moteur ANF.
- `struct SplitMix64` — Générateur pseudo-aléatoire SplitMix64 déterministe.
  - `new(seed: u64) -> Self` — (non documenté)
  - `next_u64(&mut self) -> u64` — (non documenté)
  - `below(&mut self, bound: u64) -> u64` — Valeur uniforme dans `0..bound` par rejet ; borne nulle donne 0.
- `struct AnfTask { inputs, max_degree, target: Vec<u64> }` — Fonction booléenne cible sous forme ANF.
- `struct Partitions { train, dev, test, testtest: Vec<u32> }` — Ensembles d'entrées disjoints pour apprentissage, développement, test et test held-out.
  - `is_disjoint(&self) -> bool` — Vrai si aucune valeur d'entrée n'apparaît deux fois.
- `enum LearnOutcome { Learned { coeffs: Vec<u64> }, Underdetermined { rank, unknowns }, Inconsistent }` — Résultat de `learn`.
- `monomials(inputs: u8, max_degree: u8) -> Vec<u32>` — Sous-ensembles de `{0..inputs}` de taille au plus `max_degree`, en masques `u32` croissants.
- `evaluate(monos: &[u32], coeffs: &[u64], x: u32) -> bool` — Valeur de l'ANF en `x` (XOR des monômes actifs satisfaits par `x`).
- `random_task(inputs: u8, max_degree: u8, seed: u64) -> AnfTask` — Fonction cible aléatoire, chaque coefficient à probabilité 1/2.
- `partition(inputs: u8, train: usize, dev: usize, test: usize, testtest: usize, seed: u64) -> Result<Partitions, AnfLessonError>` — Découpe les `2^inputs` entrées en tranches disjointes après un mélange seedé.
- `learn(inputs: u8, max_degree: u8, examples: &[(u32, bool)]) -> LearnOutcome` — Retrouve les coefficients ANF par Gauss-Jordan sur GF(2) ; s'abstient si le rang est insuffisant.
- `exam(inputs: u8, max_degree: u8, coeffs: &[u64], target: &AnfTask, cases: &[u32]) -> (u64, u64)` — Compte les cas corrects de l'hypothèse contre la cible ; renvoie `(correct, total)`.
- `encode_checkpoint(inputs: u8, max_degree: u8, coeffs: &[u64]) -> Vec<u8>` — Sérialise un checkpoint ; les bits au-delà du nombre de monômes sont supprimés.
- `decode_checkpoint(bytes: &[u8]) -> Result<(u8, u8, Vec<u64>), AnfLessonError>` — Analyse un checkpoint (magic, longueurs, nombre de coefficients, bits parasites).

## src/domain.rs

Disciplines, voies d'apprentissage (lanes) et machine d'états explicite des connaissances.

- `enum Domain { French, English, Mathematics, Rust }` — Les quatre disciplines de priorité ; chaque leçon appartient à exactement une.
  - `ALL: [Domain; 4]` — (non documenté)
  - `name(self) -> &'static str` (const fn) — (non documenté)
  - `parse(s: &str) -> Option<Self>` — (non documenté)
  - `acquisition_lane(self) -> Lane` (const fn) — Voie d'acquisition qui sert cette discipline.
- `enum Lane { French, English, Mathematics, Rust, Revision, Validation, GapDetection, Consolidation, Transfer }` — Files d'apprentissage spécialisées maintenues par l'ordonnanceur.
  - `ALL: [Lane; 9]` — (non documenté)
  - `index(self) -> usize` (const fn) — (non documenté)
  - `name(self) -> &'static str` (const fn) — (non documenté)
- `enum KnowledgeState { Unknown, Identified, Acquiring, Learned, Validating, Certified, Consolidated, NeedsRevision, Contradictory, Blocked }` — État explicite par connaissance ; `Certified` n'est atteignable que depuis `Validating`.
  - `ALL: [KnowledgeState; 10]` — (non documenté)
  - `name(self) -> &'static str` (const fn) — (non documenté)
  - `parse(s: &str) -> Option<Self>` — (non documenté)
  - `can_transition(self, to: KnowledgeState) -> bool` — (non documenté) ; liste blanche des transitions dans le corps de la fonction.
  - `satisfies_prerequisite(self) -> bool` (const fn) — États pouvant servir de prérequis (`Certified`, `Consolidated`).
  - `is_in_flight(self) -> bool` (const fn) — États de travail perdu lors d'un crash (`Acquiring`, `Learned`, `Validating`).

## src/engine.rs

Boucle d'apprentissage continu (OBSERVE, IDENTIFY GAP, TEACH, LEARN, EXAMINE, VALIDATE, CONSOLIDATE, REVISE) qui câble journal, registre, ordonnanceur, magasin, ressources et apprenants ANF et L1.

- `pub use linguistic::{linguistic_l1_key, L1Input}` — Ré-exportation depuis `linguistic_backend.rs` (non comptée).
- `struct AnfLessonParams { inputs, degree, seed, base_train, max_train, dev, test, testtest }` — Paramètres d'une leçon ANF ; tout est dérivé de façon déterministe.
  - `task(&self) -> AnfTask` — (non documenté)
  - `partitions(&self) -> Result<Partitions, EngineError>` — (non documenté)
  - `unknowns(&self) -> usize` — (non documenté)
- `struct AnfCurriculum { specs: Vec<LessonSpec>, params: BTreeMap<LessonId, AnfLessonParams> }` — (non documenté)
- `knowledge_key(lesson: &LessonId) -> Digest` — Clé de connaissance sous laquelle le checkpoint d'une leçon est publié.
- `anf_curriculum(per_domain: usize, inputs: u8, degree: u8, seed: u64) -> AnfCurriculum` — Curriculum de qualification : `per_domain` leçons par discipline, chaînes intra-domaine et arêtes inter-domaines ; une leçon sur cinq démarre sous-déterminée.
- `struct AnfTeacher` — Enseignant déterministe : étiquette les lignes TRAIN à partir de la vraie fonction, en mentant sur une ligne sur `lie_every` ; jamais digne de confiance (implémente `Teacher`).
- `struct AnfOracle` — Oracle indépendant exact qui réévalue la fonction du générateur (implémente `Oracle`).
- `enum EngineError { Registry(RegistryError), Journal(JournalError), Store(StoreError), Curriculum(String), ForeignJournal(LessonId) }` — (non documenté)
- `struct EngineConfig { workers, wave, max_attempts, teacher_lie_every, crash_after_waves }` — (non documenté) ; `Default` : workers 1, wave 64, max_attempts 6, honnête, pas de crash simulé.
- `struct ResumeReport { replayed_records, torn_tail, interrupted_reset, consolidated_missing_from_store }` — (non documenté)
- `struct RunSummary { waves, wall_nanos, learn_phase_nanos, stolen, crashed, store_version, states }` — (non documenté)
- `struct Engine { ... pub ledger: TeacherLedger, pub telemetry: Telemetry, ... }` — (non documenté)
  - `open(root: &Path, curriculum: AnfCurriculum, cfg: EngineConfig) -> Result<(Self, ResumeReport), EngineError>` — Ouvre (ou reprend) un moteur enraciné à `root`.
  - `open_bilingual_l1(root: &Path, native_learner: &Path, inputs: [L1Input; 2], cfg: EngineConfig) -> Result<(Self, ResumeReport), EngineError>` — Réadmission contrôlée des deux banques L1 par le même ordonnanceur, examinateur, journal et magasin ; l'exécutable doit être l'apprenant natif de confiance.
  - `open_mixed_l1_anf(root: &Path, native_learner: &Path, inputs: [L1Input; 2], cfg: EngineConfig) -> Result<(Self, ResumeReport), EngineError>` — Curriculum mixte fini : deux leçons L1 natives et six leçons ANF mathématiques ; aucune dépendance inter-backend.
  - `registry(&self) -> &LessonRegistry` — (non documenté)
  - `store(&self) -> &KnowledgeStore` — (non documenté)
  - `root(&self) -> &Path` — (non documenté)
  - `run(&mut self, cancel: &CancellationToken) -> Result<RunSummary, EngineError>` — Exécute les vagues jusqu'à ce qu'aucune leçon ne soit dispatchable (ou que le crash simulé se déclenche).

## src/executor.rs

Exécuteur parallèle borné std-only (une deque par worker avec vol, `catch_unwind` par job, annulation coopérative).

- `struct CancellationToken(Arc<AtomicBool>)` — Drapeau d'annulation coopératif partagé entre l'appelant et les jobs.
  - `new() -> Self` — Crée un jeton non annulé.
  - `cancel(&self)` — Demande l'annulation ; les jobs non démarrés seront marqués `Cancelled`.
  - `is_cancelled(&self) -> bool` — Vrai une fois `cancel` appelé sur un clone.
- `enum JobOutcome<R> { Completed(R), Panicked(String), Cancelled }` — Résultat d'un job.
  - `is_completed(&self) -> bool` — Vrai pour `Completed`.
- `struct ExecStats { workers, executed, stolen, panicked, cancelled, wall_nanos }` — Compteurs décrivant un appel `run_parallel`.
- `available_workers() -> usize` — Nombre de workers suggéré par la plateforme, ou 1.
- `run_parallel<J, R, F>(jobs: Vec<J>, workers: usize, cancel: &CancellationToken, f: F) -> (Vec<JobOutcome<R>>, ExecStats)` — Exécute `f` sur `jobs` avec jusqu'à `workers` threads ; résultats dans l'ordre d'entrée.

## src/graph.rs

Graphe de dépendances acyclique entre leçons, avec arêtes inter-domaines et rejet des cycles.

- `enum GraphError { Duplicate(LessonId), MissingPrerequisite { lesson, missing }, UnknownNode(LessonId), Cycle { from, to }, SelfLoop(LessonId) }` — (non documenté)
- `struct DependencyGraph` — (non documenté) ; `Default` via `new`.
  - `new() -> Self` — (non documenté)
  - `len(&self) -> usize` — (non documenté)
  - `is_empty(&self) -> bool` — (non documenté)
  - `contains(&self, id: &LessonId) -> bool` — (non documenté)
  - `add_node(&mut self, id: LessonId, domain: Domain, prerequisites: &[LessonId]) -> Result<(), GraphError>` — (non documenté) ; les prérequis doivent déjà exister.
  - `add_edge(&mut self, prerequisite: LessonId, lesson: LessonId) -> Result<(), GraphError>` — Ajoute `prerequisite -> lesson` après coup, en rejetant les cycles.
  - `prerequisites(&self, id: &LessonId) -> impl Iterator<Item = &LessonId>` — (non documenté)
  - `dependents(&self, id: &LessonId) -> impl Iterator<Item = &LessonId>` — (non documenté)
  - `transitive_dependents(&self, id: &LessonId) -> BTreeSet<LessonId>` — Tous les dépendants transitifs (leçons dont la validité repose sur `id`).
  - `topological_order(&self) -> Vec<LessonId>` — Ordre topologique de Kahn, départage déterministe par identifiant.
  - `cross_domain_edges(&self) -> usize` — Nombre d'arêtes dont les extrémités sont dans des domaines différents.
  - `domain_of(&self, id: &LessonId) -> Option<Domain>` — (non documenté)

## src/hash.rs

Identités de contenu : digest SHA-256 de 32 octets et encodeur canonique préfixé par la longueur.

- `struct Digest(pub [u8; 32])` — Digest SHA-256 de 32 octets.
  - `ZERO: Digest` (const) — (non documenté)
  - `of(bytes: &[u8]) -> Self` — (non documenté)
  - `to_hex(&self) -> String` — (non documenté)
  - `from_hex(text: &str) -> Option<Self>` — (non documenté)
  - `short(&self) -> String` — Préfixe court pour les journaux uniquement ; ne jamais s'en servir comme identité.
- `struct Canonical` — Encodeur canonique préfixé par la longueur : des suites de champs distinctes ne peuvent pas entrer en collision par concaténation.
  - `new(schema: &str) -> Self` — (non documenté)
  - `bytes(&mut self, bytes: &[u8]) -> &mut Self` — (non documenté)
  - `str(&mut self, s: &str) -> &mut Self` — (non documenté)
  - `u64(&mut self, v: u64) -> &mut Self` — (non documenté)
  - `digest(&mut self, d: &Digest) -> &mut Self` — (non documenté)
  - `finish(self) -> Digest` — (non documenté)

## src/host_probe.rs

Échantillonnage des ressources de l'hôte (`/proc`, `df -Pk`) et budget d'admission fail-closed pour la porte de backpressure du superviseur.

- `struct HostSample { mem_total_kib, mem_available_kib, load1, cpus, disk_free_kib }` — Lecture ponctuelle de l'hôte ; `None` signifie inconnu.
- `parse_meminfo(text: &str) -> (Option<u64>, Option<u64>)` — Extrait `(MemTotal, MemAvailable)` en KiB du texte de `/proc/meminfo`.
- `parse_loadavg(text: &str) -> Option<f64>` — Premier champ de `/proc/loadavg` s'il est fini et non négatif.
- `parse_df_pk(text: &str) -> Option<u64>` — Colonne Available (4e) de la dernière ligne non vide de `df -Pk`.
- `sample(disk_path: &Path) -> HostSample` — Prend une lecture de l'hôte ; une lecture échouée laisse son champ à `None`.
- `struct ResourceBudget { min_mem_available_kib, max_load_per_cpu, min_disk_free_kib }` — Marge minimale exigée avant d'admettre du travail (`Default` : 4 GiB, 0,85 par CPU, 2 GiB).
  - `admit(&self, s: &HostSample) -> Result<(), Vec<String>>` — `Ok` si toutes les limites sont respectées ; sinon liste chaque champ violé ou inconnu.

## src/journal.rs

Journal de transitions append-only et tolérant aux crashs, pour la reprise automatique ; rejet fail-closed de toute corruption.

- `enum JournalError { Io(io::Error), Corrupt { line, reason }, IllegalTransition { from, to } }` — Erreurs d'ouverture, de replay ou d'ajout.
- `struct Replay { states: BTreeMap<Digest, KnowledgeState>, records: u64, torn_tail: bool }` — État de chaque leçon reconstruit depuis le journal.
  - `interrupted(&self) -> Vec<Digest>` — Leçons dont l'état rejoué est en vol (travail perdu en cas de crash).
- `struct Journal` — Journal ouvert, positionné pour l'écriture.
  - `open(path: &Path) -> Result<(Journal, Replay), JournalError>` — Ouvre ou crée le journal et le rejoue ; une ligne finale incomplète est tronquée.
  - `append(&mut self, lesson: &Digest, from: KnowledgeState, to: KnowledgeState) -> Result<u64, JournalError>` — Ajoute une transition et renvoie son numéro de séquence.
  - `sync(&mut self) -> Result<(), JournalError>` — Rend les enregistrements écrits durables (`sync_data`).

## src/lesson.rs

Représentation structurée des leçons, à identité adressée par contenu (`LessonSpec`, provenance, enregistrement mutable, erreurs).

- `type LessonId = Digest` — Identité stable d'une leçon : SHA-256 de la `LessonSpec` canonique.
- `struct SourceRef { name, version, license, authors: String, sha256: Digest }` — Provenance du matériel dont la leçon est issue.
- `struct LessonSpec { domain, level, skill, prerequisites, source, payload, difficulty, scientific_value, expected_gain, user_priority }` — Partie immuable d'une leçon ; deux specs identiques ont le même `LessonId`.
  - `id(&self) -> LessonId` — (non documenté)
- `struct ErrorRecord { attempt: u32, stage: ErrorStage, detail: String }` — Erreur d'apprenant ou d'enseignant enregistrée ; jamais écartée.
- `enum ErrorStage { Teacher, Learner, Exam, Consolidation }` — (non documenté)
- `struct Lesson { id, spec, state, teacher, train_digest, test_corpus, model_version, exams, checkpoint, errors, attempts, measured_cost_nanos }` — Enregistrement mutable d'une leçon tenu par le registre.
  - `new(spec: LessonSpec) -> Self` — (non documenté)
  - `failed_exams(&self) -> usize` — (non documenté)

## src/linguistic_backend.rs

Pont natif L1 : corpus annoté gelé = enseignant, fixture séparée = oracle, exécutable natif = apprenant. Module privé `engine::linguistic` ; items publics ré-exportés par `engine`.

- `struct L1Input { domain, lexicon, proposed_train, oracle_train, held_out: String, parent_bank: Vec<u8>, provenance: Digest }` — Entrées de confiance, liées à l'identité de la leçon ; le contrôleur physique doit vérifier leur provenance.
- `linguistic_l1_key(id: &LessonId) -> Digest` — (non documenté) ; clé de connaissance L1 (`SML_TUTOR_L1_KNOWLEDGE_KEY_V1`).

(Non compté : `pub(super) struct L1Backend`, visible seulement dans `engine`.)

## src/mastery.rs

Examinateur : seul module capable de produire un `Certificate`, avec une règle d'admission fixe à 100 % sans seuil ajustable.

- `enum Cohort { TrainRecall, Dev, Test, TestTest, Reload, Retention(Digest) }` — (non documenté) ; `Retention` : réexamen d'une compétence certifiée antérieure.
  - `label(&self) -> String` — (non documenté)
- `struct CohortResult { cohort, correct, abstained, total }` — (non documenté)
  - `passed(&self) -> bool` — (non documenté) ; `total > 0`, `correct == total`, `abstained == 0`.
  - `failed(&self) -> u64` — (non documenté)
- `struct CorpusIdentity { train, dev, test, testtest: Digest }` — Identité des quatre partitions d'évaluation, sous forme de digests d'ensembles de cas.
  - `digest(&self) -> Digest` — (non documenté)
- `case_set_digest(cases: &[Digest]) -> Digest` — Digest d'un ensemble de cas, indépendant de l'ordre.
- `check_partitions(train: &[Digest], dev: &[Digest], test: &[Digest], testtest: &[Digest]) -> Result<CorpusIdentity, MasteryFailure>` — Vérifie qu'aucun cas n'est partagé entre les quatre partitions ; renvoie l'identité du corpus.
- `struct ExamReport { lesson, corpus, cohorts }` — (non documenté)
  - `passed(&self) -> bool` — (non documenté)
  - `failed_cases(&self) -> u64` — (non documenté)
  - `digest(&self) -> Digest` — (non documenté)
- `enum MasteryFailure { MissingCohort(String), DuplicateCohort(String), EmptyCohort(String), Imperfect { cohort, correct, abstained, total }, PartitionLeak(Digest), WrongLesson }` — (non documenté)
- `struct Certificate { lesson, report, corpus, examined_cases }` (champs privés) — Preuve qu'une leçon a réussi son examen ; seul constructeur : `certify`.
  - `lesson(&self) -> LessonId` — (non documenté)
  - `report_digest(&self) -> Digest` — (non documenté)
  - `corpus_digest(&self) -> Digest` — (non documenté)
  - `examined_cases(&self) -> u64` — (non documenté)
  - `digest(&self) -> Digest` — (non documenté)
- `certify(lesson: LessonId, report: &ExamReport, retained_skills: &[Digest]) -> Result<Certificate, MasteryFailure>` — Applique la règle d'admission fixe à 100 %.

## src/registry.rs

Registre de leçons : enregistrement idempotent, déduplication et transitions d'état auditées.

- `enum Registration { Inserted, Duplicate }` — (non documenté) ; `Duplicate` : même contenu déjà enregistré, opération sans effet.
- `enum RegistryError { Graph(GraphError), UnknownLesson(LessonId), IllegalTransition { lesson, from, to }, PrerequisitesNotMet(LessonId) }` — (non documenté)
- `struct TransitionEvent { lesson, from, to }` — (non documenté)
- `struct LessonRegistry` — (non documenté)
  - `new() -> Self` — (non documenté)
  - `len(&self) -> usize` — (non documenté)
  - `is_empty(&self) -> bool` — (non documenté)
  - `graph(&self) -> &DependencyGraph` — (non documenté)
  - `get(&self, id: &LessonId) -> Option<&Lesson>` — (non documenté)
  - `get_mut(&mut self, id: &LessonId) -> Option<&mut Lesson>` — (non documenté)
  - `iter(&self) -> impl Iterator<Item = &Lesson>` — (non documenté)
  - `events(&self) -> &[TransitionEvent]` — (non documenté)
  - `register(&mut self, spec: LessonSpec) -> Result<(LessonId, Registration), RegistryError>` — Enregistre la leçon et la passe à `Identified` ; ré-enregistrer la même spec est sans effet.
  - `state(&self, id: &LessonId) -> Option<KnowledgeState>` — (non documenté)
  - `prerequisites_met(&self, id: &LessonId) -> bool` — (non documenté)
  - `transition(&mut self, id: &LessonId, to: KnowledgeState) -> Result<KnowledgeState, RegistryError>` — (non documenté)
  - `restore_state(&mut self, id: &LessonId, state: KnowledgeState) -> Result<(), RegistryError>` — Force un état rejoué pendant la reprise (le replay du journal a déjà validé chaque transition).
  - `record_error(&mut self, id: &LessonId, stage: ErrorStage, detail: String) -> Result<(), RegistryError>` — (non documenté)
  - `ready(&self) -> Vec<LessonId>` — Leçons prêtes à l'acquisition : `Identified` ou `NeedsRevision`, prérequis certifiés ou consolidés.
  - `invalidate_dependents(&mut self, id: &LessonId) -> Vec<LessonId>` — Marque pour révision chaque dépendant consolidé de `id`.
  - `count_by_state(&self, domain: Domain) -> BTreeMap<KnowledgeState, usize>` — (non documenté)

## src/resources.rs

Comptabilité des ressources physiques (baux sur machines) ; machines non mesurées refusées, seules les machines `AiCompute` reçoivent des baux.

- `struct Resources { cpu_milli, gpu_slots, ram_mib, vram_mib, storage_mib }` — Quantité de ressources physiques.
  - `fits_in(&self, avail: &Resources) -> bool` — Vrai si chaque composante est inférieure ou égale à celle de `avail`.
  - `checked_sub(&self, rhs: &Resources) -> Option<Resources>` — Soustraction par composante ; `None` en cas de dépassement négatif.
  - `saturating_add(&self, rhs: &Resources) -> Resources` — Addition par composante saturée au maximum numérique.
- `enum MachineRole { AiCompute, IosBuildReserved, ControlPlane }` — Ce qu'une machine a le droit de faire.
- `struct Machine { name, role, capacity, measured, online }` — Machine connue du gestionnaire de ressources.
- `struct Lease { id: u64, machine: String, demand: Resources }` — Réservation de ressources sur une machine ; doit être rendue exactement une fois.
- `enum ResourceError { DuplicateMachine(String), UnknownMachine(String), NoEligibleMachine, Insufficient, UnknownLease(u64), IdExhausted }` — (non documenté)
- `struct ResourceManager` — Comptabilise les baux sur un ensemble fixe de machines.
  - `new(machines: Vec<Machine>) -> Result<Self, ResourceError>` — (non documenté) ; chaque machine démarre avec sa capacité entière libre.
  - `acquire(&mut self, demand: Resources) -> Result<Lease, ResourceError>` — Réserve `demand` sur la machine éligible ayant le plus de CPU libre après allocation.
  - `release(&mut self, lease: &Lease) -> Result<(), ResourceError>` — Rend les ressources d'un bail ; une seconde libération est une erreur.
  - `set_online(&mut self, name: &str, online: bool) -> Result<(), ResourceError>` — (non documenté) ; une machine hors ligne garde ses baux mais n'en reçoit plus.
  - `free(&self, name: &str) -> Option<Resources>` — Capacité libre d'une machine, ou `None` si elle n'est pas enregistrée.
  - `active_leases(&self) -> usize` — Nombre de baux émis et non encore rendus.

## src/scheduler.rs

Planificateur parallèle : voies prioritaires, partage équitable entre voies, backpressure, déduplication et concurrence adaptative (AIMD).

- `priority(lesson: &Lesson) -> i64` — Priorité d'une leçon (plus haut = plus tôt) : valeur + gain + 2·priorité utilisateur + 50·min(échecs,10) − difficulté/2 − min(coût ms, 500).
- `enum SubmitError { Backpressure(Lane), Duplicate }` — (non documenté) ; `Backpressure` : la file de la voie est pleine.
- `struct AimdLimit { min, max, current, successes }` — Limite de concurrence additive-croissance / multiplicative-décroissance.
  - `new(min: usize, max: usize, initial: usize) -> Self` — (non documenté)
  - `current(&self) -> usize` — (non documenté)
  - `on_success(&mut self)` — Une fenêtre complète de succès élève la limite d'une unité.
  - `on_overload(&mut self)` — Une faute de ressource divise la limite par deux (minimum respecté).
- `struct LaneConfig { capacity, weight, max_in_flight }` — (non documenté) ; `Default` : capacité 10 000, poids 1, sans plafond.
- `struct Scheduler { ..., pub limit: AimdLimit }` — (non documenté)
  - `new(limit: AimdLimit) -> Self` — (non documenté)
  - `with_lanes(limit: AimdLimit, cfg: impl Fn(Lane) -> LaneConfig) -> Self` — (non documenté)
  - `submit(&mut self, lane: Lane, id: LessonId, priority: i64) -> Result<(), SubmitError>` — (non documenté)
  - `queued(&self, lane: Lane) -> usize` — (non documenté)
  - `total_queued(&self) -> usize` — (non documenté)
  - `in_flight(&self) -> usize` — (non documenté)
  - `dispatched(&self, lane: Lane) -> u64` — (non documenté)
  - `dispatch(&mut self) -> Option<(Lane, LessonId)>` — Tâche suivante par round-robin à déficit pondéré ; `None` si la limite globale est atteinte.
  - `next_wave(&mut self) -> Vec<(Lane, LessonId)>` — Vide jusqu'à la limite de concurrence courante.
  - `complete(&mut self, lane: Lane, id: &LessonId, overload: bool)` — Marque une tâche terminée ; `overload` signale une faute de ressource. L'identifiant est oublié (reprise possible).
  - `cancel_queued(&mut self) -> Vec<LessonId>` — Annulation contrôlée : retire toutes les tâches en file (pas celles en cours).

## src/store.rs

Magasin de connaissances transactionnel : commit par compare-and-swap sur `HEAD`, verrou `LOCK`, objets et manifestes adressés par contenu, historique restaurable.

- `struct EntryRecord { key, lesson, certificate, object: Digest }` — (non documenté) ; `Copy`.
- `struct StagedEntry` (champs privés) — Entrée prête à publier ; constructible seulement depuis un `Certificate`.
  - `new(certificate: &Certificate, key: Digest, payload: Vec<u8>) -> Self` — (non documenté)
  - `record(&self) -> &EntryRecord` — (non documenté)
- `struct Transaction { pub base_version: u64, pub entries: Vec<StagedEntry> }` — (non documenté)
- `struct CommitOutcome { version, added, idempotent }` — (non documenté)
- `enum StoreError { Io(io::Error), Busy, Conflict { base, current }, Contradiction { key, existing, proposed }, Corrupt(String), UnknownVersion(u64), InjectedFailure(FailPoint) }` — (non documenté)
- `enum FailPoint { AfterObjects, AfterManifest }` — Points de simulation de crash pendant un commit (tests).
- `struct Snapshot { version, manifest: Option<Digest>, entries: BTreeMap<Digest, EntryRecord> }` — (non documenté)
- `struct KnowledgeStore` — (non documenté) ; champs privés `root`, `fail_point`.
  - `open(root: impl Into<PathBuf>) -> Result<Self, StoreError>` — (non documenté) ; initialise `HEAD` sous verrou si absent.
  - `inject_failure(&mut self, point: Option<FailPoint>)` — `#[doc(hidden)]` ; (non documenté).
  - `version(&self) -> Result<u64, StoreError>` — (non documenté)
  - `snapshot(&self) -> Result<Snapshot, StoreError>` — (non documenté)
  - `get(&self, key: &Digest) -> Result<Option<Vec<u8>>, StoreError>` — (non documenté)
  - `recover_stale_lock(&self) -> Result<bool, StoreError>` — Supprime un `LOCK` dont le propriétaire est prouvablement mort (même hôte, pid absent) ; renvoie si un verrou a été retiré.
  - `commit(&self, txn: &Transaction) -> Result<CommitOutcome, StoreError>` — Publie atomiquement une transaction (toutes les entrées ou aucune).
  - `commit_rebasing(&self, entries: Vec<StagedEntry>, max_attempts: u32) -> Result<CommitOutcome, StoreError>` — (non documenté) ; relance sur `Busy` et `Conflict`.
  - `history(&self) -> Result<Vec<(u64, Digest)>, StoreError>` — Versions accessibles depuis `HEAD`, de la plus récente à la plus ancienne.
  - `restore(&self, version: u64) -> Result<CommitOutcome, StoreError>` — Republie le contenu d'une version antérieure comme nouvelle version.
  - `audit(&self) -> Result<usize, StoreError>` — Audit d'intégrité exhaustif : chaque manifeste et chaque objet référencé est re-haché ; renvoie le nombre d'objets de la version courante.
  - `root(&self) -> &Path` — (non documenté)

## src/supervisor.rs

Superviseur `sml-tutord` : machine d'états, journal d'événements chaîné par SHA-256, identités de jobs, backoff, quarantaine, disjoncteur, commandes opérateur et état de santé.

- `enum SupervisorState { Starting, Running, Paused, Maintenance, Draining, Stopped, EmergencyStopped }` — État du cycle de vie du superviseur.
  - `ALL: [SupervisorState; 7]` — (non documenté)
  - `name(self) -> &'static str` — (non documenté)
  - `parse(s: &str) -> Option<Self>` — (non documenté)
  - `can_go(self, to: SupervisorState) -> bool` — Transitions en liste blanche ; tout le reste est refusé.
  - `dispatches(self) -> bool` — Vrai si de nouveaux cycles peuvent démarrer (`Running` seulement).
- `enum Mode { Simulation, Production }` — Mode d'exploitation, consigné dans chaque événement de démarrage.
  - `name(self) -> &'static str` — (non documenté)
  - `parse(s: &str) -> Option<Self>` — (non documenté)
- `enum Command { Pause, Resume, Maintenance, Drain, Stop, Emergency }` — Commandes opérateur.
  - `parse(s: &str) -> Option<Self>` — (non documenté) ; accepte `pause`, `resume`, `maintenance`, `drain`, `stop`, `emergency` (après `trim`).
- `struct JobId(pub Digest)` — Identité de contenu d'un job.
  - `short(&self) -> String` — (non documenté)
- `struct JobSpec { kind, params, deps, priority, max_attempts }` — Ce qu'il faut exécuter ; priorité et budget de tentatives ne font pas partie de l'identité.
  - `new(kind: &str) -> Self` — (non documenté) ; `max_attempts` = 3, priorité 0.
  - `param(mut self, k: &str, v: impl ToString) -> Self` — (non documenté)
  - `id(&self) -> JobId` — (non documenté) ; SHA-256 canonique (`sml-tutord-job-v1`) de kind, paramètres et dépendances triées.
- `struct CycleReport { new_certified, reexamined, rejected, store_version, detail }` — Résultat d'un cycle réussi, tel que rapporté par le runner.
- `trait CycleRunner` — Exécute un job ; méthodes : `run(&mut self, id: JobId, spec: &JobSpec, mode: Mode, cancel: &CancellationToken) -> Result<CycleReport, String>`, `verify(&mut self, _id, _spec) -> Result<(), String>` (défaut `Ok`), `rollback(&mut self, _id, _spec) -> Result<(), String>` (défaut `Ok`).
- `trait Gate` — Porte d'admission de l'hôte ; méthode : `admit(&mut self) -> Result<(), Vec<String>>`.
- `struct OpenGate` — Admet toujours ; pour les tests et la simulation.
  - (impl `Gate` : `admit` toujours `Ok`.)
- `enum JobStatus { Queued, Running, Backoff { until_ms: u64 }, Succeeded, Quarantined { reason: String } }` — Cycle de vie d'un job, dérivé du journal d'événements.
- `struct JobRecord { spec, status, attempts, last_error, report }` — (non documenté)
- `struct SupervisorConfig { max_queued, backoff_base_ms, backoff_max_ms, breaker_quarantines }` — (non documenté) ; `Default` : 256, 30 000 ms, 3 600 000 ms, disjoncteur à 3 (0 désactive).
- `enum SupervisorError { Io(io::Error), Corrupt { line, reason }, IllegalTransition(SupervisorState, SupervisorState), Backpressure(usize), UnknownDependency(JobId), EmergencyStopped, Locked }` — (non documenté) ; `Locked` : `<root>/supervisor.lock` tenu par une autre instance.
- `enum Tick { Idle(SupervisorState), Gated(Vec<String>), NothingReady, Succeeded(JobId), Failed(JobId), Quarantined(JobId), Drained }` — Ce qu'a fait un pas de l'ordonnanceur (boucle du démon et tests).
- `struct Supervisor` (champs `pub interrupted_at_open: usize`, `pub torn_tail_at_open: bool`) — (non documenté)
  - `open(root: &Path, mode: Mode, cfg: SupervisorConfig, now_ms: u64) -> Result<Self, SupervisorError>` — Ouvre ou crée le superviseur, rejoue le journal, remet en file les jobs interrompus et entre en `Starting`.
  - `state(&self) -> SupervisorState` — (non documenté)
  - `mode(&self) -> Mode` — (non documenté)
  - `root(&self) -> &Path` — (non documenté)
  - `log_seq(&self) -> u64` — (non documenté)
  - `log_head(&self) -> Digest` — (non documenté)
  - `job(&self, id: &JobId) -> Option<&JobRecord>` — (non documenté)
  - `jobs(&self) -> impl Iterator<Item = (&JobId, &JobRecord)>` — Jobs dans l'ordre de soumission.
  - `pending(&self) -> usize` — Jobs non terminés (en file, en cours ou en backoff).
  - `submit(&mut self, spec: JobSpec) -> Result<(JobId, bool), SupervisorError>` — Soumission idempotente ; `Ok((id, false))` si le job existe déjà.
  - `command(&mut self, cmd: Command) -> Result<(), SupervisorError>` — (non documenté) ; `resume` en `Starting` passe directement en `Running`.
  - `start(&mut self) -> Result<(), SupervisorError>` — Quitte `Starting` pour `Running` (le disjoncteur peut alors basculer en `Maintenance`).
  - `consecutive_quarantines(&self) -> u32` — Quarantaines depuis le dernier succès ou la dernière reprise opérateur.
  - `breaker_tripped(&self) -> bool` — Vrai si le disjoncteur retient le superviseur.
  - `next_ready(&self, now_ms: u64) -> Option<JobId>` — Job prêt de plus haute priorité ; à égalité, ordre de soumission.
  - `tick(&mut self, runner: &mut dyn CycleRunner, gate: &mut dyn Gate, cancel: &CancellationToken, now_ms: u64) -> Result<Tick, SupervisorError>` — Un pas d'ordonnancement : au plus un cycle s'exécute.
  - `health_json(&self, now_ms: u64) -> String` — Instantané de santé lisible par machine (`SML_TUTORD_HEALTH_V1`).
  - `write_health(&self, now_ms: u64) -> io::Result<()>` — Écrit `health.json` de façon atomique (fichier temporaire puis renommage).
  - `poll_control(&mut self) -> Result<Option<Command>, SupervisorError>` — Lit puis consomme une commande opérateur depuis `<root>/control`.

## src/teacher.rs

Interface d'enseignants et admission des propositions par un oracle indépendant ; registre des statistiques par enseignant.

- `enum Residency { NonResident, Resident }` — Un modèle `NonResident` est chargé pour un lot puis libéré.
- `enum TeacherKind { LanguageModel, MathSolver, ProofEngine, RustCompiler, TestBench, Dictionary, Corpus, KnowledgeBase, Ensemble }` — Type d'enseignant, pour le reporting et le routage seulement.
- `struct LessonRequest { lesson: Digest, domain: Domain, skill: String, count: u32 }` — Demande de `count` éléments de `skill` dans `domain`, pour la leçon `lesson`.
- `struct Proposal { lesson, teacher, item_key, payload, claimed_answer }` — Matériel proposé par un enseignant, avec la réponse qu'il revendique ; la revendication n'est pas un fait.
  - `digest(&self) -> Digest` — Identité de contenu de la proposition, pour la déduplication (`SML_TUTOR_PROPOSAL_V1`).
- `trait Teacher: Send + Sync` — Source de matériel ; méthodes : `name(&self) -> &str`, `kind(&self) -> TeacherKind`, `residency(&self) -> Residency`, `domains(&self) -> &[Domain]`, `propose_batch(&self, requests: &[LessonRequest]) -> Vec<Proposal>`.
- `enum Verdict { Accept, Reject(String) }` — Résultat de la vérification d'une proposition.
- `trait Oracle: Send + Sync` — Vérificateur indépendant qui ne voit que la proposition ; méthodes : `name(&self) -> &str`, `verify(&self, proposal: &Proposal) -> Verdict`.
- `struct TeacherStats { proposed, admitted, rejected, duplicates, cost_nanos }` — Compteurs d'un enseignant ; `proposed` ne compte que les propositions vérifiées.
  - `admitted_rate(&self) -> f64` — Fraction des propositions vérifiées admises ; 0,0 avant toute proposition.
- `struct TeacherLedger` — Comptabilité par enseignant des résultats de vérification et du coût.
  - `record(&mut self, teacher: &str, verdict: &Verdict, cost_nanos: u128)` — Enregistre le verdict d'une proposition vérifiée.
  - `record_duplicate(&mut self, teacher: &str)` — Enregistre une proposition déjà vue (non vérifiée).
  - `stats(&self, teacher: &str) -> Option<TeacherStats>` — (non documenté)
  - `rejection_reasons(&self, teacher: &str) -> Vec<String>` — Les 64 derniers motifs de rejet, du plus ancien au plus récent.
  - `select<'a>(&self, candidates: &'a [&'a dyn Teacher], domain: Domain) -> Option<&'a dyn Teacher>` — Choisit l'enseignant pour `domain` : les non essayés d'abord, sinon le meilleur taux admis par seconde ; égalité par nom.
- `admit_batch(oracle: &dyn Oracle, ledger: &mut TeacherLedger, seen: &mut BTreeSet<Digest>, proposals: Vec<Proposal>) -> Vec<Proposal>` — Vérifie chaque proposition avec l'oracle et ne renvoie que les admises ; un doublon n'est ni vérifié ni renvoyé.

## src/telemetry.rs

Compteurs et durées de l'engine, avec export JSON-lines sans dépendance.

- `struct Counters { processed, learned, certified, consolidated, exam_failures, abstentions, teacher_proposals, teacher_rejections, attempts, contradictions, gaps_detected, gaps_resolved, learn_nanos, validate_nanos, consolidate_nanos }` — Compteurs d'événements et durées de phase pour une portée (domaine, machine ou total).
  - `merge(&mut self, other: &Counters)` — Additionne chaque champ, avec saturation.
- `struct Telemetry` — Télémétrie d'une exécution : compteurs par domaine et par machine, et instant de départ.
  - `new() -> Self` — Crée une télémétrie vide dont l'horloge démarre maintenant.
  - `domain_mut(&mut self, domain: Domain) -> &mut Counters` — (non documenté)
  - `machine_mut(&mut self, machine: &str) -> &mut Counters` — (non documenté)
  - `domain(&self, domain: Domain) -> Counters` — Copie des compteurs d'un domaine ; zéros si jamais touché.
  - `total(&self) -> Counters` — Somme des compteurs de domaine (les machines ne sont pas re-additionnées).
  - `elapsed_secs(&self) -> f64` — (non documenté)
  - `to_json_lines(&self, wall_nanos: u128) -> String` — Une ligne JSON par domaine (toujours quatre), puis par machine, puis une ligne `total`.
- `rate(count: u64, nanos: u128) -> f64` — Événements par seconde ; 0,0 si aucun temps enregistré.

## src/bin/sml-tutord.rs

Démon `sml-tutord` : analyse des arguments `clé=valeur`, boucle de supervision (une cycle ANF par tick), écriture de `health.json`, lignes machine `SML_TUTORD_*` sur stdout.

- Aucun item `pub` (binaire : `fn main` et types privés `Args`, `HostGate`, `AnfRunner`, `plan`, `parse`, `now_ms`, `ANF_KIND = "anf-m2"`).

## examples/sml_tutor_engine_bench.rs

Banc de qualification du moteur : même curriculum ANF avec plusieurs nombres de workers, vérification que chaque configuration publie les mêmes connaissances, puis reprise après crash simulé.

- Aucun item `pub` (`struct Args`, `fn parse`, `fn tmp`, `fn main` privés).

## examples/sml_tutor_bilingual_l1.rs

Réadmission de deux banques L1 natives (français, anglais) via le moteur P1, avec vérification de la reprise à l'identique.

- Aucun item `pub` (`fn read`, `fn input`, `fn main` privés).

## examples/sml_tutor_mixed_l1_anf.rs

Qualification d'un curriculum mixte (deux leçons L1 natives, six leçons ANF mathématiques) et de son redémarrage.

- Aucun item `pub` (`fn read`, `fn input`, `fn main` privés).

---

## Formats sur disque

Les fichiers écrits par le crate. « Constante » désigne la constante Rust quand elle existe, sinon la chaîne littérale. Les dispositions sont celles déduites du code et des commentaires.

| Nom | Valeur | Fichier écrit | Disposition |
|---|---|---|---|
| `HEADER` (privée) | `SML_TUTOR_JOURNAL_V1` | `<root>/journal.log` (moteur) | Voir ci-dessous |
| `MANIFEST_MAGIC` (privée) | `SML_TUTOR_MANIFEST_V1` | `<root>/store/manifests/<sha256_hex_du_texte>` | Voir ci-dessous |
| `HEAD_MAGIC` (privée) | `SML_TUTOR_HEAD_V1` | `<root>/store/HEAD` | Voir ci-dessous |
| littéral dans `lock_owner_record` | `SML_TUTOR_LOCK_V1` | `<root>/store/LOCK` | Voir ci-dessous |
| `CHECKPOINT_MAGIC` (publique) | `b"SMLANF1\0"` (8 octets) | contenu des objets `<root>/store/objects/<digest_hex>` (checkpoints ANF) | En-tête de 14 octets (`HEADER_LEN`) |
| `LOG_HEADER` (privée) | `SML_TUTORD_LOG_V1` | `<root>/supervisor.log` | Voir ci-dessous |
| littéral dans `health_json` | `SML_TUTORD_HEALTH_V1` (champ `"schema"`) | `<root>/health.json` (et `health.json.tmp` pendant l'écriture) | Objet JSON sur une ligne, suivi de `\n` |

**Journal de transitions (`journal.log`)** : première ligne `SML_TUTOR_JOURNAL_V1`, puis une ligne par transition : `T <lesson_hex> <from_state> <to_state> <seq>\n`. Les numéros de séquence commencent à 1 et augmentent de 1. Une dernière ligne sans saut de ligne final est une écriture interrompue : elle est ignorée et tronquée à l'ouverture. Toute autre irrégularité échoue (`JournalError::Corrupt`). Commentaire du module : « It is a plain text file: `SML_TUTOR_JOURNAL_V1` / `T <lesson_hex> <from_state> <to_state> <seq>` ».

**Magasin (`store/`)** :
- `HEAD` : `SML_TUTOR_HEAD_V1\n<version> <manifest_hex|none>\n` ; initialisé à `SML_TUTOR_HEAD_V1\n0 none\n`. Remplacé par renommage atomique.
- `manifests/<hex>` : nom = SHA-256 du texte. Texte : `SML_TUTOR_MANIFEST_V1\n`, `version <n>\n`, `parent <hex|none>\n`, puis une ligne `entry <key> <lesson> <certificate> <object>\n` par entrée (en ordre croissant de clé).
- `objects/<hex>` : contenu brut adressé par SHA-256, sans en-tête (pour les ANF, le contenu est un checkpoint `SMLANF1\0...` ; pour L1, les octets de la banque native).
- `LOCK` : `SML_TUTOR_LOCK_V1 <pid> <host_id>\n`, où `host_id` est le contenu de `/proc/sys/kernel/random/boot_id` (repli : `/etc/hostname`). Fichiers temporaires : `.LOCK.<pid>.<thread>`, `LOCK.stale.<pid>.<nanos>`, `.<name>.tmp.<pid>`.

**Checkpoint ANF (`SMLANF1`)** : octets 0 à 7 = `SMLANF1\0` ; octet 8 = `inputs` (u8) ; octet 9 = `max_degree` (u8) ; octets 10 à 13 = nombre de coefficients (`count`, u32 little-endian) ; puis `ceil(count/64)` mots u64 little-endian. Les bits au-delà de `count` doivent être nuls.

**Journal du superviseur (`supervisor.log`)** : première ligne `SML_TUTORD_LOG_V1`, puis une ligne par événement : `E\t<seq>\t<prev_hex>\t<champs séparés par \t>\t<hash_hex>\n`. Le hash est `SHA-256` canonique (domaine `sml-tutord-log-v1`) sur `seq`, `prev` et les champs. Champs (le premier est l'étiquette) :
- `submit` : `id`, `kind`, `params` (`k=v` séparés par `,`), `deps` (hex séparés par `,`), `priority`, `max_attempts`
- `start` : `id`, `attempt`, `mode`
- `succeed` : `id`, `new_certified`, `reexamined`, `rejected`, `store_version`, `detail`
- `fail` : `id`, `attempt`, `error`, `until_ms`
- `quarantine` : `id`, `reason`
- `state` : `from`, `to`

Échappements dans les champs : `%`→`%25`, tab→`%09`, LF→`%0A`, CR→`%0D`, `,`→`%2C`, `=`→`%3D`.

**Autres fichiers du superviseur (racine `<root>`)** :
- `supervisor.lock` : fichier de verrou consultatif exclusif (pas de contenu).
- `control` : texte d'une commande opérateur (`pause`, `resume`, `maintenance`, `drain`, `stop`, `emergency`), consommé (supprimé) à la lecture.
- `PRODUCTION_ADMITTED` : fichier de présence requis pour `mode=production` (écrit par l'opérateur, lu par le binaire).
- `cycles/<job_hex>/` : racine du moteur de chaque cycle (contient `store/` et `journal.log`).
- `rolled-back/<job_hex>-<ms>` : destination de déplacement d'un cycle annulé (rollback non destructif).

**Identifiants de hachage (domaines `Canonical`, non des fichiers)** : préfixes `SML_TUTOR_*_V1` ou `sml-tutord-*-v1` hachés dans les identités ; listés ici car ce sont des chaînes « magiques » du code.

| Domaine | Fichier source |
|---|---|
| `SML_TUTOR_LESSON_SPEC_V1` | `lesson.rs` (identité de `LessonSpec`) |
| `SML_TUTOR_PROPOSAL_V1` | `teacher.rs` (`Proposal::digest`) |
| `SML_TUTOR_CORPUS_IDENTITY_V1` | `mastery.rs` (`CorpusIdentity::digest`) |
| `SML_TUTOR_CASE_SET_V1` | `mastery.rs` (`case_set_digest`) |
| `SML_TUTOR_EXAM_REPORT_V1` | `mastery.rs` (`ExamReport::digest`) |
| `SML_TUTOR_CERTIFICATE_V1` | `mastery.rs` (`Certificate::digest`) |
| `SML_TUTOR_ANF_KNOWLEDGE_KEY_V1` | `engine.rs` (`knowledge_key`) |
| `SML_TUTOR_L1_KNOWLEDGE_KEY_V1` | `linguistic_backend.rs` (`linguistic_l1_key`) |
| `SML_TUTOR_L1_TRIPLE_CASE_V1` | `linguistic_backend.rs` (`keys`) |
| `SML_TUTOR_L1_BRIDGE_INPUT_V1` | `linguistic_backend.rs` (`Prepared::spec`) |
| `sml-tutord-job-v1` | `supervisor.rs` (`JobSpec::id`) |
| `sml-tutord-log-v1` | `supervisor.rs` (`chain`) |

Préfixe non haché par `Canonical` : `SML_TUTOR_ANF_PARAMS_V1` (`engine.rs`, `AnfLessonParams::encode`, privé) : 23 octets littéraux, puis `inputs` (u8), `degree` (u8), puis six u64 little-endian : `seed`, `base_train`, `max_train`, `dev`, `test`, `testtest` ; son digest entre dans `payload` de la `LessonSpec`.

Tag de version de modèle (non fichier) : `sml-tutor-anf-gf2-v1` (`engine.rs`, `AnfBackend::model_version`) ; `sml-tutor-native-l1-bridge-v1` (`linguistic_backend.rs`).

---

## Lignes machine

### Sorties de `src/bin/sml-tutord.rs` (stdout, sauf mention)

- `SML_TUTORD_START mode=<simulation|production> root=<chemin> workers=<n> log_seq=<n> interrupted=<n> torn_tail=<bool>` — au démarrage.
- `SML_TUTORD_CONTROL command=<Pause|Resume|Maintenance|Drain|Stop|Emergency> state=<nom d'état>` — à chaque commande opérateur lue.
- `SML_TUTORD_CYCLE=PASS job=<id court> new_certified=<n> store_version=<n> <detail>` — cycle réussi ; `<detail>` = `anf seed=<s> lessons=<n> waves=<n> wall_ms=<ms> resumed_records=<n>`.
- `SML_TUTORD_CYCLE=FAIL job=<id court> outcome=<Tick Debug> error=<Option<String> Debug>` — cycle échoué ou mis en quarantaine.
- `SML_TUTORD_BREAKER consecutive_quarantines=<n> state=<nom>` — disjoncteur déclenché.
- `SML_TUTORD_GATED reasons=<Vec<String> Debug>` — porte d'hôte refusée.
- `SML_TUTORD_DRAINED` — drainage terminé.
- `SML_TUTORD_EXIT code=<n> state=<nom> cycles_this_run=<n> log_seq=<n> log_head=<hex>` — sortie finale.
- Sur **stderr** : `SML_TUTORD=FAIL reason=<texte>` (erreur d'arguments, mode production sans `PRODUCTION_ADMITTED`, échec d'ouverture ou d'exécution).

### Sorties de `examples/sml_tutor_engine_bench.rs` (stdout)

- `SML_TUTOR_ENGINE_HOST available_parallelism=<n> lessons=<n> inputs=<n> degree=<n> lie_every=<n>` — en-tête.
- `SML_TUTOR_ENGINE_RUN workers=<n> waves=<n> consolidated=<n>/<lessons> blocked=<n> attempts=<n> gaps_detected=<n> gaps_resolved=<n> teacher_rejections=<n> audited_objects=<n> store_identical_to_first=<bool> wall_ms=<f> learn_phase_ms=<f> learn_speedup=<f> certified_per_sec=<f> steals=<n>` — une ligne par configuration de workers.
- `SML_TUTOR_ENGINE_LANE workers=<n> lane=<domaine> certified=<n> consolidated=<n> exam_failures=<n> abstentions=<n>` — une ligne par domaine (quatre) et par configuration.
- `SML_TUTOR_ENGINE_RESUME crashed=<bool> replayed_records=<n> interrupted_reset=<n> consolidated=<n>/<lessons> store_identical_to_uninterrupted=<bool>` — après crash simulé et reprise.
- `SML_TUTOR_ENGINE=PASS` — succès final (sans champ).

### Sorties de `examples/sml_tutor_bilingual_l1.rs` (stdout)

- `SML_TUTOR_L1_ENGINE_COHORT lang=<domaine> cohort=<label> correct=<n> total=<n> abstained=<n>` — une ligne par cohorte (six par langue).
- `SML_TUTOR_L1_ENGINE=PASS languages=2 workers=<n> learned_annotations=96 held_out=1792 store_entries=2 reopen_unchanged=true` — succès final ; seul `workers` est variable, les autres champs sont des littéraux.

### Sorties de `examples/sml_tutor_mixed_l1_anf.rs` (stdout)

- `SML_TUTOR_MIXED=PASS fr=1 en=1 math=6 rust=0 workers=<n> store_entries=8 reopen_unchanged=true` — succès final ; seul `workers` est variable.

### Marqueurs consommés mais non émis par le crate

Produits par l'exécutable natif L1 (non fourni dans le crate) et analysés par `linguistic_backend.rs` :
- `SML_TUTOR_LINGUISTIC_RELOAD=PASS test=896 errors=0` (attendu exact pour la relecture).
- `SML_TUTOR_LINGUISTIC train=48 test=<n> templates=8 errors=0` (reçu d'apprentissage attendu exact).

---

## Tests

Nombre de fonctions annotées `#[test]` par fichier (total : **127**) :

| Fichier | `#[test]` |
|---|---|
| src/anf_lesson.rs | 16 |
| src/bin/sml-tutord.rs | 0 |
| src/domain.rs | 3 |
| src/engine.rs | 5 |
| src/executor.rs | 9 |
| src/graph.rs | 2 |
| src/hash.rs | 2 |
| src/host_probe.rs | 15 |
| src/journal.rs | 9 |
| src/lesson.rs | 1 |
| src/lib.rs | 0 |
| src/linguistic_backend.rs | 2 |
| src/mastery.rs | 7 |
| src/registry.rs | 2 |
| src/resources.rs | 11 |
| src/scheduler.rs | 6 |
| src/store.rs | 7 |
| src/supervisor.rs | 13 |
| src/teacher.rs | 7 |
| src/telemetry.rs | 9 |
| examples/sml_tutor_bilingual_l1.rs | 0 |
| examples/sml_tutor_engine_bench.rs | 1 |
| examples/sml_tutor_mixed_l1_anf.rs | 0 |
