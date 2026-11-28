//! R5a: bounded active diagnosis with typed probe/patch/abstain actions.
//! Select probes from TRAIN inputs using candidate predictions only. Labels
//! are disclosed by the admitted evidence tool, never by the held-out oracle.
use crate::tutor_rust_r2::{self as r2, Expr, Learning, Op};
use crate::tutor_rust_r3 as r3;
use std::error::Error;

type Result<T> = std::result::Result<T, Box<dyn Error>>;
type Example = (u8, u8, u8);
const BUDGET: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Action {
    Probe { index: usize, observed: u8 },
    Patch { candidate: usize },
    Abstain,
}

#[derive(Clone, Copy)]
enum Strategy {
    Adaptive,
    Sequential,
}

#[derive(Debug)]
struct Plan {
    actions: Vec<Action>,
    selected: Option<Expr>,
}

fn candidates(expr: &Expr) -> Result<Vec<Expr>> {
    let Expr::Apply(_, left, right) = expr else {
        return Err("unsupported expression".into());
    };
    Ok(Op::ALL
        .into_iter()
        .map(|op| Expr::apply(op, left, right))
        .collect())
}

// Smaller sum of squared bucket sizes means fewer expected surviving
// hypotheses under a uniform candidate prior. No answer is used here.
fn score(candidates: &[Expr], a: u8, b: u8) -> usize {
    let mut counts = [0usize; 256];
    for candidate in candidates {
        counts[usize::from(candidate.eval(a, b))] += 1;
    }
    counts.iter().map(|n| n * n).sum()
}

fn select(
    candidates: &[Expr],
    inputs: &[(u8, u8)],
    queried: &[bool],
    strategy: Strategy,
) -> Option<usize> {
    let available = (0..inputs.len()).filter(|&i| !queried[i]);
    match strategy {
        Strategy::Sequential => available.into_iter().next(),
        Strategy::Adaptive => available
            .map(|i| (score(candidates, inputs[i].0, inputs[i].1), i))
            .filter(|&(s, _)| s < candidates.len() * candidates.len())
            .min()
            .map(|(_, i)| i),
    }
}

// One evidence source exposes coordinates to planning and a single label
// to the probe action. The trace is replayed independently before use.
fn plan(
    initial: &Expr,
    evidence: &[Example],
    budget: usize,
    strategy: Strategy,
) -> Result<Plan> {
    let universe = candidates(initial)?;
    let inputs: Vec<_> = evidence.iter().map(|&(a, b, _)| (a, b)).collect();
    let mut queried = vec![false; inputs.len()];
    let mut observed = Vec::new();
    let mut actions = Vec::new();
    let mut remaining = universe.clone();
    for used in 0..=budget {
        match r2::learn(&remaining, &observed) {
            Learning::Unique(expr) => {
                let candidate = universe
                    .iter()
                    .position(|c| c == &expr)
                    .ok_or("candidate identity lost")?;
                actions.push(Action::Patch { candidate });
                return Ok(Plan {
                    actions,
                    selected: Some(expr),
                });
            }
            Learning::Inconsistent => break,
            Learning::Ambiguous => {}
        }
        if used == budget {
            break;
        }
        let Some(index) = select(&remaining, &inputs, &queried, strategy) else {
            break;
        };
        queried[index] = true;
        let (a, b, y) = evidence[index];
        actions.push(Action::Probe { index, observed: y });
        observed.push((a, b, y));
        remaining.retain(|c| c.eval(a, b) == y);
    }
    actions.push(Action::Abstain);
    Ok(Plan {
        actions,
        selected: None,
    })
}

fn replay(
    initial: &Expr,
    evidence: &[Example],
    actions: &[Action],
    budget: usize,
) -> Result<Option<Expr>> {
    let universe = candidates(initial)?;
    let mut observed = Vec::new();
    let mut used = vec![false; evidence.len()];
    for (step, action) in actions.iter().enumerate() {
        match *action {
            Action::Probe { index, observed: y } => {
                let &(a, b, actual) = evidence.get(index).ok_or("foreign probe")?;
                if used[index] || observed.len() == budget || y != actual {
                    return Err("invalid or over-budget probe".into());
                }
                used[index] = true;
                observed.push((a, b, y));
            }
            Action::Patch { candidate } => {
                if step + 1 != actions.len() {
                    return Err("patch is not terminal".into());
                }
                let candidate = universe.get(candidate).ok_or("foreign candidate")?;
                match r2::learn(&universe, &observed) {
                    Learning::Unique(expr) if expr == *candidate => return Ok(Some(expr)),
                    _ => return Err("unsupported patch".into()),
                }
            }
            Action::Abstain => {
                if step + 1 != actions.len() {
                    return Err("abstention is not terminal".into());
                }
                return Ok(None);
            }
        }
    }
    Err("missing terminal action".into())
}

