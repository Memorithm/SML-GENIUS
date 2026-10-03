#[path = "../examples/support/counterpart_b.rs"]
mod fixture;
use fixture::*;
#[test]
fn distance_is_exact_and_ambiguity_is_not_mutated() {
    for seed in 0..8 {
        for h in [1, 2, 4, 8, 16, 32] {
            for d in 0..5 {
                let a = target_state(seed);
                let b = distractor_state(seed, d, h);
                assert_eq!(
                    a.value.xor_popcount(&b.value)
                        + a.meaning_mask.xor_popcount(&b.meaning_mask)
                        + a.routing.xor_popcount(&b.routing),
                    h as u64
                );
                assert_eq!(a.ambiguity, b.ambiguity);
            }
        }
    }
}
#[test]
fn native_fixed_route_preserves_projection_without_fabricating_engine_evidence() {
    for seed in 0..8 {
        let s = target_state(seed);
        let actual = route(&s, RouteMode::NativeFixed).unwrap();
        assert!(!actual.active_routers.is_empty());
        assert_eq!(
            actual.selected_experts,
            supplied_decision().selected_experts
        );
        let mut x = Work::default();
        let mut y = Work::default();
        let a = project::<16>(&s, seed, RouteMode::Supplied, false, &mut x)
            .unwrap()
            .unwrap();
        let b = project::<16>(&s, seed, RouteMode::NativeFixed, false, &mut y)
            .unwrap()
            .unwrap();
        assert_eq!(a.key_q12, b.key_q12);
        assert_eq!(x.mor_calls, 0);
        assert_eq!(y.mor_calls, 1);
    }
}
#[test]
fn two_native_routes_and_contrast_costs_are_explicit() {
    for group in 0..2 {
        let mut state = target_state(0);
        state.routing = sml_core::semantic::SemanticBitField::from_set_indices(
            &(0..64).filter(|i| i % 2 == group).collect::<Vec<_>>(),
        )
        .unwrap();
        assert_eq!(
            route(&state, RouteMode::NativeDual)
                .unwrap()
                .selected_experts,
            [sml_core::PageId(group as u32)]
        );
        let mut w = Work::default();
        let _ = project::<64>(&state, 0, RouteMode::NativeDual, true, &mut w).unwrap();
        assert_eq!(w.mor_calls, 1);
        assert_eq!(w.projector_calls, 2);
        assert_eq!(w.coordinates, 512);
    }
}
