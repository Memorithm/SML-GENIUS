//! Independent destination-side requalification for Forge SML topology candidates.
//!
//! This module intentionally does not depend on Forge. It parses the versioned
//! textual candidate contract and re-executes the full Boolean oracle locally.

use core::fmt;

use crate::train::TruthTable2;

pub const FORGE_SML_TOPOLOGY_CONTRACT_V1: &str = "sml-topology/v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ForgeGateV1 {
    pub left: u16,
    pub right: u16,
    pub truth_table: TruthTable2,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForgeTopologyCandidateV1 {
    pub gates: Vec<ForgeGateV1>,
    pub output: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TopologyRequalification {
    pub rows: u64,
    pub incorrect_rows: u64,
    pub reachable_gates: u64,
    pub learned_boolean_bits: u64,
    pub wiring_metadata_bits: u64,
}

impl TopologyRequalification {
    #[must_use]
    pub const fn exact(&self) -> bool {
        self.incorrect_rows == 0
    }
}

impl ForgeTopologyCandidateV1 {
    /// Strict parser for Forge's `sml-topology/v1` candidate representation.
    pub fn parse(repr: &str) -> Result<Self, ForgeTopologyError> {
        let mut fields = repr.split('|');
        match fields.next() {
            Some(FORGE_SML_TOPOLOGY_CONTRACT_V1) => {}
            Some(_) => return Err(ForgeTopologyError::ContractVersion),
            None => return Err(ForgeTopologyError::ContractVersion),
        }

        let mut gates = Vec::new();
        let mut output = None;
        for field in fields {
            if let Some(rest) = field.strip_prefix('g') {
                if output.is_some() {
                    return Err(ForgeTopologyError::GateAfterOutput);
                }
                let (index_text, payload) = rest
                    .split_once('=')
                    .ok_or(ForgeTopologyError::GateSyntax)?;
                let index = parse_usize(index_text)?;
                if index != gates.len() {
                    return Err(ForgeTopologyError::GateIndex {
                        expected: gates.len(),
                        actual: index,
                    });
                }
                let mut values = payload.split(',');
                let left = parse_u16(values.next().ok_or(ForgeTopologyError::GateSyntax)?)?;
                let right = parse_u16(values.next().ok_or(ForgeTopologyError::GateSyntax)?)?;
                let table_code = parse_u8(values.next().ok_or(ForgeTopologyError::GateSyntax)?)?;
                if values.next().is_some() {
                    return Err(ForgeTopologyError::GateSyntax);
                }
                gates.push(ForgeGateV1 {
                    left,
                    right,
                    truth_table: TruthTable2::new(table_code)
                        .map_err(|_| ForgeTopologyError::TruthTableCode(table_code))?,
                });
                continue;
            }

            if let Some(value) = field.strip_prefix("out=") {
                if output.is_some() {
                    return Err(ForgeTopologyError::DuplicateOutput);
                }
                output = Some(parse_u16(value)?);
                continue;
            }

            return Err(ForgeTopologyError::UnknownField);
        }

        let output = output.ok_or(ForgeTopologyError::MissingOutput)?;
        if gates.is_empty() {
            return Err(ForgeTopologyError::ZeroGates);
        }
        Ok(Self { gates, output })
    }

    /// Revalidates structure and evaluates every row of the caller-owned oracle.
    pub fn requalify(
        &self,
        input_count: u8,
        max_gates: u8,
        target_truth_table: &[bool],
    ) -> Result<TopologyRequalification, ForgeTopologyError> {
        self.validate(input_count, max_gates)?;

        let expected_rows = 1_usize
            .checked_shl(input_count as u32)
            .ok_or(ForgeTopologyError::InputCount(input_count))?;
        if target_truth_table.len() != expected_rows {
            return Err(ForgeTopologyError::TruthTableLength {
                expected: expected_rows,
                actual: target_truth_table.len(),
            });
        }

        let mut incorrect_rows = 0_u64;
        for row in 0..expected_rows {
            if self.evaluate_row(input_count, row)? != target_truth_table[row] {
                incorrect_rows = incorrect_rows.saturating_add(1);
            }
        }

        let reachable_gates = self.reachable_gate_count(input_count)?;
        let reference_bits = bits_required(input_count as usize + self.gates.len());
        Ok(TopologyRequalification {
            rows: expected_rows as u64,
            incorrect_rows,
            reachable_gates: reachable_gates as u64,
            learned_boolean_bits: reachable_gates as u64 * 4,
            wiring_metadata_bits: reachable_gates as u64 * 2 * reference_bits as u64
                + reference_bits as u64,
        })
    }

    pub fn validate(&self, input_count: u8, max_gates: u8) -> Result<(), ForgeTopologyError> {
        if !(2..=8).contains(&input_count) {
            return Err(ForgeTopologyError::InputCount(input_count));
        }
        if max_gates == 0 || max_gates > 64 {
            return Err(ForgeTopologyError::MaxGates(max_gates));
        }
        if self.gates.len() != max_gates as usize {
            return Err(ForgeTopologyError::GateCount {
                expected: max_gates as usize,
                actual: self.gates.len(),
            });
        }

        for (index, gate) in self.gates.iter().enumerate() {
            let source_limit = input_count as usize + index;
            for source in [gate.left, gate.right] {
                if source as usize >= source_limit {
                    return Err(ForgeTopologyError::ForwardOrInvalidSource {
                        gate: index,
                        source,
                        limit: source_limit,
                    });
                }
            }
        }

        let output_limit = input_count as usize + self.gates.len();
        if self.output as usize >= output_limit {
            return Err(ForgeTopologyError::OutputSource {
                source: self.output,
                limit: output_limit,
            });
        }
        Ok(())
    }

    fn evaluate_row(&self, input_count: u8, row: usize) -> Result<bool, ForgeTopologyError> {
        let mut gate_values = Vec::with_capacity(self.gates.len());
        for (index, gate) in self.gates.iter().enumerate() {
            let left = resolve_signal(gate.left, input_count, row, &gate_values, index)?;
            let right = resolve_signal(gate.right, input_count, row, &gate_values, index)?;
            gate_values.push(gate.truth_table.evaluate(left, right));
        }
        resolve_signal(
            self.output,
            input_count,
            row,
            &gate_values,
            self.gates.len(),
        )
    }

    fn reachable_gate_count(&self, input_count: u8) -> Result<usize, ForgeTopologyError> {
        let mut reachable = vec![false; self.gates.len()];
        let mut stack = vec![self.output];

        while let Some(source) = stack.pop() {
            if source < input_count as u16 {
                continue;
            }
            let gate_index = source as usize - input_count as usize;
            let gate = self
                .gates
                .get(gate_index)
                .ok_or(ForgeTopologyError::GateValue(gate_index))?;
            if reachable[gate_index] {
                continue;
            }
            reachable[gate_index] = true;
            stack.push(gate.left);
            stack.push(gate.right);
        }
        Ok(reachable.into_iter().filter(|value| *value).count())
    }
}

fn resolve_signal(
    source: u16,
    input_count: u8,
    row: usize,
    gate_values: &[bool],
    current_gate: usize,
) -> Result<bool, ForgeTopologyError> {
    if source < input_count as u16 {
        return Ok(row & (1_usize << source) != 0);
    }

    let gate_index = source as usize - input_count as usize;
    if gate_index >= current_gate {
        return Err(ForgeTopologyError::ForwardOrInvalidSource {
            gate: current_gate,
            source,
            limit: input_count as usize + current_gate,
        });
    }
    gate_values
        .get(gate_index)
        .copied()
        .ok_or(ForgeTopologyError::GateValue(gate_index))
}

fn bits_required(cardinality: usize) -> u32 {
    if cardinality <= 1 {
        1
    } else {
        usize::BITS - (cardinality - 1).leading_zeros()
    }
}

fn parse_usize(value: &str) -> Result<usize, ForgeTopologyError> {
    reject_noncanonical_integer(value)?;
    value
        .parse()
        .map_err(|_| ForgeTopologyError::InvalidInteger)
}

fn parse_u16(value: &str) -> Result<u16, ForgeTopologyError> {
    reject_noncanonical_integer(value)?;
    value
        .parse()
        .map_err(|_| ForgeTopologyError::InvalidInteger)
}

fn parse_u8(value: &str) -> Result<u8, ForgeTopologyError> {
    reject_noncanonical_integer(value)?;
    value
        .parse()
        .map_err(|_| ForgeTopologyError::InvalidInteger)
}

fn reject_noncanonical_integer(value: &str) -> Result<(), ForgeTopologyError> {
    if value.is_empty()
        || !value.bytes().all(|byte| byte.is_ascii_digit())
        || (value.len() > 1 && value.starts_with('0'))
    {
        return Err(ForgeTopologyError::InvalidInteger);
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForgeTopologyError {
    ContractVersion,
    UnknownField,
    GateAfterOutput,
    GateSyntax,
    GateIndex { expected: usize, actual: usize },
    DuplicateOutput,
    MissingOutput,
    ZeroGates,
    InvalidInteger,
    TruthTableCode(u8),
    InputCount(u8),
    MaxGates(u8),
    GateCount { expected: usize, actual: usize },
    ForwardOrInvalidSource { gate: usize, source: u16, limit: usize },
    OutputSource { source: u16, limit: usize },
    GateValue(usize),
    TruthTableLength { expected: usize, actual: usize },
}

impl fmt::Display for ForgeTopologyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for ForgeTopologyError {}
