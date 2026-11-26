//! Disciplines, learning lanes and the explicit knowledge state machine.

use core::fmt;

/// The four priority disciplines. Every lesson belongs to exactly one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Domain {
    French,
    English,
    Mathematics,
    Rust,
}

impl Domain {
    pub const ALL: [Domain; 4] = [
        Domain::French,
        Domain::English,
        Domain::Mathematics,
        Domain::Rust,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Domain::French => "french",
            Domain::English => "english",
            Domain::Mathematics => "mathematics",
            Domain::Rust => "rust",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|d| d.name() == s)
    }

    /// The acquisition lane that serves this discipline.
    pub const fn acquisition_lane(self) -> Lane {
        match self {
            Domain::French => Lane::French,
            Domain::English => Lane::English,
            Domain::Mathematics => Lane::Mathematics,
            Domain::Rust => Lane::Rust,
        }
    }
}

impl fmt::Display for Domain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// Specialised learning queues maintained simultaneously by the scheduler.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Lane {
    French,
    English,
    Mathematics,
    Rust,
    Revision,
    Validation,
    GapDetection,
    Consolidation,
    Transfer,
}

impl Lane {
    pub const ALL: [Lane; 9] = [
        Lane::French,
        Lane::English,
        Lane::Mathematics,
        Lane::Rust,
        Lane::Revision,
        Lane::Validation,
        Lane::GapDetection,
        Lane::Consolidation,
        Lane::Transfer,
    ];

    pub const fn index(self) -> usize {
        self as usize
    }

    pub const fn name(self) -> &'static str {
        match self {
            Lane::French => "french",
            Lane::English => "english",
            Lane::Mathematics => "mathematics",
            Lane::Rust => "rust",
            Lane::Revision => "revision",
            Lane::Validation => "validation",
            Lane::GapDetection => "gap_detection",
            Lane::Consolidation => "consolidation",
            Lane::Transfer => "transfer",
        }
    }
}

/// Explicit per-knowledge state. Transitions are whitelisted in
/// [`KnowledgeState::can_transition`]; in particular `Certified` is reachable
/// only from `Validating`, so no component can skip the examiner.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum KnowledgeState {
    Unknown,
    Identified,
    Acquiring,
    Learned,
    Validating,
    Certified,
    Consolidated,
    NeedsRevision,
    Contradictory,
    Blocked,
}

impl KnowledgeState {
    pub const ALL: [KnowledgeState; 10] = [
        KnowledgeState::Unknown,
        KnowledgeState::Identified,
        KnowledgeState::Acquiring,
        KnowledgeState::Learned,
        KnowledgeState::Validating,
        KnowledgeState::Certified,
        KnowledgeState::Consolidated,
        KnowledgeState::NeedsRevision,
        KnowledgeState::Contradictory,
        KnowledgeState::Blocked,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            KnowledgeState::Unknown => "unknown",
            KnowledgeState::Identified => "identified",
            KnowledgeState::Acquiring => "acquiring",
            KnowledgeState::Learned => "learned",
            KnowledgeState::Validating => "validating",
            KnowledgeState::Certified => "certified",
            KnowledgeState::Consolidated => "consolidated",
            KnowledgeState::NeedsRevision => "needs_revision",
            KnowledgeState::Contradictory => "contradictory",
            KnowledgeState::Blocked => "blocked",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.name() == s)
    }

    pub fn can_transition(self, to: KnowledgeState) -> bool {
        use KnowledgeState::*;
        matches!(
            (self, to),
            (Unknown, Identified)
                | (Identified, Acquiring)
                | (Identified, Blocked)
                | (Acquiring, Learned)
                | (Acquiring, Identified)
                | (Acquiring, Blocked)
                | (Learned, Validating)
                | (Validating, Certified)
                | (Validating, Identified)
                | (Validating, Contradictory)
                | (Certified, Consolidated)
                | (Certified, Contradictory)
                | (Certified, Identified)
                | (Consolidated, NeedsRevision)
                | (Consolidated, Contradictory)
                | (NeedsRevision, Validating)
                | (NeedsRevision, Acquiring)
                | (Contradictory, Identified)
                | (Contradictory, Blocked)
                | (Blocked, Identified)
        )
    }

    /// States in which the knowledge may serve as a prerequisite.
    pub const fn satisfies_prerequisite(self) -> bool {
        matches!(
            self,
            KnowledgeState::Certified | KnowledgeState::Consolidated
        )
    }

    /// States that represent in-flight work lost on a crash.
    pub const fn is_in_flight(self) -> bool {
        matches!(
            self,
            KnowledgeState::Acquiring | KnowledgeState::Learned | KnowledgeState::Validating
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn certified_only_reachable_from_validating() {
        for s in KnowledgeState::ALL {
            let ok = s.can_transition(KnowledgeState::Certified);
            assert_eq!(ok, s == KnowledgeState::Validating, "{s:?}");
        }
    }

    #[test]
    fn consolidated_only_reachable_from_certified() {
        for s in KnowledgeState::ALL {
            let ok = s.can_transition(KnowledgeState::Consolidated);
            assert_eq!(ok, s == KnowledgeState::Certified, "{s:?}");
        }
    }

    #[test]
    fn names_roundtrip() {
        for s in KnowledgeState::ALL {
            assert_eq!(KnowledgeState::parse(s.name()), Some(s));
        }
        for d in Domain::ALL {
            assert_eq!(Domain::parse(d.name()), Some(d));
        }
        for (i, l) in Lane::ALL.iter().enumerate() {
            assert_eq!(l.index(), i);
        }
    }
}
