//! Explicit, model-owned ANF input/output projection for SBG.
//! Coordinates and domains are declared wiring, not learned representations.
use crate::{
    pvp6_executor::Pvp6BatchResult,
    semantic::{SemanticBitFieldError, SemanticState},
};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SbgField {
    Value,
    Meaning,
    Ambiguity,
    Routing,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct SbgCoordinate {
    pub field: SbgField,
    pub bit: usize,
}
impl SbgCoordinate {
    pub fn read<const S: usize>(self, state: &SemanticState<S>) -> Result<bool, SbgAnfError> {
        let field = match self.field {
            SbgField::Value => &state.value,
            SbgField::Meaning => &state.meaning_mask,
            SbgField::Ambiguity => &state.ambiguity,
            SbgField::Routing => &state.routing,
        };
        field.get(self.bit).map_err(SbgAnfError::Field)
    }
    pub fn write<const S: usize>(
        self,
        state: &mut SemanticState<S>,
        value: bool,
    ) -> Result<(), SbgAnfError> {
        let field = match self.field {
            SbgField::Value => &mut state.value,
            SbgField::Meaning => &mut state.meaning_mask,
            SbgField::Ambiguity => &mut state.ambiguity,
            SbgField::Routing => &mut state.routing,
        };
        field.set(self.bit, value).map_err(SbgAnfError::Field)
    }
}
/// Each lane projects log2(D) source coordinates to an ANF address.
/// Destinations are query-major: lane * gates + gate. Writes are simultaneous.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SbgAnfPlan<const S: usize, const D: usize> {
    inputs: Vec<Vec<SbgCoordinate>>,
    outputs: Vec<SbgCoordinate>,
    gates: usize,
}
impl<const S: usize, const D: usize> SbgAnfPlan<S, D> {
    pub fn new(
        inputs: Vec<Vec<SbgCoordinate>>,
        outputs: Vec<SbgCoordinate>,
    ) -> Result<Self, SbgAnfError> {
        // The current gate mask carrier is u16. Larger domains need a new carrier.
        if S == 0 || D < 64 || !D.is_power_of_two() || D > 65536 {
            return Err(SbgAnfError::InvalidDomain);
        }
        if inputs.is_empty() || outputs.is_empty() || outputs.len() % inputs.len() != 0 {
            return Err(SbgAnfError::Shape);
        }
        for lane in &inputs {
            if lane.len() != D.trailing_zeros() as usize {
                return Err(SbgAnfError::Shape);
            }
            let mut unique = BTreeSet::new();
            for coordinate in lane {
                if coordinate.bit >= S {
                    return Err(SbgAnfError::Coordinate);
                }
                if !unique.insert(*coordinate) {
                    return Err(SbgAnfError::DuplicateInput);
                }
            }
        }
        let mut unique = BTreeSet::new();
        for coordinate in &outputs {
            if coordinate.bit >= S {
                return Err(SbgAnfError::Coordinate);
            }
            if !unique.insert(*coordinate) {
                return Err(SbgAnfError::DuplicateOutput);
            }
        }
        let gates = outputs.len() / inputs.len();
        Ok(Self {
            inputs,
            outputs,
            gates,
        })
    }
    #[must_use]
    pub fn input_lanes(&self) -> &[Vec<SbgCoordinate>] {
        &self.inputs
    }
    #[must_use]
    pub fn destinations(&self) -> &[SbgCoordinate] {
        &self.outputs
    }
    #[must_use]
    pub fn gate_count(&self) -> usize {
        self.gates
    }
    pub fn queries(&self, state: &SemanticState<S>) -> Result<Vec<usize>, SbgAnfError> {
        self.inputs
            .iter()
            .map(|lane| {
                lane.iter()
                    .enumerate()
                    .try_fold(0_usize, |address, (bit, coordinate)| {
                        Ok(address | (usize::from(coordinate.read(state)?) << bit))
                    })
            })
            .collect()
    }
    pub fn apply(
        &self,
        state: &SemanticState<S>,
        batch: &Pvp6BatchResult,
    ) -> Result<SemanticState<S>, SbgAnfError> {
        let mut next = state.clone();
        self.apply_into(&mut next, batch)?;
        Ok(next)
    }
    /// Writes every destination of `batch` into `target`; the shape is checked
    /// before any write. Gate-major packed words are read without per-bit lookups.
    pub fn apply_into(
        &self,
        target: &mut SemanticState<S>,
        batch: &Pvp6BatchResult,
    ) -> Result<(), SbgAnfError> {
        if batch.gate_count() != self.gates || batch.query_count() != self.inputs.len() {
            return Err(SbgAnfError::Shape);
        }
        let words_per_gate = batch.query_count().div_ceil(64);
        let packed = batch.packed_words();
        for gate in 0..self.gates {
            let row = &packed[gate * words_per_gate..(gate + 1) * words_per_gate];
            for (query, coordinate) in self.outputs[gate..].iter().step_by(self.gates).enumerate() {
                let value = row[query / 64] >> (query % 64) & 1 == 1;
                coordinate.write(target, value)?;
            }
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SbgAnfError {
    InvalidDomain,
    Shape,
    Coordinate,
    DuplicateInput,
    DuplicateOutput,
    Field(SemanticBitFieldError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        pascal_vector_projection::{Pvp6PopulationProfile, Pvp6Workload},
        pvp6_executor::{Pvp6Bank, Pvp6Gate},
    };
    fn lane() -> Vec<SbgCoordinate> {
        (0..6)
            .map(|bit| SbgCoordinate {
                field: SbgField::Value,
                bit,
            })
            .collect()
    }
    #[test]
    fn projection_preserves_all_unwritten_fields_and_padding() {
        let outputs = [
            SbgField::Value,
            SbgField::Meaning,
            SbgField::Ambiguity,
            SbgField::Routing,
        ]
        .map(|field| SbgCoordinate { field, bit: 0 })
        .to_vec();
        let plan = SbgAnfPlan::<65, 64>::new(vec![lane()], outputs).unwrap();
        let mut state = SemanticState::<65>::zeroed().unwrap();
        state.value.set(0, true).unwrap();
        state.routing.set(64, true).unwrap();
        let queries = plan.queries(&state).unwrap();
        assert_eq!(queries, vec![1]);
        let bank = Pvp6Bank::<64>::new(vec![Pvp6Gate::from_monomials(&[1]).unwrap(); 4]).unwrap();
        let batch = bank
            .evaluate(
                Pvp6Workload::new(1, 1, 1, Pvp6PopulationProfile::AffineUniform),
                &queries,
            )
            .unwrap();
        let next = plan.apply(&state, &batch).unwrap();
        for coordinate in plan.destinations() {
            assert_eq!(coordinate.read(&next), Ok(true));
        }
        assert_eq!(next.routing.get(64), Ok(true));
        assert_eq!(state.meaning_mask.count_ones(), 0);
        assert_eq!(next.value.words().len(), 2);
    }
    #[test]
    fn input_bit_order_spans_all_four_fields() {
        let inputs = vec![
            SbgCoordinate {
                field: SbgField::Value,
                bit: 0,
            },
            SbgCoordinate {
                field: SbgField::Meaning,
                bit: 1,
            },
            SbgCoordinate {
                field: SbgField::Ambiguity,
                bit: 2,
            },
            SbgCoordinate {
                field: SbgField::Routing,
                bit: 3,
            },
            SbgCoordinate {
                field: SbgField::Value,
                bit: 4,
            },
            SbgCoordinate {
                field: SbgField::Routing,
                bit: 5,
            },
        ];
        let plan = SbgAnfPlan::<8, 64>::new(vec![inputs.clone()], vec![inputs[0]]).unwrap();
        for address in 0..64 {
            let mut state = SemanticState::zeroed().unwrap();
            for (i, c) in inputs.iter().enumerate() {
                c.write(&mut state, address & (1 << i) != 0).unwrap();
            }
            assert_eq!(plan.queries(&state).unwrap(), vec![address]);
        }
    }
    #[test]
    fn malformed_wiring_is_rejected_before_execution() {
        let c = lane()[0];
        assert_eq!(
            SbgAnfPlan::<8, 32>::new(vec![lane()], vec![c]),
            Err(SbgAnfError::InvalidDomain)
        );
        assert_eq!(
            SbgAnfPlan::<8, 131072>::new(vec![lane()], vec![c]),
            Err(SbgAnfError::InvalidDomain)
        );
        assert_eq!(
            SbgAnfPlan::<8, 64>::new(vec![], vec![c]),
            Err(SbgAnfError::Shape)
        );
        assert_eq!(
            SbgAnfPlan::<8, 64>::new(vec![vec![c; 6]], vec![c]),
            Err(SbgAnfError::DuplicateInput)
        );
        assert_eq!(
            SbgAnfPlan::<8, 64>::new(vec![lane()], vec![c, c]),
            Err(SbgAnfError::DuplicateOutput)
        );
        assert_eq!(
            SbgAnfPlan::<8, 64>::new(
                vec![lane()],
                vec![SbgCoordinate {
                    field: SbgField::Value,
                    bit: 8
                }]
            ),
            Err(SbgAnfError::Coordinate)
        );
    }
    #[test]
    fn foreign_batch_shape_cannot_partially_modify_state() {
        let plan = SbgAnfPlan::<8, 64>::new(vec![lane()], vec![lane()[0]]).unwrap();
        let bank = Pvp6Bank::<64>::new(vec![Pvp6Gate::from_monomials(&[0]).unwrap()]).unwrap();
        let batch = bank
            .evaluate(
                Pvp6Workload::new(1, 1, 2, Pvp6PopulationProfile::AffineUniform),
                &[0],
            )
            .unwrap();
        let state = SemanticState::zeroed().unwrap();
        assert_eq!(plan.apply(&state, &batch), Err(SbgAnfError::Shape));
        assert_eq!(state.value.count_ones(), 0);
    }
}
