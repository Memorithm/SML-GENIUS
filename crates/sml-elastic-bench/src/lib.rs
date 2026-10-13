#![forbid(unsafe_code)]

//! EL-NG1: qualifies the ElasticXxx representation-transition control plane
//! on an exactly accounted, SML-owned byte n-gram.
//!
//! The model ([`sml_core::elastic_ngram_packed::ElasticNgram`]) stays dependency-free
//! in `sml-core`; this crate is the only place ElasticXxx is linked, pinned to
//! an exact git revision. It checks what the control plane does *and what it
//! does not do*: ElasticXxx validates structure (epochs, capabilities,
//! attestations) but cannot know a transition is quality-destroying or
//! information-theoretically impossible, so those gates live here.
//!
//! Frozen protocol: `docs/EL-NG1-ELASTIC-NGRAM-PREREGISTRATION.md`. TEST is
//! never read: [`frozen_splits`] drops it at load.

pub mod eng2_heap_swap;
pub mod eng2_trie_gate;

use std::collections::BTreeSet;
use std::error::Error;
use std::fmt::Write as _;
use std::time::Instant;

use elastic_core::{
    AdmissibleTransition, CapabilityRequirement, CapabilitySet, ContractId, DimensionId,
    FrontierError, Invariant, InvariantKind, LogicalResourceId, ObjectiveId, ObservationSignalId,
    RepresentationEpoch, RepresentationId, RepresentationState, ResourceClassId, ResourceSpec,
    TargetContract, TransitionAttestations, TransitionError, TransitionMechanism, VersionFrontier,
};
use sml_core::anf_char::CharSplits;
use sml_core::char_baselines::NGramModel;
use sml_core::elastic_ngram_packed::{ElasticNgram, ElasticNgramError, WIDTHS};

/// Result type used throughout the bench.
pub type BenchResult<T> = Result<T, Box<dyn Error>>;

/// Context lengths of the frozen grid.
pub const CONTEXTS: [usize; 6] = [0, 1, 2, 3, 4, 5];
/// Longest context of the base model.
pub const MAX_CONTEXT: usize = 5;
/// Frozen budget schedule, KiB, applied in order.
pub const SCHEDULE_KIB: [u64; 11] = [2048, 1024, 512, 256, 128, 64, 32, 16, 8, 256, 2048];
/// Frozen quality gate: allowed VAL bits/byte above the best grid state.
pub const GATE_DELTA: f64 = 0.50;
/// Bits in one KiB.
pub const BITS_PER_KIB: u64 = 8192;
/// Frozen ElasticXxx resource identity.
pub const RESOURCE_ID: &str = "sml.elastic-ngram";
/// Frozen invariant contract identity.
pub const GATE_CONTRACT: &str = "sml.elastic-ngram.val-bpb-gate";
/// Frozen corpus size (Candide body, A6-E4).
pub const FROZEN_CORPUS_BYTES: usize = 210_655;

/// One grid state: maximum context length and count width.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Config {
    pub k: usize,
    pub w: u8,
}

/// The initial committed state.
pub const INITIAL: Config = Config {
    k: MAX_CONTEXT,
    w: 32,
};

/// Tunable only by tests; [`Params::frozen`] is the preregistered setting.
#[derive(Clone, Debug)]
pub struct Params {
    pub schedule_kib: Vec<u64>,
    pub gate_delta: f64,
}

impl Params {
    /// The preregistered parameters.
    pub fn frozen() -> Self {
        Self {
            schedule_kib: SCHEDULE_KIB.to_vec(),
            gate_delta: GATE_DELTA,
        }
    }
}

/// Splits a corpus 80/10/10 and returns TRAIN and VAL only.
pub fn frozen_splits(bytes: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let splits = CharSplits::new(bytes);
    (splits.train, splits.val)
}

// ------------------------------------------------------------ ElasticXxx glue

fn representation_id(c: Config) -> BenchResult<RepresentationId> {
    Ok(RepresentationId::new(format!(
        "sml.elastic-ngram.k{}.w{}",
        c.k, c.w
    ))?)
}

fn state(c: Config, epoch: u64) -> BenchResult<RepresentationState> {
    Ok(RepresentationState::new(
        representation_id(c)?,
        1,
        RepresentationEpoch::new(epoch),
    ))
}

