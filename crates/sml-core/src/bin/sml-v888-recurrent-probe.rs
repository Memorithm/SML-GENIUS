//! Experimental V8CSR002 consumer using SML's workspace and direct router.
//! Boolean OR recurrence is a declared synthetic rule; contacts are accounting only.
use sml_core::{BooleanWorkspace, DirectRouteTable, ExecutionCounters, PageId};
use std::io;

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

struct Graph<'a> {
    bytes: &'a [u8],
    nodes: usize,
    edges: usize,
    contacts: u64,
    loops: u64,
}

impl<'a> Graph<'a> {
    fn q(&self, offset: usize) -> u64 {
        u64::from_le_bytes(
            self.bytes[offset..offset + 8]
                .try_into()
                .expect("validated length"),
        )
    }

    fn offset(&self, node: usize) -> usize {
        self.q(24 + 8 * self.nodes + 8 * node) as usize
    }

    fn target(&self, edge: usize) -> usize {
        let offset = 32 + 16 * self.nodes + 4 * edge;
        u32::from_le_bytes(
            self.bytes[offset..offset + 4]
                .try_into()
                .expect("validated length"),
        ) as usize
    }

    fn count(&self, edge: usize) -> u64 {
        self.q(32 + 16 * self.nodes + 4 * self.edges + 8 * edge)
    }

    fn read(bytes: &'a [u8]) -> io::Result<Self> {
        if bytes.len() < 24 || &bytes[..8] != b"V8CSR002" {
            return Err(invalid("invalid recurrent CSR version/header"));
        }
        let q = |offset: usize| {
            u64::from_le_bytes(bytes[offset..offset + 8].try_into().expect("header length"))
        };
        let n = q(8);
        let e = q(16);
        if n == 0 || n > 1_000_000 || e > 20_000_000 || bytes.len() as u64 != 32 + 16 * n + 12 * e {
            return Err(invalid("recurrent CSR dimensions/length"));
        }
        let mut graph = Self {
            bytes,
            nodes: n as usize,
            edges: e as usize,
            contacts: 0,
            loops: 0,
        };
        let mut previous_id = 0;
        for node in 0..graph.nodes {
            let id = graph.q(24 + 8 * node);
            if id <= previous_id {
                return Err(invalid("zero/duplicate/unsorted root ID"));
            }
            previous_id = id;
        }
        let mut previous_offset = 0;
        for node in 0..=graph.nodes {
            let offset = graph.q(24 + 8 * graph.nodes + 8 * node);
            if offset < previous_offset
                || offset > e
                || (node == 0 && offset != 0)
                || (node == graph.nodes && offset != e)
            {
                return Err(invalid("invalid CSR offsets"));
            }
            previous_offset = offset;
        }
        for source in 0..graph.nodes {
            let mut previous = None;
            for edge in graph.offset(source)..graph.offset(source + 1) {
                let target = graph.target(edge);
                let count = graph.count(edge);
                if target >= graph.nodes
                    || previous.is_some_and(|value| value >= target)
                    || count == 0
                {
                    return Err(invalid("invalid target order/range/contact count"));
                }
                graph.contacts = graph
                    .contacts
                    .checked_add(count)
                    .ok_or_else(|| invalid("contact overflow"))?;
                graph.loops += u64::from(source == target);
                previous = Some(target);
            }
        }
        Ok(graph)
    }

    fn step(&self, input: &BooleanWorkspace, max_edges: u64) -> io::Result<Step> {
        if input.len() != self.nodes {
            return Err(invalid("workspace shape"));
        }
        // Admission before execution; count this full source-bit scan explicitly.
        let mut admitted = 0_u64;
        let mut active_sources = 0_u64;
        for source in 0..self.nodes {
            if input.get(source) == Some(true) {
                active_sources += 1;
                admitted += (self.offset(source + 1) - self.offset(source)) as u64;
            }
        }
        if admitted > max_edges {
            return Err(invalid("active edge budget exceeded"));
        }
        let mut bits = vec![false; self.nodes];
        let mut contacts = 0_u64;
        let mut loops = 0_u64;
        for source in 0..self.nodes {
            if input.get(source) != Some(true) {
                continue;
            }
            for edge in self.offset(source)..self.offset(source + 1) {
                let target = self.target(edge);
                bits[target] = true;
                contacts += self.count(edge);
                loops += u64::from(source == target);
            }
        }
        Ok(Step {
            output: BooleanWorkspace::from_bits(&bits),
            counters: ExecutionCounters {
                boolean_ops: admitted,
                recurrent_cycles: 1,
                ..ExecutionCounters::default()
            },
            source_bits_examined: 2 * self.nodes as u64,
            active_sources,
            edges: admitted,
            contacts,
            loops,
        })
    }
}

