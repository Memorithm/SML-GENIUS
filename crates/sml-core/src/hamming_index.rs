//! HLE-2 sub-linear routing index: exact Hamming top-k over keys of a
//! validated width (1..=64 bits) using multi-index hashing (MIH).
//!
//! The key is split into `m` disjoint substrings; each substring has an
//! exact-match bucket table. A query probes, for substring radius
//! `s = 0, 1, 2, ...`, every bucket whose substring is within Hamming
//! distance `s` of the query substring. By pigeonhole, once radius `s` has
//! been probed in every table, every key at full distance `<= m*(s+1)-1` has
//! been examined. The search stops as soon as `k` examined keys lie within
//! that guaranteed bound, so the result is **exactly** the full-scan top-k
//! ordered by `(distance, PageId)`, including ties. When probing would cost
//! more than a full scan, the index falls back to a full scan (still exact)
//! and reports it in [`IndexStats`].
//!
//! No approximate mode is provided; recall is 1 by construction and is
//! checked against [`full_scan_top_k`] in tests.

use crate::PageId;
use std::collections::HashMap;

/// Errors raised by the routing index (all fail closed).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IndexError {
    /// Width must be in `1..=64`.
    BadWidth(u32),
    /// A key uses bits at or above the declared width.
    KeyOutOfRange(PageId),
    /// The query uses bits at or above the declared width.
    QueryOutOfRange,
    /// Two entries share a `PageId`.
    DuplicatePage(PageId),
    /// `k` must be at least 1.
    ZeroK,
}

/// Per-query work counters.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct IndexStats {
    /// Distinct keys whose full distance was computed.
    pub candidates: u64,
    /// Buckets probed (including empty ones).
    pub probes: u64,
    /// Queries answered by the exact full-scan fallback.
    pub fallbacks: u64,
}

fn width_mask(width: u32) -> u64 {
    if width == 64 {
        u64::MAX
    } else {
        (1u64 << width) - 1
    }
}

fn validate(width: u32, entries: &[(PageId, u64)]) -> Result<(), IndexError> {
    if width == 0 || width > 64 {
        return Err(IndexError::BadWidth(width));
    }
    let mask = width_mask(width);
    let mut ids: Vec<u32> = Vec::with_capacity(entries.len());
    for &(id, key) in entries {
        if key & !mask != 0 {
            return Err(IndexError::KeyOutOfRange(id));
        }
        ids.push(id.0);
    }
    ids.sort_unstable();
    if let Some(w) = ids.windows(2).find(|w| w[0] == w[1]) {
        return Err(IndexError::DuplicatePage(PageId(w[0])));
    }
    Ok(())
}

/// Reference: exact top-k by `(distance, PageId)` with a full scan.
pub fn full_scan_top_k(
    width: u32,
    entries: &[(PageId, u64)],
    query: u64,
    k: usize,
) -> Result<Vec<(u32, PageId)>, IndexError> {
    validate(width, entries)?;
    if k == 0 {
        return Err(IndexError::ZeroK);
    }
    if query & !width_mask(width) != 0 {
        return Err(IndexError::QueryOutOfRange);
    }
    let mut all: Vec<(u32, u32)> = entries
        .iter()
        .map(|&(id, key)| ((key ^ query).count_ones(), id.0))
        .collect();
    all.sort_unstable();
    all.truncate(k);
    Ok(to_pages(all))
}

/// Multi-index-hashing exact Hamming top-k index.
#[derive(Debug, Clone)]
pub struct MultiIndexHamming {
    width: u32,
    /// `(shift, bits)` per substring.
    chunks: Vec<(u32, u32)>,
    tables: Vec<HashMap<u64, Vec<u32>>>,
    ids: Vec<PageId>,
    keys: Vec<u64>,
}

impl MultiIndexHamming {
    /// Build with an automatic substring count: `m = ceil(width / log2(n))`,
    /// clamped to `1..=width`.
    pub fn new(width: u32, entries: Vec<(PageId, u64)>) -> Result<Self, IndexError> {
        let n = entries.len().max(2) as f64;
        let b = n.log2().ceil().max(1.0) as u32;
        let m = width.div_ceil(b.min(width.max(1))).clamp(1, width.max(1));
        Self::with_substrings(width, entries, m)
    }

