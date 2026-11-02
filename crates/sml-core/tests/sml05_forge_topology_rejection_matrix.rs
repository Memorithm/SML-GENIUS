//! Table-driven rejection matrix for Forge `sml-topology/v1` requalification.
//!
//! Covers every public error path of `forge_topology.rs`:
//! `ForgeTopologyCandidateV1::parse` (contract version, field grammar, gate
//! indices, canonical integers, truth-table codes, output cardinality),
//! `validate` (input count, gate budget, gate-count mismatch, forward /
//! self / out-of-range sources, output source) on both parsed and forged
//! (directly constructed) candidates, and `requalify` (oracle length plus
//! all `validate` errors), with positive boundary controls, accounting
//! checks and state-unchanged-on-rejection checks.
//!
//! Not reachable from the public API (documented, not tested):
//! - `ForgeTopologyError::GateValue`: `requalify` calls `validate` first, which
//!   proves every gate/output source is strictly backward and in range, so
//!   `resolve_signal` / `reachable_gate_count` never index past `gates`.
//! - `ForwardOrInvalidSource` from `resolve_signal` during evaluation: same
//!   reason; only the `validate` emission of this variant is reachable.
//! - `InputCount` from the `checked_shl` in `requalify`: `validate` already
//!   bounds `input_count` to `2..=8`.

use sml_core::{
    forge_topology::{
        ForgeGateV1, ForgeTopologyCandidateV1, ForgeTopologyError, FORGE_SML_TOPOLOGY_CONTRACT_V1,
    },
    train::TruthTable2,
};

const EXACT: &str = "sml-topology/v1|g0=0,1,6|g1=3,2,8|g2=0,0,0|out=4";

fn target3() -> Vec<bool> {
    (0_usize..8)
        .map(|r| ((r & 1 != 0) ^ (r & 2 != 0)) & (r & 4 != 0))
        .collect()
}

fn gate(left: u16, right: u16, code: u8) -> ForgeGateV1 {
    ForgeGateV1 {
        left,
        right,
        truth_table: TruthTable2::new(code).unwrap(),
    }
}

fn exact() -> ForgeTopologyCandidateV1 {
    ForgeTopologyCandidateV1::parse(EXACT).unwrap()
}

#[test]
fn contract_constant_is_v1() {
    assert_eq!(FORGE_SML_TOPOLOGY_CONTRACT_V1, "sml-topology/v1");
}

