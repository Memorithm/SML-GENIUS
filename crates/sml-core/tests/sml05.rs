use sml_core::forge_topology::{
    ForgeTopologyCandidateV1, ForgeTopologyError, FORGE_SML_TOPOLOGY_CONTRACT_V1,
};

fn target_table() -> Vec<bool> {
    (0_usize..8)
        .map(|row| {
            let x0 = row & 1 != 0;
            let x1 = row & 2 != 0;
            let x2 = row & 4 != 0;
            (x0 ^ x1) & x2
        })
        .collect()
}

const EXACT_FORGE_REPR: &str = "sml-topology/v1|g0=0,1,6|g1=3,2,8|g2=0,0,0|out=4";

#[test]
fn parser_accepts_canonical_forge_v1_representation() {
    let candidate = ForgeTopologyCandidateV1::parse(EXACT_FORGE_REPR).unwrap();

    assert_eq!(candidate.gates.len(), 3);
    assert_eq!(candidate.gates[0].truth_table.code(), 6);
    assert_eq!(candidate.gates[1].truth_table.code(), 8);
    assert_eq!(candidate.output, 4);
}

#[test]
fn destination_requalification_reexecutes_complete_oracle() {
    let candidate = ForgeTopologyCandidateV1::parse(EXACT_FORGE_REPR).unwrap();
    let report = candidate.requalify(3, 3, &target_table()).unwrap();

    assert!(report.exact());
    assert_eq!(report.rows, 8);
    assert_eq!(report.incorrect_rows, 0);
    assert_eq!(report.reachable_gates, 2);
    assert_eq!(report.learned_boolean_bits, 8);
    assert_eq!(report.wiring_metadata_bits, 15);
}

#[test]
fn destination_requalification_rejects_wrong_but_structural_candidate() {
    let candidate =
        ForgeTopologyCandidateV1::parse("sml-topology/v1|g0=0,1,0|g1=3,2,8|g2=0,0,0|out=4")
            .unwrap();
    let report = candidate.requalify(3, 3, &target_table()).unwrap();

    assert!(!report.exact());
    assert!(report.incorrect_rows > 0);
}

#[test]
fn parser_is_strict_about_version_order_and_integer_canonicality() {
    assert_eq!(
        ForgeTopologyCandidateV1::parse("sml-topology/v2|g0=0,1,6|g1=3,2,8|g2=0,0,0|out=4"),
        Err(ForgeTopologyError::ContractVersion)
    );
    assert_eq!(
        ForgeTopologyCandidateV1::parse("sml-topology/v1|g1=0,1,6|g0=3,2,8|g2=0,0,0|out=4"),
        Err(ForgeTopologyError::GateIndex {
            expected: 0,
            actual: 1
        })
    );
    assert_eq!(
        ForgeTopologyCandidateV1::parse("sml-topology/v1|g0=00,1,6|g1=3,2,8|g2=0,0,0|out=4"),
        Err(ForgeTopologyError::InvalidInteger)
    );
}

#[test]
fn requalification_rejects_forward_reference() {
    let candidate =
        ForgeTopologyCandidateV1::parse("sml-topology/v1|g0=3,1,6|g1=0,2,8|g2=0,0,0|out=4")
            .unwrap();

    assert!(matches!(
        candidate.requalify(3, 3, &target_table()),
        Err(ForgeTopologyError::ForwardOrInvalidSource { gate: 0, .. })
    ));
}

#[test]
fn public_contract_constant_matches_wire_prefix() {
    assert_eq!(FORGE_SML_TOPOLOGY_CONTRACT_V1, "sml-topology/v1");
}
