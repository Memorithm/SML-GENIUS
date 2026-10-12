//! A6-I6 controlled epistemic fourth-order watchlist challenge.
//!
//! A public, PREDECLAREd tuple is supplied to a native input-support guard.
//! No fourth-order teacher coefficients or OOD labels are ever shown to the
//! guard/committee. Positives and mandatory unmonitored negatives share
//! identically sized 512-step passive TRAIN and 512-step held-out TEST.
//! This is evidence about a local watchlist, never a coverage guarantee.

use std::{error::Error, time::Instant};

use sml_core::{
    anf_budgeted_four_support::AnfBudgetedFourSupportGuard,
    anf_passive_committee::PassiveCommittee,
    anf_wide_sparse::{WideAnfGate, WideAnfLimits},
    toy_tasks::SplitMix64,
};

const WIDTHS: [usize; 7] = [8, 16, 32, 64, 128, 256, 1024];
const TRAIN_STEPS: usize = 512;
const TEST_STEPS: usize = 512;
const SEEDS: usize = 3;
const ROLE_SALT: u64 = 0xA6C1_6001_51ED_0001;
const TRAIN_SALT: u64 = 0xA6C1_6002_51ED_0002;
const NOISE_SALT: u64 = 0xA6C1_6003_51ED_0003;
const TEST_SALT: u64 = 0xA6C1_6004_51ED_0004;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Regime {
    WatchedAbsent,
    WatchedIncluded,
    UnwatchedAbsent,
    FifthUnwatchedAbsent,
}

impl Regime {
    const ALL: [Self; 4] = [
        Self::WatchedAbsent,
        Self::WatchedIncluded,
        Self::UnwatchedAbsent,
        Self::FifthUnwatchedAbsent,
    ];
    fn name(self) -> &'static str {
        match self {
            Self::WatchedAbsent => "WATCHED_ABSENT",
            Self::WatchedIncluded => "WATCHED_INCLUDED",
            Self::UnwatchedAbsent => "UNWATCHED_ABSENT",
            Self::FifthUnwatchedAbsent => "FIFTH_UNWATCHED_ABSENT",
        }
    }
    fn family(self) -> usize {
        match self {
            Self::WatchedAbsent | Self::WatchedIncluded => 0,
            Self::UnwatchedAbsent => 1,
            Self::FifthUnwatchedAbsent => 2,
        }
    }
    fn hidden_roles(self) -> &'static [usize] {
        match self {
            Self::WatchedAbsent | Self::WatchedIncluded => &[4, 5, 6, 7],
            Self::UnwatchedAbsent => &[0, 1, 2, 3],
            Self::FifthUnwatchedAbsent => &[0, 1, 2, 3, 4],
        }
    }
    fn bank_contains_truth(self) -> bool {
        self == Self::WatchedIncluded
    }
}

fn seed(width: usize, index: usize, family: usize, salt: u64) -> u64 {
    (index as u64 + 1).wrapping_mul(0xd134_2543_de82_ef95)
        ^ (family as u64 + 1).wrapping_mul(0xbf58_476d_1ce4_e5b9)
        ^ width as u64
        ^ salt
}