#[test]
fn parse_rejection_matrix() {
    use ForgeTopologyError as E;
    let cases: &[(&str, &str, E)] = &[
        ("empty string", "", E::ContractVersion),
        (
            "wrong version",
            "sml-topology/v2|g0=0,1,6|out=2",
            E::ContractVersion,
        ),
        (
            "version prefix only",
            "sml-topology|g0=0,1,6|out=2",
            E::ContractVersion,
        ),
        (
            "version with trailing space",
            "sml-topology/v1 |g0=0,1,6|out=2",
            E::ContractVersion,
        ),
        (
            "uppercase version",
            "SML-TOPOLOGY/V1|g0=0,1,6|out=2",
            E::ContractVersion,
        ),
        ("version only", "sml-topology/v1", E::MissingOutput),
        (
            "gates without output",
            "sml-topology/v1|g0=0,1,6",
            E::MissingOutput,
        ),
        (
            "output without gates",
            "sml-topology/v1|out=0",
            E::ZeroGates,
        ),
        (
            "empty field",
            "sml-topology/v1||g0=0,1,6|out=2",
            E::UnknownField,
        ),
        (
            "trailing separator",
            "sml-topology/v1|g0=0,1,6|out=2|",
            E::UnknownField,
        ),
        (
            "unknown field",
            "sml-topology/v1|g0=0,1,6|x=1|out=2",
            E::UnknownField,
        ),
        (
            "output key without equals",
            "sml-topology/v1|g0=0,1,6|out2",
            E::UnknownField,
        ),
        (
            "uppercase gate key",
            "sml-topology/v1|G0=0,1,6|out=2",
            E::UnknownField,
        ),
        (
            "gate after output",
            "sml-topology/v1|g0=0,1,6|out=2|g1=0,1,6",
            E::GateAfterOutput,
        ),
        (
            "duplicate output",
            "sml-topology/v1|g0=0,1,6|out=2|out=2",
            E::DuplicateOutput,
        ),
        (
            "gate missing equals",
            "sml-topology/v1|g0|out=2",
            E::GateSyntax,
        ),
        (
            "gate one value",
            "sml-topology/v1|g0=0|out=2",
            E::GateSyntax,
        ),
        (
            "gate two values",
            "sml-topology/v1|g0=0,1|out=2",
            E::GateSyntax,
        ),
        (
            "gate four values",
            "sml-topology/v1|g0=0,1,6,7|out=2",
            E::GateSyntax,
        ),
        (
            "gate index skipped",
            "sml-topology/v1|g1=0,1,6|out=2",
            E::GateIndex {
                expected: 0,
                actual: 1,
            },
        ),
        (
            "gate index repeated",
            "sml-topology/v1|g0=0,1,6|g0=0,1,6|out=2",
            E::GateIndex {
                expected: 1,
                actual: 0,
            },
        ),
        (
            "gate index empty",
            "sml-topology/v1|g=0,1,6|out=2",
            E::InvalidInteger,
        ),
        (
            "gate index leading zero",
            "sml-topology/v1|g00=0,1,6|out=2",
            E::InvalidInteger,
        ),
        (
            "left leading zero",
            "sml-topology/v1|g0=01,1,6|out=2",
            E::InvalidInteger,
        ),
        (
            "right signed",
            "sml-topology/v1|g0=0,+1,6|out=2",
            E::InvalidInteger,
        ),
        (
            "table negative",
            "sml-topology/v1|g0=0,1,-6|out=2",
            E::InvalidInteger,
        ),
        (
            "left whitespace",
            "sml-topology/v1|g0= 0,1,6|out=2",
            E::InvalidInteger,
        ),
        (
            "empty left",
            "sml-topology/v1|g0=,1,6|out=2",
            E::InvalidInteger,
        ),
        (
            "empty table",
            "sml-topology/v1|g0=0,1,|out=2",
            E::InvalidInteger,
        ),
        (
            "left u16 overflow",
            "sml-topology/v1|g0=65536,1,6|out=2",
            E::InvalidInteger,
        ),
        (
            "table u8 overflow",
            "sml-topology/v1|g0=0,1,256|out=2",
            E::InvalidInteger,
        ),
        (
            "table code 16",
            "sml-topology/v1|g0=0,1,16|out=2",
            E::TruthTableCode(16),
        ),
        (
            "table code 255",
            "sml-topology/v1|g0=0,1,255|out=2",
            E::TruthTableCode(255),
        ),
        (
            "output empty",
            "sml-topology/v1|g0=0,1,6|out=",
            E::InvalidInteger,
        ),
        (
            "output leading zero",
            "sml-topology/v1|g0=0,1,6|out=02",
            E::InvalidInteger,
        ),
        (
            "output hex",
            "sml-topology/v1|g0=0,1,6|out=0x2",
            E::InvalidInteger,
        ),
        (
            "output u16 overflow",
            "sml-topology/v1|g0=0,1,6|out=65536",
            E::InvalidInteger,
        ),
    ];
    for (name, repr, expected) in cases {
        assert_eq!(
            ForgeTopologyCandidateV1::parse(repr),
            Err(*expected),
            "case `{name}`: {repr:?}"
        );
    }
}

#[test]
fn parse_positive_boundaries() {
    // Max integers and max table code parse; structural checks are deferred
    // to validate().
    let c = ForgeTopologyCandidateV1::parse("sml-topology/v1|g0=65535,65535,15|out=65535").unwrap();
    assert_eq!(c.gates, vec![gate(65535, 65535, 15)]);
    assert_eq!(c.output, 65535);
    let c = ForgeTopologyCandidateV1::parse("sml-topology/v1|g0=0,0,0|out=0").unwrap();
    assert_eq!(c.gates, vec![gate(0, 0, 0)]);
    assert_eq!(c.output, 0);
    // Exactly 64 gates parse and validate at the budget ceiling.
    let mut repr = String::from(FORGE_SML_TOPOLOGY_CONTRACT_V1);
    for i in 0..64 {
        repr.push_str(&format!("|g{i}=0,1,6"));
    }
    repr.push_str("|out=65");
    let c = ForgeTopologyCandidateV1::parse(&repr).unwrap();
    assert_eq!(c.gates.len(), 64);
    assert_eq!(c.validate(2, 64), Ok(()));
}

