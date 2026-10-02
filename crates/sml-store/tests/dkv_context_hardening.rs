//! Engineering conformance, NOT a new DKV-3a scientific/promotion campaign.
//! Inputs are typed raw fixture records; expected answers never enter resolution.
use std::{
    collections::BTreeMap,
    fs,
    sync::atomic::{AtomicU64, Ordering},
};

use sml_core::{
    context_ledger::{ContextSegmentId, ContextSegmentRef, ContextStreamId, RawContextLedger},
    delta_kv::{DeltaKvCounters, DeltaKvMemory, DELTA_KV_ONE},
};
use sml_store::{
    delta_kv_context::{
        ContextError, ContextLimits, DeltaKvContextMemory, PromotionDecision, PromotionEvidence,
        VerificationLimits, VerifiedContext,
    },
    raw_context::{
        build_raw_segment_artifact, parse_raw_segment_artifact, sha256_raw_bytes,
        RawContextStoreLimits,
    },
};

const KEY: [i16; 4] = [4096, 0, 2048, -1024];
const VALUE: [i16; 2] = [4096, -4096];
const RATE: u16 = 4096;
fn limits() -> ContextLimits {
    ContextLimits {
        max_writes: 32,
        max_sources_per_write: 8,
        max_source_links: 64,
        max_ledger_segments: 32,
        max_ledger_semantic_links: 64,
        max_element_bytes: 1_000_000,
        max_staging_bytes: 1_000_000,
    }
}
fn verification_limits() -> VerificationLimits {
    VerificationLimits {
        max_sources: 32,
        max_raw_bytes: 4096,
    }
}
fn store_limits() -> RawContextStoreLimits {
    RawContextStoreLimits {
        max_file_bytes: 8192,
        max_raw_bytes: 4096,
        max_semantic_links: 8,
    }
}
struct World {
    ledger: RawContextLedger,
    bytes: BTreeMap<ContextSegmentId, Vec<u8>>,
}
impl World {
    fn new() -> Self {
        Self {
            ledger: RawContextLedger::new(ContextStreamId::new(1).unwrap()),
            bytes: BTreeMap::new(),
        }
    }
    fn append(&mut self, text: &str, semantic: &[u64]) -> ContextSegmentId {
        let count = self.ledger.segments().len() as u64;
        // Intentionally non-contiguous segment IDs: no id-1 array indexing.
        let id = ContextSegmentId::new(self.ledger.stream_id(), 10 + count * 7).unwrap();
        let stats = self.ledger.stats();
        let reference = ContextSegmentRef::new(
            id,
            stats.raw_bytes,
            text.len() as u64,
            stats.tokenizer_tokens,
            0,
            count,
            sha256_raw_bytes(text.as_bytes()).unwrap(),
            semantic.to_vec(),
        )
        .unwrap();
        self.ledger.append(reference).unwrap();
        self.bytes.insert(id, text.as_bytes().to_vec());
        id
    }
    fn memory(&self) -> DeltaKvContextMemory {
        DeltaKvContextMemory::new(self.ledger.stream_id(), 4, 2, limits()).unwrap()
    }
    fn verify<'a>(
        &'a self,
        memory: &'a DeltaKvContextMemory,
        key: &[i16],
    ) -> Result<VerifiedContext<'a>, ContextError> {
        memory
            .query(key)?
            .verify(&self.ledger, verification_limits(), |reference| {
                Ok(self.bytes[&reference.identity].clone())
            })
    }
}

