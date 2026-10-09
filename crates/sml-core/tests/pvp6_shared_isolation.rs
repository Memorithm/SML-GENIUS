#![allow(dead_code)] // Shared frozen fixture also used by the standalone qualification target.
#[path = "support/a4_closed_loop.rs"]
mod fixture;
use sml_core::{
    pvp6_executor::{Pvp6Bank, Pvp6Gate},
    semantic::run_semantic_recurrent,
    semantic::session::SemanticSession,
    HaltingPolicy, PageId,
};

#[test]
fn forked_complete_pipeline_retains_old_evidence_and_isolated_revisions() {
    let initial = fixture::initial(0x50425636);
    let mut left = fixture::kernel(0x50425636, 2000, 8);
    let mut right = left.clone();
    let unchanged = left.clone();
    let bank = Pvp6Bank::new(vec![Pvp6Gate::from_monomials(&[0, 2]).unwrap()]).unwrap();
    right.replace_expert(PageId(0), 0, bank).unwrap();
    assert_eq!(left, unchanged);
    assert_ne!(left, right);
    let policy = HaltingPolicy {
        min_depth: 8,
        max_depth: 8,
        stable_hamming_threshold: 0,
    };
    let left_out = run_semantic_recurrent(initial.clone(), &mut left, policy).unwrap();
    let right_out = run_semantic_recurrent(initial, &mut right, policy).unwrap();
    assert_eq!(left_out.depth, 8);
    assert_eq!(right_out.depth, 8);
    assert_eq!(left.cycles().len(), 8);
    assert_eq!(right.cycles().len(), 8);
    assert_eq!(unchanged.cycles().len(), 0);
    assert_ne!(left, right);
}

#[test]
fn session_forks_preserve_original_trace_and_complete_continuation() {
    let initial = fixture::initial(0x50425636);
    let kernel = fixture::kernel(0x50425636, 2000, 8);
    let policy = HaltingPolicy {
        min_depth: 8,
        max_depth: 8,
        stable_hamming_threshold: 0,
    };
    let mut original = SemanticSession::new(initial, kernel, policy).unwrap();
    original.advance(2).unwrap();
    let before = original.clone();
    let mut resumed = original.checkpoint().resume();
    resumed.advance(6).unwrap();
    assert_eq!(original, before);
    assert_eq!(original.depth(), 2);
    assert_eq!(resumed.depth(), 8);
    assert_eq!(&resumed.trace()[..2], original.trace());
    assert_eq!(resumed.inner().cycles().len(), 8);
    assert_eq!(original.inner().cycles().len(), 2);
    let mut one_shot = original.checkpoint().resume();
    one_shot.advance(6).unwrap();
    assert_eq!(resumed, one_shot);
}