#[test]
fn validate_rejection_matrix_on_parsed_candidate() {
    use ForgeTopologyError as E;
    let c = exact();
    let snapshot = c.clone();
    let cases: &[(&str, u8, u8, E)] = &[
        ("input count 0", 0, 3, E::InputCount(0)),
        ("input count 1", 1, 3, E::InputCount(1)),
        ("input count 9", 9, 3, E::InputCount(9)),
        ("input count max", u8::MAX, 3, E::InputCount(u8::MAX)),
        ("input checked before budget", 1, 0, E::InputCount(1)),
        ("max gates 0", 3, 0, E::MaxGates(0)),
        ("max gates 65", 3, 65, E::MaxGates(65)),
        ("max gates max", 3, u8::MAX, E::MaxGates(u8::MAX)),
        (
            "budget above gate count",
            3,
            4,
            E::GateCount {
                expected: 4,
                actual: 3,
            },
        ),
        (
            "budget below gate count",
            3,
            2,
            E::GateCount {
                expected: 2,
                actual: 3,
            },
        ),
        // With 2 inputs, g1 references signal 3 == limit 2+1.
        (
            "fewer inputs makes g1 forward",
            2,
            3,
            E::ForwardOrInvalidSource {
                gate: 1,
                source: 3,
                limit: 3,
            },
        ),
    ];
    for (name, inputs, max_gates, expected) in cases {
        assert_eq!(
            c.validate(*inputs, *max_gates),
            Err(*expected),
            "case `{name}`"
        );
        assert_eq!(
            c.requalify(*inputs, *max_gates, &target3()),
            Err(*expected),
            "requalify case `{name}`"
        );
        assert_eq!(c, snapshot, "state changed in case `{name}`");
    }
    assert_eq!(c.validate(3, 3), Ok(()));
    // More inputs keep all sources backward; only output index semantics shift.
    assert_eq!(c.validate(8, 3), Ok(()));
}

#[test]
fn validate_rejection_matrix_on_forged_candidates() {
    use ForgeTopologyError as E;
    let forged = |gates: Vec<ForgeGateV1>, output: u16| ForgeTopologyCandidateV1 { gates, output };
    let cases: Vec<(&str, ForgeTopologyCandidateV1, u8, u8, E)> = vec![
        (
            "empty gates, budget 0",
            forged(vec![], 0),
            2,
            0,
            E::MaxGates(0),
        ),
        (
            "empty gates, budget 1",
            forged(vec![], 0),
            2,
            1,
            E::GateCount {
                expected: 1,
                actual: 0,
            },
        ),
        (
            "65 gates",
            forged(vec![gate(0, 1, 6); 65], 0),
            2,
            64,
            E::GateCount {
                expected: 64,
                actual: 65,
            },
        ),
        (
            "left self reference",
            forged(vec![gate(2, 0, 6)], 2),
            2,
            1,
            E::ForwardOrInvalidSource {
                gate: 0,
                source: 2,
                limit: 2,
            },
        ),
        (
            "right self reference",
            forged(vec![gate(0, 2, 6)], 2),
            2,
            1,
            E::ForwardOrInvalidSource {
                gate: 0,
                source: 2,
                limit: 2,
            },
        ),
        (
            "left checked before right",
            forged(vec![gate(5, 4, 6)], 2),
            2,
            1,
            E::ForwardOrInvalidSource {
                gate: 0,
                source: 5,
                limit: 2,
            },
        ),
        (
            "forward reference in later gate",
            forged(vec![gate(0, 1, 6), gate(0, 4, 8)], 3),
            2,
            2,
            E::ForwardOrInvalidSource {
                gate: 1,
                source: 4,
                limit: 3,
            },
        ),
        (
            "u16 max source",
            forged(vec![gate(u16::MAX, 0, 6)], 2),
            2,
            1,
            E::ForwardOrInvalidSource {
                gate: 0,
                source: u16::MAX,
                limit: 2,
            },
        ),
        (
            "output equals limit",
            forged(vec![gate(0, 1, 6)], 3),
            2,
            1,
            E::OutputSource {
                source: 3,
                limit: 3,
            },
        ),
        (
            "output u16 max",
            forged(vec![gate(0, 1, 6)], u16::MAX),
            2,
            1,
            E::OutputSource {
                source: u16::MAX,
                limit: 3,
            },
        ),
    ];
    for (name, c, inputs, max_gates, expected) in cases {
        let snapshot = c.clone();
        assert_eq!(
            c.validate(inputs, max_gates),
            Err(expected),
            "case `{name}`"
        );
        let rows = 1_usize << inputs;
        assert_eq!(
            c.requalify(inputs, max_gates, &vec![false; rows]),
            Err(expected),
            "requalify case `{name}`"
        );
        assert_eq!(c, snapshot, "state changed in case `{name}`");
    }
}