    /// Build with an explicit substring count `m` (clamped to `1..=width`).
    pub fn with_substrings(
        width: u32,
        entries: Vec<(PageId, u64)>,
        m: u32,
    ) -> Result<Self, IndexError> {
        validate(width, &entries)?;
        let m = m.clamp(1, width);
        let mut chunks = Vec::with_capacity(m as usize);
        let mut shift = 0;
        for i in 0..m {
            let bits = width / m + u32::from(i < width % m);
            chunks.push((shift, bits));
            shift += bits;
        }
        let mut tables = vec![HashMap::new(); m as usize];
        let mut ids = Vec::with_capacity(entries.len());
        let mut keys = Vec::with_capacity(entries.len());
        for (idx, &(id, key)) in entries.iter().enumerate() {
            ids.push(id);
            keys.push(key);
            for (t, &(s, b)) in chunks.iter().enumerate() {
                tables[t]
                    .entry((key >> s) & width_mask(b))
                    .or_insert_with(Vec::new)
                    .push(idx as u32);
            }
        }
        Ok(Self {
            width,
            chunks,
            tables,
            ids,
            keys,
        })
    }

    /// Number of indexed keys.
    pub fn len(&self) -> usize {
        self.keys.len()
    }

    /// True when no key is indexed.
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    /// Number of substrings.
    pub fn substrings(&self) -> usize {
        self.chunks.len()
    }

    /// Exact top-k `(distance, PageId)`, ordered by `(distance, PageId)`.
    pub fn top_k(
        &self,
        query: u64,
        k: usize,
        stats: &mut IndexStats,
    ) -> Result<Vec<(u32, PageId)>, IndexError> {
        if k == 0 {
            return Err(IndexError::ZeroK);
        }
        if query & !width_mask(self.width) != 0 {
            return Err(IndexError::QueryOutOfRange);
        }
        let n = self.keys.len();
        let k = k.min(n);
        if k == 0 {
            return Ok(Vec::new());
        }
        let m = self.chunks.len() as u32;
        let max_bits = self.chunks.iter().map(|c| c.1).max().unwrap_or(0);
        let mut seen: Vec<u64> = Vec::new();
        let mut found: Vec<(u32, u32)> = Vec::new();
        let mut probes_so_far = 0u64;
        for s in 0..=max_bits {
            // Cost of probing radius s in every table.
            let cost: u64 = self.chunks.iter().map(|&(_, b)| binom(b, s)).sum();
            if (probes_so_far + cost).saturating_mul(PROBE_WEIGHT) > n as u64 {
                stats.fallbacks += 1;
                stats.candidates += n as u64;
                let mut all: Vec<(u32, u32)> = self
                    .keys
                    .iter()
                    .zip(&self.ids)
                    .map(|(&key, &id)| ((key ^ query).count_ones(), id.0))
                    .collect();
                all.select_nth_unstable(k - 1);
                all.truncate(k);
                all.sort_unstable();
                return Ok(to_pages(all));
            }
            probes_so_far += cost;
            if seen.is_empty() {
                seen = vec![0u64; n.div_ceil(64)];
            }
            for (t, &(shift, bits)) in self.chunks.iter().enumerate() {
                if s > bits {
                    continue;
                }
                let qs = (query >> shift) & width_mask(bits);
                for_each_flip(bits, s, &mut |flip| {
                    stats.probes += 1;
                    if let Some(bucket) = self.tables[t].get(&(qs ^ flip)) {
                        for &i in bucket {
                            let i = i as usize;
                            let (w, b) = (i / 64, 1u64 << (i % 64));
                            if seen[w] & b == 0 {
                                seen[w] |= b;
                                stats.candidates += 1;
                                found.push(((self.keys[i] ^ query).count_ones(), self.ids[i].0));
                            }
                        }
                    }
                });
            }
            let bound = m * (s + 1) - 1;
            let within = found.iter().filter(|c| c.0 <= bound).count();
            if within >= k || s == max_bits {
                found.sort_unstable();
                found.truncate(k);
                return Ok(to_pages(found));
            }
        }
        found.sort_unstable();
        found.truncate(k);
        Ok(to_pages(found))
    }
}

/// Relative cost of one bucket probe versus one full-scan key comparison,
/// used only to decide when the exact full-scan fallback is cheaper.
const PROBE_WEIGHT: u64 = 8;

