use super::*;
use crate::hle4::{fit_orders, order_bpc};

#[test]
fn slot_memory_is_bounded_and_evicts_lowest_priority() {
    let mut m = SlotMemory::new(4, 7); // one set of 4 ways
    assert_eq!(m.capacity(), 4);
    assert_eq!(m.state_bytes(), 4 * SLOT_BYTES);
    for k in 0..4u64 {
        m.update(k, k as u8, true, 200);
    }
    m.update(99, 9, true, 10); // evicts one of the aged 200s
    m.update(100, 10, true, 250); // evicts the low-priority 99
    assert!(!m.read(99).hit);
    assert!(m.read(100).hit);
    let live = (0..4u64).filter(|&k| m.read(k).hit).count();
    assert_eq!(live, 3);
    // Non-write miss does not insert.
    m.update(500, 1, false, 255);
    assert!(!m.read(500).hit);
    // Confidence grows on a correct revisit.
    m.update(100, 10, false, 0);
    assert_eq!(m.read(100).conf, 1);
}

#[test]
fn hindsight_labels_look_only_within_horizon() {
    let t = b"abcdefghXabcdefghXzzzzzzzzzzzzzzzzzzzzabcdefghY".to_vec();
    let l = hindsight_labels(&t, 12);
    // pos 8 ('X' after 8-byte key "abcdefgh") recurs at pos 17 (within 12).
    assert!(l[8][0]);
    // pos 17 recurs at 46 with a different byte ('Y'): no label.
    assert!(!l[17][0]);
    let l2 = hindsight_labels(&t, 5);
    assert!(!l2[8][0]);
}

#[test]
fn memory_trace_is_causal_and_bounded() {
    let t: Vec<u8> = b"the quick brown fox. the quick brown fox. ".repeat(4);
    let ord = fit_base(&t);
    let f = base_features(&t, &ord, 64, false);
    let a = memory_trace(&t, &f, 64, 3, &WritePolicy::Always);
    let pos = 21 + 9;
    assert!(a[pos][0].hit && a[pos][0].byte == t[pos]);
    let mut t2 = t.clone();
    t2[pos] = b'#';
    let f2 = base_features(&t2, &ord, 64, false);
    let b = memory_trace(&t2, &f2, 64, 3, &WritePolicy::Always);
    assert_eq!(a[..=pos], b[..=pos]);
    // Zero-rate random gate never writes, so never hits.
    let z = memory_trace(&t, &f, 64, 3, &WritePolicy::Random { rate: 0.0, seed: 1 });
    assert!(z.iter().all(|r| !r[0].hit && !r[1].hit));
}

fn planted(n: usize, seed: u64) -> Vec<u8> {
    // Bytes are random letters except: bit 0 of t[pos] copies bit 3 of t[pos-5].
    let mut r = Rng::new(seed);
    let mut t: Vec<u8> = (0..n).map(|_| b'a' + r.below(16) as u8).collect();
    for p in 5..n {
        t[p] = (t[p] & !1) | ((t[p - 5] >> 3) & 1);
    }
    t
}

#[test]
fn walsh_screen_and_search_find_planted_long_range_bit() {
    let train = planted(6000, 1);
    let select = planted(3000, 2);
    let ord = fit_base(&train);
    let ft = base_features(&train, &ord, 64, true);
    let fs = base_features(&select, &ord, 64, false);
    let scr = walsh_screen(&train, &ft, 1);
    let target = 8 * 4 + 3; // t[pos-5], bit 3
    let best = (0..POOL)
        .max_by(|&a, &b| scr[a].total_cmp(&scr[b]))
        .unwrap();
    assert_eq!(best, target, "{scr:?}");
    let (g, st) = search_groups(
        GroupMode::Searched,
        &train,
        &select,
        &fs.order_st[0],
        &scr,
        1,
        6,
        20,
        5,
    );
    assert!(g[0].contains(&(target as u8)), "{g:?}");
    assert!(st.evaluations > 1);
    let (c, _) = search_groups(
        GroupMode::Contiguous,
        &train,
        &select,
        &fs.order_st[0],
        &scr,
        1,
        6,
        0,
        5,
    );
    assert!(!c[0].contains(&(target as u8)));
}

#[test]
fn learned_gate_fits_and_mixer_uses_memory_on_repetitive_text() {
    let t: Vec<u8> = b"alpha beta gamma delta epsilon zeta eta theta iota. ".repeat(40);
    let (train, test) = t.split_at(t.len() * 3 / 4);
    let ord = fit_base(train);
    let ft = base_features(train, &ord, 256, true);
    let fe = base_features(test, &ord, 256, false);
    let (w, rate) = fit_write_gate(train, &ft, 256, 1);
    assert!(rate > 0.0 && rate <= 1.0);
    let pol = WritePolicy::Learned(w);
    let mt = memory_trace(train, &ft, 256, 1, &pol);
    let me = memory_trace(test, &fe, 256, 1, &pol);
    let arm = |memory| Arm5 {
        cache: false,
        matcher: false,
        memory,
        heads: 0,
        routed: false,
        mix_lr: 0.02,
    };
    let s = |t, f, m| Stream5 {
        t,
        f,
        mem: Some(m),
        heads: vec![],
    };
    let a = run_arm5(&arm(false), &s(train, &ft, &mt), &s(test, &fe, &me));
    let b = run_arm5(&arm(true), &s(train, &ft, &mt), &s(test, &fe, &me));
    assert!(b.test_bpc < a.test_bpc - 0.02, "{a:?} {b:?}");
}

