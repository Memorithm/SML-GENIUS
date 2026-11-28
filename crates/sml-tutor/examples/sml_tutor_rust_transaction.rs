//! R5b: coordinated, typed repair tools with an independent publication gate.
//! All source comes from the closed Rust grammar. Experimental roots only.
use sml_core::{tutor_rust_r2 as r2, tutor_rust_r3 as r3, tutor_rust_r4 as r4, tutor_rust_r5 as r5};
use sml_tutor::hash::Digest;
use std::fs;
use std::io::{Read, Write};
use std::path::Path;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Verdict {
    Pass,
    Reject,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Event {
    Inspect {
        parent: Digest,
        damaged: Digest,
        train: Digest,
    },
    Stage(Digest),
    Exam(Digest, Verdict),
    Commit(Digest),
    Rollback(Digest),
}

fn inspect(parent: &str, damaged: &str, train: &str) -> Event {
    Event::Inspect {
        parent: Digest::of(parent.as_bytes()),
        damaged: Digest::of(damaged.as_bytes()),
        train: Digest::of(train.as_bytes()),
    }
}

fn exam(parent: &str, candidate: &str) -> Result<Verdict> {
    if r2::examine(parent)?.iter().sum::<u64>() != 458_752 {
        return Err("retention count mismatch".into());
    }
    match r3::examine(parent, candidate) {
        Ok(counts) if counts.iter().sum::<u64>() == 458_752 => Ok(Verdict::Pass),
        Ok(_) => Err("unexpected R3 exam count".into()),
        Err(e) if e.to_string().starts_with("R3 exhaustive mismatch") => Ok(Verdict::Reject),
        Err(e) => Err(e),
    }
}

// Independently re-run the examination and reconstruct the only permissible
// trajectory. A claimed successful exam or a commit without it is not trusted.
fn replay(
    parent: &str,
    damaged: &str,
    train: &str,
    candidate: &str,
    events: &[Event],
) -> Result<Verdict> {
    let digest = Digest::of(candidate.as_bytes());
    let verdict = exam(parent, candidate)?;
    let terminal = match verdict {
        Verdict::Pass => Event::Commit(digest),
        Verdict::Reject => Event::Rollback(digest),
    };
    let expected = [
        inspect(parent, damaged, train),
        Event::Stage(digest),
        Event::Exam(digest, verdict),
        terminal,
    ];
    if events != expected {
        return Err("unsupported transaction trajectory".into());
    }
    Ok(verdict)
}

fn read(path: &Path) -> Result<String> {
    let mut raw = String::new();
    fs::File::open(path)?.take(8193).read_to_string(&mut raw)?;
    if raw.is_empty() || raw.len() > 8192 {
        return Err("bank input size".into());
    }
    Ok(raw)
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

fn trace_text(events: &[Event]) -> String {
    let mut out = String::from("SML_TUTOR_R5B_TRACE_V1\n");
    for event in events {
        let line = match event {
            Event::Inspect {
                parent,
                damaged,
                train,
            } => format!(
                "inspect|{}|{}|{}",
                parent.to_hex(),
                damaged.to_hex(),
                train.to_hex()
            ),
            Event::Stage(d) => format!("stage|{}", d.to_hex()),
            Event::Exam(d, v) => format!("exam|{}|{v:?}", d.to_hex()),
            Event::Commit(d) => format!("commit|{}", d.to_hex()),
            Event::Rollback(d) => format!("rollback|{}", d.to_hex()),
        };
        out.push_str(&line);
        out.push('\n');
    }
    out
}

fn transaction(
    root: &Path,
    parent: &str,
    damaged: &str,
    train: &str,
    candidate: &str,
) -> Result<(Verdict, Vec<Event>)> {
    fs::create_dir(root)?;
    let staged = root.join("staged.bank");
    let output = root.join("repaired.bank");
    let digest = Digest::of(candidate.as_bytes());
    let mut events = vec![inspect(parent, damaged, train)];
    write_new(&staged, candidate.as_bytes())?;
    events.push(Event::Stage(digest));
    let actual = read(&staged)?;
    if Digest::of(actual.as_bytes()) != digest {
        return Err("staged bank identity mismatch".into());
    }
    let verdict = exam(parent, &actual)?;
    events.push(Event::Exam(digest, verdict));
    events.push(match verdict {
        Verdict::Pass => Event::Commit(digest),
        Verdict::Reject => Event::Rollback(digest),
    });
    if replay(parent, damaged, train, &actual, &events)? != verdict || read(&staged)? != actual {
        return Err("independent trajectory replay mismatch".into());
    }
    // A hard link publishes an already-synced candidate atomically without
    // replacing an existing destination. No production store is involved.
    if verdict == Verdict::Pass {
        fs::hard_link(&staged, &output)?;
    }
    fs::remove_file(&staged)?;
    write_new(&root.join("trace.txt"), trace_text(&events).as_bytes())?;
    if verdict == Verdict::Pass {
        if read(&output)? != actual {
            return Err("published bank mismatch".into());
        }
    } else if output.exists() {
        return Err("rejected candidate was published".into());
    }
    Ok((verdict, events))
}

fn qualify(root: &Path, parent: &str, clean: &str, train: &str) -> Result<()> {
    fs::create_dir(root)?;
    let damaged = r4::mutation_challenge(parent, clean, train)?;
    if damaged == clean {
        return Err("missing mutation".into());
    }
    // Negative control: submit the still-damaged candidate, not an arbitrary
    // invalid file. It compiles but must fail its independent semantic exam.
    let rejected = transaction(&root.join("rejected"), parent, &damaged, train, &damaged)?;
    if rejected.0 != Verdict::Reject {
        return Err("negative control passed".into());
    }
    let mut forged = rejected.1;
    let digest = Digest::of(damaged.as_bytes());
    forged[2] = Event::Exam(digest, Verdict::Pass);
    forged[3] = Event::Commit(digest);
    if replay(parent, &damaged, train, &damaged, &forged).is_ok() {
        return Err("forged commit accepted".into());
    }
    // The proposal tool cannot read the clean bank. The examiner, not the
    // proposer, decides whether the separately staged result can be published.
    let proposal = r4::propose_repair(parent, &damaged, train)?;
    let accepted = transaction(&root.join("accepted"), parent, &damaged, train, &proposal)?;
    if accepted.0 != Verdict::Pass
        || read(&root.join("accepted/repaired.bank"))? != clean
        || root.join("rejected/staged.bank").exists()
        || root.join("accepted/staged.bank").exists()
    {
        return Err("transaction qualification failed".into());
    }
    println!("SML_TUTOR_R5B=PASS tools=4 rejected=1 committed=1 forged_rejected=1 exact_recovery=true r3_cases=458752 retained_r2=458752 replay=true");
    Ok(())
}

// Canonical evidence serialization, with coordinates represented by admitted
// TRAIN row index. Replaying a typed proposal reconstructs the bank.
fn proposal_text(proposal: &r5::BankProposal) -> String {
    let mut text = String::from("SML_TUTOR_R5C_PROPOSAL_V1\n");
    for (task, actions) in proposal.traces.iter().enumerate() {
        for action in actions {
            use std::fmt::Write as _;
            match action {
                r5::Action::Probe { index, observed } => {
                    writeln!(text, "{task}|probe|{index}|{observed}").unwrap();
                }
                r5::Action::Patch { candidate } => {
                    writeln!(text, "{task}|patch|{candidate}").unwrap();
                }
                r5::Action::Abstain => {
                    writeln!(text, "{task}|abstain").unwrap();
                }
            }
        }
    }
    text
}

fn qualify_active(root: &Path, parent: &str, clean: &str, train: &str) -> Result<()> {
    fs::create_dir(root)?;
    let damaged = r4::mutation_challenge(parent, clean, train)?;
    if damaged == clean {
        return Err("missing mutation".into());
    }
    // With no budget, no bank may reach the staging tool.
    let abstained = r5::propose_active_bank(parent, &damaged, train, 0)?;
    r5::verify_active_bank_proposal(parent, &damaged, train, 0, &abstained)?;
    if abstained.candidate.is_some() {
        return Err("zero-budget proposal unexpectedly resolved".into());
    }
    write_new(
        &root.join("abstained.trace.txt"),
        proposal_text(&abstained).as_bytes(),
    )?;
    let proposal = r5::propose_active_bank(parent, &damaged, train, 8)?;
    r5::verify_active_bank_proposal(parent, &damaged, train, 8, &proposal)?;
    let candidate = proposal.candidate.as_deref().ok_or("active bank abstained")?;
    let probes = proposal
        .traces
        .iter()
        .flatten()
        .filter(|action| matches!(action, r5::Action::Probe { .. }))
        .count();
    // Reject a substituted candidate before any staging operation.
    let mut forged = proposal.clone();
    forged.candidate = Some(damaged.clone());
    if r5::verify_active_bank_proposal(parent, &damaged, train, 8, &forged).is_ok() {
        return Err("forged active candidate accepted".into());
    }
    let rejected = transaction(&root.join("rejected"), parent, &damaged, train, &damaged)?;
    if rejected.0 != Verdict::Reject {
        return Err("negative control passed".into());
    }
    write_new(&root.join("damaged.bank"), damaged.as_bytes())?;
    write_new(
        &root.join("proposal.trace.txt"),
        proposal_text(&proposal).as_bytes(),
    )?;
    // The verified adaptive candidate, not an all-label fallback or the clean
    // reference, is submitted to the independent transaction examiner.
    let accepted = transaction(&root.join("accepted"), parent, &damaged, train, candidate)?;
    if accepted.0 != Verdict::Pass
        || read(&root.join("accepted/repaired.bank"))? != clean
        || root.join("rejected/repaired.bank").exists()
        || root.join("accepted/staged.bank").exists()
        || root.join("rejected/staged.bank").exists()
    {
        return Err("unified active transaction failed".into());
    }
    println!("SML_TUTOR_R5D=PASS tasks=7 probes={probes} budget_per_task=8 abstained=1 forged_rejected=1 rolled_back=1 committed=1 exact_recovery=true r3_cases=458752 retained_r2=458752 replay=true");
    Ok(())
}

fn main() -> Result<()> {
    let mut args: Vec<_> = std::env::args_os().skip(1).collect();
    let active = args.first().is_some_and(|flag| flag == "--active");
    if active {
        args.remove(0);
    }
    if args.len() != 4 {
        return Err("usage: [--active] root r2.bank r3.bank r3.train".into());
    }
    let run = if active { qualify_active } else { qualify };
    run(
        Path::new(&args[0]),
        &read(Path::new(&args[1]))?,
        &read(Path::new(&args[2]))?,
        &read(Path::new(&args[3]))?,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actual_compile_exam_commit_and_rollback() {
        let root =
            std::env::temp_dir().join(format!("sml-tutor-r5b-transaction-{}", std::process::id()));
        let parent = r2::induce(&r2::fixture()).unwrap();
        let train = r3::fixture();
        let clean = r3::induce(&parent, &train).unwrap();
        qualify(&root, &parent, &clean, &train).unwrap();
        assert!(qualify(&root, &parent, &clean, &train).is_err());
        assert!(!root.join("rejected/repaired.bank").exists());
        assert_eq!(read(&root.join("accepted/repaired.bank")).unwrap(), clean);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn adaptive_proposal_reaches_real_transaction_gate() {
        let root = std::env::temp_dir().join(format!("sml-tutor-r5d-{}", std::process::id()));
        let parent = r2::induce(&r2::fixture()).unwrap();
        let train = r3::fixture();
        let clean = r3::induce(&parent, &train).unwrap();
        qualify_active(&root, &parent, &clean, &train).unwrap();
        assert_eq!(read(&root.join("accepted/repaired.bank")).unwrap(), clean);
        assert!(!root.join("rejected/repaired.bank").exists());
        assert!(root.join("proposal.trace.txt").is_file());
        assert!(root.join("abstained.trace.txt").is_file());
        fs::remove_dir_all(root).unwrap();
    }
}