#[test]
fn empty_unknown_partial_duplicate_and_foreign_provenance_reject_without_mutation() {
    let mut world = World::new();
    let a = world.append("1\tcalm\t10\tcalm\t-", &[1]);
    let b = world.append("1\tcalm\t10\tcalm\t-", &[1]);
    let wrong = world.append("2\tother\t01\tother\t-", &[2]);
    let foreign = ContextSegmentId::new(ContextStreamId::new(2).unwrap(), 10).unwrap();
    let absent = ContextSegmentId::new(world.ledger.stream_id(), 999).unwrap();
    let mut memory = world.memory();
    for sources in [
        vec![],
        vec![a],
        vec![a, a],
        vec![wrong],
        vec![foreign],
        vec![absent],
    ] {
        let before = memory.clone();
        assert!(memory
            .write(&world.ledger, 1, &KEY, &VALUE, RATE, &sources)
            .is_err());
        assert_eq!(memory, before);
    }
    assert!(matches!(
        memory.query(&KEY),
        Err(ContextError::NoWriteEvidence)
    ));
    memory
        .write(&world.ledger, 1, &KEY, &VALUE, RATE, &[b, a])
        .unwrap();
    assert_eq!(memory.evidence()[0].sources(), &[a, b]);
    assert_eq!(world.verify(&memory, &KEY).unwrap().sources().len(), 2);
}

#[test]
fn numeric_and_shape_errors_are_transactional() {
    let mut world = World::new();
    let id = world.append("1\tcalm\t10\tcalm\t-", &[1]);
    let mut memory = world.memory();
    for (key, target, rate) in [
        (&[0, 0, 0, 0][..], &VALUE[..], RATE),
        (&KEY[..], &VALUE[..], 4097),
        (&KEY[..3], &VALUE[..], RATE),
        (&KEY[..], &VALUE[..1], RATE),
    ] {
        let before = memory.clone();
        assert!(memory
            .write(&world.ledger, 1, key, target, rate, &[id])
            .is_err());
        assert_eq!(memory, before);
    }
}

#[test]
fn overlapping_unwritten_key_and_wrong_dimension_cannot_obtain_receipt() {
    let mut world = World::new();
    let id = world.append("1\tcalm\t10\tcalm\t-", &[1]);
    let mut memory = world.memory();
    memory
        .write(&world.ledger, 1, &KEY, &VALUE, RATE, &[id])
        .unwrap();
    let overlapping = [4096, 0, 2048, 1024];
    assert!(memory.read(&overlapping).unwrap().iter().any(|&v| v != 0));
    assert!(matches!(
        memory.query(&overlapping),
        Err(ContextError::NoWriteEvidence)
    ));
    assert!(matches!(
        memory.query(&KEY[..3]),
        Err(ContextError::ShapeMismatch)
    ));
    assert_eq!(world.verify(&memory, &KEY).unwrap().key(), KEY);
}

#[test]
fn all_versions_are_verified_once_and_query_identity_is_retained() {
    let mut world = World::new();
    let a = world.append("1\towner\t10\tAlice\t-", &[1]);
    let mut memory = world.memory();
    memory
        .write(&world.ledger, 1, &KEY, &VALUE, RATE, &[a])
        .unwrap();
    let b = world.append("2\towner\t01\tJonas\t1", &[2]);
    memory
        .write(&world.ledger, 2, &KEY, &[-4096, 4096], RATE, &[b])
        .unwrap();
    memory
        .write(&world.ledger, 2, &KEY, &[-4096, 4096], RATE, &[b])
        .unwrap();
    assert_eq!(
        memory
            .evidence()
            .iter()
            .map(|r| r.context_generation())
            .collect::<Vec<_>>(),
        [1, 2, 2]
    );
    assert_eq!(memory.query(&KEY).unwrap().records().count(), 3);
    let verified = world.verify(&memory, &KEY).unwrap();
    assert_eq!(verified.context_generation(), 2);
    assert_eq!(verified.sources().len(), 2);
    assert_eq!(verified.raw_bytes(), world.ledger.stats().raw_bytes);
    assert_eq!(
        resolve(&verified, "owner"),
        Ok(Answer {
            bits: [false, true],
            value: "Jonas".into()
        })
    );
    assert_eq!(
        resolve(&verified, "invoice"),
        Err(ResolveError::WrongQuestion)
    );
}

