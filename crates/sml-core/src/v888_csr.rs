//! Canonical bounded V8CSR002 Boolean recurrent-graph executor for SML model experiments.
//!
//! Reuses the exact validated CSR semantics previously confined to the diagnostic
//! binary. An OR transition is a synthetic research rule, not a biological
//! synapse or a trained neural operation. Contact multiplicities are only
//! accounted, never treated as learned weights.
//!
//! This implementation scans all source bits twice per step (see
//! `source_bits_examined`); it is NOT an event-sparse physical kernel, and its
//! existence does not qualify V888 as a useful language-model topology.
//! A consumer must pin graph provenance, compare matched topology controls,
//! and independently qualify quality and resource use before promotion.

use crate::{BooleanWorkspace, ExecutionCounters};
use std::io;

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

pub struct V888CsrGraph<'a> {
    bytes: &'a [u8],
    pub nodes: usize,
    pub edges: usize,
    pub contacts: u64,
    pub loops: u64,
}

impl<'a> V888CsrGraph<'a> {
    fn q(&self, offset: usize) -> u64 {
        u64::from_le_bytes(
            self.bytes[offset..offset + 8]
                .try_into()
                .expect("validated length"),
        )
    }

    pub fn offset(&self, node: usize) -> usize {
        self.q(24 + 8 * self.nodes + 8 * node) as usize
    }

    pub fn target(&self, edge: usize) -> usize {
        let offset = 32 + 16 * self.nodes + 4 * edge;
        u32::from_le_bytes(
            self.bytes[offset..offset + 4]
                .try_into()
                .expect("validated length"),
        ) as usize
    }

    pub fn count(&self, edge: usize) -> u64 {
        self.q(32 + 16 * self.nodes + 4 * self.edges + 8 * edge)
    }

    pub fn read(bytes: &'a [u8]) -> io::Result<Self> {
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

    pub fn step(&self, input: &BooleanWorkspace, max_edges: u64) -> io::Result<V888CsrStep> {
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
        Ok(V888CsrStep {
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

pub struct V888CsrStep {
    pub output: BooleanWorkspace,
    pub counters: ExecutionCounters,
    pub source_bits_examined: u64,
    pub active_sources: u64,
    pub edges: u64,
    pub contacts: u64,
    pub loops: u64,
}