struct Step {
    output: BooleanWorkspace,
    counters: ExecutionCounters,
    source_bits_examined: u64,
    active_sources: u64,
    edges: u64,
    contacts: u64,
    loops: u64,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() != 1 {
        return Err("usage: sml-v888-recurrent-probe GRAPH.csr".into());
    }
    let mut file = std::fs::File::open(&args[0])?;
    let size = file.metadata()?.len();
    if size > 256 * 1024 * 1024 {
        return Err("probe input byte budget exceeded".into());
    }
    let mut bytes = Vec::new();
    std::io::Read::read_to_end(&mut std::io::Read::take(&mut file, size + 1), &mut bytes)?;
    if bytes.len() as u64 != size {
        return Err("input changed during read".into());
    }
    let graph = Graph::read(&bytes)?;
    let source = (0..graph.nodes)
        .filter(|&node| {
            (graph.offset(node)..graph.offset(node + 1)).any(|edge| graph.target(edge) == node)
        })
        .max_by_key(|&node| graph.offset(node + 1) - graph.offset(node))
        .ok_or("no self loop for recurrence probe")?;
    let observed = (graph.offset(source)..graph.offset(source + 1))
        .map(|edge| graph.target(edge))
        .find(|&target| target != source)
        .ok_or("no non-self connection for probe")?;
    let mut stimulus = vec![false; graph.nodes];
    stimulus[source] = true;
    let input = BooleanWorkspace::from_bits(&stimulus);
    let first = graph.step(&input, 20_000_000)?;
    let second = graph.step(&first.output, 20_000_000)?;
    if first.output.get(source) != Some(true)
        || second.output.get(source) != Some(true)
        || first.output.get(observed) != Some(true)
        || second.output.get(observed) != Some(true)
        || first.edges < 2
    {
        return Err("self-loop recurrence lost".into());
    }
    let router = DirectRouteTable::new(1, vec![PageId(0), PageId(1)])?;
    let route_input = BooleanWorkspace::from_bits(&[second.output.get(observed).unwrap_or(false)]);
    let mut counters = second.counters;
    let page = router.route(&route_input, &mut counters)?;
    if page != PageId(1) || counters.route_lookups != 1 {
        return Err("SML direct router coupling failed".into());
    }
    println!("schema=sml-v888-recurrent-probe/v1");
    println!(
        "nodes={} edges={} contacts={} self_loops={}",
        graph.nodes, graph.edges, graph.contacts, graph.loops
    );
    println!(
        "step1_edges={} step2_edges={} step2_contacts={} step2_loops={}",
        first.edges, second.edges, second.contacts, second.loops
    );
    println!(
        "step2_source_bits_examined={} step2_active_sources={} route_lookups={}",
        second.source_bits_examined, second.active_sources, counters.route_lookups
    );
    println!("SML_V888_RECURRENT_CONSUMER=PASS");
    println!("SML_V888_MODEL_QUALITY=NOT_MEASURED");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Vec<u8> {
        let mut bytes = b"V8CSR002".to_vec();
        for value in [3_u64, 4, 10, 20, 30, 0, 2, 3, 4] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        for target in [0_u32, 1, 2, 1] {
            bytes.extend_from_slice(&target.to_le_bytes());
        }
        for count in [2_u64, 3, 5, 7] {
            bytes.extend_from_slice(&count.to_le_bytes());
        }
        bytes
    }

    #[test]
    fn all_states_equal_independent_dense_oracle_with_no_same_step_cascade() {
        let bytes = fixture();
        let graph = Graph::read(&bytes).unwrap();
        let matrix = [
            [true, true, false],
            [false, false, true],
            [false, true, false],
        ];
        let counts = [2_u64, 1, 1];
        for mask in 0..8 {
            let bits: Vec<bool> = (0..3).map(|node| mask & (1 << node) != 0).collect();
            let result = graph.step(&BooleanWorkspace::from_bits(&bits), 4).unwrap();
            let expected: Vec<bool> = (0..3)
                .map(|target| (0..3).any(|source| bits[source] && matrix[source][target]))
                .collect();
            assert_eq!(result.output, BooleanWorkspace::from_bits(&expected));
            let edges: u64 = (0..3)
                .filter(|&node| bits[node])
                .map(|node| counts[node])
                .sum();
            assert_eq!(result.edges, edges);
            assert_eq!(result.counters.boolean_ops, edges);
            assert_eq!(result.counters.pairwise_comparisons, 0);
            assert_eq!(result.counters.recurrent_cycles, 1);
            assert_eq!(result.source_bits_examined, 6);
            assert_eq!(result.active_sources, u64::from((mask as u32).count_ones()));
            let expected_contacts = [5_u64, 5, 7]
                .into_iter()
                .enumerate()
                .filter(|(node, _)| bits[*node])
                .map(|(_, contacts)| contacts)
                .sum();
            assert_eq!(result.contacts, expected_contacts);
            assert_eq!(result.loops, u64::from(bits[0]));
        }
    }