#[test]
fn empty_limits_corruption_and_source_substitution_cannot_mint_receipt() {
    let mut world = World::new();
    let id = world.append("1\tcalm\t10\tcalm\t-", &[1]);
    let mut memory = world.memory();
    memory
        .write(&world.ledger, 1, &KEY, &VALUE, RATE, &[id])
        .unwrap();
    for bytes in [
        vec![],
        b"1\tcalm\t10\tFAKE\t-".to_vec(),
        b"unrelated".to_vec(),
    ] {
        assert!(matches!(
            memory
                .query(&KEY)
                .unwrap()
                .verify(&world.ledger, verification_limits(), |_| Ok(bytes.clone())),
            Err(ContextError::SourceMismatch)
        ));
    }
    let mut calls = 0;
    for budget in [
        VerificationLimits {
            max_sources: 0,
            max_raw_bytes: 999,
        },
        VerificationLimits {
            max_sources: 8,
            max_raw_bytes: 1,
        },
    ] {
        assert!(memory
            .query(&KEY)
            .unwrap()
            .verify(&world.ledger, budget, |_| {
                calls += 1;
                Ok(vec![])
            })
            .is_err());
    }
    assert_eq!(calls, 0);
}

#[test]
fn forked_stale_and_foreign_ledgers_reject() {
    let mut world = World::new();
    let a = world.append("1\tcalm\t10\tcalm\t-", &[1]);
    let mut memory = world.memory();
    memory
        .write(&world.ledger, 1, &KEY, &VALUE, RATE, &[a])
        .unwrap();
    let mut fork = World::new();
    let fork_id = fork.append("1\tcalm\t10\tFAKE\t-", &[1]);
    assert_eq!(a, fork_id);
    assert!(matches!(
        fork.verify(&memory, &KEY),
        Err(ContextError::ContextMismatch)
    ));
    let before = memory.clone();
    assert!(matches!(
        memory.write(&fork.ledger, 1, &KEY, &VALUE, RATE, &[fork_id]),
        Err(ContextError::ContextMismatch)
    ));
    assert_eq!(memory, before);
    let mut foreign = RawContextLedger::new(ContextStreamId::new(2).unwrap());
    let foreign_id = ContextSegmentId::new(foreign.stream_id(), 10).unwrap();
    foreign
        .append(
            ContextSegmentRef::new(
                foreign_id,
                0,
                1,
                0,
                0,
                0,
                sha256_raw_bytes(b"x").unwrap(),
                vec![1],
            )
            .unwrap(),
        )
        .unwrap();
    assert!(memory
        .query(&KEY)
        .unwrap()
        .verify(&foreign, verification_limits(), |_| Ok(vec![]))
        .is_err());
    world.append("2\tnew\t01\tfresh\t-", &[2]);
    assert!(matches!(
        world.verify(&memory, &KEY),
        Err(ContextError::ContextMismatch)
    ));
}

#[test]
fn write_and_element_limits_fail_before_logical_mutation() {
    let mut world = World::new();
    let id = world.append("1\tcalm\t10\tcalm\t-", &[1]);
    let mut cap = limits();
    cap.max_writes = 1;
    cap.max_source_links = 1;
    let mut memory = DeltaKvContextMemory::new(world.ledger.stream_id(), 4, 2, cap).unwrap();
    memory
        .write(&world.ledger, 1, &KEY, &VALUE, RATE, &[id])
        .unwrap();
    let before = memory.clone();
    assert!(matches!(
        memory.write(&world.ledger, 1, &KEY, &VALUE, RATE, &[id]),
        Err(ContextError::BudgetExceeded)
    ));
    assert_eq!(memory, before);
    assert!(
        memory.element_plan().max_retained_element_bytes
            > memory.element_plan().matrix_coefficient_bytes
    );
    cap.max_element_bytes = 1;
    assert!(DeltaKvContextMemory::new(world.ledger.stream_id(), 4, 2, cap).is_err());
    assert!(DeltaKvContextMemory::new(world.ledger.stream_id(), usize::MAX, 2, limits()).is_err());
    cap = limits();
    cap.max_staging_bytes = 1;
    assert!(DeltaKvContextMemory::new(world.ledger.stream_id(), 4, 2, cap).is_err());
}

