//! DEC-2: Boolean text decision heads for the DEC-1 typed-decision bench.
//!
//! Each text becomes a set of hashed bit features (lowercase word unigrams and
//! bigrams). A pair of texts becomes the bits both share plus two thermometer
//! codes (overlap count and Jaccard). A head scores a bit set with ternary
//! weights of a few bit planes ({-1, 0, +1} for one plane): the score is a sum
//! over planes of 2^p·(popcount(x & w+_p) - popcount(x & w-_p)) plus an integer
//! bias. Training is an integer averaged perceptron followed by quantization to
//! 1, 3 or 7 levels and an integer bias refit. Everything is integer arithmetic and deterministic.
//!
//! The three heads mirror Laya's question types: `Binary` (noul), `Choice`
//! (one of N candidates scored by one shared head) and `Ordinal` (argmax over
//! levels). This learns each task from labels; it does not read instructions.

/// Hashed feature space for word unigrams and bigrams (power of two).
pub const HASH_BITS: u32 = 1 << 16;
/// Thermometer codes appended after the hashed space for pair features.
pub const THERMO: u32 = 32;
/// Total feature dimension.
pub const DIM: usize = (HASH_BITS + 2 * THERMO) as usize;
/// Pre-registered grids, selected on DEV only.
pub const EPOCH_GRID: [usize; 4] = [1, 2, 4, 8];
/// Kept fraction of non-zero weights, as 1 / KEEP_DIV[i] (1 keeps all).
pub const KEEP_DIV: [usize; 6] = [32, 16, 8, 4, 2, 1];
/// Bias-only refit passes after ternarization.
pub const BIAS_PASSES: usize = 4;
/// Pre-registered weight levels: 1 (ternary), 3 and 7 (two and three bit planes).
pub const LEVEL_GRID: [i64; 3] = [1, 3, 7];

fn fnv1a(tag: u8, a: &str, b: &str) -> u32 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in core::iter::once(tag)
        .chain(a.bytes())
        .chain(core::iter::once(b' '))
        .chain(b.bytes())
    {
        h ^= u64::from(byte);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    (h as u32) & (HASH_BITS - 1)
}