fn target_contract(c: Config) -> BenchResult<TargetContract> {
    Ok(TargetContract::New {
        id: representation_id(c)?,
        schema_version: 1,
    })
}

/// Capabilities: exactly the 30 grid states at schema 1.
pub fn capabilities() -> BenchResult<CapabilitySet> {
    let mut caps = CapabilitySet::default();
    for config in grid_configs() {
        caps.insert(representation_id(config)?, 1);
    }
    Ok(caps)
}

/// Every grid state in frozen order.
pub fn grid_configs() -> Vec<Config> {
    CONTEXTS
        .iter()
        .flat_map(|&k| WIDTHS.iter().map(move |&w| Config { k, w }))
        .collect()
}

/// The frozen resource declaration.
pub fn resource_spec() -> BenchResult<ResourceSpec> {
    Ok(ResourceSpec::builder(
        ResourceClassId::REPRESENTATIONAL,
        LogicalResourceId::new(RESOURCE_ID)?,
    )
    .allow(DimensionId::REPRESENTATION)
    .preserve(
        Invariant::new(InvariantKind::UpholdContract(ContractId::new(
            GATE_CONTRACT,
        )?))
        .along(DimensionId::REPRESENTATION),
    )
    .optimize(ObjectiveId::MEMORY_FOOTPRINT)
    .admit(AdmissibleTransition::new(
        TransitionMechanism::Reencode,
        DimensionId::REPRESENTATION,
    ))
    .admit(AdmissibleTransition::new(
        TransitionMechanism::Recompute,
        DimensionId::REPRESENTATION,
    ))
    .require_capability(CapabilityRequirement::new(
        TransitionMechanism::Reencode,
        DimensionId::REPRESENTATION,
    ))
    .require_capability(CapabilityRequirement::new(
        TransitionMechanism::Recompute,
        DimensionId::REPRESENTATION,
    ))
    .observe(ObservationSignalId::FREE_CAPACITY)
    .build()?)
}

fn attestation(mechanism: TransitionMechanism) -> TransitionAttestations {
    match mechanism {
        TransitionMechanism::Reencode => {
            TransitionAttestations::none().attest_reencoder_available()
        }
        TransitionMechanism::Recompute => {
            TransitionAttestations::none().attest_recompute_source_available()
        }
        TransitionMechanism::Reinterpret => TransitionAttestations::none(),
    }
}

// ------------------------------------------------------------------- the grid

/// One measured grid state.
#[derive(Clone, Debug)]
pub struct GridPoint {
    pub config: Config,
    pub bits: u64,
    pub entries: u64,
    pub bpb: f64,
    pub accuracy: f64,
}

/// All 30 states plus the reference (best) VAL bits/byte.
#[derive(Clone, Debug)]
pub struct Grid {
    pub points: Vec<GridPoint>,
    pub reference_bpb: f64,
}

/// Materialises every grid state from `base` and scores it on `val`.
pub fn build_models(base: &ElasticNgram) -> BenchResult<Vec<(Config, ElasticNgram)>> {
    grid_configs()
        .into_iter()
        .map(|c| Ok((c, base.reencode(c.k, c.w)?)))
        .collect()
}

/// Measures the grid from already materialised models.
pub fn measure_grid(models: &[(Config, ElasticNgram)], val: &[u8]) -> Grid {
    let points: Vec<GridPoint> = models
        .iter()
        .map(|(config, model)| {
            let score = model.score(val);
            let acct = model.account();
            GridPoint {
                config: *config,
                bits: acct.total_bits(),
                entries: acct.entries,
                bpb: score.bits_per_byte,
                accuracy: score.accuracy,
            }
        })
        .collect();
    let reference_bpb = points.iter().map(|p| p.bpb).fold(f64::INFINITY, f64::min);
    Grid {
        points,
        reference_bpb,
    }
}

fn best_within(grid: &Grid, budget_bits: u64) -> Option<&GridPoint> {
    grid.points
        .iter()
        .filter(|p| p.bits <= budget_bits)
        .min_by(|a, b| {
            a.bpb
                .total_cmp(&b.bpb)
                .then(a.bits.cmp(&b.bits))
                .then(a.config.k.cmp(&b.config.k))
                .then(a.config.w.cmp(&b.config.w))
        })
}