#[test]
fn ingestion_and_query_costs_are_separate() {
    let mut world = World::new();
    let id = world.append("1\tcalm\t10\tcalm\t-", &[1]);
    let mut memory = world.memory();
    memory
        .write(&world.ledger, 1, &KEY, &VALUE, RATE, &[id])
        .unwrap();
    memory.read(&KEY).unwrap();
    assert_eq!(memory.numeric_counters().read_macs, 16);
    assert_eq!(memory.context_counters().ingest_prediction_macs, 8);
    assert_eq!(memory.context_counters().query_prediction_macs, 8);
    assert_eq!(memory.context_counters().matrix_coefficients_copied, 8);
    assert_eq!(memory.context_counters().ledger_descriptors_copied, 1);
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Answer {
    bits: [bool; 2],
    value: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
enum ResolveError {
    Malformed,
    WrongQuestion,
    Conflict,
    BrokenSupersession,
}
struct Parsed<'a> {
    id: u64,
    topic: &'a str,
    bits: [bool; 2],
    value: &'a str,
    supersedes: Option<u64>,
}
fn parse(raw: &[u8]) -> Result<Parsed<'_>, ResolveError> {
    let text = std::str::from_utf8(raw).map_err(|_| ResolveError::Malformed)?;
    let fields: Vec<_> = text.split('\t').collect();
    if fields.len() != 5 || fields[3].is_empty() {
        return Err(ResolveError::Malformed);
    }
    let id: u64 = fields[0].parse().map_err(|_| ResolveError::Malformed)?;
    if id == 0 {
        return Err(ResolveError::Malformed);
    }
    let bits = match fields[2] {
        "00" => [false, false],
        "01" => [false, true],
        "10" => [true, false],
        "11" => [true, true],
        _ => return Err(ResolveError::Malformed),
    };
    let supersedes = if fields[4] == "-" {
        None
    } else {
        Some(fields[4].parse().map_err(|_| ResolveError::Malformed)?)
    };
    Ok(Parsed {
        id,
        topic: fields[1],
        bits,
        value: fields[3],
        supersedes,
    })
}
/// Resolves only verified bytes using the declared fixture grammar. No FACTS or
/// expected-answer table is visible to this path; this is not an NLP parser.
fn resolve(receipt: &VerifiedContext<'_>, topic: &str) -> Result<Answer, ResolveError> {
    let mut active = BTreeMap::new();
    let mut seen = BTreeMap::new();
    for source in receipt.sources() {
        let record = parse(source.bytes())?;
        if record.topic != topic {
            return Err(ResolveError::WrongQuestion);
        }
        if !source
            .reference()
            .semantic_record_ids()
            .contains(&record.id)
            || seen.insert(record.id, ()).is_some()
        {
            return Err(ResolveError::Malformed);
        }
        if let Some(previous) = record.supersedes {
            if active.remove(&previous).is_none() {
                return Err(ResolveError::BrokenSupersession);
            }
        }
        active.insert(
            record.id,
            Answer {
                bits: record.bits,
                value: record.value.into(),
            },
        );
    }
    let answer = active.values().next().ok_or(ResolveError::Malformed)?;
    if active.values().any(|a| a != answer) {
        return Err(ResolveError::Conflict);
    }
    Ok(answer.clone())
}

#[test]
fn contradiction_and_typed_supersession_not_substrings() {
    for second in [
        "2\tlimit\t01\t20\t-",
        "2\tlimit\t01\t20 supersedes everything\t-",
        "2\tlimit\t01\t20\t999",
        "2\tlimit\t01\t20\t1",
    ] {
        let mut world = World::new();
        let a = world.append("1\tlimit\t10\t10\t-", &[1]);
        let mut memory = world.memory();
        memory
            .write(&world.ledger, 1, &KEY, &VALUE, RATE, &[a])
            .unwrap();
        let b = world.append(second, &[2]);
        memory
            .write(&world.ledger, 2, &KEY, &[-4096, 4096], RATE, &[b])
            .unwrap();
        let result = resolve(&world.verify(&memory, &KEY).unwrap(), "limit");
        if second.ends_with("\t1") {
            assert_eq!(result.unwrap().value, "20");
        } else {
            assert!(result.is_err());
        }
    }
}