fn load_docs() -> Vec<(String, Vec<u8>)> {
    let dir = std::env::var("HLE5_CORPUS_DIR").map_or_else(
        |_| std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs"),
        std::path::PathBuf::from,
    );
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "md"))
        .collect();
    files.sort();
    files
        .iter()
        .map(|p| {
            (
                p.file_name().unwrap().to_string_lossy().into_owned(),
                std::fs::read(p).unwrap(),
            )
        })
        .collect()
}

fn stats(v: &[f64]) -> (f64, f64) {
    let mean = v.iter().sum::<f64>() / v.len() as f64;
    let spread =
        v.iter().cloned().fold(f64::MIN, f64::max) - v.iter().cloned().fold(f64::MAX, f64::min);
    (mean, spread)
}

/// HLE-5 table. Run with
/// `cargo test --release -p sml-core --lib hle5_table -- --ignored --nocapture`.
/// Env: `HLE5_CORPUS_DIR` (default repo docs), `HLE5_TRAIN` (default 1000000),
/// `HLE5_SEEDS` (3), `HLE5_CAP` (memory slots, 1024), `HLE5_WINDOW` (1024),
/// `HLE5_HEADS` (4), `HLE5_WIDTH` (12), `HLE5_ITERS` (24).
#[test]
#[ignore]
fn hle5_table() {
    let env = |k: &str, d: usize| {
        std::env::var(k)
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(d)
    };
    let docs = load_docs();
    let (ta, sa, ea) = split_docs(&docs);
    let train = &ta[..ta.len().min(env("HLE5_TRAIN", 1_000_000))];
    let select = &sa[..sa.len().min(60_000)];
    let test = &ea[..ea.len().min(80_000)];
    let seeds: Vec<u64> = (0..env("HLE5_SEEDS", 3) as u64)
        .map(|s| 7 + 1000 * s)
        .collect();
    let (cap, win) = (env("HLE5_CAP", 1024), env("HLE5_WINDOW", 1024));
    let (heads, width, iters) = (
        env("HLE5_HEADS", 4),
        env("HLE5_WIDTH", 12),
        env("HLE5_ITERS", 24),
    );
    println!(
        "HLE5,docs={},train_bytes={},select_bytes={},test_bytes={},seeds={seeds:?},cap_slots={cap},mem_state_bytes={},window={win},heads={heads},width={width},iters={iters}",
        docs.len(), train.len(), select.len(), test.len(), cap * SLOT_BYTES
    );
    let t0 = std::time::Instant::now();
    let ord = fit_base(train);
    for (m, k) in fit_orders(train, &[2, 3, 4, 5, 6], 2.0).iter().zip(2..) {
        println!(
            "HLE5,baseline,order{k}_alone,test_bpc={:.4}",
            order_bpc(m, k, test)
        );
    }
    let ft = base_features(train, &ord, win, true);
    let fs = base_features(select, &ord, win, false);
    let fe = base_features(test, &ord, win, false);
    let tg = std::time::Instant::now();
    let (gate, rate) = fit_write_gate(train, &ft, win, 1);
    println!(
        "HLE5,gate,weights={gate:?},train_write_rate={rate:.4},fit_s={:.1}",
        tg.elapsed().as_secs_f64()
    );
    let scr = walsh_screen(train, &ft, 3);
    let mut top: Vec<usize> = (0..POOL).collect();
    top.sort_by(|&a, &b| scr[b].total_cmp(&scr[a]));
    println!(
        "HLE5,walsh_top8(byte_back,bit)={:?}",
        top[..8]
            .iter()
            .map(|&i| (i / 8 + 1, i % 8))
            .collect::<Vec<_>>()
    );
    let fit = &train[..train.len().min(150_000)];

    struct SeedOut {
        rows: Vec<(String, ArmResult)>,
        log: Vec<String>,
    }
    let per_seed: Vec<SeedOut> = std::thread::scope(|sc| {
        let hs: Vec<_> = seeds
            .iter()
            .map(|&seed| {
                let (gate, ft, fs, fe, scr) = (&gate, &ft, &fs, &fe, &scr);
                sc.spawn(move || {
                    let mut log = Vec::new();
                    let pols = [
                        ("always", WritePolicy::Always),
                        ("random_rate_matched", WritePolicy::Random { rate, seed }),
                        ("learned", WritePolicy::Learned(gate.clone())),
                    ];
                    let mut mem = HashMap::new();
                    for (n, p) in &pols {
                        let tm = std::time::Instant::now();
                        let me = memory_trace(test, fe, cap, seed, p);
                        let ns = tm.elapsed().as_nanos() as f64 / test.len() as f64;
                        let hits = me.iter().filter(|r| r[0].hit || r[1].hit).count() as f64 / test.len() as f64;
                        log.push(format!("HLE5,memory,{n},seed={seed},test_hit_rate={hits:.4},pass_ns_per_byte={ns:.0}"));
                        mem.insert(*n, (memory_trace(train, ft, cap, seed, p), me));
                    }
                    let mut hs = HashMap::new();
                    for (n, mode) in [("contiguous", GroupMode::Contiguous), ("random", GroupMode::Random), ("searched", GroupMode::Searched)] {
                        let tsr = std::time::Instant::now();
                        let (gr, st) = search_groups(mode, fit, select, &fs.order_st[0], scr, heads, width, iters, seed);
                        let ss = tsr.elapsed().as_secs_f64();
                        log.push(format!("HLE5,groups,{n},seed={seed},groups={gr:?},evals={},accepted={},select_bpc={:.4},search_s={ss:.1}", st.evaluations, st.accepted, st.select_bpc));
                        let streams: Vec<(Vec<f32>, Vec<f32>)> = gr
                            .iter()
                            .enumerate()
                            .map(|(i, g)| {
                                let m = fit_head(train, i as u64, g);
                                (head_stream(train, &m, i as u64, g, true), head_stream(test, &m, i as u64, g, false))
                            })
                            .collect();
                        hs.insert(n, streams);
                    }
                    let a = |cache, matcher, memory, heads, routed| Arm5 { cache, matcher, memory, heads, routed, mix_lr: 0.01 };
                    let arms: Vec<(&str, Arm5, &str, &str)> = vec![
                        ("B0_mix_order3+4", a(false, false, false, 0, false), "always", "contiguous"),
                        ("B1_order34+cache+match(unbounded,HLE4)", a(true, true, false, 0, false), "always", "contiguous"),
                        ("M0_order34+cache+slot_always", a(true, false, true, 0, false), "always", "contiguous"),
                        ("M1_order34+cache+slot_random_rate", a(true, false, true, 0, false), "random_rate_matched", "contiguous"),
                        ("M2_order34+cache+slot_learned", a(true, false, true, 0, false), "learned", "contiguous"),
                        ("M3_B1+slot_always", a(true, true, true, 0, false), "always", "contiguous"),
                        ("M4_B1+slot_learned", a(true, true, true, 0, false), "learned", "contiguous"),
                        ("G0_B1+heads_contiguous", a(true, true, false, heads, false), "always", "contiguous"),
                        ("G1_B1+heads_random", a(true, true, false, heads, false), "always", "random"),
                        ("G2_B1+heads_searched", a(true, true, false, heads, false), "always", "searched"),
                        ("R0_B1+routed", a(true, true, false, 0, true), "always", "contiguous"),
                        ("F0_full_controls(slot_always+random_heads+routed)", a(true, true, true, heads, true), "always", "random"),
                        ("F1_full(slot_learned+searched_heads+routed)", a(true, true, true, heads, true), "learned", "searched"),
                    ];
                    let rows = arms
                        .into_iter()
                        .map(|(name, arm, mp, gm)| {
                            let (mt, me) = &mem[mp];
                            let (htr, hte): (Vec<&[f32]>, Vec<&[f32]>) =
                                hs[gm].iter().map(|(a, b)| (a.as_slice(), b.as_slice())).unzip();
                            let r = run_arm5(
                                &arm,
                                &Stream5 { t: train, f: ft, mem: Some(mt), heads: htr },
                                &Stream5 { t: test, f: fe, mem: Some(me), heads: hte },
                            );
                            (name.to_string(), r)
                        })
                        .collect();
                    SeedOut { rows, log }
                })
            })
            .collect();
        hs.into_iter().map(|h| h.join().unwrap()).collect()
    });
    for s in &per_seed {
        s.log.iter().for_each(|l| println!("{l}"));
    }
    println!("HLE5,arm,test_bpc_mean,spread(max-min),n,per_seed,mixer_params,train_s_mean,eval_ns_per_byte_mean");
    for (i, (name, _)) in per_seed[0].rows.iter().enumerate() {
        let v: Vec<f64> = per_seed.iter().map(|s| s.rows[i].1.test_bpc).collect();
        let (m, sp) = stats(&v);
        let tr = per_seed.iter().map(|s| s.rows[i].1.train_s).sum::<f64>() / per_seed.len() as f64;
        let ev = per_seed
            .iter()
            .map(|s| s.rows[i].1.eval_ns_per_byte)
            .sum::<f64>()
            / per_seed.len() as f64;
        println!(
            "HLE5,{name},{m:.4},{sp:.4},{},{:?},{},{tr:.1},{ev:.0}",
            v.len(),
            v.iter()
                .map(|x| (x * 1e4).round() / 1e4)
                .collect::<Vec<_>>(),
            per_seed[0].rows[i].1.mixer_params
        );
    }
    println!("HLE5,elapsed_s={:.0}", t0.elapsed().as_secs_f64());
}