// ---------------------------------------------------------------- controller

/// Result of one schedule step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Stay,
    Committed(TransitionMechanism),
    NoCandidate,
    QualityInfeasible,
}

/// Trace row of one schedule step.
#[derive(Clone, Debug)]
pub struct Step {
    pub budget_kib: u64,
    pub outcome: Outcome,
    pub committed: Config,
    pub epoch: u64,
    pub committed_bits: u64,
    pub committed_bpb: f64,
    pub over_budget: bool,
    pub target_bpb: Option<f64>,
    pub shadow_bpb: Option<f64>,
}

fn mechanism_for(from: Config, to: Config) -> TransitionMechanism {
    if to.k <= from.k && to.w <= from.w {
        TransitionMechanism::Reencode
    } else {
        TransitionMechanism::Recompute
    }
}

/// Runs the frozen controller through the ElasticXxx frontier.
pub fn run_controller(
    train: &[u8],
    val: &[u8],
    grid: &Grid,
    base: &ElasticNgram,
    params: &Params,
) -> BenchResult<Vec<Step>> {
    let caps = capabilities()?;
    let mut frontier = VersionFrontier::new(state(INITIAL, 0)?);
    let mut model = base.clone();
    let mut current = INITIAL;
    let mut steps = Vec::new();
    for &kib in &params.schedule_kib {
        let budget_bits = kib * BITS_PER_KIB;
        let mut target_bpb = None;
        let mut shadow_bpb = None;
        let outcome = match best_within(grid, budget_bits) {
            None => Outcome::NoCandidate,
            Some(p) if p.bpb > grid.reference_bpb + params.gate_delta => Outcome::QualityInfeasible,
            Some(p) if p.config == current => Outcome::Stay,
            Some(p) => {
                let target = p.config;
                target_bpb = Some(p.bpb);
                let mechanism = mechanism_for(current, target);
                let to = frontier
                    .committed()
                    .derive_target(target_contract(target)?, mechanism)?;
                frontier.propose(to, mechanism)?;
                let shadow = match mechanism {
                    TransitionMechanism::Reencode => model.reencode(target.k, target.w)?,
                    _ => ElasticNgram::train(train, MAX_CONTEXT)?.reencode(target.k, target.w)?,
                };
                shadow_bpb = Some(shadow.score(val).bits_per_byte);
                let att = attestation(mechanism);
                frontier.validate_pending(&caps, att)?;
                frontier.commit(&caps, att)?;
                model = shadow;
                current = target;
                Outcome::Committed(mechanism)
            }
        };
        let committed_bits = model.account().total_bits();
        steps.push(Step {
            budget_kib: kib,
            outcome,
            committed: current,
            epoch: frontier.committed().epoch.get(),
            committed_bits,
            committed_bpb: model.score(val).bits_per_byte,
            over_budget: committed_bits > budget_bits,
            target_bpb,
            shadow_bpb,
        });
    }
    Ok(steps)
}

/// Independent derivation of the expected trace, by sorting the whole grid
/// instead of the controller's `min_by` scan.
pub fn expected_trace(grid: &Grid, params: &Params) -> Vec<(Outcome, Config)> {
    let mut ranked: Vec<&GridPoint> = grid.points.iter().collect();
    ranked.sort_by(|a, b| {
        a.bpb
            .total_cmp(&b.bpb)
            .then(a.bits.cmp(&b.bits))
            .then(a.config.cmp(&b.config))
    });
    let mut current = INITIAL;
    let mut out = Vec::new();
    for &kib in &params.schedule_kib {
        let budget = kib * BITS_PER_KIB;
        let outcome = match ranked.iter().find(|p| p.bits <= budget) {
            None => Outcome::NoCandidate,
            Some(p) if p.bpb > grid.reference_bpb + params.gate_delta => Outcome::QualityInfeasible,
            Some(p) if p.config == current => Outcome::Stay,
            Some(p) => {
                let mechanism = mechanism_for(current, p.config);
                current = p.config;
                Outcome::Committed(mechanism)
            }
        };
        out.push((outcome, current));
    }
    out
}

// ------------------------------------------------------------------ criteria

/// One preregistered criterion verdict.
#[derive(Clone, Debug)]
pub struct Criterion {
    pub id: &'static str,
    pub passed: bool,
    pub detail: String,
}

