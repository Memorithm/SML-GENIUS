//! RTLR-V5 experimental pipeline: TRAIN-only mine -> disjoint TRAIN fit -> DEV.
//! Paired standalone V3, V4 preset, V5 discovered, and Boolean-only baseline.
//! Synthetic generated features, CPU Rust; NOT trained SML25B/physical MOR.
use sml_core::{
    anf_delay_line::AnfDelayLine,
    rtlr_boolean_hybrid::RtlrBooleanHybrid,
    rtlr_elastic::{RtlrElastic, RtlrElasticConfig, RtlrV2Policy},
    train_lag_discovery::TrainLagDiscovery,
};
use std::{env, error::Error, fs, path::Path, time::Instant};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const BLOCK: usize = 65_536;
const HALO: usize = 1024;
const DISCOVER: usize = 2 * BLOCK;
const FIT: usize = 6 * BLOCK;
const DEV: usize = BLOCK;
const TRAIN_SEED: u64 = 0x91f8_31a2_00c7_6d15;
const DEV_SEED: u64 = 0x1357_9bdf_2468_ace0;
const NULL_OFFSET: u64 = 0xe1aa_1b56_1f43_09b7;

fn mix(mut x: u64) -> u64 {
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}
fn seed_for(base: u64, panel: u64) -> u64 {
    if panel == 0 { base } else { base ^ mix(panel.wrapping_mul(0x9e37_79b9_7f4a_7c15)) }
}
fn data_range(start: usize, end: usize, seed: u64) -> Vec<u8> {
    (start..end).map(|i| {
        mix(seed.wrapping_add(i as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15)) as u8
    }).collect()
}
fn label(source: &[u8], unrelated: &[u8], i: usize, lag: usize) -> bool {
    if lag == 0 {
        unrelated[i] & 1 != 0
    } else {
        ((source[i-lag-1] & source[i-lag]).count_ones() & 1) != 0
    }
}
fn prob(x: f64) -> f64 { 1.0 / (1.0 + (-4.0 * x).exp()) }
fn bpc(p: f64, y: bool) -> f64 {
    let p = p.clamp(1e-12, 1.0 - 1e-12);
    if y { -p.log2() } else { -(1.0-p).log2() }
}
fn prime_v3(m: &mut RtlrElastic, seed: u64, start: usize) -> Result<()> {
    let h = data_range(start - HALO, start, seed);
    for i in 2..HALO { m.advance_inference(h[i-2],h[i-1])?; }
    Ok(())
}
fn prime_v4(m: &mut RtlrBooleanHybrid, seed: u64, start: usize) -> Result<()> {
    let h = data_range(start - HALO, start, seed);
    for i in 2..HALO { m.advance_inference(h[i-2],h[i-1])?; }
    Ok(())
}
fn train_v3(m: &mut RtlrElastic, source: &[u8], unrelated: &[u8], lag: usize) -> Result<(f64,f64)> {
    let mut total = 0.0;
    let mut anchor = 0.0;
    for i in HALO..HALO+BLOCK {
        let y = label(source, unrelated, i, lag);
        let p = prob(m.advance_train(source[i-2],source[i-1])?);
        total += bpc(p,y);
        anchor += bpc(prob(m.anchor_state()),y);
        m.learn_credit(4.0 * (p - f64::from(u8::from(y))))?;
    }
    Ok((total/BLOCK as f64, anchor/BLOCK as f64))
}
fn train_v4(m: &mut RtlrBooleanHybrid, source: &[u8], unrelated: &[u8], lag: usize) -> Result<(f64,f64)> {
    let mut total = 0.0;
    let mut anchor = 0.0;
    for i in HALO..HALO+BLOCK {
        let y = label(source, unrelated, i, lag);
        m.advance_train(source[i-2],source[i-1])?;
        let (p,a) = m.pending_recurrent_and_anchor().ok_or("missing recurrent TRAIN forward")?;
        total += bpc(p,y);
        anchor += bpc(a,y);
        m.learn_target(y)?;
    }
    Ok((total/BLOCK as f64, anchor/BLOCK as f64))
}
fn eval_v3(m: &mut RtlrElastic, source: &[u8], unrelated: &[u8], lag: usize) -> Result<f64> {
    let mut sum = 0.0;
    for i in HALO..HALO+DEV {
        let y = label(source, unrelated, i, lag);
        sum += bpc(prob(m.advance_inference(source[i-2],source[i-1])?), y);
    }
    Ok(sum/DEV as f64)
}
fn eval_v4(m: &mut RtlrBooleanHybrid, source: &[u8], unrelated: &[u8], lag: usize) -> Result<f64> {
    let mut sum = 0.0;
    for i in HALO..HALO+DEV {
        let y = label(source, unrelated, i, lag);
        sum += bpc(m.advance_inference(source[i-2],source[i-1])?, y);
    }
    Ok(sum/DEV as f64)
}
fn run(panel: u64, lag: usize, seed: u64, root: &Path) -> Result<()> {
    if panel>2 || ![0,64,1023].contains(&lag) || ![11,23,37].contains(&seed) {
        return Err("usage: sml-rtlr-v5-pipeline-bench PANEL(0..2) LAG(0|64|1023) SEED(11|23|37) UNIQUE_DIR".into());
    }
    fs::create_dir(root)?;
    let train_seed = seed_for(TRAIN_SEED,panel);
    let dev_seed = seed_for(DEV_SEED,panel);
    let mut miner = TrainLagDiscovery::new(HALO,32,3,64,0.01)?;
    let discovery_timer = Instant::now();
    for doc in 0..2 {
        let start = HALO + doc*BLOCK;
        let source = data_range(start-HALO,start+BLOCK,train_seed);
        let unrelated = data_range(start-HALO,start+BLOCK,train_seed^NULL_OFFSET);
        miner.reset_history()?;
        for i in 2..HALO { miner.prime_pair(source[i-2],source[i-1])?; }
        for i in HALO..HALO+BLOCK {
            miner.observe_train_pair(source[i-2],source[i-1])?;
            miner.learn_train_target(label(&source,&unrelated,i,lag))?;
        }
    }
    let found = miner.freeze()?;
    let discovery_seconds = discovery_timer.elapsed().as_secs_f64();
    let candidate_count = found.len();
    let inferred = found.first().map_or(0,|x|x.lag);
    if inferred != lag || (lag==0 && candidate_count != 0) || (lag>0 && candidate_count != 1) {
        return Err(format!("TRAIN candidate fail: task={lag} found={found:?}").into());
    }
    let miner_bytes = miner.logical_discovery_bytes();
    drop(miner);
    let config = RtlrElasticConfig {
        max_active: 8,
        learning_rate: 0.005,
        grow_above_bpc: 4.1,
        shrink_below_bpc: 3.8,
        patience: 2,
        warmup_documents: 2,
        seed,
    };
    let policy = RtlrV2Policy {
        ewma_alpha: 0.25,
        plateau_tolerance_bpc: 0.05,
        marginal_cost_bpc_per_cell: 0.001,
    };
    let mut baseline = RtlrElastic::new_v3(config,policy,&root.join("v3-only"))?;
    let mut preset = RtlrBooleanHybrid::new(
        RtlrElastic::new_v3(config,policy,&root.join("v4-preset"))?,
        HALO,vec![1,16,128,1023],0.25
    )?;
    let mut mined = if inferred == 0 { None } else {
        Some(RtlrBooleanHybrid::new(
            RtlrElastic::new_v3(config,policy,&root.join("v5-mined"))?,
            HALO,vec![inferred],0.25
        )?)
    };
    let mut t_v3=0.0;
    let mut t_v4=0.0;
    let mut t_v5=0.0;
    let mut train_positive=0_u64;
    for doc in 2..8 {
        let start = HALO + doc*BLOCK;
        let source = data_range(start-HALO,start+BLOCK,train_seed);
        let unrelated = data_range(start-HALO,start+BLOCK,train_seed^NULL_OFFSET);
        baseline.reset_state();
        preset.reset_state()?;
        if let Some(m) = &mut mined { m.reset_state()?; }
        prime_v3(&mut baseline,train_seed,start)?;
        prime_v4(&mut preset,train_seed,start)?;
        if let Some(m) = &mut mined { prime_v4(m,train_seed,start)?; }
        for i in HALO..HALO+BLOCK { train_positive += u64::from(label(&source,&unrelated,i,lag)); }
        let tick = Instant::now();
        let (l,a) = train_v3(&mut baseline,&source,&unrelated,lag)?;
        t_v3 += tick.elapsed().as_secs_f64();
        baseline.observe_train_document_v2(l,a)?;
        let tick = Instant::now();
        let (l,a) = train_v4(&mut preset,&source,&unrelated,lag)?;
        t_v4 += tick.elapsed().as_secs_f64();
        preset.observe_train_document_v2(l,a)?;
        if let Some(m) = &mut mined {
            let tick = Instant::now();
            let (l,a) = train_v4(m,&source,&unrelated,lag)?;
            t_v5 += tick.elapsed().as_secs_f64();
            m.observe_train_document_v2(l,a)?;
        }
    }
    let trained_preset = preset.expert_weights();
    let trained_mined = mined.as_ref().map(RtlrBooleanHybrid::expert_weights);
    let trained_prior = ((train_positive as f64)+1.0)/(FIT as f64+2.0);
    let source = data_range(0,HALO+DEV,dev_seed);
    let unrelated = data_range(0,HALO+DEV,dev_seed^NULL_OFFSET);
    baseline.reset_state();
    preset.reset_state()?;
    if let Some(m) = &mut mined { m.reset_state()?; }
    prime_v3(&mut baseline,dev_seed,HALO)?;
    prime_v4(&mut preset,dev_seed,HALO)?;
    if let Some(m) = &mut mined { prime_v4(m,dev_seed,HALO)?; }
    let tick = Instant::now();
    let v3_loss = eval_v3(&mut baseline,&source,&unrelated,lag)?;
    let v3_secs = tick.elapsed().as_secs_f64();
    let tick = Instant::now();
    let v4_loss = eval_v4(&mut preset,&source,&unrelated,lag)?;
    let v4_secs = tick.elapsed().as_secs_f64();
    let (v5_loss,v5_secs,v5_active,v5_payload,v5_weight) = if let Some(m) = &mut mined {
        let tick = Instant::now();
        let bpc = eval_v4(m,&source,&unrelated,lag)?;
        let sec = tick.elapsed().as_secs_f64();
        if m.train_updates()!=FIT as u64 || Some(m.expert_weights())!=trained_mined {
            return Err("DEV modified learned v5 weights or TRAIN count".into());
        }
        (bpc,sec,m.active_cells(),m.logical_payload_bytes(),m.expert_weights()[1])
    } else {
        // Empty mining result: v5 is an explicit no-candidate fallback to V3,
        // NOT a separately trained Boolean hybrid.
        (v3_loss,0.0,baseline.active(),baseline.active_payload_bytes(),0.0)
    };
    if preset.train_updates()!=FIT as u64 || preset.expert_weights()!=trained_preset {
        return Err("DEV modified v4 selector".into());
    }
    let mut ring = AnfDelayLine::new(HALO)?;
    for i in 2..HALO { ring.observe_pair(source[i-2],source[i-1]); }
    let tick = Instant::now();
    let mut bit_loss=0.0;
    let mut prior_loss=0.0;
    let mut dev_positive=0_u64;
    for i in HALO..HALO+DEV {
        ring.observe_pair(source[i-2],source[i-1]);
        let y = label(&source,&unrelated,i,lag);
        dev_positive += u64::from(y);
        let p = if inferred==0 {
            trained_prior
        } else if ring.read_lag(inferred).ok_or("missing Boolean DEV causal history")? {
            0.999
        } else { 0.001 };
        bit_loss += bpc(p,y);
        prior_loss += bpc(trained_prior,y);
    }
    let bit_secs = tick.elapsed().as_secs_f64();
    let bit_loss = bit_loss/DEV as f64;
    let prior_loss=prior_loss/DEV as f64;
    let discovered_mps=DISCOVER as f64/discovery_seconds/1e6;
    let rates = [discovered_mps, FIT as f64/t_v3/1e6, FIT as f64/t_v4/1e6,
        DEV as f64/v3_secs/1e6, DEV as f64/v4_secs/1e6, DEV as f64/bit_secs/1e6];
    if rates.iter().any(|v|!v.is_finite()||*v<=0.0)
        || [v3_loss,v4_loss,v5_loss,bit_loss,prior_loss].iter().any(|v|!v.is_finite()||*v<0.0) {
        return Err("nonfinite or invalid CPU metrics".into());
    }
    println!(
        "RTLR_V5_PIPELINE_V1\t{panel}\t{lag}\t{seed}\t{DISCOVER}\t{FIT}\t{DEV}\t{inferred}\t{candidate_count}\t{v3_loss:.9}\t{v4_loss:.9}\t{v5_loss:.9}\t{bit_loss:.9}\t{prior_loss:.9}\t{discovered_mps:.6}\t{:.6}\t{:.6}\t{:.6}\t{:.6}\t{:.6}\t{:.6}\t{:.6}\t{}\t{miner_bytes}\t{v5_active}\t{v5_payload}\t{v5_weight:.9}\t{train_positive}\t{dev_positive}",
        rates[1],rates[2],if mined.is_some(){FIT as f64/t_v5/1e6}else{0.0},
        rates[3],rates[4],if mined.is_some(){DEV as f64/v5_secs/1e6}else{0.0},
        rates[5],ring.storage_bytes()
    );
    Ok(())
}
fn main()->Result<()> {
    let args:Vec<String>=env::args().collect();
    if args.len()!=5 {
        return Err("usage: sml-rtlr-v5-pipeline-bench PANEL LAG SEED UNIQUE_DIR".into());
    }
    run(args[1].parse()?,args[2].parse()?,args[3].parse()?,Path::new(&args[4]))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_panels_and_future_label_boundary() {
        assert_eq!(seed_for(TRAIN_SEED,0),TRAIN_SEED);
        assert_ne!(seed_for(TRAIN_SEED,1),seed_for(TRAIN_SEED,2));
        let source=data_range(0,HALO+2,TRAIN_SEED);
        let independent=data_range(0,HALO+2,TRAIN_SEED^NULL_OFFSET);
        for lag in [64,1023] {
            let target=label(&source,&independent,HALO,lag);
            assert_eq!(target,((source[HALO-lag-1]&source[HALO-lag]).count_ones()&1)!=0);
        }
        assert_eq!(bpc(0.5,true),1.0);
    }
}
