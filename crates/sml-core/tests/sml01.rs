use sml_core::{
    anf::{AnfMonomial, AnfPolynomial},
    atlas::{AtlasPageManifest, CircuitAtlas},
    audit::{validate_non_attention_candidate, CandidateAuditError},
    gf2::{Gf2LinearMap, Gf2Vector},
    memory::{BoundedAssociativeMemory, ReadOutcome, ReplacementPolicy, WriteOutcome},
    BooleanWorkspace, ExecutionCounters, PageId, ParameterBudget,
};

#[test]
fn gf2_xor_and_dot_are_exact_and_accounted() {
    let left = Gf2Vector::from_bits(&[true, false, true, true]);
    let right = Gf2Vector::from_bits(&[true, true, false, true]);
    let mut counters = ExecutionCounters::default();

    let sum = left.xor(&right, &mut counters).unwrap();
    assert_eq!(sum.to_u64_le().unwrap(), 0b0110);
    assert!(!left.dot_parity(&right, &mut counters).unwrap());

    assert_eq!(counters.gf2_ops, 11);
    assert_eq!(counters.pairwise_comparisons, 0);
}

#[test]
fn gf2_linear_map_produces_deterministic_address_bits() {
    let map = Gf2LinearMap::new(
        3,
        vec![
            Gf2Vector::from_bits(&[true, false, true]),
            Gf2Vector::from_bits(&[false, true, true]),
        ],
    )
    .unwrap();
    let input = Gf2Vector::from_bits(&[true, true, false]);
    let mut counters = ExecutionCounters::default();

    let output = map.apply(&input, &mut counters).unwrap();
    assert_eq!(output.to_u64_le().unwrap(), 0b11);
    assert_eq!(counters.gf2_ops, 10);
}

#[test]
fn canonical_anf_composes_boolean_relations() {
    let x0 = AnfMonomial::new(vec![0]).unwrap();
    let x0x1 = AnfMonomial::new(vec![0, 1]).unwrap();
    let polynomial = AnfPolynomial::new(2, true, vec![x0, x0x1]).unwrap();
    let mut counters = ExecutionCounters::default();

    assert!(polynomial.evaluate(&[true, true], &mut counters).unwrap());
    assert_eq!(counters.anf_monomials, 2);
    assert_eq!(counters.boolean_ops, 1);
    assert_eq!(counters.gf2_ops, 2);
}

#[test]
fn anf_rejects_noncanonical_duplicate_monomials() {
    let x0a = AnfMonomial::new(vec![0]).unwrap();
    let x0b = AnfMonomial::new(vec![0]).unwrap();
    assert!(AnfPolynomial::new(1, false, vec![x0a, x0b]).is_err());
}

#[test]
fn associative_memory_exposes_collision_and_replacement() {
    let mut memory =
        BoundedAssociativeMemory::new(1, 4, ReplacementPolicy::ReplaceExisting).unwrap();
    let mut counters = ExecutionCounters::default();

    let first = BooleanWorkspace::from_bits(&[true, false, true, false]);
    let second = BooleanWorkspace::from_bits(&[false, true, false, true]);

    assert_eq!(
        memory.write(10, first, &mut counters).unwrap(),
        WriteOutcome::Inserted { slot: 0 }
    );
    assert_eq!(
        memory.write(20, second.clone(), &mut counters).unwrap(),
        WriteOutcome::Replaced {
            slot: 0,
            evicted_key: 10
        }
    );

    assert!(matches!(
        memory.read(10, &mut counters),
        ReadOutcome::KeyMismatch {
            slot: 0,
            resident_key: 20
        }
    ));
    assert!(matches!(
        memory.read(20, &mut counters),
        ReadOutcome::Hit {
            slot: 0,
            value,
            write_epoch: 1
        } if value == second
    ));

    assert_eq!(memory.semantic_bits(), 326);
    assert_eq!(counters.memory_collisions, 1);
    assert_eq!(counters.memory_replacements, 1);
    assert_eq!(counters.memory_hits, 1);
    assert_eq!(counters.memory_misses, 1);
    assert_eq!(counters.pairwise_comparisons, 0);
}

#[test]
fn associative_memory_can_reject_collision_without_mutation() {
    let mut memory =
        BoundedAssociativeMemory::new(1, 2, ReplacementPolicy::RejectCollision).unwrap();
    let mut counters = ExecutionCounters::default();

    memory
        .write(
            1,
            BooleanWorkspace::from_bits(&[true, false]),
            &mut counters,
        )
        .unwrap();
    assert!(matches!(
        memory
            .write(
                2,
                BooleanWorkspace::from_bits(&[false, true]),
                &mut counters
            )
            .unwrap(),
        WriteOutcome::CollisionRejected {
            slot: 0,
            resident_key: 1
        }
    ));
    assert!(matches!(
        memory.read(1, &mut counters),
        ReadOutcome::Hit { write_epoch: 0, .. }
    ));
}

#[test]
fn atlas_accounts_metadata_alignment_and_activation() {
    let page0 = AtlasPageManifest::new(
        PageId(0),
        ParameterBudget {
            boolean: 8,
            ternary: 0,
            residual4: 0,
        },
        8,
        16,
    )
    .unwrap();
    let page1 = AtlasPageManifest::new(
        PageId(1),
        ParameterBudget {
            boolean: 0,
            ternary: 4,
            residual4: 0,
        },
        0,
        8,
    )
    .unwrap();
    let atlas = CircuitAtlas::new(vec![page0, page1]).unwrap();

    assert_eq!(atlas.total_parameters(), 12);
    assert_eq!(atlas.total_semantic_bits(), 24);
    assert_eq!(atlas.total_physical_bytes(), 24);

    let mut counters = ExecutionCounters::default();
    let activation = atlas.activate(PageId(1), &mut counters).unwrap();
    assert_eq!(activation.learned_parameters, 4);
    assert_eq!(activation.semantic_bits, 8);
    assert_eq!(activation.physical_bytes, 8);
    assert_eq!(counters.atlas_page_activations, 1);
    assert_eq!(counters.atlas_parameter_touches, 4);
    assert_eq!(counters.atlas_storage_bits_touched, 8);
}

#[test]
fn atlas_rejects_sparse_or_reordered_page_ids() {
    let page = AtlasPageManifest::new(
        PageId(7),
        ParameterBudget {
            boolean: 1,
            ternary: 0,
            residual4: 0,
        },
        0,
        1,
    )
    .unwrap();
    assert!(CircuitAtlas::new(vec![page]).is_err());
}

#[test]
fn non_attention_audit_fails_closed() {
    let mut counters = ExecutionCounters::default();
    assert_eq!(validate_non_attention_candidate(&counters), Ok(()));

    counters.pairwise_comparisons = 1;
    assert_eq!(
        validate_non_attention_candidate(&counters),
        Err(CandidateAuditError::PairwiseComparisons(1))
    );

    counters.pairwise_comparisons = 0;
    counters.attention_score_ops = 2;
    assert_eq!(
        validate_non_attention_candidate(&counters),
        Err(CandidateAuditError::AttentionScoreOperations(2))
    );
}
