//! GB-11 P0: relaxed training of routed, shared-input sparse ANF banks.
//! Modes: `smoke` (debug teacher 0x6B99, not preregistered data) and
//! `prereg` (frozen matrix: 6 arms x 2 teachers x 5 seeds). Writes one CSV row
//! per run to stdout. `p0b` and `p0b-smoke` run the P0b amendment arms
//! (relaxed init, random-restart hard-EM wiring refinement); `p0b-bounded`
//! recompiles the `hem_lut` banks under the bounded-density contract. Synthetic data only; no timing, energy or LM
//! claim.

use std::time::Instant;

use sml_core::gb11_relaxed_anf::{
    hybrid_train_and_report, pvp6_bounded_parity, train_and_report, Arm, Dataset, HybridArm,
    RunReport, Teacher, TeacherFamily, TrainConfig, ARMS, GATES, HYBRID_ARMS,
};

const TRAIN: usize = 16_384;
const VAL: usize = 4_096;
const TEST: usize = 8_192;
const NOISE: f64 = 0.05;
const SEEDS: [u64; 5] = [1, 2, 3, 4, 5];
type TeacherSpec = (&'static str, TeacherFamily, u64);

const TEACHERS: [TeacherSpec; 2] = [
    ("T-sparse", TeacherFamily::Sparse, 0x6B11),
    ("T-dense", TeacherFamily::Dense, 0x6B12),
];

#[derive(Clone, Copy)]
enum JobArm {
    Relaxed(Arm),
    Hybrid(HybridArm),
}

impl JobArm {
    fn name(self) -> &'static str {
        match self {
            Self::Relaxed(arm) => arm.name,
            Self::Hybrid(arm) => arm.name,
        }
    }
}

struct Job {
    teacher: &'static str,
    arm: JobArm,
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
        job.arm.name(),
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

/// Re-trains the P0b `hem_lut` arm (deterministic, same models as `p0b`) and
/// recompiles each learned bank into one bounded-density `Pvp6Bank`.
fn bounded_recompile() {
    println!(
        "teacher,seed,test_bit,k_total,exact_sub_banks,bounded_transpositions,\
         bounded_checks,bounded_mismatches"
    );
    let arm = HYBRID_ARMS[1];
    for &(name, family, seed) in &TEACHERS {
        let teacher = Teacher::generate(family, seed);
        let train = Dataset::sample(&teacher, TRAIN, seed * 1000 + 1, NOISE);
        let val = Dataset::sample(&teacher, VAL, seed * 1000 + 2, NOISE);
        let test = Dataset::sample(&teacher, TEST, seed * 1000 + 3, 0.0);
        for &run in &SEEDS {
            let (hard, _, report) = hybrid_train_and_report(&arm, run, &train, &val, &test)
                .expect("Pvp6Bank compilation of a learned bank");
            let (checks, mismatches, transpositions) =
                pvp6_bounded_parity(&hard, &test).expect("bounded Pvp6Bank compilation");
            println!(
                "{name},{run},{:.5},{},{},{transpositions},{checks},{mismatches}",
                report.test_bit_accuracy,
                report.expert_k.iter().sum::<usize>(),
                report.sub_banks.iter().sum::<usize>(),
            );
        }
    }
}

fn main() {
    let mode = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "smoke".to_owned());
    if mode == "p0b-bounded" {
        bounded_recompile();
        return;
    }
    let debug = vec![("T-debug", TeacherFamily::Sparse, 0x6B99)];
    let relaxed = |arms: &[Arm]| {
        arms.iter()
            .copied()
            .map(JobArm::Relaxed)
            .collect::<Vec<_>>()
    };
    let hybrid: Vec<JobArm> = HYBRID_ARMS.iter().copied().map(JobArm::Hybrid).collect();
    let (teachers, arms, seeds): (Vec<TeacherSpec>, Vec<JobArm>, Vec<u64>) = match mode.as_str() {
        "smoke" => (
            debug,
            relaxed(&[ARMS[0], ARMS[1], ARMS[3], ARMS[5]]),
            vec![1],
        ),
        "prereg" => (TEACHERS.to_vec(), relaxed(&ARMS), SEEDS.to_vec()),
        "p0b-smoke" => (debug, hybrid, vec![1]),
        "p0b" => (TEACHERS.to_vec(), hybrid, SEEDS.to_vec()),
        other => {
            eprintln!("unknown mode {other}; use smoke, prereg, p0b-smoke or p0b");
            std::process::exit(2);
        }
    };
    let hybrid_mode = mode.starts_with("p0b");

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
         purity,min_load,and_ops,xor_ops,parity_checks,parity_mismatches,seconds{}",
        if hybrid_mode {
            ",best_restart,refine_rounds"
        } else {
            ""
        }
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
                let line = match job.arm {
                    JobArm::Relaxed(arm) => {
                        let (_, report) = train_and_report(&arm, &config, train, val, test)
                            .expect("Pvp6Bank compilation of a learned bank");
                        row(job, &report, start.elapsed().as_secs_f64())
                    }
                    JobArm::Hybrid(arm) => {
                        let (_, (restart, rounds), report) =
                            hybrid_train_and_report(&arm, job.seed, train, val, test)
                                .expect("Pvp6Bank compilation of a learned bank");
                        format!(
                            "{},{restart},{rounds}",
                            row(job, &report, start.elapsed().as_secs_f64())
                        )
                    }
                };
                eprintln!("{line}");
                rows.lock().expect("rows")[index] = line;
            });
        }
    });
    for line in rows.into_inner().expect("rows") {
        println!("{line}");
    }
}
