use sml_decision::dec16_loso::{base_training, fold_training, FOLDS};
use sml_decision::dec4_instr::Intent;

const ALL: [Intent; 5] = [
    Intent::Continues,
    Intent::NotContinues,
    Intent::Opening,
    Intent::Middle,
    Intent::End,
];

#[test]
fn folds_hold_164_wordings_and_are_excluded_from_their_training() {
    let n: usize = FOLDS
        .iter()
        .map(|(_, f)| ALL.iter().map(|&i| f(i).len()).sum::<usize>())
        .sum();
    assert_eq!(n, 164);
    for i in ALL {
        let base = base_training(i);
        for (k, (_, f)) in FOLDS.iter().enumerate() {
            for w in f(i) {
                assert!(!base.iter().any(|x| x == w));
                for j in ALL {
                    assert!(!fold_training(j, k).iter().any(|x| x == w), "{w}");
                }
            }
            for (m, (_, g)) in FOLDS.iter().enumerate() {
                if m != k {
                    for w in g(i) {
                        assert!(fold_training(i, k).iter().any(|x| x == w));
                    }
                }
            }
        }
    }
}