/// PUBLIC physical risk tuple is predetermined by the role permutation and
/// handed to the guard regardless of whether the true teacher needs it.
fn physical_roles(width: usize, index: usize) -> [usize; 8] {
    let mut rng = SplitMix64::new(seed(width, index, 0, ROLE_SALT));
    let mut positions = (0..width).collect::<Vec<_>>();
    for i in (1..width).rev() {
        let j = rng.below(i + 1);
        positions.swap(i, j);
    }
    let mut result = [0; 8];
    result.copy_from_slice(&positions[..8]);
    result
}
fn limits() -> WideAnfLimits {
    WideAnfLimits::new(2048, 256, 8192, 8192, 8192)
}
fn from_terms(width: usize, terms: Vec<Vec<usize>>) -> WideAnfGate {
    WideAnfGate::from_monomials((0..width).collect(), terms, width, limits()).unwrap()
}
fn candidates(width: usize, roles: &[usize;8]) -> Vec<WideAnfGate> {
    let base = vec![vec![roles[0]], vec![roles[1]]];
    [
        base.clone(),
        [base.clone(), vec![vec![roles[0], roles[1]]]].concat(),
        [base.clone(), vec![vec![roles[2]]]].concat(),
        [base, vec![vec![roles[3]]]].concat(),
    ]
    .into_iter()
    .map(|terms| from_terms(width, terms))
    .collect()
}
fn teacher(width: usize, roles: &[usize;8], regime: Regime) -> WideAnfGate {
    let mut terms = vec![vec![roles[0]], vec![roles[1]]];
    if !regime.bank_contains_truth() {
        terms.push(regime.hidden_roles().iter().map(|&role| roles[role]).collect());
    }
    from_terms(width, terms)
}
fn hidden_on(bits: u16, regime: Regime) -> bool {
    regime.hidden_roles().iter().all(|&role| bits & (1<<role) != 0)
}
fn allowed_train_patterns(regime: Regime) -> Vec<u16> {
    (0..256)
        .filter(|&m| !hidden_on(m, regime))
        .collect()
}
fn train_input(
    width: usize,
    roles: &[usize;8],
    role_pattern: u16,
    rng: &mut SplitMix64,
) -> Vec<bool> {
    let mut input = (0..width).map(|_| rng.next_u64() & 1 != 0).collect::<Vec<_>>();
    for (role, &physical) in roles.iter().enumerate() {
        input[physical] = role_pattern & (1 << role) != 0;
    }
    input
}
fn ood_input(
    width: usize,
    roles: &[usize;8],
    regime: Regime,
    t: usize,
    rng: &mut SplitMix64,
) -> Vec<bool> {
    let mut input = (0..width).map(|_| rng.next_u64() & 1 != 0).collect::<Vec<_>>();
    let selected = regime.hidden_roles();
    if t % 2 == 1 {
        for &role in selected {
            input[roles[role]] = true;
        }
    } else {
        input[roles[selected[0]]] = false;
    }
    input
}

#[derive(Debug)]
struct Frozen {
    width: usize,
    index: usize,
    regime: Regime,
    noisy: bool,
    roles: [usize;8],
    committee: PassiveCommittee,
    watch: AnfBudgetedFourSupportGuard,
    true_gate: WideAnfGate,
    train_flips: usize,
    seal_ms: f64,
}

fn fit(
    width: usize,
    index: usize,
    regime: Regime,
    noisy: bool,
) -> Result<Frozen, Box<dyn Error>> {
    let roles = physical_roles(width, index);
    let public_watch = [roles[4],roles[5],roles[6],roles[7]];
    let true_gate=teacher(width,&roles,regime);
    let reference = candidates(width, &roles);
    let mut committee=PassiveCommittee::new(reference, vec![4.0,2.0,2.0,1.0], 1.0/16.0, TRAIN_STEPS)?;
    let mut watch=AnfBudgetedFourSupportGuard::new(width,vec![public_watch],1,TRAIN_STEPS)?;
    let allowed=allowed_train_patterns(regime);
    if allowed.len()!= if regime==Regime::FifthUnwatchedAbsent { 248 } else { 240 } {
        return Err("unexpected censored TRAIN fixture size".into());
    }
    let mut rng=SplitMix64::new(seed(width,index,regime.family(),TRAIN_SALT));
    let mut noise_rng=SplitMix64::new(seed(width,index,regime.family(),NOISE_SALT));
    let mut flips=0usize;
    for t in 0..TRAIN_STEPS {
        let q=train_input(width,&roles,allowed[t % allowed.len()],&mut rng);
        let true_label=true_gate.evaluate_scalar(&q)?;
        let h0_label=q[roles[0]] ^ q[roles[1]];
        if true_label!=h0_label {
            return Err("teacher distinguishable during preregistered censored TRAIN".into());
        }
        let flip=noisy && noise_rng.next_u64() & 15 == 0;
        flips+=usize::from(flip);
        committee.observe(&q,true_label ^ flip)?;
        watch.observe(&q)?;
    }
    let start=Instant::now();
    watch.seal()?;
    let seal_ms=start.elapsed().as_secs_f64()*1000.0;
    let expected_patterns=if regime.family()==0 {15} else {16};
    if watch.observed_patterns(0)!=Some(expected_patterns) {
        return Err(format!("wrong public watch coverage: expected {expected_patterns} observed {:?}",watch.observed_patterns(0)).into());
    }
    if !committee.bank_adequate(0.125)? {
        return Err("unexpected bank mismatch alarm on indistinguishable censored TRAIN".into());
    }
    Ok(Frozen{
        width,index,regime,noisy,roles,committee,watch,true_gate,train_flips:flips,seal_ms
    })
}

