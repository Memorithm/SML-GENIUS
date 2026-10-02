//! Algebraic controls only. No new empirical scaling/promotion grid is claimed.
use std::collections::BTreeSet;
use sml_core::delta_kv::{DeltaKvCounters, DeltaKvMemory};

fn dot(a: &[i16], b: &[i16]) -> i64 {
    assert_eq!(a.len(),b.len());
    a.iter().zip(b).map(|(&x,&y)| i64::from(x)*i64::from(y)).sum()
}
fn constant_magnitude_key(dim: usize, seed: u64) -> Vec<i16> {
    assert!(dim >= 8 && dim % 8 == 0);
    (0..dim).map(|i| if ((i as u64 * 13 + seed) % 17) & 1 == 0 {1024} else {-1024}).collect()
}
fn flip_mask(key: &[i16], mask: u8) -> Vec<i16> {
    let block = key.len()/8;
    key.iter().enumerate().map(|(i,&v)| if mask & (1 << (i/block)) != 0 {-v} else {v}).collect()
}
fn distinct_masks(flips: u32) -> Vec<u8> {
    // Avoid complementary duplicates too: the legacy f=4 probe silently had
    // only two antipodal keys. Report this change, do not overwrite its results.
    let mut masks = Vec::new();
    for mask in 0..=255_u16 {
        let mask = mask as u8;
        if mask.count_ones() == flips && !masks.contains(&!mask) { masks.push(mask); }
        if masks.len() == 5 { break; }
    }
    assert_eq!(masks.len(),5);
    masks
}
#[test]
fn corrected_controls_have_five_distinct_keys_and_exact_dimension_invariant_cosine() {
    for dim in [8,16,32,64,128] {
        for seed in 0..3 {
            let target = constant_magnitude_key(dim,seed);
            let norm = dot(&target,&target);
            for flips in 1..=4 {
                let keys: Vec<_> = distinct_masks(flips).iter().map(|&mask| flip_mask(&target,mask)).collect();
                assert_eq!(keys.iter().collect::<BTreeSet<_>>().len(),5);
                for key in &keys {
                    assert_eq!(dot(key,key),norm);
                    // Exact rational cosine: dot/norm = 1 - flips/4.
                    assert_eq!(4*dot(&target,key),(4-i64::from(flips))*norm);
                }
            }
        }
    }
}
#[test]
fn preserve_legacy_four_block_degeneracy_as_a_regression_counterexample() {
    let masks: BTreeSet<_> = (0..5).map(|i| (0..4).fold(0_u8,|mask,j|mask | (1<<((i+2*j)%8)))).collect();
    assert_eq!(masks.len(),2);
    assert!(masks.contains(&0b01010101));
    assert!(masks.contains(&0b10101010));
}
#[test]
fn weighted_coordinate_fraction_is_not_an_exact_cosine() {
    let key = [1_i16,2,3,4,5,6,7,8];
    let mut changed = key; changed[0] = -changed[0];
    let norm = dot(&key,&key);
    assert_eq!(dot(&key,&changed),norm-2);
    assert_ne!(4*dot(&key,&changed),3*norm);
}
#[test]
fn unseen_orthogonal_control_has_exact_zero_dot_and_zero_read() {
    let key = vec![1024_i16;16];
    let unknown: Vec<i16> = (0..16).map(|i|if i%2==0 {1024} else {-1024}).collect();
    assert_ne!(key,unknown);
    assert_eq!(dot(&key,&unknown),0);
    let mut memory = DeltaKvMemory::new(16,2).unwrap();
    let mut counters = DeltaKvCounters::default();
    memory.update(&key,&[4096,-4096],4096,&mut counters).unwrap();
    assert_eq!(memory.read(&unknown,&mut counters).unwrap(),[0,0]);
}
#[test]
fn q12_sign_success_and_numeric_error_are_different_metrics() {
    let mut memory = DeltaKvMemory::new(3,1).unwrap();
    let mut counters = DeltaKvCounters::default();
    memory.update(&[4096,4096,4096],&[4096],4096,&mut counters).unwrap();
    let value = memory.read(&[4096,4096,4096],&mut counters).unwrap()[0];
    assert_eq!(value,4095);
    assert!(value>0);
    assert_eq!(counters.saturations,0);
}
