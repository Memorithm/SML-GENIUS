use sml_decision::dec2_text::{accuracy, select_on_dev_dim, words, Example, Kind};
use sml_decision::dec4_instr::{
    answer, content_bits, features, held_out, intents, seen, wording, Intent, DIM4,
};

const ALL: [Intent; 5] = [
    Intent::Continues,
    Intent::NotContinues,
    Intent::Opening,
    Intent::Middle,
    Intent::End,
];

#[test]
fn wordings_are_disjoint_and_sized() {
    for i in ALL {
        assert!(seen(i).len() >= 2 && !held_out(i).is_empty());
        for w in held_out(i) {
            for j in ALL {
                assert!(!seen(j).contains(w), "{w} is also a seen wording");
            }
        }
    }
}

#[test]
fn each_example_gets_two_opposite_questions() {
    for label in 0..2 {
        let a: Vec<usize> = intents("t1", "x", label)
            .iter()
            .map(|&i| answer(i, label))
            .collect();
        assert_eq!(a.iter().sum::<usize>(), 1);
    }
    for label in 0..3 {
        for id in ["a", "b", "c", "d"] {
            let it = intents("t3", id, label);
            assert_eq!(it.len(), 2);
            assert_ne!(it[0], it[1]);
            assert_eq!((answer(it[0], label), answer(it[1], label)), (1, 0));
        }
    }
    assert!(intents("t2", "x", 0).is_empty());
}

#[test]
fn blind_rows_ignore_the_question() {
    let c = content_bits("t1", &["the cat sat", "on the mat"]);
    let q1 = wording(seen(Intent::Continues), "train", "id", Intent::Continues);
    let q2 = wording(
        seen(Intent::NotContinues),
        "train",
        "id",
        Intent::NotContinues,
    );
    assert_eq!(features("t1", &c, None), features("t1", &c, None));
    assert_ne!(features("t1", &c, Some(q1)), features("t1", &c, Some(q2)));
    let f = features("t1", &c, Some(q1));
    assert!(f.windows(2).all(|w| w[0] < w[1]));
    assert!(f.iter().all(|&i| (i as usize) < DIM4));
}

/// Content says yes/no through one planted word; the negated question flips it.
fn planted(n: usize, seed: usize) -> Vec<Example> {
    let mut out = Vec::new();
    for i in 0..n {
        let y = (i * 7 + seed) % 2;
        let a = format!("w{} w{} w{}", i % 97, (i * 3 + seed) % 89, i % 13);
        let b = if y == 1 {
            format!("{a} glue")
        } else {
            format!("v{} v{}", i % 31, i % 7)
        };
        let c = content_bits("t1", &[&a, &b]);
        for intent in intents("t1", &i.to_string(), y) {
            let q = wording(seen(intent), "train", &i.to_string(), intent);
            out.push(Example {
                x: vec![features("t1", &c, Some(q))],
                y: answer(intent, y),
            });
        }
    }
    out
}

#[test]
fn head_learns_to_read_the_negation() {
    let train = planted(600, 1);
    let test = planted(200, 2);
    let (q, int, _) = select_on_dev_dim(Kind::Binary, DIM4, &train, &test[..200]);
    assert!(accuracy(Kind::Binary, &int.head, &test[200..]).0 >= 190);
    assert!(accuracy(Kind::Binary, &q.head, &test[200..]).0 >= 180);
    assert!(words(held_out(Intent::NotContinues)[1]).contains(&"false".to_string()));
}