fn verdict(id: &'static str, result: BenchResult<Result<String, String>>) -> Criterion {
    match result {
        Ok(Ok(detail)) => Criterion {
            id,
            passed: true,
            detail,
        },
        Ok(Err(detail)) => Criterion {
            id,
            passed: false,
            detail,
        },
        Err(error) => Criterion {
            id,
            passed: false,
            detail: format!("error: {error}"),
        },
    }
}

fn c1_parity(train: &[u8], val: &[u8]) -> BenchResult<Result<String, String>> {
    for &k in &CONTEXTS {
        let elastic = ElasticNgram::train(train, k)?.score(val);
        let baseline = NGramModel::train(train, k).score(val);
        let same = elastic.tokens == baseline.tokens
            && elastic.bits_per_byte.to_bits() == baseline.bits_per_byte.to_bits()
            && elastic.accuracy.to_bits() == baseline.accuracy.to_bits();
        if !same {
            return Ok(Err(format!(
                "k={k}: elastic {} bpb vs baseline {} bpb",
                elastic.bits_per_byte, baseline.bits_per_byte
            )));
        }
    }
    Ok(Ok(
        "6 context lengths bit-identical to NGramModel on VAL".into()
    ))
}

fn c2_packed(models: &[(Config, ElasticNgram)]) -> BenchResult<Result<String, String>> {
    for (config, model) in models {
        let (bytes, bits) = model.encode()?;
        let accounted = model.account().total_bits();
        if bits != accounted {
            return Ok(Err(format!(
                "{config:?}: packed {bits} != accounted {accounted}"
            )));
        }
        if &ElasticNgram::decode(&bytes, bits)? != model {
            return Ok(Err(format!("{config:?}: decode(encode(m)) != m")));
        }
    }
    Ok(Ok(format!(
        "{} states: packed length == accounting, round trip exact",
        models.len()
    )))
}

fn c3_algebra(models: &[(Config, ElasticNgram)]) -> BenchResult<Result<String, String>> {
    let (mut pairs, mut refusals) = (0usize, 0usize);
    for (ci, mi) in models {
        for (ct, mt) in models {
            if ct.k <= ci.k && ct.w <= ci.w {
                pairs += 1;
                if &mi.reencode(ct.k, ct.w)? != mt {
                    return Ok(Err(format!(
                        "{ci:?} -> {ct:?} differs from direct reencode"
                    )));
                }
            } else {
                refusals += 1;
                let refused = matches!(
                    mi.reencode(ct.k, ct.w),
                    Err(ElasticNgramError::ContextWidening { .. }
                        | ElasticNgramError::WidthWidening { .. })
                );
                if !refused {
                    return Ok(Err(format!("{ci:?} -> {ct:?}: widening was not refused")));
                }
            }
        }
    }
    Ok(Ok(format!(
        "{pairs} narrowing pairs path-independent (identity included), {refusals} widenings refused"
    )))
}

fn rejected_with(
    frontier: &mut VersionFrontier,
    caps: &CapabilitySet,
    att: TransitionAttestations,
    pattern: fn(&TransitionError) -> bool,
) -> bool {
    let rejected =
        matches!(frontier.validate_pending(caps, att), Err(FrontierError::Core(e)) if pattern(&e));
    frontier.rollback();
    rejected
}