#[test]
fn exact_values_do_not_accept_numeric_or_code_substrings() {
    for (topic, actual, wrong) in [
        ("invoice", "17731", "7731"),
        ("config", "retry_limit=70", "retry_limit=7"),
    ] {
        let mut world = World::new();
        let text = format!("1\t{topic}\t10\t{actual}\t-");
        let id = world.append(&text, &[1]);
        let mut memory = world.memory();
        memory
            .write(&world.ledger, 1, &KEY, &VALUE, RATE, &[id])
            .unwrap();
        let answer = resolve(&world.verify(&memory, &KEY).unwrap(), topic).unwrap();
        assert_eq!(answer.value, actual);
        assert_ne!(answer.value, wrong);
    }
}

static TEMP: AtomicU64 = AtomicU64::new(1);
#[test]
fn persisted_artifact_roundtrip_and_corruption_fail_closed() {
    let mut world = World::new();
    let id = world.append("1\tcalm\t10\tcalm\t-", &[1]);
    let mut memory = world.memory();
    memory
        .write(&world.ledger, 1, &KEY, &VALUE, RATE, &[id])
        .unwrap();
    let path = std::env::temp_dir().join(format!(
        "sml-dkv-hardening-{}-{}.ctx",
        std::process::id(),
        TEMP.fetch_add(1, Ordering::Relaxed)
    ));
    let reference = &world.ledger.segments()[0];
    let artifact = build_raw_segment_artifact(reference, &world.bytes[&id]).unwrap();
    fs::write(&path, &artifact).unwrap();
    let receipt = memory
        .query(&KEY)
        .unwrap()
        .verify(&world.ledger, verification_limits(), |_| {
            Ok(parse_raw_segment_artifact(&fs::read(&path).unwrap(), store_limits())?.raw_bytes)
        })
        .unwrap();
    assert_eq!(receipt.sources()[0].bytes(), world.bytes[&id]);
    drop(receipt);
    let mut broken = artifact;
    *broken.last_mut().unwrap() ^= 1;
    fs::write(&path, broken).unwrap();
    assert!(memory
        .query(&KEY)
        .unwrap()
        .verify(&world.ledger, verification_limits(), |_| {
            Ok(parse_raw_segment_artifact(&fs::read(&path).unwrap(), store_limits())?.raw_bytes)
        })
        .is_err());
    fs::remove_file(path).unwrap();
}

#[test]
fn promotion_is_separate_from_ci_and_requires_every_evidence_field() {
    for mask in 0..128_u8 {
        let e = PromotionEvidence {
            bound_source_contract: mask & 1 != 0,
            no_unverified_source_answers: mask & 2 != 0,
            no_false_sourced_misses: mask & 4 != 0,
            raw_provenance_complete: mask & 8 != 0,
            matched_correctness: Some(mask & 16 != 0),
            resource_evidence_complete: mask & 32 != 0,
            sbg_mor_projected_recurrence_qualified: mask & 64 != 0,
        };
        assert_eq!(
            e.decision() == PromotionDecision::EligibleForIndependentReview,
            mask == 127
        );
    }
    let e = PromotionEvidence {
        bound_source_contract: true,
        no_unverified_source_answers: true,
        no_false_sourced_misses: true,
        raw_provenance_complete: true,
        matched_correctness: None,
        resource_evidence_complete: true,
        sbg_mor_projected_recurrence_qualified: true,
    };
    assert_eq!(e.decision(), PromotionDecision::Blocked);
    println!("DKV_CONTEXT_PROMOTION=BLOCKED; SBG_MOR_PROJECTED_RECURRENCE=NOT_EXECUTED");
}

#[test]
fn newest_sign_correct_does_not_imply_exact_q12() {
    let mut m = DeltaKvMemory::new(1, 1).unwrap();
    let mut c = DeltaKvCounters::default();
    m.update(&[3000], &[DELTA_KV_ONE], RATE, &mut c).unwrap();
    let value = m.read(&[3000], &mut c).unwrap()[0];
    assert!(value > 0);
    // Separate error criterion is required; do not assert a general exactness law.
    assert!((value - i32::from(DELTA_KV_ONE)).abs() <= 1);
}
