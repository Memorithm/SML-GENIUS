//! GB-11 P0: relaxed training of routed, shared-input sparse ANF banks.
//! Modes: `smoke` (debug teacher 0x6B99, not preregistered data) and
//! `prereg` (frozen matrix: 6 arms x 2 teachers x 5 seeds). Writes one CSV row
//! per run to stdout. Synthetic data only; no timing, energy or LM claim.

use std::time::Instant;

use sml_core::gb11_relaxed_anf::{
    train_and_report, Arm, Dataset, RunReport, Teacher, TeacherFamily, TrainConfig, ARMS, GATES,
};

const TRAIN: usize = 16_384;
const VAL: usize = 4_096;
const TEST: usize = 8_192;
const NOISE: f64 = 0.05;
const SEEDS: [u64; 5] = [1, 2, 3, 4, 5];
const TEACHERS: [(&str, TeacherFamily, u64); 2] = [
    ("T-sparse", TeacherFamily::Sparse, 0x6B11),
    ("T-dense", TeacherFamily::Dense, 0x6B12),
];

struct Job {
    teacher: &'static str,
    arm: Arm,
    seed: u64,
    splits: usize,
}

fn quantile(sorted: &[usize], q: f64) -> usize {
    sorted[((sorted.len() - 1) as f64 * q).round() as usize]
}

fn join(values: &[usize]) -> String {
    values
        .iter()
        .map(usize::to_string)
        .collect::<Vec<_>>()
        .join("/")
}

fn row(job: &Job, r: &RunReport, seconds: f64) -> String {
    let mut k = r.expert_k.clone();
    k.sort_unstable();
    format!(
        "{},{},{},{:.6},{:.5},{:.5},{:.5},{:.5},{:.5},{},{},{},{},{:.3},{},{},{},{},{:.4},{:.4},{},{},{},{},{:.1}",
        job.teacher,
        job.arm.name,
        job.seed,
        r.final_loss,
        r.val_bit_accuracy,
        r.test_bit_accuracy,
        r.test_byte_accuracy,
        r.soft_test_bit_accuracy,
        r.soft_test_bit_accuracy - r.test_bit_accuracy,
        quantile(&k, 0.5),
        quantile(&k, 0.95),
        k[k.len() - 1],
        k.iter().sum::<usize>(),
        r.mean_degree,
        r.zero_gates,
        join(&r.sub_banks),
        join(&r.distinct_address_bits),
        join(&r.router_k),
        r.routing_purity,
        r.min_bank_load,
        r.and_ops,
        r.xor_ops,
        r.parity_checks,
        r.parity_mismatches,
        seconds,
    )
}

fn main() {
    let mode = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "smoke".to_owned());
    let (teachers, arms, seeds): (Vec<(&str, TeacherFamily, u64)>, Vec<Arm>, Vec<u64>) =
        match mode.as_str() {
            "smoke" => (
                vec![("T-debug", TeacherFamily::Sparse, 0x6B99)],
                vec![ARMS[0], ARMS[1], ARMS[3], ARMS[5]],
                vec![1],
            ),
            "prereg" => (TEACHERS.to_vec(), ARMS.to_vec(), SEEDS.to_vec()),
            other => {
                eprintln!("unknown mode {other}; use smoke or prereg");
                std::process::exit(2);
            }
        };

    let datasets: Vec<(Dataset, Dataset, Dataset)> = teachers
        .iter()
        .map(|&(_, family, seed)| {
            let teacher = Teacher::generate(family, seed);
            (
                Dataset::sample(&teacher, TRAIN, seed * 1000 + 1, NOISE),
                Dataset::sample(&teacher, VAL, seed * 1000 + 2, NOISE),
                Dataset::sample(&teacher, TEST, seed * 1000 + 3, 0.0),
            )
        })
        .collect();
    let mut jobs = Vec::new();
    for (splits, &(teacher, _, _)) in teachers.iter().enumerate() {
        for &arm in &arms {
            for &seed in &seeds {
                jobs.push(Job {
                    teacher,
                    arm,
                    seed,
                    splits,
                });
            }
        }
    }

    println!(
        "mode={mode},gates_per_bank={GATES},train={TRAIN},val={VAL},test={TEST},noise={NOISE}"
    );
    println!(
        "teacher,arm,seed,final_loss,val_bit,test_bit,test_byte,soft_test_bit,soft_minus_hard,\
         k_median,k_p95,k_max,k_total,mean_degree,zero_gates,sub_banks,distinct_bits,router_k,\
         purity,min_load,and_ops,xor_ops,parity_checks,parity_mismatches,seconds"
    );
    let workers = std::thread::available_parallelism().map_or(1, |n| n.get());
    let next = std::sync::atomic::AtomicUsize::new(0);
    let rows = std::sync::Mutex::new(vec![String::new(); jobs.len()]);
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| loop {
                let index = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let Some(job) = jobs.get(index) else { break };
                let (train, val, test) = &datasets[job.splits];
                let start = Instant::now();
                let mut config = TrainConfig::preregistered(job.seed);
                if mode == "smoke" {
                    let env =
                        |name: &str| std::env::var(name).ok().and_then(|v| v.parse::<f64>().ok());
                    if let Some(v) = env("GB11_DIV") {
                        config.diversity = v;
                    }
                    if let Some(v) = env("GB11_STE") {
                        config.ste_fraction = v;
                    }
                    if let Some(v) = env("GB11_INIT") {
                        config.selector_init = v;
                    }
                    if let Some(v) = env("GB11_EPOCHS") {
                        config.epochs = v as usize;
                    }
                    if let Some(v) = env("GB11_SEL_LR") {
                        config.selector_lr_scale = v;
                    }
                    if let Some(v) = env("GB11_GUMBEL") {
                        config.selector_gumbel = v;
                    }
                    if let Some(v) = env("GB11_TAU_END") {
                        config.tau_end = v;
                    }
                }
                let (_, report) = train_and_report(&job.arm, &config, train, val, test)
                    .expect("Pvp6Bank compilation of a learned bank");
                let line = row(job, &report, start.elapsed().as_secs_f64());
                eprintln!("{line}");
                rows.lock().expect("rows")[index] = line;
            });
        }
    });
    for line in rows.into_inner().expect("rows") {
        println!("{line}");
    }
}