fn to_pages(v: Vec<(u32, u32)>) -> Vec<(u32, PageId)> {
    v.into_iter().map(|(d, id)| (d, PageId(id))).collect()
}

fn binom(n: u32, r: u32) -> u64 {
    if r > n {
        return 0;
    }
    let r = r.min(n - r) as u64;
    let mut acc = 1u64;
    for i in 0..r {
        acc = acc.saturating_mul(n as u64 - i) / (i + 1);
    }
    acc
}

/// Call `f` with every `bits`-wide mask having exactly `r` set bits.
fn for_each_flip(bits: u32, r: u32, f: &mut dyn FnMut(u64)) {
    if r == 0 {
        f(0);
        return;
    }
    if r > bits {
        return;
    }
    // Gosper's hack over `bits` positions.
    let mut v: u64 = width_mask(r);
    let limit = bits;
    loop {
        f(v);
        let c = v & v.wrapping_neg();
        let rr = v.wrapping_add(c);
        if rr == 0 {
            return;
        }
        v = (((rr ^ v) >> 2) / c) | rr;
        if limit < 64 && v >> limit != 0 {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rng(s: &mut u64) -> u64 {
        *s = s.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = *s;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn check(width: u32, entries: Vec<(PageId, u64)>, queries: &[u64], ks: &[usize]) {
        for m in [1, 2, 3, 4, 8] {
            let idx = MultiIndexHamming::with_substrings(width, entries.clone(), m).unwrap();
            for &q in queries {
                for &k in ks {
                    let mut st = IndexStats::default();
                    let got = idx.top_k(q, k, &mut st).unwrap();
                    let want = full_scan_top_k(width, &entries, q, k).unwrap();
                    assert_eq!(got, want, "w={width} m={m} q={q:x} k={k}");
                }
            }
        }
        let auto = MultiIndexHamming::new(width, entries.clone()).unwrap();
        for &q in queries {
            let mut st = IndexStats::default();
            assert_eq!(
                auto.top_k(q, 3, &mut st).unwrap(),
                full_scan_top_k(width, &entries, q, 3).unwrap()
            );
        }
    }

    #[test]
    fn flips_enumerate_binomial() {
        for bits in [1u32, 5, 16, 64] {
            for r in 0..=3u32.min(bits) {
                let mut n = 0u64;
                for_each_flip(bits, r, &mut |v| {
                    assert_eq!(v.count_ones(), r);
                    assert!(bits == 64 || v >> bits == 0);
                    n += 1;
                });
                assert_eq!(n, binom(bits, r), "bits={bits} r={r}");
            }
        }
    }

    #[test]
    fn exact_on_random_keys() {
        let mut s = 7;
        for width in [1u32, 7, 16, 33, 64] {
            let mask = width_mask(width);
            let entries: Vec<_> = (0..300)
                .map(|i| (PageId(1000 - i), rng(&mut s) & mask))
                .collect();
            let queries: Vec<u64> = (0..20).map(|_| rng(&mut s) & mask).collect();
            check(width, entries, &queries, &[1, 4, 17, 300, 500]);
        }
    }

    #[test]
    fn exact_on_adversarial_keys() {
        // All-equal keys (maximal ties), keys clustered near the query,
        // keys exactly at substring boundaries and complements.
        let width = 64;
        let mut e: Vec<(PageId, u64)> = (0..64)
            .map(|i| (PageId(200 - i), 0xAAAA_AAAA_AAAA_AAAA))
            .collect();
        e.extend((0..64).map(|i| (PageId(300 + i), 1u64 << i)));
        e.extend((0..64).map(|i| (PageId(400 + i), !(1u64 << i))));
        e.push((PageId(9), u64::MAX));
        e.push((PageId(8), 0));
        let queries = [
            0u64,
            u64::MAX,
            0xAAAA_AAAA_AAAA_AAAA,
            0x5555_5555_5555_5555,
            1,
            0xFFFF,
        ];
        check(width, e, &queries, &[1, 2, 63, 64, 65, 130, 1000]);
    }

    #[test]
    fn fails_closed() {
        assert_eq!(
            MultiIndexHamming::new(0, vec![]).unwrap_err(),
            IndexError::BadWidth(0)
        );
        assert_eq!(
            MultiIndexHamming::new(65, vec![]).unwrap_err(),
            IndexError::BadWidth(65)
        );
        assert_eq!(
            MultiIndexHamming::new(8, vec![(PageId(3), 256)]).unwrap_err(),
            IndexError::KeyOutOfRange(PageId(3))
        );
        assert_eq!(
            MultiIndexHamming::new(8, vec![(PageId(3), 1), (PageId(3), 2)]).unwrap_err(),
            IndexError::DuplicatePage(PageId(3))
        );
        let idx = MultiIndexHamming::new(8, vec![(PageId(1), 1)]).unwrap();
        let mut st = IndexStats::default();
        assert_eq!(
            idx.top_k(256, 1, &mut st).unwrap_err(),
            IndexError::QueryOutOfRange
        );
        assert_eq!(idx.top_k(1, 0, &mut st).unwrap_err(), IndexError::ZeroK);
        let empty = MultiIndexHamming::new(8, vec![]).unwrap();
        assert!(empty.top_k(1, 1, &mut st).unwrap().is_empty());
    }

    /// HLE-2 routing-index bench (timing is host-specific; run with
    /// `cargo test --release -p sml-core --lib hle2_index_bench -- --ignored --nocapture`).
    #[test]
    #[ignore]
    fn hle2_index_bench() {
        use std::time::Instant;
        println!("HLE2_INDEX,dist,width,pages,k,m,mih_ns_per_q,scan_ns_per_q,mih_candidates_per_q,fallback_rate,recall");
        for &(dist, width) in &[("uniform", 64u32), ("clustered", 64), ("uniform", 16)] {
            for &p in &[16usize, 256, 4096, 65536] {
                let mut s = 0xC0FFEE ^ p as u64;
                let mask = width_mask(width);
                let centers: Vec<u64> = (0..32).map(|_| rng(&mut s) & mask).collect();
                let noisy = |s: &mut u64| {
                    let c = centers[(rng(s) % 32) as usize];
                    let mut flip = 0u64;
                    for _ in 0..3 {
                        flip |= 1u64 << (rng(s) % width as u64);
                    }
                    (c ^ flip) & mask
                };
                let entries: Vec<(PageId, u64)> = (0..p)
                    .map(|i| {
                        let key = if dist == "clustered" {
                            noisy(&mut s)
                        } else {
                            rng(&mut s) & mask
                        };
                        (PageId(i as u32), key)
                    })
                    .collect();
                let queries: Vec<u64> = (0..2000)
                    .map(|_| {
                        if dist == "clustered" {
                            noisy(&mut s)
                        } else {
                            rng(&mut s) & mask
                        }
                    })
                    .collect();
                let k = 4;
                let idx = MultiIndexHamming::new(width, entries.clone()).unwrap();
                let mut st = IndexStats::default();
                let t = Instant::now();
                let got: Vec<_> = queries
                    .iter()
                    .map(|&q| idx.top_k(q, k, &mut st).unwrap())
                    .collect();
                let mih = t.elapsed().as_nanos() as f64 / queries.len() as f64;
                let t = Instant::now();
                let want: Vec<_> = queries
                    .iter()
                    .map(|&q| {
                        let mut all: Vec<(u32, u32)> = entries
                            .iter()
                            .map(|&(id, key)| ((key ^ q).count_ones(), id.0))
                            .collect();
                        let kk = k.min(all.len());
                        all.select_nth_unstable(kk - 1);
                        all.truncate(kk);
                        all.sort_unstable();
                        to_pages(all)
                    })
                    .collect();
                let scan = t.elapsed().as_nanos() as f64 / queries.len() as f64;
                let hits: usize = got
                    .iter()
                    .zip(&want)
                    .map(|(g, w)| g.iter().filter(|x| w.contains(x)).count())
                    .sum();
                let recall = hits as f64 / want.iter().map(Vec::len).sum::<usize>() as f64;
                println!(
                    "HLE2_INDEX,{dist},{width},{p},{k},{},{mih:.0},{scan:.0},{:.1},{:.3},{recall:.4}",
                    idx.substrings(),
                    st.candidates as f64 / queries.len() as f64,
                    st.fallbacks as f64 / queries.len() as f64
                );
                assert_eq!(got, want);
            }
        }
    }
}
