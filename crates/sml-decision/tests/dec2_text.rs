use sml_decision::dec2_text::{
    accuracy, epoch_order, pair_bits, planes, predict, quantize, select_on_dev, text_bits,
    train_averaged, words, Example, Head, Kind, DIM, HASH_BITS, THERMO,
};

/// Deterministic toy data: feature `signal + y` is planted, plus noise bits.
fn planted(n: usize, classes: usize, seed: usize) -> Vec<Example> {
    (0..n)
        .map(|i| {
            let y = (i * 7 + seed) % classes;
            let mut x: Vec<u32> = (0..6)
                .map(|k| ((i * 131 + k * 977 + seed * 31) % 50_000) as u32 + 100)
                .collect();
            x.push(10 + y as u32);
            x.sort_unstable();
            x.dedup();
            Example { x: vec![x], y }
        })
        .collect()
}

#[test]
fn words_and_bits_are_canonical() {
    assert_eq!(words("Hello, WORLD-42 ok"), ["hello", "world", "42", "ok"]);
    let a = text_bits("the cat sat on the mat");
    let b = text_bits("the cat sat on the mat");
    assert_eq!(a, b);
    assert!(a.windows(2).all(|w| w[0] < w[1]));
    assert!(a.iter().all(|&i| i < HASH_BITS));
    // 5 distinct unigrams + 5 bigrams, barring hash collisions.
    assert!(a.len() <= 10 && a.len() >= 9);
}

#[test]
fn pair_thermometers_follow_overlap() {
    let a = text_bits("alpha beta gamma delta");
    let same = pair_bits(&a, &a);
    let therm: Vec<u32> = same.iter().copied().filter(|&i| i >= HASH_BITS).collect();
    // Overlap thermometer: |A∩B| bits; Jaccard = 1 lights all 32 Jaccard bits.
    assert_eq!(therm.len(), a.len().min(THERMO as usize) + THERMO as usize);
    let none = pair_bits(&a, &text_bits("zeta eta theta"));
    assert!(none.is_empty());
    assert!(same.iter().all(|&i| (i as usize) < DIM));
}

#[test]
fn epoch_order_is_a_fixed_permutation() {
    let p = epoch_order(100, 3);
    let mut s = p.clone();
    s.sort_unstable();
    assert_eq!(s, (0..100).collect::<Vec<_>>());
    assert_eq!(p, epoch_order(100, 3));
    assert_ne!(p, epoch_order(100, 4));
}

#[test]
fn binary_head_learns_planted_bit_and_survives_ternary() {
    let train = planted(400, 2, 1);
    let test = planted(200, 2, 2);
    let (_, avg) = train_averaged(Kind::Binary, &train, &[2]).pop().unwrap();
    assert_eq!(accuracy(Kind::Binary, &avg, &test).0, 200);
    let t = quantize(Kind::Binary, &avg, 1, 1, &train);
    assert!(t.w.iter().all(|&v| (-1..=1).contains(&v)));
    assert_eq!(accuracy(Kind::Binary, &t, &test).0, 200);
}

#[test]
fn ordinal_head_learns_three_levels() {
    let train = planted(600, 3, 1);
    let test = planted(300, 3, 5);
    let (q, int, grid) = select_on_dev(Kind::Ordinal(3), &train, &test[..150]);
    // The planted bit decides the level; noise bits may cost a rare error.
    assert!(accuracy(Kind::Ordinal(3), &int.head, &test[150..]).0 >= 147);
    assert!(accuracy(Kind::Ordinal(3), &q.head, &test[150..]).0 >= 147);
    assert_eq!(grid.len(), 4 * (1 + 3 * 6));
    assert!(q.head.w.iter().all(|&v| v.abs() <= q.levels));
}

#[test]
fn choice_head_ranks_the_planted_candidate() {
    let mk = |i: usize| {
        let gold = i % 4;
        let x = (0..4)
            .map(|k| {
                let mut v = vec![(i * 13 + k * 101) as u32 % 40_000 + 200];
                if k == gold {
                    v.push(5);
                }
                v.sort_unstable();
                v
            })
            .collect();
        Example { x, y: gold }
    };
    let train: Vec<Example> = (0..200).map(mk).collect();
    let test: Vec<Example> = (200..300).map(mk).collect();
    let (q, _, _) = select_on_dev(Kind::Choice, &train, &test[..50]);
    assert!(test[50..]
        .iter()
        .all(|e| predict(Kind::Choice, &q.head, e) == e.y));
}

#[test]
fn quantize_respects_keep_fraction_and_levels() {
    let mut w = vec![0i64; DIM];
    for (i, v) in [
        (1usize, 100i64),
        (2, -50),
        (3, 25),
        (4, -12),
        (5, 6),
        (6, 3),
        (7, -1),
        (8, 1),
    ] {
        w[i] = v;
    }
    let avg = Head {
        classes: 1,
        w,
        b: vec![0],
    };
    let no_train: Vec<Example> = Vec::new();
    let half = quantize(Kind::Binary, &avg, 7, 2, &no_train);
    assert_eq!(half.nonzero(), 4);
    assert_eq!(&half.w[1..5], &[7, -4, 2, -1]);
    let ter = quantize(Kind::Binary, &avg, 1, 1, &no_train);
    assert_eq!(ter.nonzero(), 8);
    assert!(ter.w.iter().all(|&v| (-1..=1).contains(&v)));
    assert_eq!((planes(1), planes(3), planes(7)), (1, 2, 3));
}

#[test]
fn selection_is_deterministic() {
    let train = planted(300, 3, 3);
    let dev = planted(100, 3, 4);
    let a = select_on_dev(Kind::Ordinal(3), &train, &dev);
    let b = select_on_dev(Kind::Ordinal(3), &train, &dev);
    assert_eq!(a.0.head, b.0.head);
    assert_eq!(a.1.head, b.1.head);
    assert_eq!(a.2, b.2);
}
