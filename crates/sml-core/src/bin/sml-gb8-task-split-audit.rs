//! GB-8A: audit exact-content leakage in the existing six synthetic SML tasks.
//!
//! This is NOT model evaluation. It tests whether the nominal train/dev/test
//! partitions carry distinct input event streams before any training claim.
//! Command: cargo run --locked --release -p sml-core --bin sml-gb8-task-split-audit

use std::collections::HashSet;

use sml_core::tasks::{
    copy_after_marker, delayed_bit_recall, distractor_rejection, keyed_fact_recall,
    overwrite_bit_recall, two_fact_conjunction, TaskInstance,
};

const SPLITS: [(&str, u64, u32); 4] = [
    ("train", 0, 1000),
    ("dev", 10_000, 200),
    ("test", 20_000, 200),
    ("ood", 30_000, 200),
];
const FAMILIES: [&str; 6] = [
    "delayed",
    "overwrite",
    "keyed",
    "copy",
    "conjunction",
    "distractor",
];

/// Generate an unmodified SML task fixture for a specific case ID, including
/// held-out longer spans in OOD. Family identities and cases are frozen.
fn generate(family: usize, case_id: u64, is_ood: bool) -> TaskInstance {
    let span = if is_ood {
        64 + (case_id % 64) as usize
    } else {
        (case_id % 16) as usize
    };
    let copy_len = if is_ood {
        32 + (case_id % 64) as usize
    } else {
        1 + (case_id % 8) as usize
    };
    match family {
        0 => delayed_bit_recall(case_id, span),
        1 => overwrite_bit_recall(case_id, span),
        2 => keyed_fact_recall(case_id, span).unwrap(),
        3 => copy_after_marker(case_id, copy_len).unwrap(),
        4 => two_fact_conjunction(case_id, span).unwrap(),
        5 => distractor_rejection(case_id, span).unwrap(),
        _ => unreachable!("frozen six families"),
    }
}

/// Comparison uses *semantic input streams*, deliberately excluding case ID
/// and the expected gold answer. If content repeats between partitions, no
/// amount of distinct case IDs justifies calling the examples independent.
fn input_key(family: usize, instance: &TaskInstance) -> String {
    let mut s = format!("{family}|");
    for event in &instance.events {
        s.push_str(&format!("{event:?};"));
    }
    s
}

struct Audit {
    /// Only inputs of a particular SML task family are compared.
    inputs: Vec<Vec<HashSet<String>>>,
    records: [[usize; 6]; 4],
}

impl Audit {
    fn build() -> Self {
        let mut inputs = vec![vec![HashSet::<String>::new(); FAMILIES.len()]; SPLITS.len()];
        let mut records = [[0_usize; 6]; 4];
        let mut all_case_ids = HashSet::new();

        for (split, &(_, start, count)) in SPLITS.iter().enumerate() {
            for case_id in start..start + u64::from(count) {
                assert!(all_case_ids.insert(case_id), "case IDs must be disjoint");
                for family in 0..FAMILIES.len() {
                    let instance = generate(family, case_id, split == 3);
                    assert_eq!(instance.case_id, case_id);
                    assert_eq!(
                        instance.oracle_matches(),
                        Ok(true),
                        "source task gold disagrees with independent solver"
                    );
                    let key = input_key(family, &instance);
                    inputs[split][family].insert(key);
                    records[split][family] += 1;
                }
            }
        }
        Self { inputs, records }
    }

    fn overlaps(&self, a: usize, b: usize, family: usize) -> usize {
        self.inputs[a][family]
            .intersection(&self.inputs[b][family])
            .count()
    }

    fn report(&self) {
        println!("split,family,total,unique_inputs,shared_with_train,shared_with_dev,shared_with_test,shared_with_ood");
        for (split, (name, _, _)) in SPLITS.iter().enumerate() {
            for (family, label) in FAMILIES.iter().enumerate() {
                let shared: Vec<_> = (0..SPLITS.len())
                    .map(|other| {
                        if other == split {
                            0
                        } else {
                            self.overlaps(split, other, family)
                        }
                    })
                    .collect();
                println!(
                    "{name},{label},{},{},{},{},{},{}",
                    self.records[split][family],
                    self.inputs[split][family].len(),
                    shared[0],
                    shared[1],
                    shared[2],
                    shared[3]
                );
            }
        }
        let leaks = (0..FAMILIES.len())
            .filter(|&family| self.overlaps(0, 2, family) > 0)
            .count();
        println!("GB8_SYNTHETIC_TEST_LEAKAGE_FAMILIES={leaks}");
        println!(
            "GB8_SYNTHETIC_IID_TEST_STATUS={}",
            if leaks == 0 {
                "NO_EXACT_INPUT_OVERLAP_DETECTED"
            } else {
                "UNQUALIFIED_EXACT_INPUT_OVERLAP"
            }
        );
        println!("GB8_SYNTHETIC_MODEL_ACCURACY=UNMEASURED");
    }
}

fn main() {
    Audit::build().report();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oracle_and_case_partitions_are_verified() {
        let audit = Audit::build();
        for (split, &(_, _, cases)) in SPLITS.iter().enumerate() {
            for family in 0..FAMILIES.len() {
                assert_eq!(audit.records[split][family], cases as usize);
                assert!(!audit.inputs[split][family].is_empty());
            }
        }
    }

    #[test]
    fn input_content_leakage_is_explicit_and_ood_is_distinct() {
        let audit = Audit::build();
        // Delayed-bit recall has only a tiny set of short input patterns.
        // Different case_id values alone cannot prevent a train/test overlap.
        assert!(audit.overlaps(0, 2, 0) > 0);
        for family in 0..FAMILIES.len() {
            assert_eq!(
                audit.overlaps(0, 3, family),
                0,
                "longer-span OOD fixtures must not match training verbatim"
            );
        }
    }

    #[test]
    fn same_case_deterministically_generates_identical_events() {
        for family in 0..FAMILIES.len() {
            let a = generate(family, 21_007, false);
            let b = generate(family, 21_007, false);
            assert_eq!(input_key(family, &a), input_key(family, &b));
            assert_eq!(a.expected, b.expected);
        }
    }
}