fn c4_verdicts(base: &ElasticNgram) -> BenchResult<Result<String, String>> {
    let caps = capabilities()?;
    let start = Config { k: 2, w: 8 };
    let wider = Config { k: 2, w: 16 };
    let outside = Config { k: 6, w: 8 };
    let fresh = || -> BenchResult<VersionFrontier> { Ok(VersionFrontier::new(state(start, 5)?)) };
    let mut failures = Vec::new();

    let mut f = fresh()?;
    f.propose(state(wider, 6)?, TransitionMechanism::Reencode)?;
    if !rejected_with(&mut f, &caps, TransitionAttestations::none(), |e| {
        matches!(e, TransitionError::MissingReencoderAttestation)
    }) {
        failures.push("a: reencode without attestation");
    }

    f.propose(state(wider, 5)?, TransitionMechanism::Recompute)?;
    if !rejected_with(
        &mut f,
        &caps,
        attestation(TransitionMechanism::Recompute),
        |e| matches!(e, TransitionError::EpochMustAdvance { .. }),
    ) {
        failures.push("b: no epoch advance");
    }

    f.propose(state(outside, 6)?, TransitionMechanism::Recompute)?;
    if !rejected_with(
        &mut f,
        &caps,
        attestation(TransitionMechanism::Recompute),
        |e| matches!(e, TransitionError::UnsupportedTarget { .. }),
    ) {
        failures.push("c: unsupported target");
    }

    f.propose(state(wider, 6)?, TransitionMechanism::Reinterpret)?;
    if !rejected_with(&mut f, &caps, TransitionAttestations::none(), |e| {
        matches!(e, TransitionError::MissingSemanticEquivalenceAttestation)
    }) {
        failures.push("d: unattested reinterpret");
    }

    // (e) ElasticXxx accepts a widening reencode once a reencoder is attested;
    // the SML-owned model refuses it. The disagreement is the finding.
    f.propose(state(wider, 6)?, TransitionMechanism::Reencode)?;
    let elastic_ok = f
        .validate_pending(&caps, attestation(TransitionMechanism::Reencode))
        .is_ok();
    f.rollback();
    let narrow = base.reencode(start.k, start.w)?;
    let sml_refuses = matches!(
        narrow.reencode(wider.k, wider.w),
        Err(ElasticNgramError::WidthWidening { .. })
    );
    if !elastic_ok {
        failures.push("e: ElasticXxx unexpectedly refused the attested widening");
    }
    if !sml_refuses {
        failures.push("e: SML feasibility gate did not refuse the widening");
    }

    if failures.is_empty() {
        Ok(Ok("a-d rejected as expected; e: ElasticXxx Ok, SML gate refuses (structural validation is blind to feasibility)".into()))
    } else {
        Ok(Err(failures.join("; ")))
    }
}

/// Checks the executed trace against the independent oracle and invariants.
pub fn check_trace(grid: &Grid, params: &Params, steps: &[Step]) -> Criterion {
    let expected = expected_trace(grid, params);
    let mut problems = Vec::new();
    if expected.len() != steps.len() {
        problems.push(format!(
            "{} steps vs {} expected",
            steps.len(),
            expected.len()
        ));
    }
    let mut previous_epoch = 0u64;
    for (i, (step, (outcome, config))) in steps.iter().zip(&expected).enumerate() {
        if step.outcome != *outcome || step.committed != *config {
            problems.push(format!(
                "step {i}: executed {:?}/{:?} != oracle {outcome:?}/{config:?}",
                step.outcome, step.committed
            ));
        }
        let moved = matches!(step.outcome, Outcome::Committed(_));
        if moved != (step.epoch > previous_epoch) || (!moved && step.epoch != previous_epoch) {
            problems.push(format!(
                "step {i}: epoch {} -> {} inconsistent with {:?}",
                previous_epoch, step.epoch, step.outcome
            ));
        }
        previous_epoch = step.epoch;
        if matches!(step.outcome, Outcome::Stay | Outcome::Committed(_)) {
            if step.over_budget {
                problems.push(format!("step {i}: committed state exceeds budget"));
            }
            if step.committed_bpb > grid.reference_bpb + params.gate_delta {
                problems.push(format!(
                    "step {i}: committed state violates the quality gate"
                ));
            }
        }
        if let (Some(shadow), Some(target)) = (step.shadow_bpb, step.target_bpb) {
            if shadow.to_bits() != target.to_bits() {
                problems.push(format!(
                    "step {i}: shadow bpb {shadow} != grid bpb {target}"
                ));
            }
        }
    }
    if problems.is_empty() {
        Criterion {
            id: "C5",
            passed: true,
            detail: format!("{} steps match the independent oracle; budget, gate, epoch and shadow-equality invariants hold", steps.len()),
        }
    } else {
        Criterion {
            id: "C5",
            passed: false,
            detail: problems.join("; "),
        }
    }
}