    #[test]
    fn structural_corruption_and_contact_overflow_fail_closed() {
        fn put_u64(bytes: &mut [u8], offset: usize, value: u64) {
            bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
        }

        fn put_u32(bytes: &mut [u8], offset: usize, value: u32) {
            bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }

        let original = fixture();
        let mut corruptions = Vec::new();

        let mut zero_root = original.clone();
        put_u64(&mut zero_root, 24, 0);
        corruptions.push(zero_root);

        let mut duplicate_root = original.clone();
        put_u64(&mut duplicate_root, 32, 10);
        corruptions.push(duplicate_root);

        let mut unsorted_root = original.clone();
        put_u64(&mut unsorted_root, 40, 15);
        corruptions.push(unsorted_root);

        for (offset, value) in [(48, 1), (56, 5), (64, 1), (72, 3)] {
            let mut bad_offsets = original.clone();
            put_u64(&mut bad_offsets, offset, value);
            corruptions.push(bad_offsets);
        }

        let mut out_of_range_target = original.clone();
        put_u32(&mut out_of_range_target, 80, 3);
        corruptions.push(out_of_range_target);

        let mut duplicate_target = original.clone();
        put_u32(&mut duplicate_target, 84, 0);
        corruptions.push(duplicate_target);

        let mut zero_contacts = original.clone();
        put_u64(&mut zero_contacts, 96, 0);
        corruptions.push(zero_contacts);

        let mut overflowing_contacts = original.clone();
        put_u64(&mut overflowing_contacts, 96, u64::MAX);
        corruptions.push(overflowing_contacts);

        for (case, bytes) in corruptions.iter().enumerate() {
            assert!(
                Graph::read(bytes).is_err(),
                "malformed CSR case {case} was admitted"
            );
        }
    }

    #[test]
    fn empty_edge_graph_accepts_zero_budget_without_activation() {
        let mut bytes = b"V8CSR002".to_vec();
        for value in [1_u64, 0, 7, 0, 0] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        let graph = Graph::read(&bytes).unwrap();
        assert_eq!(graph.contacts, 0);
        assert_eq!(graph.loops, 0);

        let step = graph
            .step(&BooleanWorkspace::from_bits(&[true]), 0)
            .unwrap();
        assert_eq!(step.output, BooleanWorkspace::from_bits(&[false]));
        assert_eq!(step.active_sources, 1);
        assert_eq!(step.source_bits_examined, 2);
        assert_eq!(step.edges, 0);
        assert_eq!(step.contacts, 0);
        assert_eq!(step.loops, 0);
        assert_eq!(step.counters.boolean_ops, 0);
        assert_eq!(step.counters.recurrent_cycles, 1);
    }