#[derive(Clone,Copy,Debug,Default)]
struct Counts {
    baseline_accepted:usize,
    baseline_errors:usize,
    guarded_accepted:usize,
    guarded_errors:usize,
    novel_inputs:usize,
    unnecessary_abstentions:usize,
    bank_inadequate:usize,
}
impl Counts {
    fn add(&mut self, other: Self) {
        self.baseline_accepted+=other.baseline_accepted;
        self.baseline_errors+=other.baseline_errors;
        self.guarded_accepted+=other.guarded_accepted;
        self.guarded_errors+=other.guarded_errors;
        self.novel_inputs+=other.novel_inputs;
        self.unnecessary_abstentions+=other.unnecessary_abstentions;
        self.bank_inadequate+=other.bank_inadequate;
    }
}

fn heldout(frozen:&Frozen)->Result<Counts,Box<dyn Error>>{
    let mut rng=SplitMix64::new(seed(frozen.width,frozen.index,frozen.regime.family(),TEST_SALT));
    let mut count=Counts::default();
    for t in 0..TEST_STEPS {
        let q=ood_input(frozen.width,&frozen.roles,frozen.regime,t,&mut rng);
        let real=frozen.true_gate.evaluate_scalar(&q)?;
        let check=frozen.watch.predict_guarded(&frozen.committee,&q,0.90,0.125)?;
        count.novel_inputs+=usize::from(!check.support.watched_supported);
        count.bank_inadequate+=usize::from(!check.bank_adequate);
        if let Some(baseline)=check.committee.selective_prediction {
            count.baseline_accepted+=1;
            count.baseline_errors+=usize::from(baseline!=real);
        }
        if let Some(guarded)=check.guarded_prediction {
            count.guarded_accepted+=1;
            count.guarded_errors+=usize::from(guarded!=real);
        } else if check.committee.selective_prediction==Some(real) {
            count.unnecessary_abstentions+=1;
        }
    }
    Ok(count)
}