fn c6_rollback(base: &ElasticNgram, val: &[u8]) -> BenchResult<Result<String, String>> {
    let caps = capabilities()?;
    let before = base.packed_fingerprint()?;
    let mut frontier = VersionFrontier::new(state(INITIAL, 0)?);
    let target = Config { k: 4, w: 16 };
    let to = frontier
        .committed()
        .derive_target(target_contract(target)?, TransitionMechanism::Reencode)?;
    frontier.propose(to, TransitionMechanism::Reencode)?;
    let double = frontier.propose(state(target, 9)?, TransitionMechanism::Reencode);
    if !matches!(double, Err(FrontierError::ProposalAlreadyStaged)) {
        return Ok(Err(
            "a second proposal was accepted while one was staged".into()
        ));
    }
    let shadow = base.reencode(target.k, target.w)?;
    let _ = shadow.score(val);
    frontier.validate_pending(&caps, attestation(TransitionMechanism::Reencode))?;
    // The (injected) quality gate fails: roll back instead of committing.
    let staged = frontier.rollback();
    if staged.is_none() {
        return Ok(Err("rollback returned no staged transition".into()));
    }
    if frontier.pending().is_some() || frontier.committed() != &state(INITIAL, 0)? {
        return Ok(Err(
            "committed state changed or a proposal remained staged".into()
        ));
    }
    if base.packed_fingerprint()? != before {
        return Ok(Err("committed model fingerprint changed".into()));
    }
    let again = frontier
        .committed()
        .derive_target(target_contract(target)?, TransitionMechanism::Reencode)?;
    frontier.propose(again, TransitionMechanism::Reencode)?;
    Ok(Ok(
        "rollback restored state and model fingerprint; frontier reusable; double-stage refused"
            .into(),
    ))
}

fn c7_eir() -> BenchResult<(Result<String, String>, String)> {
    let spec = resource_spec()?;
    let first = elastic_eir::lower(&spec)?;
    let second = elastic_eir::lower(&resource_spec()?)?;
    let fingerprint = first.fingerprint().to_string();
    if fingerprint != second.fingerprint().to_string() {
        return Ok((
            Err("two lowerings gave different fingerprints".into()),
            fingerprint,
        ));
    }
    let Some(resource) = first.resource(RESOURCE_ID) else {
        return Ok((
            Err("resource missing from the lowered document".into()),
            fingerprint,
        ));
    };
    let mechanisms: BTreeSet<_> = resource
        .transitions()
        .iter()
        .map(|t| t.transition().mechanism())
        .collect();
    let expected: BTreeSet<_> = [
        TransitionMechanism::Reencode,
        TransitionMechanism::Recompute,
    ]
    .into();
    if mechanisms != expected
        || !resource
            .transitions()
            .iter()
            .all(|t| t.capability_grounded())
    {
        return Ok((
            Err(format!("unexpected admitted transitions: {mechanisms:?}")),
            fingerprint,
        ));
    }
    Ok((
        Ok(
            "deterministic fingerprint; Reencode and Recompute admitted and capability-grounded"
                .into(),
        ),
        fingerprint,
    ))
}

// -------------------------------------------------------------------- report

