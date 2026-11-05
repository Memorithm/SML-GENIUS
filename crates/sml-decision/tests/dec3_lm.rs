use sml_decision::dec2_text::{EXTRA_BASE, THERMO};
use sml_decision::dec3_lm::{bpc_bits, gain_bits, with_extra, Lm};

fn corpus() -> Vec<String> {
    (0..300)
        .map(|i| {
            format!(
                "the model reads sentence number {} and the boolean banks count every byte of it",
                i % 17
            )
        })
        .collect()
}

#[test]
fn training_lowers_cost_and_is_deterministic() {
    let texts = corpus();
    let mut a = Lm::new();
    let cold = a.bpc(&texts[0]);
    a.train(texts.iter().map(String::as_str));
    let warm = a.bpc(&texts[0]);
    assert!(warm < cold, "cold {cold} warm {warm}");
    let mut b = Lm::new();
    b.train(texts.iter().map(String::as_str));
    assert_eq!(a.weights_sha(), b.weights_sha());
    // Frozen scoring does not change the weights.
    let sha = a.weights_sha();
    a.bpc("anything at all");
    a.boundary_gain("the boolean banks count", "every byte of it");
    assert_eq!(a.weights_sha(), sha);
}

#[test]
fn boundary_gain_favours_the_true_continuation() {
    let mut lm = Lm::new();
    lm.train(corpus().iter().map(String::as_str));
    let a = "the model reads sentence number 3 and the boolean banks count";
    let true_gain = lm.boundary_gain(a, "every byte of it");
    let false_gain = lm.boundary_gain(a, "zebra quantum lattice");
    assert!(true_gain > false_gain, "{true_gain} vs {false_gain}");
}

#[test]
fn thermometers_sit_in_the_extension_block() {
    assert!(bpc_bits(0.5).is_empty());
    assert_eq!(bpc_bits(9.0).len(), THERMO as usize);
    assert_eq!(
        bpc_bits(2.0),
        (EXTRA_BASE..EXTRA_BASE + 8).collect::<Vec<_>>()
    );
    assert!(gain_bits(-10.0).is_empty());
    assert_eq!(gain_bits(0.25).len(), 9);
    assert!(gain_bits(100.0)
        .iter()
        .all(|&i| (EXTRA_BASE + THERMO..EXTRA_BASE + 2 * THERMO).contains(&i)));
    assert_eq!(
        with_extra(vec![5, 900], &[EXTRA_BASE, 6]),
        vec![5, 6, 900, EXTRA_BASE]
    );
}