#[derive(Debug)]
pub struct ActiveReport {
    pub tasks: usize,
    pub adaptive_probes: usize,
    pub sequential_probes: usize,
    pub new_cases: u64,
    pub retained_cases: u64,
}

/// Both strategies see identical TRAIN evidence, candidate set and budget.
/// The original bank creates the challenge and scores the repaired result;
/// it is not passed to either planner. No external evidence is synthesized.
pub fn qualify(memory: &str, clean: &str, train: &str) -> Result<ActiveReport> {
    let parent = r3::verified_memory(memory, &r2::grammar())?;
    let initial = r3::restore(clean, &r3::extend(&parent))?;
    let admitted = r3::admit(train)?;
    let mut report = ActiveReport {
        tasks: 0,
        adaptive_probes: 0,
        sequential_probes: 0,
        new_cases: 0,
        retained_cases: 0,
    };
    let mut repaired = Vec::new();
    for (original, evidence) in initial.iter().zip(&admitted) {
        let mutant = candidates(original)?
            .into_iter()
            .find(|c| evidence.iter().any(|&(a, b, y)| c.eval(a, b) != y))
            .ok_or("no observable corruption")?;
        let active = plan(&mutant, evidence, BUDGET, Strategy::Adaptive)?;
        let fixed = replay(&mutant, evidence, &active.actions, BUDGET)?
            .ok_or("active diagnosis abstained")?;
        if active.selected.as_ref() != Some(&fixed) || fixed.source() != original.source() {
            return Err("active repair differs from original bank".into());
        }
        let passive = plan(&mutant, evidence, BUDGET, Strategy::Sequential)?;
        let passive_result = replay(&mutant, evidence, &passive.actions, BUDGET)?;
        if passive.selected != passive_result {
            return Err("sequential replay mismatch".into());
        }
        if let Some(ref expr) = passive_result {
            if expr.source() != original.source() {
                return Err("sequential patch differs from original bank".into());
            }
        }
        report.tasks += 1;
        report.adaptive_probes += active.actions.len() - 1;
        report.sequential_probes += passive.actions.len() - 1;
        repaired.push(fixed);
    }
    let bank = r3::checkpoint(&repaired);
    if bank != clean {
        return Err("bank recovery mismatch".into());
    }
    report.new_cases = r3::examine(memory, &bank)?.iter().sum();
    report.retained_cases = r2::examine(memory)?.iter().sum();
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_repair_and_trace_replay() {
        let parent = r2::induce(&r2::fixture()).unwrap();
        let train = r3::fixture();
        let child = r3::induce(&parent, &train).unwrap();
        let report = qualify(&parent, &child, &train).unwrap();
        assert_eq!(report.tasks, 7);
        assert!(report.adaptive_probes <= 7 * BUDGET);
        assert_eq!(report.new_cases, 458_752);
        assert_eq!(report.retained_cases, 458_752);
    }

    #[test]
    fn reject_unsupported_actions_and_abstain_without_evidence() {
        let initial = Expr::apply(Op::Add, &Expr::A, &Expr::B);
        let evidence = [(1, 2, 3)];
        let none = plan(&initial, &evidence, 0, Strategy::Adaptive).unwrap();
        assert_eq!(none.actions, vec![Action::Abstain]);
        assert_eq!(replay(&initial, &evidence, &none.actions, 0).unwrap(), None);
        assert!(replay(&initial, &evidence, &[Action::Patch { candidate: 0 }], 1).is_err());
        assert!(replay(
            &initial,
            &evidence,
            &[Action::Probe { index: 0, observed: 4 }, Action::Abstain],
            1
        )
        .is_err());
        assert!(replay(
            &initial,
            &evidence,
            &[Action::Probe { index: 1, observed: 3 }, Action::Abstain],
            1
        )
        .is_err());
        let probe = Action::Probe { index: 0, observed: 3 };
        assert!(replay(
            &initial,
            &evidence,
            &[probe.clone(), probe, Action::Abstain],
            2
        )
        .is_err());
    }
}
