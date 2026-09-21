use sml_core::{
    mor::{ExpertProposal, RouterId, MOR_SCALE_PPM},
    mor_affinity::{MorAffinityPolicy, SmlFingerprintPlan},
    mor_affinity_train::{
        MorAffinityTrainer, MorAffinityTrainingConfig, MorAffinityTrainingError,
        MorAffinityTrainingExample, NativeRouterTemplate,
    },
    BooleanWorkspace, PageId,
};

fn template(id: u32, page: u32) -> NativeRouterTemplate {
    NativeRouterTemplate {
        router_id: RouterId(id),
        reliability_ppm: MOR_SCALE_PPM,
        confidence_ppm: MOR_SCALE_PPM,
        balance_bias_ppm: 0,
        proposals: vec![ExpertProposal {
            page_id: PageId(page),
            score_ppm: 900_000,
        }],
    }
}

fn example(bits: &[bool], router: u32) -> MorAffinityTrainingExample {
    MorAffinityTrainingExample {
        state: BooleanWorkspace::from_bits(bits),
        target_router: RouterId(router),
    }
}

#[test]
fn trainer_recovers_separable_router_signatures() {
    let policy = MorAffinityPolicy::default();
    let plan = SmlFingerprintPlan::new(vec![0, 1, 2, 3], policy).unwrap();
    let examples = vec![
        example(&[true, true, false, false], 0),
        example(&[true, true, false, true], 0),
        example(&[true, false, false, false], 0),
        example(&[false, false, true, true], 1),
        example(&[false, false, true, false], 1),
        example(&[false, true, true, true], 1),
    ];

    let artifact = MorAffinityTrainer::train(
        plan,
        vec![template(0, 7), template(1, 9)],
        &examples,
        policy,
        MorAffinityTrainingConfig {
            max_atoms_per_router: 2,
            min_separation_ppm: 500_000,
            min_examples_per_router: 2,
        },
    )
    .unwrap();

    assert_eq!(artifact.report.examples, 6);
    assert_eq!(artifact.report.router_count, 2);
    assert_eq!(artifact.report.selected_atoms, 4);
    assert_eq!(artifact.report.training_correct, 6);
    assert_eq!(artifact.report.training_accuracy_ppm, MOR_SCALE_PPM);

    let router0 = &artifact.bank.routers()[0];
    let router1 = &artifact.bank.routers()[1];
    assert_eq!(router0.atoms.len(), 2);
    assert_eq!(router1.atoms.len(), 2);
    assert!(router0
        .atoms
        .iter()
        .any(|atom| atom.fingerprint_bit == 0 && atom.expected));
    assert!(router0
        .atoms
        .iter()
        .any(|atom| atom.fingerprint_bit == 2 && !atom.expected));
    assert!(router1
        .atoms
        .iter()
        .any(|atom| atom.fingerprint_bit == 0 && !atom.expected));
    assert!(router1
        .atoms
        .iter()
        .any(|atom| atom.fingerprint_bit == 2 && atom.expected));
}

#[test]
fn learned_bank_generalizes_to_matching_unseen_state() {
    let policy = MorAffinityPolicy::default();
    let plan = SmlFingerprintPlan::new(vec![0, 1, 2], policy).unwrap();
    let examples = vec![
        example(&[true, true, false], 0),
        example(&[true, false, false], 0),
        example(&[true, true, false], 0),
        example(&[false, false, true], 1),
        example(&[false, true, true], 1),
        example(&[false, false, true], 1),
    ];
    let artifact = MorAffinityTrainer::train(
        plan,
        vec![template(0, 3), template(1, 4)],
        &examples,
        policy,
        MorAffinityTrainingConfig {
            max_atoms_per_router: 2,
            min_separation_ppm: 500_000,
            min_examples_per_router: 2,
        },
    )
    .unwrap();

    let unseen = BooleanWorkspace::from_bits(&[true, false, false]);
    let batch = artifact.bank.materialize(&unseen).unwrap();
    let router0 = batch
        .evidence
        .iter()
        .find(|evidence| evidence.router_id == RouterId(0))
        .unwrap();
    let router1 = batch
        .evidence
        .iter()
        .find(|evidence| evidence.router_id == RouterId(1))
        .unwrap();

    assert!(router0.affinity_ppm > router1.affinity_ppm);
}

#[test]
fn trainer_is_deterministic() {
    let policy = MorAffinityPolicy::default();
    let plan = SmlFingerprintPlan::new(vec![0, 1], policy).unwrap();
    let examples = vec![
        example(&[true, false], 0),
        example(&[true, true], 0),
        example(&[false, true], 1),
        example(&[false, false], 1),
    ];
    let config = MorAffinityTrainingConfig {
        max_atoms_per_router: 2,
        min_separation_ppm: 400_000,
        min_examples_per_router: 2,
    };

    let first = MorAffinityTrainer::train(
        plan.clone(),
        vec![template(0, 0), template(1, 1)],
        &examples,
        policy,
        config,
    )
    .unwrap();
    let second = MorAffinityTrainer::train(
        plan,
        vec![template(0, 0), template(1, 1)],
        &examples,
        policy,
        config,
    )
    .unwrap();

    assert_eq!(first, second);
}

#[test]
fn router_without_discriminating_bit_fails_closed() {
    let policy = MorAffinityPolicy::default();
    let plan = SmlFingerprintPlan::new(vec![0, 1], policy).unwrap();
    let examples = vec![
        example(&[true, false], 0),
        example(&[false, true], 0),
        example(&[true, false], 1),
        example(&[false, true], 1),
    ];

    assert_eq!(
        MorAffinityTrainer::train(
            plan,
            vec![template(0, 0), template(1, 1)],
            &examples,
            policy,
            MorAffinityTrainingConfig {
                max_atoms_per_router: 2,
                min_separation_ppm: 100_000,
                min_examples_per_router: 2,
            },
        )
        .unwrap_err(),
        MorAffinityTrainingError::UndiscriminatedRouter(RouterId(0))
    );
}

#[test]
fn insufficient_class_support_fails_closed() {
    let policy = MorAffinityPolicy::default();
    let plan = SmlFingerprintPlan::new(vec![0], policy).unwrap();
    let examples = vec![
        example(&[true], 0),
        example(&[false], 1),
        example(&[false], 1),
    ];

    assert_eq!(
        MorAffinityTrainer::train(
            plan,
            vec![template(0, 0), template(1, 1)],
            &examples,
            policy,
            MorAffinityTrainingConfig {
                max_atoms_per_router: 1,
                min_separation_ppm: 100_000,
                min_examples_per_router: 2,
            },
        )
        .unwrap_err(),
        MorAffinityTrainingError::InsufficientPositiveExamples {
            router_id: RouterId(0),
            actual: 1,
            minimum: 2,
        }
    );
}