fn main()->Result<(),Box<dyn Error>>{
    println!("A6_I6_CONFIG,widths=8;16;32;64;128;256;1024,seeds=3,panels=clean;noise_1_in_16,regimes=WATCHED_ABSENT;WATCHED_INCLUDED;UNWATCHED_ABSENT;FIFTH_UNWATCHED_ABSENT,watch_order=4,max_watch=1,train=512,test=512,assumed_noise=1_in_16,prior=4;2;2;1,confidence=0.90,mismatch_threshold=0.125");
    let mut frozen=Vec::new();
    for width in WIDTHS {
        for index in 0..SEEDS {
            for regime in Regime::ALL {
                for noisy in [false,true] {
                    let model=fit(width,index,regime,noisy)?;
                    let panel=if noisy {"noise_1_in_16"} else {"clean"};
                    println!(
                        "A6_I6_FROZEN,width={width},seed={index},regime={},panel={panel},train=512,committee_classes={},bank_adequate={},watch_patterns={},watch_payload_bytes={},watch_count={},train_label_flips={},seal_ms={:.6}",
                        regime.name(),
                        model.committee.observed_equivalence_classes(),
                        model.committee.bank_adequate(0.125)?,
                        model.watch.observed_patterns(0).unwrap_or(0),
                        model.watch.logical_payload_bytes(),
                        model.watch.watched_count(),
                        model.train_flips,model.seal_ms,
                    );
                    frozen.push(model);
                }
            }
        }
    }
    if frozen.len()!=168 {return Err("frozen policy count must be 168".into());}
    println!("A6_I6_ALL_FROZEN,policies=168,distinct_tasks=84,watched_tuples_per_policy=1");
    // Generate all OOD TEST only after all committee histories and watch
    // assignments have frozen. No label from TEST can affect prediction.
    let mut summary=[Counts::default();4];
    for f in &frozen{
        let count=heldout(f)?;
        let panel=if f.noisy {"noise_1_in_16"} else {"clean"};
        let idx=f.regime.index();
        summary[idx].add(count);
        println!(
            "A6_I6_TEST,width={},seed={},regime={},panel={panel},samples=512,committee_accepted={},committee_errors={},watch_accepted={},watch_errors={},novel_inputs={},unnecessary_abstentions={},bank_inadequate={},watch_payload_bytes={}",
            f.width,f.index,f.regime.name(),
            count.baseline_accepted,count.baseline_errors,count.guarded_accepted,
            count.guarded_errors,count.novel_inputs,count.unnecessary_abstentions,
            count.bank_inadequate,f.watch.logical_payload_bytes(),
        );
    }
    for (i,regime) in Regime::ALL.into_iter().enumerate(){
        let c=summary[i];
        println!(
            "A6_I6_SUMMARY,regime={},fits=42,samples=21504,committee_accepted={},committee_errors={},watch_accepted={},watch_errors={},novel_inputs={},unnecessary_abstentions={},bank_inadequate={}",
            regime.name(),c.baseline_accepted,c.baseline_errors,
            c.guarded_accepted,c.guarded_errors,c.novel_inputs,
            c.unnecessary_abstentions,c.bank_inadequate,
        );
    }
    let watched_absent=summary[0];
    let watched_included=summary[1];
    let unwatched_absent=summary[2];
    let fifth_absent=summary[3];
    let expected=|c:Counts,old_errors:usize,accept:usize,guard_errors:usize,novel:usize|{
        c.baseline_accepted==21504 && c.baseline_errors==old_errors
            && c.guarded_accepted==accept && c.guarded_errors==guard_errors
            && c.novel_inputs==novel && c.bank_inadequate==0
    };
    let passed=expected(watched_absent,10752,10752,0,10752)
        && expected(watched_included,0,10752,0,10752)
        && watched_included.unnecessary_abstentions==10752
        && expected(unwatched_absent,10752,21504,10752,0)
        && expected(fifth_absent,10752,21504,10752,0);
    println!(
        "A6_I6_COMPLETE,policies=168,tests=86016,watched_blocked_errors={},unwatched_confident_errors={},fifth_confident_errors={},unnecessary_abstentions={},all_preregistered_conditions_passed={passed}",
        watched_absent.baseline_errors-watched_absent.guarded_errors,
        unwatched_absent.guarded_errors,fifth_absent.guarded_errors,
        watched_included.unnecessary_abstentions,
    );
    if !passed {return Err("A6-I6 recorded results differ from frozen predictions".into());}
    Ok(())
}

impl Regime {
    fn index(self)->usize {
        match self{
            Self::WatchedAbsent=>0,
            Self::WatchedIncluded=>1,
            Self::UnwatchedAbsent=>2,
            Self::FifthUnwatchedAbsent=>3,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn watched_and_unwatched_training_distinguish_support_without_teacher_labels() {
        for width in [8,16,64,1024] {
            let roles=physical_roles(width,0);
            for mode in Regime::ALL {
                let all=allowed_train_patterns(mode);
                assert_eq!(all.len(),if mode==Regime::FifthUnwatchedAbsent{248}else{240});
                let mut observed=0u16;
                for pattern in all {
                    let watch_pattern=[4,5,6,7].into_iter().fold(0u8,|acc,role|{
                        (acc<<1)|u8::from(pattern & (1<<role)!=0)
                    });
                    observed|=1u16<<watch_pattern;
                }
                assert_eq!(observed.count_ones(),if mode.family()==0 {15}else{16});
                let _ = roles;
            }
        }
    }
    #[test]
    fn fit_then_score_all_adversarial_regimes_at_small_width(){
        for regime in Regime::ALL {
            let f=fit(8,0,regime,false).unwrap();
            let c=heldout(&f).unwrap();
            assert_eq!(c.baseline_accepted,512);
            assert_eq!(c.baseline_errors,if regime.bank_contains_truth(){0}else{256});
            assert_eq!(c.guarded_accepted,if regime.family()==0{256}else{512});
            assert_eq!(c.novel_inputs,if regime.family()==0{256}else{0});
        }
    }
}
