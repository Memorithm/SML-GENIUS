//! A6-E5: runtime choose direct native WideAnfGate vs bounded packed truth
//! table materialization, using caller-MEASURED costs and actual capacity.
//!
//! An ANF function bank with K addresses is not a function of K variables.
//! A single materialized d-input gate costs 2^d bits, and is rejected when
//! fan-in, RAM, L3 or amortization violates explicit caller-owned budgets.
//! The optional compiled table is 64-byte aligned by element alignment,
//! not merely by aligning the Vec header. No unsafe or NVIDIA SDK is used.

use crate::anf_wide_sparse::{WideAnfError, WideAnfGate};

pub const MAX_MATERIALIZED_FAN_IN: usize = 16;
pub const MAX_MATERIALIZATION_SOURCE_WIDTH: usize = 4096;
const BITS_PER_BLOCK: usize = 512;

#[repr(align(64))]
#[derive(Clone, Debug, PartialEq, Eq)]
struct AlignedTruthBlock([u64; 8]);

impl Default for AlignedTruthBlock {
    fn default() -> Self {
        Self([0; 8])
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExecChoice {
    Direct,
    Materialized,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlannerError {
    InvalidProfile,
    MemoryBudget,
    SourceTooShort,
    Native(WideAnfError),
}

impl std::fmt::Display for PlannerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for PlannerError {}

impl From<WideAnfError> for PlannerError {
    fn from(error: WideAnfError) -> Self {
        Self::Native(error)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MeasuredExecutionProfile {
    pub expected_queries: usize,
    pub direct_ns_per_query: u64,
    pub lookup_ns_per_query: u64,
    pub estimated_compile_ns: u64,
    /// Includes aligned payload *and* input-position metadata.
    pub max_materialization_ram_bytes: usize,
    pub max_materialization_l3_bytes: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExecutionDecision {
    pub choice: ExecChoice,
    pub predicted_table_bytes: usize,
    pub dense_bits: usize,
    pub predicted_direct_ns: u128,
    pub predicted_materialized_ns: u128,
}

fn checked_table_bytes(gate: &WideAnfGate) -> Option<(usize, usize)> {
    let width = gate.fan_in();
    if width == 0 || width > MAX_MATERIALIZED_FAN_IN {
        return None;
    }
    let addresses = 1usize.checked_shl(width as u32)?;
    let blocks = addresses.div_ceil(BITS_PER_BLOCK);
    let payload = blocks.checked_mul(std::mem::size_of::<AlignedTruthBlock>())?;
    let metadata = width.checked_mul(std::mem::size_of::<usize>())?;
    Some((addresses, payload.checked_add(metadata)?))
}

/// Choose only when the bounded table is expected to amortize:
/// Q * t_direct > t_compile + Q * t_lookup AND table <= both budgets.
/// The timing values are passed from measurements of the real target runner;
/// this function is not itself evidence of such measurements.
pub fn choose_execution(
    gate: &WideAnfGate,
    p: MeasuredExecutionProfile,
) -> Result<ExecutionDecision, PlannerError> {
    if p.expected_queries == 0
        || p.direct_ns_per_query == 0
        || p.lookup_ns_per_query == 0
        || p.max_materialization_ram_bytes == 0
        || p.max_materialization_l3_bytes == 0
    {
        return Err(PlannerError::InvalidProfile);
    }
    let (addresses, table_bytes) = checked_table_bytes(gate).unwrap_or((0, 0));
    let source_width = gate.input_positions().iter().copied().max().unwrap_or(0) + 1;
    let source_scratch_safe = source_width <= MAX_MATERIALIZATION_SOURCE_WIDTH
        && source_width <= p.max_materialization_ram_bytes;
    let n = p.expected_queries as u128;
    let direct = n * u128::from(p.direct_ns_per_query);
    let lookup = n * u128::from(p.lookup_ns_per_query);
    let total = lookup
        .checked_add(u128::from(p.estimated_compile_ns))
        .ok_or(PlannerError::InvalidProfile)?;
    let choice = if source_scratch_safe
        && table_bytes != 0
        && table_bytes <= p.max_materialization_ram_bytes
        && table_bytes <= p.max_materialization_l3_bytes
        && direct > total
    {
        ExecChoice::Materialized
    } else {
        ExecChoice::Direct
    };
    Ok(ExecutionDecision {
        choice,
        predicted_table_bytes: table_bytes,
        dense_bits: addresses,
        predicted_direct_ns: direct,
        predicted_materialized_ns: total,
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompiledTruthTable {
    input_positions: Vec<usize>,
    blocks: Vec<AlignedTruthBlock>,
    addresses: usize,
}

impl CompiledTruthTable {
    pub fn evaluate(&self, source: &[bool]) -> Result<bool, PlannerError> {
        if self.input_positions.iter().any(|&index| index >= source.len()) {
            return Err(PlannerError::SourceTooShort);
        }
        let address = self
            .input_positions
            .iter()
            .enumerate()
            .fold(0usize, |acc, (bit, &physical)| {
                acc | (usize::from(source[physical]) << bit)
            });
        if address >= self.addresses {
            return Err(PlannerError::SourceTooShort);
        }
        Ok(self.blocks[address / BITS_PER_BLOCK].0[(address % BITS_PER_BLOCK) / 64]
            & (1u64 << (address % 64))
            != 0)
    }

    pub fn logical_payload_bytes(&self) -> usize {
        self.blocks.len() * std::mem::size_of::<AlignedTruthBlock>()
            + self.input_positions.len() * std::mem::size_of::<usize>()
    }

    pub fn allocated_payload_bytes(&self) -> usize {
        self.blocks.capacity() * std::mem::size_of::<AlignedTruthBlock>()
            + self.input_positions.capacity() * std::mem::size_of::<usize>()
    }

    pub fn truth_address_count(&self) -> usize {
        self.addresses
    }

    pub fn actual_truth_alignment(&self) -> usize {
        (self.blocks.as_ptr() as usize) % 64
    }
}

/// Compile only if the *measured input profile* chooses materialization.
/// The caller retains the native gate for DIRECT cases. The allocation
/// checks are conservative; the actual vector capacities are rechecked.
pub fn materialize_if_profitable(
    gate: &WideAnfGate,
    profile: MeasuredExecutionProfile,
) -> Result<(ExecutionDecision, Option<CompiledTruthTable>), PlannerError> {
    let decision = choose_execution(gate, profile)?;
    if decision.choice == ExecChoice::Direct {
        return Ok((decision, None));
    }
    let input_positions = gate.input_positions().to_vec();
    let width = input_positions.len();
    let addresses = 1usize << width;
    let mut blocks = vec![AlignedTruthBlock::default(); addresses.div_ceil(BITS_PER_BLOCK)];
    let source_width = input_positions
        .iter()
        .copied()
        .max()
        .ok_or(PlannerError::InvalidProfile)?
        + 1;
    let mut source = vec![false; source_width];
    for address in 0..addresses {
        for (bit, &physical) in input_positions.iter().enumerate() {
            source[physical] = address & (1usize << bit) != 0;
        }
        if gate.evaluate_scalar(&source)? {
            blocks[address / BITS_PER_BLOCK].0[(address % BITS_PER_BLOCK) / 64] |=
                1u64 << (address % 64);
        }
    }
    let result = CompiledTruthTable {
        input_positions,
        blocks,
        addresses,
    };
    if result.allocated_payload_bytes() > profile.max_materialization_ram_bytes
        || result.allocated_payload_bytes() > profile.max_materialization_l3_bytes
        || result.actual_truth_alignment() != 0
    {
        return Err(PlannerError::MemoryBudget);
    }
    Ok((decision, Some(result)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anf_wide_sparse::WideAnfLimits;

    fn test_gate() -> WideAnfGate {
        WideAnfGate::from_monomials(
            vec![0, 2, 4],
            vec![vec![0], vec![1, 2]],
            5,
            WideAnfLimits::new(5, 32, 256, 256, 256),
        )
        .unwrap()
    }

    fn profile() -> MeasuredExecutionProfile {
        MeasuredExecutionProfile {
            expected_queries: 10_000,
            direct_ns_per_query: 200,
            lookup_ns_per_query: 2,
            estimated_compile_ns: 2000,
            max_materialization_ram_bytes: 4096,
            max_materialization_l3_bytes: 4096,
        }
    }

    #[test]
    fn native_and_materialized_truth_are_bit_exact_with_noncontiguous_inputs() {
        let gate = test_gate();
        let (plan, compiled) = materialize_if_profitable(&gate, profile()).unwrap();
        assert_eq!(plan.choice, ExecChoice::Materialized);
        let compiled = compiled.unwrap();
        assert_eq!(compiled.truth_address_count(), 8);
        assert_eq!(compiled.actual_truth_alignment(), 0);
        assert_eq!(compiled.logical_payload_bytes(), plan.predicted_table_bytes);
        assert!(compiled.allocated_payload_bytes() <= profile().max_materialization_ram_bytes);
        for input in 0usize..32 {
            let source = (0..5).map(|b| input & (1usize << b) != 0).collect::<Vec<_>>();
            assert_eq!(
                compiled.evaluate(&source).unwrap(),
                gate.evaluate_scalar(&source).unwrap()
            );
        }
        assert_eq!(compiled.evaluate(&[true]), Err(PlannerError::SourceTooShort));
    }

    #[test]
    fn rejects_when_not_amortized_or_budget_exceeds_l3() {
        let gate = test_gate();
        let mut p = profile();
        p.expected_queries = 1;
        assert_eq!(choose_execution(&gate, p).unwrap().choice, ExecChoice::Direct);
        assert!(materialize_if_profitable(&gate, p).unwrap().1.is_none());
        p = profile();
        p.max_materialization_l3_bytes = 32;
        assert_eq!(choose_execution(&gate, p).unwrap().choice, ExecChoice::Direct);
        p.max_materialization_l3_bytes = 4096;
        p.direct_ns_per_query = 1;
        p.lookup_ns_per_query = 2;
        assert_eq!(choose_execution(&gate, p).unwrap().choice, ExecChoice::Direct);
    }

    #[test]
    fn never_materializes_exponential_truth_for_large_fan_in() {
        let gate = WideAnfGate::from_monomials(
            (0..17).collect(),
            vec![vec![0], vec![16]],
            17,
            WideAnfLimits::new(32, 64, 1024, 1024, 1024),
        )
        .unwrap();
        let (plan, table) = materialize_if_profitable(&gate, profile()).unwrap();
        assert_eq!(plan.choice, ExecChoice::Direct);
        assert!(table.is_none());
    }

    #[test]
    fn malformed_execution_profiles_fail_closed() {
        let gate = test_gate();
        let mut p = profile();
        p.expected_queries = 0;
        assert_eq!(choose_execution(&gate, p), Err(PlannerError::InvalidProfile));
        p = profile();
        p.lookup_ns_per_query = 0;
        assert_eq!(choose_execution(&gate, p), Err(PlannerError::InvalidProfile));
    }
}
