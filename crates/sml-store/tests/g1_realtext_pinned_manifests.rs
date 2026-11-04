//! Committed real-text manifests must parse under `G1_REALTEXT_MANIFEST v1`.
//! The corpus bytes are not in Git, so only the manifest grammar is checked.
use sml_store::g1_realtext_corpus::parse_manifest;

#[test]
fn g1_pes2o7_v1_manifest_parses() {
    let text = include_str!("../../../docs/g1-realtext/g1-pes2o7-v1.manifest");
    parse_manifest(text).expect("g1-pes2o7-v1 manifest must parse");
}
