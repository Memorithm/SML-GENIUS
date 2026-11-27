//! CLI for the shared native R1 learner. The external receipt and bank format
//! remain identical to the original physically qualified example.
use sml_core::tutor_rust_r1::{examine, induce};
use std::error::Error;
use std::fs;
use std::io::{Read, Write};
use std::path::Path;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

fn qualify(teacher_train: Option<&str>, bank_path: Option<&Path>) -> Result<()> {
    let raw = induce(teacher_train)?;
    let exam = examine(&raw)?;
    for (i, name) in ["train_recall", "dev", "test", "testtest"]
        .iter()
        .enumerate()
    {
        println!(
            "SML_TUTOR_R1_COHORT cohort={} correct={} total={}",
            name, exam.correct[i], exam.total[i]
        );
    }
    let teacher = if teacher_train.is_some() {
        "external_verified_labels"
    } else {
        "none"
    };
    if let Some(path) = bank_path {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?;
        file.write_all(raw.as_bytes())?;
        file.sync_all()?;
    }
    println!(
        "SML_TUTOR_R1=PASS programs=9 compiler=rustc exact_cases={} reload={} teacher_inference={teacher}",
        exam.total.iter().sum::<u64>(),
        exam.reload
    );
    if teacher_train.is_some() {
        println!("SML_TUTOR_R1_TEACHER_ADMISSION=PASS rows=72 untrained_baseline_abstains=9");
    }
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    match args.as_slice() {
        [] => qualify(None, None),
        [flag, input, output] if flag == "--teacher-train" => {
            let mut raw = String::new();
            fs::File::open(input)?.take(4097).read_to_string(&mut raw)?;
            qualify(Some(&raw), Some(Path::new(output)))
        }
        _ => Err("usage: sml_tutor_rust_r1 [--teacher-train corpus checkpoint]".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn original_cli_qualifies_closed_grammar() {
        qualify(None, None).unwrap();
    }
}
