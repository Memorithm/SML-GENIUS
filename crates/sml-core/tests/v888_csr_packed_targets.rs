use sml_core::{v888_csr::V888CsrGraph, BooleanWorkspace};

fn graph_fixture(nodes: usize) -> Vec<u8> {
    let mut offsets = Vec::with_capacity(nodes + 1);
    let mut targets = Vec::<u32>::new();
    let mut counts = Vec::<u64>::new();
    offsets.push(0_u64);
    for source in 0..nodes {
        let mut destinations = vec![source, (source + 1) % nodes, (source * 7 + 3) % nodes];
        destinations.sort_unstable();
        destinations.dedup();
        for target in destinations {
            targets.push(target as u32);
            counts.push((source + target) as u64 % 7 + 1);
        }
        offsets.push(targets.len() as u64);
    }
    let mut bytes = b"V8CSR002".to_vec();
    bytes.extend_from_slice(&(nodes as u64).to_le_bytes());
    bytes.extend_from_slice(&(targets.len() as u64).to_le_bytes());
    for id in 1..=nodes {
        bytes.extend_from_slice(&(id as u64).to_le_bytes());
    }
    for offset in offsets {
        bytes.extend_from_slice(&offset.to_le_bytes());
    }
    for target in targets {
        bytes.extend_from_slice(&target.to_le_bytes());
    }
    for count in counts {
        bytes.extend_from_slice(&count.to_le_bytes());
    }
    bytes
}

#[test]
fn packed_target_writes_match_independent_dense_csr_oracle() {
    for nodes in [1, 2, 63, 64, 65, 127, 128, 129] {
        let encoded = graph_fixture(nodes);
        let graph = V888CsrGraph::read(&encoded).unwrap();
        for mask in 0..48 {
            let bits: Vec<_> = (0..nodes)
                .map(|i| ((i * 17 + mask * 23) % 31) < (mask % 31))
                .collect();
            let input = BooleanWorkspace::from_bits(&bits);
            let untouched = input.clone();
            let mut expected = vec![false; nodes];
            let mut expected_edges = 0_u64;
            let mut expected_contacts = 0_u64;
            let mut expected_loops = 0_u64;
            let mut expected_sources = 0_u64;
            for source in 0..nodes {
                if !bits[source] {
                    continue;
                }
                expected_sources += 1;
                for edge in graph.offset(source)..graph.offset(source + 1) {
                    expected_edges += 1;
                    let target = graph.target(edge);
                    expected[target] = true;
                    expected_contacts += graph.count(edge);
                    expected_loops += u64::from(source == target);
                }
            }
            let step = graph.step(&input, expected_edges).unwrap();
            assert_eq!(step.output, BooleanWorkspace::from_bits(&expected));
            assert_eq!(step.output.active_indices().collect::<Vec<_>>(),
                expected.iter().enumerate()
                    .filter_map(|(i, active)| active.then_some(i))
                    .collect::<Vec<_>>());
            assert_eq!(step.active_sources, expected_sources);
            assert_eq!(step.edges, expected_edges);
            assert_eq!(step.contacts, expected_contacts);
            assert_eq!(step.loops, expected_loops);
            assert_eq!(step.counters.boolean_ops, expected_edges);
            assert_eq!(step.counters.recurrent_cycles, 1);
            assert_eq!(step.source_bits_examined, 2 * nodes as u64);
            assert_eq!(step.source_words_examined, 2 * nodes.div_ceil(64) as u64);
            assert_eq!(input, untouched, "input must be immutable");
            if expected_edges > 0 {
                assert!(graph.step(&input, expected_edges - 1).is_err());
                assert_eq!(input, untouched, "budget errors must be transactional");
            }
        }
    }
}

#[test]
fn packed_recurrent_cycles_match_dense_reference_through_word_boundaries() {
    let nodes = 129;
    let bytes = graph_fixture(nodes);
    let graph = V888CsrGraph::read(&bytes).unwrap();
    let mut expected = vec![false; nodes];
    expected[0] = true;
    expected[64] = true;
    expected[128] = true;
    let mut current = BooleanWorkspace::from_bits(&expected);
    for _ in 0..9 {
        let mut next = vec![false; nodes];
        for source in 0..nodes {
            if expected[source] {
                for edge in graph.offset(source)..graph.offset(source + 1) {
                    next[graph.target(edge)] = true;
                }
            }
        }
        let step = graph.step(&current, graph.edges as u64).unwrap();
        assert_eq!(step.output, BooleanWorkspace::from_bits(&next));
        current = step.output;
        expected = next;
    }
}
