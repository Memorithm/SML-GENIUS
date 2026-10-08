//! Additional destination checks: address-width ladder, 65 gates, field padding,
//! and coefficient updates. This is exact CPU semantics, not a speed campaign.
use sml_core::{
    pascal_vector_projection::Pvp6PopulationProfile,
    pvp6_executor::{Pvp6Bank, Pvp6Gate},
    pvp6_sbg::{SbgAnfPlan, SbgCoordinate, SbgField},
    pvp6_sbg_kernel::{SbgAnfKernel, SbgAnfKernelError},
    semantic::{SemanticBitField, SemanticRewriter, SemanticState},
};
fn terms<const D: usize>(gate: usize, updated: bool) -> [u16; 4] {
    [
        if updated { 3 } else { 0 },
        if gate % 2 == 0 { 1 } else { 2 },
        (D / 2) as u16,
        (D - 1) as u16,
    ]
}
fn bank<const D: usize>(updated: bool) -> Pvp6Bank<D> {
    Pvp6Bank::new(
        (0..65)
            .map(|g| Pvp6Gate::from_monomials(&terms::<D>(g, updated)).unwrap())
            .collect(),
    )
    .unwrap()
}
fn width<const D: usize>() {
    let inputs = (0..D.trailing_zeros() as usize)
        .map(|bit| SbgCoordinate {
            field: SbgField::Value,
            bit,
        })
        .collect();
    let outputs = (0..65)
        .map(|bit| SbgCoordinate {
            field: SbgField::Value,
            bit,
        })
        .collect();
    let plan = SbgAnfPlan::<65, D>::new(vec![inputs], outputs).unwrap();
    for seed in [0, 1, u64::MAX, 0xaaaa_5555_aaaa_5555] {
        let mut state = SemanticState {
            value: SemanticBitField::from_words(vec![seed, 1]).unwrap(),
            meaning_mask: SemanticBitField::ones().unwrap(),
            ambiguity: SemanticBitField::from_words(vec![seed.rotate_right(7), 1]).unwrap(),
            routing: SemanticBitField::from_words(vec![!seed, 0]).unwrap(),
        };
        let mut kernel = SbgAnfKernel::new(
            bank::<D>(false),
            plan.clone(),
            Pvp6PopulationProfile::AffineUniform,
            3,
        )
        .unwrap();
        for depth in 1..=3 {
            let updated = depth >= 2;
            if depth == 2 {
                assert_eq!(kernel.replace_bank(bank::<D>(true)), Ok(1));
            }
            let literals = (0..D.trailing_zeros() as usize)
                .map(|bit| state.value.get(bit).unwrap())
                .collect::<Vec<_>>();
            let address = literals
                .iter()
                .enumerate()
                .fold(0usize, |x, (i, &v)| x | (usize::from(v) << i));
            let mut expected = state.clone();
            let mut expected_words = vec![0u64; 2];
            for gate in 0..65 {
                let mut value = false;
                for mask in terms::<D>(gate, updated) {
                    let product = literals
                        .iter()
                        .enumerate()
                        .all(|(i, &literal)| mask & (1 << i) == 0 || literal);
                    value ^= product;
                }
                expected.value.set(gate, value).unwrap();
                if value {
                    expected_words[gate / 64] |= 1u64 << (gate % 64);
                }
            }
            let actual = kernel.rewrite(&state, depth).unwrap();
            assert_eq!(actual.state, expected);
            let cycle = kernel.cycles().last().unwrap();
            assert_eq!(cycle.queries, vec![address]);
            assert_eq!(cycle.outputs, expected_words);
            assert_eq!(cycle.outputs[1] >> 1, 0);
            assert_eq!(cycle.bank_revision, u64::from(updated));
            assert_eq!(actual.state.meaning_mask, state.meaning_mask);
            assert_eq!(actual.state.ambiguity, state.ambiguity);
            assert_eq!(actual.state.routing, state.routing);
            state = actual.state;
        }
    }
}
#[test]
fn seven_domain_widths_and_65_gates_match_literal_oracle_after_updates() {
    width::<64>();
    width::<128>();
    width::<256>();
    width::<512>();
    width::<1024>();
    width::<2048>();
    width::<65536>();
    println!("A4_DOMAIN_BOUNDARIES,domains=7,seeds=4,cycles=84,gates=65,mismatches=0");
}
#[test]
fn outputs_at_last_partial_word_bit_update_all_four_fields() {
    let plan = SbgAnfPlan::<65, 64>::new(
        vec![(0..6)
            .map(|bit| SbgCoordinate {
                field: SbgField::Value,
                bit,
            })
            .collect()],
        [
            SbgField::Value,
            SbgField::Meaning,
            SbgField::Ambiguity,
            SbgField::Routing,
        ]
        .map(|field| SbgCoordinate { field, bit: 64 })
        .to_vec(),
    )
    .unwrap();
    let mut kernel = SbgAnfKernel::new(
        Pvp6Bank::new(vec![Pvp6Gate::from_monomials(&[0]).unwrap(); 4]).unwrap(),
        plan,
        Pvp6PopulationProfile::AffineUniform,
        1,
    )
    .unwrap();
    let mut state = SemanticState::zeroed().unwrap();
    state.value.set(3, true).unwrap();
    let step = kernel.rewrite(&state, 1).unwrap();
    assert_eq!(step.state.value.words(), &[8, 1]);
    assert_eq!(step.state.meaning_mask.words(), &[0, 1]);
    assert_eq!(step.state.ambiguity.words(), &[0, 1]);
    assert_eq!(step.state.routing.words(), &[0, 1]);
    let delta = kernel.cycles()[0].delta;
    assert_eq!(
        (
            delta.value,
            delta.meaning_mask,
            delta.ambiguity,
            delta.routing
        ),
        (1, 1, 1, 1)
    );
}
#[test]
fn mixed_density_bank_update_is_rejected_atomically() {
    let plan = SbgAnfPlan::<8, 64>::new(
        vec![(0..6)
            .map(|bit| SbgCoordinate {
                field: SbgField::Value,
                bit,
            })
            .collect()],
        (0..2)
            .map(|bit| SbgCoordinate {
                field: SbgField::Value,
                bit,
            })
            .collect(),
    )
    .unwrap();
    let bank = Pvp6Bank::new(vec![Pvp6Gate::from_monomials(&[0]).unwrap(); 2]).unwrap();
    let mut kernel =
        SbgAnfKernel::new(bank, plan, Pvp6PopulationProfile::AffineUniform, 4).unwrap();
    kernel
        .rewrite(&SemanticState::zeroed().unwrap(), 1)
        .unwrap();
    let before = kernel.clone();
    let wrong = Pvp6Bank::new(vec![
        Pvp6Gate::from_monomials(&[0]).unwrap(),
        Pvp6Gate::from_monomials(&[0, 1]).unwrap(),
    ])
    .unwrap();
    assert_eq!(kernel.replace_bank(wrong), Err(SbgAnfKernelError::Density));
    assert_eq!(kernel, before);
}
