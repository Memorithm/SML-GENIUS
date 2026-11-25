//! Transfer a learned recurrent checkpoint to the native SBG executor.
//! The checkpoint supplies every nonzero ANF coefficient; no fitting occurs.
//! The supplied wiring maps 64 independent memory cells across SBG fields.
use sml_core::{
    anf_local::LocalAnfGate,
    pascal_vector_projection::Pvp6PopulationProfile,
    pvp6_executor::{Pvp6Bank, Pvp6Gate},
    pvp6_sbg::{SbgAnfPlan, SbgCoordinate, SbgField},
    pvp6_sbg_kernel::SbgAnfKernel,
    semantic::{SemanticRewriter, SemanticState},
};
use std::{env, fs::File, io::Read};

fn restore(raw: &str) -> Result<LocalAnfGate, String> {
    let lines: Vec<_> = raw.lines().collect();
    if raw.len() > 128 || lines.len() != 3 {
        return Err("checkpoint size/schema".into());
    }
    if lines[0] != "SML_TUTOR_RECURRENT_V1" || lines[1] != "0,1,2" {
        return Err("checkpoint domain".into());
    }
    let word = lines[2].parse::<u64>().map_err(|e| e.to_string())?;
    LocalAnfGate::new(vec![0, 1, 2], vec![word], 3).map_err(|e| format!("{e:?}"))
}

fn kernel(gate: &LocalAnfGate, steps: u32) -> Result<SbgAnfKernel<64, 64>, String> {
    let terms: Vec<_> = (0..gate.monomial_count())
        .filter(|&m| gate.coefficient(m))
        .map(|m| m as u16)
        .collect();
    let coord = |field, bit| SbgCoordinate { field, bit };
    let inputs = (0..64)
        .map(|i| {
            vec![
                coord(SbgField::Value, i),
                coord(SbgField::Meaning, i),
                coord(SbgField::Ambiguity, i),
                coord(SbgField::Routing, i),
                coord(SbgField::Routing, (i + 1) % 64),
                coord(SbgField::Routing, (i + 2) % 64),
            ]
        })
        .collect();
    let outputs = (0..64).map(|i| coord(SbgField::Value, i)).collect();
    let plan = SbgAnfPlan::new(inputs, outputs).map_err(|e| format!("{e:?}"))?;
    let compiled = Pvp6Gate::from_monomials(&terms).map_err(|e| format!("{e:?}"))?;
    let bank = Pvp6Bank::new(vec![compiled]).map_err(|e| format!("{e:?}"))?;
    SbgAnfKernel::new(bank, plan, Pvp6PopulationProfile::AffineUniform, steps)
        .map_err(|e| format!("{e:?}"))
}

fn rollouts(gate: &LocalAnfGate, steps: u32) -> Result<(u64, u64), String> {
    if !(3..=6).contains(&steps) {
        return Err("unsupported trajectory depth".into());
    }
    let sequences = 1_usize << (2 * steps);
    let mut errors = 0;
    let mut samples = 0;
    for initial in [false, true] {
        for start in (0..sequences).step_by(64) {
            let mut model = kernel(gate, steps)?;
            let mut state = SemanticState::<64>::zeroed().map_err(|e| format!("{e:?}"))?;
            let mut expected = [initial; 64];
            for lane in 0..64 {
                state.value.set(lane, initial).unwrap();
            }
            for t in 0..steps {
                let prior = state.value.clone();
                for (lane, target) in expected.iter_mut().enumerate() {
                    let command = (start + lane) >> (2 * t);
                    let input = command & 1 != 0;
                    let enable = command & 2 != 0;
                    state.meaning_mask.set(lane, input).unwrap();
                    state.ambiguity.set(lane, enable).unwrap();
                    // Irrelevant routing bits deliberately vary independently.
                    let nuisance = (start + lane + t as usize) % 3 == 0;
                    state.routing.set(lane, nuisance).unwrap();
                    if enable {
                        *target = input;
                    }
                }
                let before = state.clone();
                state = model
                    .rewrite(&state, t + 1)
                    .map_err(|e| format!("{e:?}"))?
                    .state;
                let trace = model.cycles().last().ok_or("missing SBG cycle")?;
                if trace.queries.len() != 64
                    || trace.outputs.len() != 64
                    || state.meaning_mask != before.meaning_mask
                    || state.ambiguity != before.ambiguity
                    || state.routing != before.routing
                {
                    return Err("SBG trace or field preservation mismatch".into());
                }
                for (lane, &target) in expected.iter().enumerate() {
                    let address = trace.queries[lane];
                    if address & 1 != usize::from(prior.get(lane).unwrap()) {
                        return Err("state feedback mismatch".into());
                    }
                    let local =
                        gate.evaluate(&[address & 1 != 0, address & 2 != 0, address & 4 != 0]);
                    let output = state.value.get(lane).unwrap();
                    if output != local || trace.outputs[lane] != u64::from(output) {
                        return Err("ANF/SBG execution mismatch".into());
                    }
                    errors += u64::from(output != target);
                    samples += 1;
                }
            }
            let saved = model.clone();
            if model.rewrite(&state, steps + 1).is_ok() || model != saved {
                return Err("hard depth cap or rollback failure".into());
            }
        }
    }
    Ok((errors, samples))
}

fn qualify(raw: &str) -> Result<(), String> {
    let gate = restore(raw)?;
    for steps in [3, 5, 6] {
        let (errors, samples) = rollouts(&gate, steps)?;
        println!("SML_TUTOR_SBG_RECURRENT steps={steps} errors={errors} samples={samples}");
        if errors != 0 || samples != 2 * u64::from(steps) * (1_u64 << (2 * steps)) {
            return Err("SBG recurrent oracle veto".into());
        }
    }
    println!("SML_TUTOR_SBG_RECURRENT=PASS lanes=64 fitting=none wiring=supplied");
    Ok(())
}

fn main() -> Result<(), String> {
    let args: Vec<_> = env::args().skip(1).collect();
    if args.len() != 1 {
        return Err("usage: RECURRENT_GATE".into());
    }
    let mut raw = String::new();
    File::open(&args[0])
        .map_err(|e| e.to_string())?
        .take(129)
        .read_to_string(&mut raw)
        .map_err(|e| e.to_string())?;
    qualify(&raw)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn learned_checkpoint_fixture_transfers_to_native_sbg() {
        qualify("SML_TUTOR_RECURRENT_V1\n0,1,2\n98\n").unwrap();
    }

    #[test]
    fn corrupted_coefficients_and_schema_fail_closed() {
        let wrong = restore("SML_TUTOR_RECURRENT_V1\n0,1,2\n2\n").unwrap();
        assert!(rollouts(&wrong, 3).unwrap().0 > 0);
        assert!(restore("SML_TUTOR_RECURRENT_V1\n0,1,2\n256\n").is_err());
        assert!(restore("SML_TUTOR_RECURRENT_V1\n1,0,2\n98\n").is_err());
    }
}
