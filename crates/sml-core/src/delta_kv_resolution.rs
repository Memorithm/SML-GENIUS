//! Diagnostic resolution is not confidence, source presence or acceptance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolution2 {
    Unknown,
    Partial { values: [bool; 2], known: [bool; 2] },
    Complete([bool; 2]),
}
impl Resolution2 {
    pub fn from_bits(values: [bool; 2], ambiguous: [bool; 2]) -> Self {
        let known = ambiguous.map(|b| !b);
        match known {
            [false, false] => Self::Unknown,
            [true, true] => Self::Complete(values),
            _ => Self::Partial { values, known },
        }
    }
    pub fn exact_pair(self) -> Option<[bool; 2]> {
        match self {
            Self::Complete(v) => Some(v),
            _ => None,
        }
    }
    pub fn resolved_count(self) -> usize {
        match self {
            Self::Unknown => 0,
            Self::Partial { known, .. } => known.into_iter().filter(|b| *b).count(),
            Self::Complete(_) => 2,
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn partial_success_is_never_a_complete_pair() {
        let r = Resolution2::from_bits([true, false], [false, true]);
        assert_eq!(r.resolved_count(), 1);
        assert_eq!(r.exact_pair(), None);
    }
    #[test]
    fn exhaustive_value_and_mask_contract() {
        for v in 0..4 {
            for m in 0..4 {
                let values = [v & 1 != 0, v & 2 != 0];
                let ambiguity = [m & 1 != 0, m & 2 != 0];
                let r = Resolution2::from_bits(values, ambiguity);
                assert_eq!(r.exact_pair(), if m == 0 { Some(values) } else { None });
                assert_eq!(
                    r.resolved_count(),
                    ambiguity.iter().filter(|b| !**b).count()
                );
            }
        }
    }
}