    #[test]
    fn exhaustive_three_node_csr_matches_dense_oracle_and_exact_budget() {
        const N: usize = 3;
        for adjacency in 0_u16..(1_u16 << (N * N)) {
            let mut edges = Vec::new();
            for source in 0..N {
                for target in 0..N {
                    if adjacency & (1 << (source * N + target)) != 0 {
                        edges.push((source, target));
                    }
                }
            }

            let mut bytes = b"V8CSR002".to_vec();
            bytes.extend_from_slice(&(N as u64).to_le_bytes());
            bytes.extend_from_slice(&(edges.len() as u64).to_le_bytes());
            for root in 0..N {
                bytes.extend_from_slice(&(10 + root as u64).to_le_bytes());
            }
            let mut offset = 0_u64;
            bytes.extend_from_slice(&offset.to_le_bytes());
            for source in 0..N {
                offset += edges.iter().filter(|(from, _)| *from == source).count() as u64;
                bytes.extend_from_slice(&offset.to_le_bytes());
            }
            for &(_, target) in &edges {
                bytes.extend_from_slice(&(target as u32).to_le_bytes());
            }
            for &(source, target) in &edges {
                let contacts = 1 + (source * N + target) as u64;
                bytes.extend_from_slice(&contacts.to_le_bytes());
            }

            let graph = Graph::read(&bytes).unwrap();
            assert_eq!(graph.edges, edges.len());
            assert_eq!(
                graph.contacts,
                edges
                    .iter()
                    .map(|&(source, target)| 1 + (source * N + target) as u64)
                    .sum::<u64>()
            );
            assert_eq!(
                graph.loops,
                edges
                    .iter()
                    .filter(|(source, target)| source == target)
                    .count() as u64
            );

            for stimulus in 0_u8..(1_u8 << N) {
                let input_bits: Vec<bool> =
                    (0..N).map(|node| stimulus & (1 << node) != 0).collect();
                let active_edges: Vec<(usize, usize)> = edges
                    .iter()
                    .copied()
                    .filter(|(source, _)| input_bits[*source])
                    .collect();
                let expected: Vec<bool> = (0..N)
                    .map(|target| active_edges.iter().any(|&(_, to)| to == target))
                    .collect();
                let input = BooleanWorkspace::from_bits(&input_bits);
                let budget = active_edges.len() as u64;
                let step = graph.step(&input, budget).unwrap();

                assert_eq!(
                    step.output,
                    BooleanWorkspace::from_bits(&expected),
                    "adjacency={adjacency} stimulus={stimulus}"
                );
                assert_eq!(step.edges, budget);
                assert_eq!(
                    step.contacts,
                    active_edges
                        .iter()
                        .map(|&(source, target)| 1 + (source * N + target) as u64)
                        .sum::<u64>()
                );
                assert_eq!(
                    step.loops,
                    active_edges
                        .iter()
                        .filter(|(source, target)| source == target)
                        .count() as u64
                );
                assert_eq!(
                    step.active_sources,
                    input_bits.iter().filter(|&&active| active).count() as u64
                );
                assert_eq!(step.source_bits_examined, (2 * N) as u64);
                assert_eq!(step.counters.boolean_ops, budget);
                assert_eq!(step.counters.recurrent_cycles, 1);
                assert_eq!(step.counters.pairwise_comparisons, 0);
                if budget > 0 {
                    assert!(graph.step(&input, budget - 1).is_err());
                }

                // Reuse the same parsed graph for three more synchronous cycles.
                // The oracle uses the dense adjacency mask, not the CSR reader.
                let mut state = expected;
                let mut workspace = step.output;
                for cycle in 2..=4 {
                    let next_expected: Vec<bool> = (0..N)
                        .map(|target| {
                            (0..N).any(|source| {
                                state[source] && adjacency & (1 << (source * N + target)) != 0
                            })
                        })
                        .collect();
                    let active_count =
                        edges.iter().filter(|(source, _)| state[*source]).count() as u64;
                    let next = graph.step(&workspace, active_count).unwrap();
                    assert_eq!(
                        next.output,
                        BooleanWorkspace::from_bits(&next_expected),
                        "adjacency={adjacency} stimulus={stimulus} cycle={cycle}"
                    );
                    assert_eq!(next.edges, active_count);
                    assert_eq!(next.counters.boolean_ops, active_count);
                    assert_eq!(next.counters.recurrent_cycles, 1);
                    assert_eq!(workspace, BooleanWorkspace::from_bits(&state));
                    state = next_expected;
                    workspace = next.output;
                }
            }
        }
    }

    #[test]
    fn malformed_inputs_and_work_budget_fail_closed() {
        let bytes = fixture();
        for length in 0..bytes.len() {
            assert!(Graph::read(&bytes[..length]).is_err());
        }
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert!(Graph::read(&trailing).is_err());
        let mut wrong_version = bytes.clone();
        wrong_version[7] = b'1';
        assert!(Graph::read(&wrong_version).is_err());
        let graph = Graph::read(&bytes).unwrap();
        let input = BooleanWorkspace::from_bits(&[true, false, false]);
        assert!(graph.step(&input, 1).is_err());
        assert!(graph.step(&BooleanWorkspace::from_bits(&[]), 4).is_err());
        let zero = graph
            .step(&BooleanWorkspace::from_bits(&[false; 3]), 0)
            .unwrap();
        assert_eq!(zero.edges, 0);
        assert_eq!(zero.output, BooleanWorkspace::from_bits(&[false; 3]));
    }
}
