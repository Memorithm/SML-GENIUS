//! BAF-3: deterministic, auditable matched-write routing controls.
//!
//! Run with `cargo run -p sml-core --release --example baf_focus_compare`.
//! Output is CSV; no timing, RSS, speed, quality or energy claims. This
//! benchmark measures only Boolean next-state errors and selection counts.
//! The Hamming control reuses the signature/ranking contract of HLE-0 but
//! deliberately uses identical fixed BAF page writes, not HLE-0 learned
//! hard-logic networks; therefore it is a ROUTER control, not a full-engine
//! comparison. No Transformer/Q/K/V attention is involved.

use sml_core::{
    boolean_associative_focus::{
        AnfActivation, AssociativeFocusConfig, AssociativeFocusPage, BooleanAssociativeFocus,
    },
    boolean_associative_joint::{fit_joint_focus, JointFocusPolicy},
    boolean_associative_train::{FocusFitPolicy, FocusTransition},
    PageId,
};

const N_PAGES: usize = 4;
const HAMMING_KEYS: u64 = 16;
const HAMMING_MAX_ROUNDS: usize = 8;

fn cfg() -> AssociativeFocusConfig {
    AssociativeFocusConfig {
        event_bits: 3,
        state_bits: 4,
        max_selected_pages: 2,
    }
}

fn pages() -> Vec<AssociativeFocusPage> {
    // Two writers, and two inert pages for key-based routing abstention.
    // Priority determines ownership; payload and page count are identical
    // across all router controls.
    [(10, 4), (20, 8), (30, 0), (40, 0)]
        .into_iter()
        .map(|(id, mask)| AssociativeFocusPage {
            id: PageId(id),
            priority: 100 - id,
            activation: AnfActivation {
                bias: false,
                terms: vec![],
            },
            write_mask: mask,
            write_value: mask,
        })
        .collect()
}

fn sample(case_id: u64, event: u64, state: u64) -> FocusTransition {
    // This data generator is used ONLY to create labeled panels. The fit
    // functions are passed observed final transitions, not its gate rules.
    let target = state
        | if (event & 1 != 0) && (state & 1 != 0) {
            4
        } else {
            0
        }
        | if (event & 2 != 0) && (state & 2 != 0) {
            8
        } else {
            0
        };
    FocusTransition {
        case_id,
        event,
        state,
        expected_next_state: target,
    }
}