/// Everything one run produces.
#[derive(Debug)]
pub struct Report {
    pub train_bytes: usize,
    pub val_bytes: usize,
    pub grid: Grid,
    pub steps: Vec<Step>,
    pub criteria: Vec<Criterion>,
    pub eir_fingerprint: String,
    pub timings: Vec<(&'static str, f64)>,
}

impl Report {
    /// True when every preregistered criterion passed.
    pub fn all_passed(&self) -> bool {
        self.criteria.iter().all(|c| c.passed)
    }
}

/// Runs the whole preregistered protocol on `train`/`val`.
pub fn run(train: &[u8], val: &[u8], params: &Params) -> BenchResult<Report> {
    let mut timings = Vec::new();
    let mut lap =
        |name: &'static str, since: Instant| timings.push((name, since.elapsed().as_secs_f64()));

    let t = Instant::now();
    let base = ElasticNgram::train(train, MAX_CONTEXT)?;
    lap("train_base", t);

    let t = Instant::now();
    let models = build_models(&base)?;
    let grid = measure_grid(&models, val);
    lap("materialise_and_score_grid", t);

    let mut criteria = Vec::new();
    let t = Instant::now();
    criteria.push(verdict("C1", c1_parity(train, val)));
    lap("C1_parity", t);
    let t = Instant::now();
    criteria.push(verdict("C2", c2_packed(&models)));
    lap("C2_packed_oracle", t);
    let t = Instant::now();
    criteria.push(verdict("C3", c3_algebra(&models)));
    lap("C3_reencode_algebra", t);
    let t = Instant::now();
    criteria.push(verdict("C4", c4_verdicts(&base)));
    lap("C4_frontier_verdicts", t);

    let t = Instant::now();
    let steps = run_controller(train, val, &grid, &base, params)?;
    criteria.push(check_trace(&grid, params, &steps));
    lap("C5_controller", t);

    let t = Instant::now();
    criteria.push(verdict("C6", c6_rollback(&base, val)));
    lap("C6_rollback", t);

    let (c7, eir_fingerprint) = match c7_eir() {
        Ok((result, fingerprint)) => (verdict("C7", Ok(result)), fingerprint),
        Err(error) => (verdict("C7", Err(error)), String::new()),
    };
    criteria.push(c7);

    Ok(Report {
        train_bytes: train.len(),
        val_bytes: val.len(),
        grid,
        steps,
        criteria,
        eir_fingerprint,
        timings,
    })
}

/// Human-readable report (also the CI artifact).
pub fn render(report: &Report, frozen_corpus: bool) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "# EL-NG1 ElasticNGram control-plane qualification");
    let _ = writeln!(
        s,
        "corpus: {} (TRAIN {} B, VAL {} B; TEST not read)",
        if frozen_corpus {
            "frozen Candide"
        } else {
            "NOT THE FROZEN CORPUS - smoke only, no evidential value"
        },
        report.train_bytes,
        report.val_bytes
    );
    let _ = writeln!(
        s,
        "eir_fingerprint: {} (non-cryptographic)",
        report.eir_fingerprint
    );
    let _ = writeln!(s, "\n## Grid (exact bits; VAL bits/byte)");
    let _ = writeln!(s, "k  w   KiB        entries   bpb      acc");
    for p in &report.grid.points {
        let _ = writeln!(
            s,
            "{}  {:>2}  {:>9.1}  {:>8}  {:.4}  {:.4}",
            p.config.k,
            p.config.w,
            p.bits as f64 / BITS_PER_KIB as f64,
            p.entries,
            p.bpb,
            p.accuracy
        );
    }
    let _ = writeln!(s, "reference_bpb={:.4}", report.grid.reference_bpb);
    let _ = writeln!(s, "\n## Schedule trace");
    let _ = writeln!(
        s,
        "budget_KiB  outcome                      state    epoch  KiB       bpb     over_budget"
    );
    for st in &report.steps {
        let _ = writeln!(
            s,
            "{:>9}   {:<27}  k{}/w{:<2}  {:>5}  {:>8.1}  {:.4}  {}",
            st.budget_kib,
            format!("{:?}", st.outcome),
            st.committed.k,
            st.committed.w,
            st.epoch,
            st.committed_bits as f64 / BITS_PER_KIB as f64,
            st.committed_bpb,
            st.over_budget
        );
    }
    let _ = writeln!(s, "\n## Criteria");
    for c in &report.criteria {
        let _ = writeln!(
            s,
            "{} {} - {}",
            c.id,
            if c.passed { "PASS" } else { "FAIL" },
            c.detail
        );
    }
    let _ = writeln!(s, "\n## Timings (shared runner, informational)");
    for (name, secs) in &report.timings {
        let _ = writeln!(s, "{name}: {secs:.3}s");
    }
    let passed = report.criteria.iter().filter(|c| c.passed).count();
    let _ = writeln!(
        s,
        "\nEL_NG1_RESULT,criteria_passed={}/{},frozen_corpus={}",
        passed,
        report.criteria.len(),
        frozen_corpus
    );
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use sml_core::toy_tasks::SplitMix64;

    const WORDS: [&str; 12] = [
        "le ", "la ", "de ", "et ", "un ", "une ", "dans ", "pour ", "mais ", "avec ", "sans ",
        "tout ",
    ];

    fn corpus(len: usize) -> Vec<u8> {
        let mut rng = SplitMix64::new(11);
        let mut out = Vec::new();
        while out.len() < len {
            out.extend_from_slice(WORDS[rng.below(WORDS.len())].as_bytes());
            if rng.below(7) == 0 {
                out.push(b'.');
            }
        }
        out.truncate(len);
        out
    }

    fn splits() -> (Vec<u8>, Vec<u8>) {
        frozen_splits(&corpus(64 * 1400))
    }

    #[test]
    fn full_protocol_passes_on_a_synthetic_corpus() {
        let (train, val) = splits();
        let report = run(&train, &val, &Params::frozen()).unwrap();
        for c in &report.criteria {
            assert!(c.passed, "{} failed: {}", c.id, c.detail);
        }
        assert_eq!(report.criteria.len(), 7);
        assert_eq!(report.grid.points.len(), 30);
        assert!(report.eir_fingerprint.starts_with("fp:"));
        assert!(render(&report, false).contains("NOT THE FROZEN CORPUS"));
    }

    #[test]
    fn zero_budget_yields_no_candidate_and_keeps_state() {
        let (train, val) = splits();
        let params = Params {
            schedule_kib: vec![0],
            gate_delta: GATE_DELTA,
        };
        let report = run(&train, &val, &params).unwrap();
        let step = &report.steps[0];
        assert_eq!(step.outcome, Outcome::NoCandidate);
        assert_eq!(step.committed, INITIAL);
        assert_eq!(step.epoch, 0);
        assert!(
            step.over_budget,
            "the unmet budget must be reported, not hidden"
        );
        assert!(
            report.all_passed(),
            "an honest NoCandidate is a valid outcome"
        );
    }

    #[test]
    fn impossible_gate_yields_quality_infeasible() {
        let (train, val) = splits();
        let params = Params {
            schedule_kib: vec![2048, 64],
            gate_delta: -1.0,
        };
        let report = run(&train, &val, &params).unwrap();
        assert!(report
            .steps
            .iter()
            .all(|s| s.outcome == Outcome::QualityInfeasible && s.committed == INITIAL));
        assert!(report.all_passed());
    }

    #[test]
    fn controller_exercises_both_mechanisms_and_the_oracle_catches_tampering() {
        let (train, val) = splits();
        // The synthetic model is small, so the schedule is scaled down to
        // force a descent (Reencode) and a re-growth (Recompute).
        let params = Params {
            schedule_kib: vec![64, 2, 1, 64],
            gate_delta: GATE_DELTA,
        };
        let report = run(&train, &val, &params).unwrap();
        assert!(report.all_passed());
        let mechanisms: BTreeSet<_> = report
            .steps
            .iter()
            .filter_map(|s| match s.outcome {
                Outcome::Committed(m) => Some(m),
                _ => None,
            })
            .collect();
        assert!(
            mechanisms.contains(&TransitionMechanism::Reencode),
            "descending budgets must reencode: {:?}",
            report.steps.iter().map(|s| s.outcome).collect::<Vec<_>>()
        );
        assert!(
            mechanisms.contains(&TransitionMechanism::Recompute),
            "the final ascending budget must recompute"
        );

        let mut tampered = report.steps.clone();
        tampered[1].committed.k += 1;
        assert!(!check_trace(&report.grid, &params, &tampered).passed);
        let mut over = report.steps.clone();
        if let Some(step) = over
            .iter_mut()
            .find(|s| matches!(s.outcome, Outcome::Committed(_)))
        {
            step.over_budget = true;
        }
        assert!(!check_trace(&report.grid, &params, &over).passed);
        let mut wrong_shadow = report.steps.clone();
        if let Some(step) = wrong_shadow.iter_mut().find(|s| s.shadow_bpb.is_some()) {
            step.shadow_bpb = step.shadow_bpb.map(|b| b + 1e-9);
        }
        assert!(!check_trace(&report.grid, &params, &wrong_shadow).passed);
    }

    #[test]
    fn test_split_is_never_returned() {
        let bytes = corpus(64 * 100);
        let (train, val) = frozen_splits(&bytes);
        assert_eq!(train.len(), bytes.len() * 8 / 10);
        assert_eq!(val.len(), bytes.len() * 9 / 10 - train.len());
        assert!(train.len() + val.len() < bytes.len());
    }

    #[test]
    fn resource_declaration_lowers_deterministically() {
        let (result, fingerprint) = c7_eir().unwrap();
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(fingerprint, c7_eir().unwrap().1);
    }
}
