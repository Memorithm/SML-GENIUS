//! Deterministic public-API qualification, not a performance benchmark.
use sml_core::{
    pascal_vector_projection::{Pvp6Architecture, Pvp6PopulationProfile},
    pvp6_executor::{Pvp6Bank, Pvp6Gate},
    pvp6_sbg::{SbgAnfPlan, SbgCoordinate, SbgField},
    pvp6_sbg_kernel::SbgAnfKernel,
    pvp6_sbg_trace::{decode_sbg_trace, encode_sbg_trace, SbgTraceFrame},
    semantic::{run_semantic_recurrent, SemanticBitField, SemanticRewriter, SemanticState},
    HaltingPolicy,
};
fn word(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9e3779b97f4a7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d049bb133111eb);
    x ^ (x >> 31)
}
fn initial(seed: u64) -> SemanticState<64> {
    SemanticState {
        value: SemanticBitField::from_words(vec![word(seed)]).unwrap(),
        meaning_mask: SemanticBitField::from_words(vec![word(seed ^ 1)]).unwrap(),
        ambiguity: SemanticBitField::from_words(vec![word(seed ^ 2)]).unwrap(),
        routing: SemanticBitField::from_words(vec![word(seed ^ 3)]).unwrap(),
    }
}
fn plan(lanes: usize) -> SbgAnfPlan<64, 64> {
    let inputs = (0..lanes)
        .map(|lane| {
            vec![
                SbgCoordinate {
                    field: SbgField::Value,
                    bit: lane % 64,
                },
                SbgCoordinate {
                    field: SbgField::Value,
                    bit: (lane + 1) % 64,
                },
                SbgCoordinate {
                    field: SbgField::Meaning,
                    bit: (lane + 2) % 64,
                },
                SbgCoordinate {
                    field: SbgField::Meaning,
                    bit: (lane + 3) % 64,
                },
                SbgCoordinate {
                    field: SbgField::Ambiguity,
                    bit: (lane + 4) % 64,
                },
                SbgCoordinate {
                    field: SbgField::Routing,
                    bit: (lane + 5) % 64,
                },
            ]
        })
        .collect();
    let outputs = (0..lanes)
        .map(|bit| SbgCoordinate {
            field: SbgField::Value,
            bit,
        })
        .collect();
    SbgAnfPlan::new(inputs, outputs).unwrap()
}
fn read(state: &SemanticState<64>, c: SbgCoordinate) -> bool {
    match c.field {
        SbgField::Value => state.value.get(c.bit),
        SbgField::Meaning => state.meaning_mask.get(c.bit),
        SbgField::Ambiguity => state.ambiguity.get(c.bit),
        SbgField::Routing => state.routing.get(c.bit),
    }
    .unwrap()
}
fn literal_next(
    state: &SemanticState<64>,
    plan: &SbgAnfPlan<64, 64>,
    terms: &[u16],
) -> (SemanticState<64>, Vec<usize>, Vec<u64>) {
    let mut next = state.clone();
    let mut queries = vec![];
    let mut outputs = vec![];
    for (lane, destination) in plan.input_lanes().iter().zip(plan.destinations()) {
        let literals = lane.iter().map(|&c| read(state, c)).collect::<Vec<_>>();
        let mut address = 0;
        for (i, &value) in literals.iter().enumerate() {
            if value {
                address |= 1 << i;
            }
        }
        queries.push(address);
        let mut value = false;
        for &term in terms {
            let mut product = true;
            for (i, &literal) in literals.iter().enumerate() {
                if term & (1 << i) != 0 {
                    product &= literal;
                }
            }
            value ^= product;
        }
        match destination.field {
            SbgField::Value => next.value.set(destination.bit, value),
            SbgField::Meaning => next.meaning_mask.set(destination.bit, value),
            SbgField::Ambiguity => next.ambiguity.set(destination.bit, value),
            SbgField::Routing => next.routing.set(destination.bit, value),
        }
        .unwrap();
        outputs.push(u64::from(value));
    }
    (next, queries, outputs)
}
fn name(p: Pvp6PopulationProfile) -> &'static str {
    match p {
        Pvp6PopulationProfile::AffineUniform => "AFFINE_HINT",
        Pvp6PopulationProfile::LowPopcount => "LOW_HINT",
        Pvp6PopulationProfile::HighPopcount => "HIGH_HINT",
    }
}
fn main() {
    let mut cases = 0;
    let mut cycles = 0;
    let mut output_words = 0;
    let mut arms = [0; 3];
    println!("A4_SBG_PANEL,schema=1,state_bits=64,domain=64,scope=synthetic_correctness");
    for lanes in [1, 32] {
        for (family, terms) in [&[0u16, 1][..], &[0u16, 1, 3, 7][..]].iter().enumerate() {
            for profile in [
                Pvp6PopulationProfile::AffineUniform,
                Pvp6PopulationProfile::LowPopcount,
                Pvp6PopulationProfile::HighPopcount,
            ] {
                for seed in [0, 1, 0x50425636, u64::MAX] {
                    for depth in [1, 2, 4, 8] {
                        let initial = initial(seed);
                        let plan = plan(lanes);
                        let bank =
                            Pvp6Bank::new(vec![Pvp6Gate::from_monomials(terms).unwrap()]).unwrap();
                        let base = SbgAnfKernel::new(bank, plan.clone(), profile, depth).unwrap();
                        let mut driven = base.clone();
                        // Fixed-depth control: only the last declared cycle can halt on stability.
                        let policy = HaltingPolicy {
                            min_depth: depth,
                            max_depth: depth,
                            stable_hamming_threshold: 0,
                        };
                        let outcome =
                            run_semantic_recurrent(initial.clone(), &mut driven, policy).unwrap();
                        let mut manual = base;
                        let mut state = initial;
                        let mut frames = vec![];
                        let mut checkpoint = None;
                        println!("CASE,{cases},lanes={lanes},family={family},hint={},seed={seed:016x},depth={depth}",name(profile));
                        for d in 1..=depth {
                            let (expected, queries, outputs) = literal_next(&state, &plan, terms);
                            state = manual.rewrite(&state, d).unwrap().state;
                            assert_eq!(state, expected);
                            let cycle = manual.cycles().last().unwrap();
                            assert_eq!(cycle.queries, queries);
                            assert_eq!(cycle.outputs, outputs);
                            assert!(cycle.selection.diagnostic_only);
                            arms[match cycle.selection.architecture {
                                Pvp6Architecture::PascalAnfMaterialize => 0,
                                Pvp6Architecture::SparseAnfScan => 1,
                                Pvp6Architecture::SubmaskCoefficientQuery => 2,
                            }] += 1;
                            let q = queries
                                .iter()
                                .map(|q| format!("{q:02x}"))
                                .collect::<Vec<_>>()
                                .join(":");
                            let o = outputs
                                .iter()
                                .map(|w| format!("{w:016x}"))
                                .collect::<Vec<_>>()
                                .join(":");
                            println!(
                                "FRAME,{cases},{d},{},{:016x},{:016x},{:016x},{:016x},{q},{o}",
                                cycle.bank_revision,
                                state.value.words()[0],
                                state.meaning_mask.words()[0],
                                state.ambiguity.words()[0],
                                state.routing.words()[0]
                            );
                            frames.push(SbgTraceFrame {
                                depth: d,
                                bank_revision: cycle.bank_revision,
                                state: state.clone(),
                            });
                            cycles += 1;
                            output_words += outputs.len();
                            if d == (depth / 2).max(1) {
                                checkpoint = Some(manual.checkpoint(policy).unwrap());
                            }
                        }
                        assert_eq!(manual, driven);
                        assert_eq!(state, outcome.state);
                        let resumed = checkpoint
                            .unwrap()
                            .resume_checked(driven.bank(), driven.plan())
                            .unwrap();
                        assert_eq!(resumed.outcome, outcome);
                        assert_eq!(resumed.kernel, driven);
                        let bytes = encode_sbg_trace(&frames, depth as usize).unwrap();
                        assert_eq!(
                            decode_sbg_trace::<64>(&bytes, depth as usize).unwrap(),
                            frames
                        );
                        cases += 1;
                    }
                }
            }
        }
    }
    assert_eq!((cases, cycles, output_words), (192, 720, 11880));
    assert_eq!(arms, [540, 120, 60]);
    println!("A4_SBG_SUMMARY,cases={cases},cycles={cycles},state_words=2880,output_words={output_words},checkpoint_replays=192,trace_roundtrips=192,pascal=540,sparse=120,submask=60,mismatches=0");
    println!("PROMOTION,BLOCKED,declared_SBG_execution_only_not_learning_or_speed");
}