fn panels() -> [(&'static str, Vec<FocusTransition>); 4] {
    let panel = |offset: u64, event_extra: u64, state_extra: u64| {
        (0..4u64)
            .flat_map(|event| {
                (0..4u64).map(move |state| {
                    sample(
                        offset + 4 * event + state,
                        event | event_extra,
                        state | state_extra,
                    )
                })
            })
            .collect()
    };
    [
        ("train", panel(0, 0, 0)),
        ("validation", panel(100, 4, 0)),
        ("test", panel(200, 0, 4)),
        ("ood", panel(300, 4, 8)),
    ]
}

fn evaluate<F: Fn(u64, u64) -> (u64, usize)>(
    observations: &[FocusTransition],
    run: F,
) -> (u64, u64) {
    let mut errors = 0u64;
    let mut selected = 0u64;
    for row in observations {
        let (state, pages) = run(row.event, row.state);
        errors += (state ^ row.expected_next_state).count_ones() as u64;
        selected += pages as u64;
    }
    (errors, selected)
}

fn eval_baf(model: &BooleanAssociativeFocus, rows: &[FocusTransition]) -> (u64, u64) {
    evaluate(rows, |e, s| {
        let out = model
            .step(e, s)
            .expect("generated input within declared width");
        (out.next_state, out.selected.len())
    })
}

/// HLE-0-style router CONTROL ONLY: signature = event XOR state, and pages
/// ranked by (popcount(signature XOR key), page ID), with fixed top-k.
/// The payload is exactly the same masked BAF write used by every control.
fn route_hamming(
    pages: &[AssociativeFocusPage],
    keys: &[u64; N_PAGES],
    event: u64,
    state: u64,
) -> (u64, usize) {
    let signature = (event ^ state) & 0b1111;
    let mut ranking: Vec<_> = pages
        .iter()
        .enumerate()
        .map(|(index, page)| ((signature ^ keys[index]).count_ones(), page.id.0, index))
        .collect();
    ranking.sort_unstable_by_key(|&(distance, id, _)| (distance, id));
    let mut selected: Vec<_> = ranking
        .iter()
        .take(cfg().max_selected_pages)
        .map(|&(_, _, index)| index)
        .collect();
    selected.sort_unstable_by_key(|&index| (u32::MAX - pages[index].priority, pages[index].id.0));
    let mut next = state;
    let mut claimed = 0u64;
    for index in selected {
        let page = &pages[index];
        let effective = page.write_mask & !claimed;
        next = (next & !effective) | (page.write_value & effective);
        claimed |= effective;
    }
    (next, cfg().max_selected_pages)
}

#[derive(Debug, Clone, Copy)]
struct HammingFit {
    keys: [u64; N_PAGES],
    train_eval_count: u64,
    admitted: bool,
}

/// Hamming page keys are fitted ONLY against TRAIN final-state labels.
/// VALIDATION is a single post-fit veto, identical to the ANF control gate.
/// The Hamming model has a deliberately small fixed 4-bit key vocabulary.
fn fit_hamming(
    pages: &[AssociativeFocusPage],
    train: &[FocusTransition],
    validation: &[FocusTransition],
) -> HammingFit {
    let initial = [0u64, 1, 2, 3];
    let mut keys = initial;
    let mut incumbent = evaluate(train, |e, s| route_hamming(pages, &keys, e, s)).0;
    let baseline = incumbent;
    let mut evaluations = 0u64;
    for _ in 0..HAMMING_MAX_ROUNDS {
        let mut round_loss = incumbent;
        let mut winner = None;
        for index in 0..N_PAGES {
            for key in 0..HAMMING_KEYS {
                if key == keys[index] {
                    continue;
                }
                let mut trial = keys;
                trial[index] = key;
                let loss = evaluate(train, |e, s| route_hamming(pages, &trial, e, s)).0;
                evaluations += 1;
                if loss < round_loss {
                    round_loss = loss;
                    winner = Some(trial);
                }
            }
        }
        let Some(next) = winner else {
            break;
        };
        keys = next;
        incumbent = round_loss;
    }
    // Train-selected candidate is frozen before consulting validation labels.
    let baseline_val = evaluate(validation, |e, s| route_hamming(pages, &initial, e, s)).0;
    let candidate_val = evaluate(validation, |e, s| route_hamming(pages, &keys, e, s)).0;
    let admitted = incumbent < baseline && candidate_val <= baseline_val;
    HammingFit {
        keys: if admitted { keys } else { initial },
        train_eval_count: evaluations,
        admitted,
    }
}

fn fit_anf(
    pages: &[AssociativeFocusPage],
    train: &[FocusTransition],
    validation: &[FocusTransition],
    degree: usize,
) -> (BooleanAssociativeFocus, u64, bool) {
    let policy = JointFocusPolicy {
        local: FocusFitPolicy {
            max_degree: degree,
            ..FocusFitPolicy::default()
        },
        ..JointFocusPolicy::default()
    };
    let fitted = fit_joint_focus(
        cfg(),
        pages,
        &[PageId(10), PageId(20)],
        train,
        validation,
        policy,
    )
    .expect("static bounded dataset and page layouts are valid");
    (
        fitted.model,
        fitted.report.examined_train_candidates,
        fitted.report.admitted,
    )
}

fn csv_row(
    name: &str,
    split: &str,
    rows: &[FocusTransition],
    evaluated: (u64, u64),
    train_evaluations: u64,
    admitted: bool,
) {
    println!(
        "{name},{split},{},{},{},{train_evaluations},{admitted}",
        rows.len(),
        evaluated.0,
        evaluated.1
    );
}

fn main() {
    let data = panels();
    let train = &data[0].1;
    let validation = &data[1].1;
    let atlas = pages();

    let baseline =
        BooleanAssociativeFocus::new(cfg(), atlas.clone()).expect("hard-coded BAF-0 config valid");
    let mut unconditional_pages = atlas.clone();
    for page in &mut unconditional_pages {
        page.activation.bias = true;
    }
    let unconditional = BooleanAssociativeFocus::new(cfg(), unconditional_pages)
        .expect("hard-coded unconditional config valid");
    let (degree1, one_eval, one_admitted) = fit_anf(&atlas, train, validation, 1);
    let (degree2, two_eval, two_admitted) = fit_anf(&atlas, train, validation, 2);
    let hamming = fit_hamming(&atlas, train, validation);

    println!("model,split,cases,error_bits,selected_pages,train_candidate_evaluations,admitted");
    for (split, rows) in &data {
        csv_row("inactive", split, rows, eval_baf(&baseline, rows), 0, false);
        csv_row(
            "unconditional",
            split,
            rows,
            eval_baf(&unconditional, rows),
            0,
            false,
        );
        csv_row(
            "anf_degree1",
            split,
            rows,
            eval_baf(&degree1, rows),
            one_eval,
            one_admitted,
        );
        csv_row(
            "anf_degree2",
            split,
            rows,
            eval_baf(&degree2, rows),
            two_eval,
            two_admitted,
        );
        csv_row(
            "hamming_key_router",
            split,
            rows,
            evaluate(rows, |e, s| route_hamming(&atlas, &hamming.keys, e, s)),
            hamming.train_eval_count,
            hamming.admitted,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panels_disjoint_and_keep_expected_true_transition() {
        let data = panels();
        for i in 0..data.len() {
            for j in (i + 1)..data.len() {
                for a in &data[i].1 {
                    for b in &data[j].1 {
                        assert_ne!(a.case_id, b.case_id);
                        assert_ne!((a.event, a.state), (b.event, b.state));
                    }
                }
            }
        }
        assert_eq!(sample(0, 3, 3).expected_next_state, 15);
    }

    #[test]
    fn joint_anf_fits_independent_boolean_conjunctions() {
        let data = panels();
        let (model, _budget, admitted) = fit_anf(&pages(), &data[0].1, &data[1].1, 2);
        assert!(admitted);
        assert_eq!(eval_baf(&model, &data[0].1).0, 0);
        assert_eq!(eval_baf(&model, &data[1].1).0, 0);
        assert_eq!(eval_baf(&model, &data[2].1).0, 0);
        assert_eq!(eval_baf(&model, &data[3].1).0, 0);
    }

    #[test]
    fn hamming_router_reproducible_train_only_key_fit() {
        let data = panels();
        let a = fit_hamming(&pages(), &data[0].1, &data[1].1);
        let b = fit_hamming(&pages(), &data[0].1, &data[1].1);
        assert_eq!(a.keys, b.keys);
        assert_eq!(a.admitted, b.admitted);
        assert_eq!(a.train_eval_count, b.train_eval_count);
    }
}