/// Lowercase ASCII-alphanumeric words; other characters separate words.
pub fn words(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for c in text.chars() {
        if c.is_ascii_alphanumeric() {
            cur.push(c.to_ascii_lowercase());
        } else if !cur.is_empty() {
            out.push(core::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Sorted, de-duplicated hashed unigram and bigram bits of one text.
pub fn text_bits(text: &str) -> Vec<u32> {
    let w = words(text);
    let mut bits: Vec<u32> = w.iter().map(|x| fnv1a(b'u', x, "")).collect();
    bits.extend(w.windows(2).map(|p| fnv1a(b'b', &p[0], &p[1])));
    bits.sort_unstable();
    bits.dedup();
    bits
}

fn intersect(a: &[u32], b: &[u32]) -> Vec<u32> {
    let (mut i, mut j, mut out) = (0, 0, Vec::new());
    while i < a.len() && j < b.len() {
        match a[i].cmp(&b[j]) {
            core::cmp::Ordering::Less => i += 1,
            core::cmp::Ordering::Greater => j += 1,
            core::cmp::Ordering::Equal => {
                out.push(a[i]);
                i += 1;
                j += 1;
            }
        }
    }
    out
}

/// Pair features: shared hashed bits, then the overlap-count thermometer
/// (bit k on iff |A∩B| >= k+1) and the Jaccard thermometer
/// (bit k on iff 64·|A∩B| > (k+1)·|A∪B|, i.e. Jaccard above (k+1)/64).
pub fn pair_bits(a: &[u32], b: &[u32]) -> Vec<u32> {
    let mut out = intersect(a, b);
    let inter = out.len();
    let union = a.len() + b.len() - inter;
    for k in 0..THERMO as usize {
        if inter > k {
            out.push(HASH_BITS + k as u32);
        }
    }
    for k in 0..THERMO as usize {
        if 64 * inter > (k + 1) * union {
            out.push(HASH_BITS + THERMO + k as u32);
        }
    }
    out
}

/// Deterministic permutation of 0..n for one epoch (splitmix64 Fisher–Yates).
pub fn epoch_order(n: usize, epoch: usize) -> Vec<usize> {
    let mut state = 0x9e37_79b9_7f4a_7c15u64 ^ (epoch as u64).wrapping_mul(0xd1b5_4a32_d192_ed03);
    let mut next = move || {
        state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    };
    let mut v: Vec<usize> = (0..n).collect();
    for i in (1..n).rev() {
        let j = (next() % (i as u64 + 1)) as usize;
        v.swap(i, j);
    }
    v
}

/// Integer averaged perceptron over `classes` rows of `DIM` weights.
/// `avg()` returns c·w − u, the average scaled by the step count c.
#[derive(Clone)]
pub struct Perceptron {
    classes: usize,
    w: Vec<i64>,
    u: Vec<i64>,
    b: Vec<i64>,
    ub: Vec<i64>,
    c: i64,
}

impl Perceptron {
    pub fn new(classes: usize) -> Self {
        Self {
            classes,
            w: vec![0; classes * DIM],
            u: vec![0; classes * DIM],
            b: vec![0; classes],
            ub: vec![0; classes],
            c: 1,
        }
    }

    fn score(&self, class: usize, x: &[u32]) -> i64 {
        let row = &self.w[class * DIM..(class + 1) * DIM];
        self.b[class] + x.iter().map(|&i| row[i as usize]).sum::<i64>()
    }

    fn add(&mut self, class: usize, x: &[u32], sign: i64, bias: bool) {
        let c = self.c;
        for &i in x {
            let k = class * DIM + i as usize;
            self.w[k] += sign;
            self.u[k] += sign * c;
        }
        if bias {
            self.b[class] += sign;
            self.ub[class] += sign * c;
        }
    }

    fn tick(&mut self) {
        self.c += 1;
    }

    /// Averaged weights and biases, scaled by the step count.
    pub fn avg(&self) -> (Vec<i64>, Vec<i64>) {
        let w = self
            .w
            .iter()
            .zip(&self.u)
            .map(|(w, u)| self.c * w - u)
            .collect();
        let b = self
            .b
            .iter()
            .zip(&self.ub)
            .map(|(b, u)| self.c * b - u)
            .collect();
        (w, b)
    }
}

/// A scoring head: dense integer weights (ternary or averaged) plus biases.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Head {
    pub classes: usize,
    pub w: Vec<i64>,
    pub b: Vec<i64>,
}

impl Head {
    pub fn score(&self, class: usize, x: &[u32]) -> i64 {
        let row = &self.w[class * DIM..(class + 1) * DIM];
        self.b[class] + x.iter().map(|&i| row[i as usize]).sum::<i64>()
    }

    /// Argmax over classes; ties go to the lowest class.
    pub fn argmax(&self, x: &[u32]) -> usize {
        let mut best = (i64::MIN, 0);
        for c in 0..self.classes {
            let s = self.score(c, x);
            if s > best.0 {
                best = (s, c);
            }
        }
        best.1
    }

    pub fn nonzero(&self) -> usize {
        self.w.iter().filter(|&&v| v != 0).count()
    }
}

/// The three typed tasks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// One score, yes iff score >= 0. One class row.
    Binary,
    /// One shared score per candidate, argmax wins (ties: first). One row.
    Choice,
    /// Argmax over `levels` rows.
    Ordinal(usize),
}

/// One training or evaluation example. For `Binary` and `Ordinal` the bit
/// sets hold one entry; for `Choice` one entry per candidate.
#[derive(Clone, Debug)]
pub struct Example {
    pub x: Vec<Vec<u32>>,
    /// Binary: 0 no / 1 yes. Choice: candidate index. Ordinal: level.
    pub y: usize,
}

pub fn predict(kind: Kind, head: &Head, e: &Example) -> usize {
    match kind {
        Kind::Binary => usize::from(head.score(0, &e.x[0]) >= 0),
        Kind::Choice => {
            let mut best = (i64::MIN, 0);
            for (k, x) in e.x.iter().enumerate() {
                let s = head.score(0, x);
                if s > best.0 {
                    best = (s, k);
                }
            }
            best.1
        }
        Kind::Ordinal(_) => head.argmax(&e.x[0]),
    }
}

fn rows(kind: Kind) -> usize {
    match kind {
        Kind::Binary | Kind::Choice => 1,
        Kind::Ordinal(l) => l,
    }
}

fn step(kind: Kind, p: &mut Perceptron, e: &Example, bias_only: bool) {
    let weights = !bias_only;
    match kind {
        Kind::Binary => {
            let yes = p.score(0, &e.x[0]) >= 0;
            if yes != (e.y == 1) {
                let sign = if e.y == 1 { 1 } else { -1 };
                if weights {
                    p.add(0, &e.x[0], sign, true);
                } else {
                    p.add(0, &[], sign, true);
                }
            }
        }
        Kind::Choice => {
            if bias_only {
                return;
            }
            let mut best = (i64::MIN, 0);
            for (k, x) in e.x.iter().enumerate() {
                let s = p.score(0, x);
                if s > best.0 {
                    best = (s, k);
                }
            }
            if best.1 != e.y {
                let (gold, pred) = (e.x[e.y].clone(), e.x[best.1].clone());
                p.add(0, &gold, 1, false);
                p.add(0, &pred, -1, false);
            }
        }
        Kind::Ordinal(levels) => {
            let mut best = (i64::MIN, 0);
            for c in 0..levels {
                let s = p.score(c, &e.x[0]);
                if s > best.0 {
                    best = (s, c);
                }
            }
            if best.1 != e.y {
                let x: &[u32] = if weights { &e.x[0] } else { &[] };
                p.add(e.y, x, 1, true);
                p.add(best.1, x, -1, true);
            }
        }
    }
    p.tick();
}

/// Trains `max(epochs)` epochs and returns the averaged head after each
/// requested epoch count (ascending).
pub fn train_averaged(kind: Kind, train: &[Example], epochs: &[usize]) -> Vec<(usize, Head)> {
    let mut p = Perceptron::new(rows(kind));
    let mut out = Vec::new();
    let last = epochs.iter().copied().max().unwrap_or(0);
    for epoch in 1..=last {
        for i in epoch_order(train.len(), epoch) {
            step(kind, &mut p, &train[i], false);
        }
        if epochs.contains(&epoch) {
            let (w, b) = p.avg();
            out.push((
                epoch,
                Head {
                    classes: p.classes,
                    w,
                    b,
                },
            ));
        }
    }
    out
}

/// Quantizes averaged weights to signed `levels` (1 = ternary {-1,0,+1},
/// 3 = 2 magnitude bit planes, 7 = 3 planes). Weights below the magnitude of
/// the k-th largest non-zero weight, k = nonzero / keep_div, become 0; a kept
/// weight v becomes sign(v)·max(1, round(levels·|v| / max|v|)). Integer
/// biases are then refit on TRAIN with `BIAS_PASSES` bias-only passes.
pub fn quantize(kind: Kind, avg: &Head, levels: i64, keep_div: usize, train: &[Example]) -> Head {
    let mut mags: Vec<i64> = avg.w.iter().filter(|&&v| v != 0).map(|v| v.abs()).collect();
    mags.sort_unstable_by(|a, b| b.cmp(a));
    let k = (mags.len() / keep_div.max(1)).max(1);
    let cut = mags.get(k - 1).copied().unwrap_or(1).max(1);
    let top = mags.first().copied().unwrap_or(1).max(1);
    let w: Vec<i64> = avg
        .w
        .iter()
        .map(|&v| {
            if v.abs() < cut {
                return 0;
            }
            // round(levels·|v| / top), half up, at least 1.
            let q = ((2 * levels * v.abs() + top) / (2 * top)).clamp(1, levels);
            q * v.signum()
        })
        .collect();
    let mut p = Perceptron::new(avg.classes);
    p.w.clone_from(&w);
    for pass in 1..=BIAS_PASSES {
        for i in epoch_order(train.len(), 1000 + pass) {
            step(kind, &mut p, &train[i], true);
        }
    }
    Head {
        classes: avg.classes,
        w,
        b: p.b,
    }
}

/// Number of magnitude bit planes a quantized head needs.
pub fn planes(levels: i64) -> u32 {
    64 - (levels as u64).leading_zeros()
}

pub fn accuracy(kind: Kind, head: &Head, set: &[Example]) -> (usize, usize) {
    let correct = set.iter().filter(|e| predict(kind, head, e) == e.y).count();
    let err = match kind {
        Kind::Ordinal(_) => set
            .iter()
            .map(|e| predict(kind, head, e).abs_diff(e.y))
            .sum(),
        _ => 0,
    };
    (correct, err)
}

/// One DEV-selected configuration.
#[derive(Clone, Debug)]
pub struct Selected {
    pub epochs: usize,
    /// 0 for the averaged integer arm.
    pub levels: i64,
    pub keep_div: usize,
    pub dev_correct: usize,
    pub head: Head,
}

/// One grid row: epochs, levels (0 = integer arm), keep_div, DEV correct, non-zero weights.
pub type GridRow = (usize, i64, usize, usize, usize);

/// Grid search on DEV: returns (quantized pick, integer pick, all grid rows).
/// Quantized ties prefer fewer bit planes, then fewer non-zero weights, then
/// fewer epochs.
pub fn select_on_dev(
    kind: Kind,
    train: &[Example],
    dev: &[Example],
) -> (Selected, Selected, Vec<GridRow>) {
    let mut grid = Vec::new();
    let mut best_q: Option<Selected> = None;
    let mut best_i: Option<Selected> = None;
    for (epochs, avg) in train_averaged(kind, train, &EPOCH_GRID) {
        let (c, _) = accuracy(kind, &avg, dev);
        grid.push((epochs, 0, 0, c, avg.nonzero()));
        if best_i.as_ref().is_none_or(|b| c > b.dev_correct) {
            best_i = Some(Selected {
                epochs,
                levels: 0,
                keep_div: 0,
                dev_correct: c,
                head: avg.clone(),
            });
        }
        for &levels in &LEVEL_GRID {
            for &div in &KEEP_DIV {
                let q = quantize(kind, &avg, levels, div, train);
                let (c, _) = accuracy(kind, &q, dev);
                let nz = q.nonzero();
                grid.push((epochs, levels, div, c, nz));
                let better = best_q.as_ref().is_none_or(|b| {
                    (
                        c,
                        core::cmp::Reverse(planes(levels)),
                        core::cmp::Reverse(nz),
                    ) > (
                        b.dev_correct,
                        core::cmp::Reverse(planes(b.levels)),
                        core::cmp::Reverse(b.head.nonzero()),
                    )
                });
                if better {
                    best_q = Some(Selected {
                        epochs,
                        levels,
                        keep_div: div,
                        dev_correct: c,
                        head: q,
                    });
                }
            }
        }
    }
    (
        best_q.expect("grid is not empty"),
        best_i.expect("grid is not empty"),
        grid,
    )
}