#[test]
fn requalify_truth_table_length_matrix() {
    let c = exact();
    let snapshot = c.clone();
    for len in [0_usize, 1, 4, 7, 9, 16, 256] {
        assert_eq!(
            c.requalify(3, 3, &vec![false; len]),
            Err(ForgeTopologyError::TruthTableLength {
                expected: 8,
                actual: len
            }),
            "len {len}"
        );
        assert_eq!(c, snapshot);
    }
    // Structural errors take precedence over oracle length.
    assert_eq!(
        c.requalify(1, 3, &[]),
        Err(ForgeTopologyError::InputCount(1))
    );
    assert_eq!(
        c.requalify(3, 4, &[]),
        Err(ForgeTopologyError::GateCount {
            expected: 4,
            actual: 3
        })
    );
}

#[test]
fn requalify_positive_controls_and_accounting() {
    let report = exact().requalify(3, 3, &target3()).unwrap();
    assert!(report.exact());
    assert_eq!(report.rows, 8);
    assert_eq!(report.incorrect_rows, 0);
    // g2 is dead; reference cardinality 3+3=6 -> 3 bits.
    assert_eq!(report.reachable_gates, 2);
    assert_eq!(report.learned_boolean_bits, 8);
    assert_eq!(report.wiring_metadata_bits, 2 * 2 * 3 + 3);

    // Inverted oracle: every row wrong, structure still accepted.
    let inverted: Vec<bool> = target3().into_iter().map(|b| !b).collect();
    let report = exact().requalify(3, 3, &inverted).unwrap();
    assert!(!report.exact());
    assert_eq!(report.incorrect_rows, 8);

    // Output wired straight to an input: zero reachable gates.
    let c = ForgeTopologyCandidateV1::parse("sml-topology/v1|g0=0,1,6|out=1").unwrap();
    let oracle: Vec<bool> = (0..4).map(|r| r & 2 != 0).collect();
    let report = c.requalify(2, 1, &oracle).unwrap();
    assert!(report.exact());
    assert_eq!(report.reachable_gates, 0);
    assert_eq!(report.learned_boolean_bits, 0);
    // cardinality 3 -> 2 bits; only the output reference is charged.
    assert_eq!(report.wiring_metadata_bits, 2);

    // Lower input boundary (2) with XOR.
    let c = ForgeTopologyCandidateV1::parse("sml-topology/v1|g0=0,1,6|out=2").unwrap();
    let report = c.requalify(2, 1, &[false, true, true, false]).unwrap();
    assert!(report.exact());
    assert_eq!((report.rows, report.reachable_gates), (4, 1));

    // Upper input boundary (8): 256 rows, gate reads input 7.
    let c = ForgeTopologyCandidateV1::parse("sml-topology/v1|g0=7,7,15|out=8").unwrap();
    let report = c.requalify(8, 1, &[true; 256]).unwrap();
    assert!(report.exact());
    assert_eq!(report.rows, 256);
}
