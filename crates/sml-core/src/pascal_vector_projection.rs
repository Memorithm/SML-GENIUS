//! PVP6 architecture selection for the self-contained SML runtime.
//!
//! The selector is deliberately small and deterministic. It records the
//! architecture decision made from the native Thor campaign without making a
//! speed or model-quality claim. The campaign was a synthetic, shared-CPU
//! diagnostic and remains a qualification input, not a production promotion.

/// Frozen protocol identity used by the native campaign.
pub const PVP6_PROTOCOL_V1: &str = "sml-pvp6-anf-fixture/v1";
/// Scope of the evidence attached to this policy.
pub const PVP6_NATIVE_EVIDENCE_SCOPE: &str = "Thor/aarch64/shared-CPU-diagnostic";

/// Population schedule used by the PVP6 fixture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pvp6PopulationProfile {
    AffineUniform,
    LowPopcount,
    HighPopcount,
}

/// Workload coordinates consumed by the architecture policy.
///
/// The coordinates intentionally describe the workload rather than the host.
/// A non-measured workload is accepted, but it receives the conservative Pascal
/// default and is marked as not measured by the Thor campaign.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pvp6Workload {
    pub density: u32,
    pub query_count: u32,
    pub reuse: u32,
    pub population: Pvp6PopulationProfile,
}

impl Pvp6Workload {
    #[must_use]
    pub const fn new(
        density: u32,
        query_count: u32,
        reuse: u32,
        population: Pvp6PopulationProfile,
    ) -> Self {
        Self {
            density,
            query_count,
            reuse,
            population,
        }
    }

    pub fn select(self) -> Result<Pvp6Selection, Pvp6SelectionError> {
        if self.density == 0 {
            return Err(Pvp6SelectionError::ZeroDensity);
        }
        if self.query_count == 0 {
            return Err(Pvp6SelectionError::ZeroQueryCount);
        }
        if self.reuse == 0 {
            return Err(Pvp6SelectionError::ZeroReuse);
        }

        let measured_on_thor = self.is_native_thor_measured();
        let (architecture, rationale) =
            if self.density == 4 && self.query_count == 32 && self.reuse == 1 {
                match self.population {
                    Pvp6PopulationProfile::LowPopcount => (
                        Pvp6Architecture::SubmaskCoefficientQuery,
                        Pvp6SelectionRationale::ThorNativeMeasuredSubmaskException,
                    ),
                    Pvp6PopulationProfile::AffineUniform | Pvp6PopulationProfile::HighPopcount => (
                        Pvp6Architecture::SparseAnfScan,
                        Pvp6SelectionRationale::ThorNativeMeasuredSparseException,
                    ),
                }
            } else if measured_on_thor {
                (
                    Pvp6Architecture::PascalAnfMaterialize,
                    Pvp6SelectionRationale::ThorNativeMeasuredPascalDefault,
                )
            } else {
                (
                    Pvp6Architecture::PascalAnfMaterialize,
                    Pvp6SelectionRationale::ConservativeUnmeasuredPascalDefault,
                )
            };

        Ok(Pvp6Selection {
            architecture,
            rationale,
            measured_on_thor,
            diagnostic_only: true,
        })
    }

    #[must_use]
    pub fn is_native_thor_measured(self) -> bool {
        THOR_MEASURED_CELLS.contains(&self)
    }
}

/// Architectures admitted by the current PVP6 policy.
///
/// Resident truth-table lookup is a control/lower-bound arm, not a materializing
/// architecture, and generic truth materialization is intentionally not selected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pvp6Architecture {
    PascalAnfMaterialize,
    SparseAnfScan,
    SubmaskCoefficientQuery,
}

impl Pvp6Architecture {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PascalAnfMaterialize => "PASCAL_ANF_MATERIALIZE",
            Self::SparseAnfScan => "SPARSE_ANF_SCAN",
            Self::SubmaskCoefficientQuery => "SUBMASK_COEFFICIENT_QUERY",
        }
    }
}

/// Why the policy selected an architecture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pvp6SelectionRationale {
    ThorNativeMeasuredPascalDefault,
    ThorNativeMeasuredSparseException,
    ThorNativeMeasuredSubmaskException,
    ConservativeUnmeasuredPascalDefault,
}

/// Result of the deterministic policy decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pvp6Selection {
    pub architecture: Pvp6Architecture,
    pub rationale: Pvp6SelectionRationale,
    pub measured_on_thor: bool,
    /// Always true until a separate model-aligned promotion protocol exists.
    pub diagnostic_only: bool,
}

/// Invalid workload coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pvp6SelectionError {
    ZeroDensity,
    ZeroQueryCount,
    ZeroReuse,
}

/// Exact workload cells present in the native Thor campaign.
///
/// These cells are copied from the campaign input protocol. The array is used
/// only to label provenance; it does not extrapolate a timing result to new
/// workloads.
pub const THOR_MEASURED_CELLS: [Pvp6Workload; 20] = [
    Pvp6Workload::new(4, 512, 1, Pvp6PopulationProfile::AffineUniform),
    Pvp6Workload::new(32, 512, 1, Pvp6PopulationProfile::AffineUniform),
    Pvp6Workload::new(256, 512, 1, Pvp6PopulationProfile::AffineUniform),
    Pvp6Workload::new(1024, 512, 1, Pvp6PopulationProfile::AffineUniform),
    Pvp6Workload::new(256, 128, 1, Pvp6PopulationProfile::AffineUniform),
    Pvp6Workload::new(256, 128, 4, Pvp6PopulationProfile::AffineUniform),
    Pvp6Workload::new(256, 128, 16, Pvp6PopulationProfile::AffineUniform),
    Pvp6Workload::new(256, 512, 4, Pvp6PopulationProfile::AffineUniform),
    Pvp6Workload::new(256, 512, 16, Pvp6PopulationProfile::AffineUniform),
    Pvp6Workload::new(256, 2048, 1, Pvp6PopulationProfile::AffineUniform),
    Pvp6Workload::new(256, 2048, 4, Pvp6PopulationProfile::AffineUniform),
    Pvp6Workload::new(256, 2048, 16, Pvp6PopulationProfile::AffineUniform),
    Pvp6Workload::new(256, 128, 1, Pvp6PopulationProfile::LowPopcount),
    Pvp6Workload::new(256, 512, 1, Pvp6PopulationProfile::LowPopcount),
    Pvp6Workload::new(256, 128, 1, Pvp6PopulationProfile::HighPopcount),
    Pvp6Workload::new(256, 512, 1, Pvp6PopulationProfile::HighPopcount),
    Pvp6Workload::new(4, 32, 1, Pvp6PopulationProfile::AffineUniform),
    Pvp6Workload::new(4, 32, 1, Pvp6PopulationProfile::LowPopcount),
    Pvp6Workload::new(4, 32, 1, Pvp6PopulationProfile::HighPopcount),
    Pvp6Workload::new(1024, 2048, 16, Pvp6PopulationProfile::AffineUniform),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measured_cells_choose_pascal_except_the_three_low_query_exceptions() {
        let cases = [
            (
                Pvp6Workload::new(4, 512, 1, Pvp6PopulationProfile::AffineUniform),
                Pvp6Architecture::PascalAnfMaterialize,
            ),
            (
                Pvp6Workload::new(256, 2048, 16, Pvp6PopulationProfile::AffineUniform),
                Pvp6Architecture::PascalAnfMaterialize,
            ),
            (
                Pvp6Workload::new(4, 32, 1, Pvp6PopulationProfile::AffineUniform),
                Pvp6Architecture::SparseAnfScan,
            ),
            (
                Pvp6Workload::new(4, 32, 1, Pvp6PopulationProfile::HighPopcount),
                Pvp6Architecture::SparseAnfScan,
            ),
            (
                Pvp6Workload::new(4, 32, 1, Pvp6PopulationProfile::LowPopcount),
                Pvp6Architecture::SubmaskCoefficientQuery,
            ),
        ];

        for (workload, expected) in cases {
            let selection = workload.select().expect("valid workload");
            assert_eq!(selection.architecture, expected);
            assert!(selection.measured_on_thor);
            assert!(selection.diagnostic_only);
        }
    }

    #[test]
    fn unmeasured_workloads_keep_the_conservative_pascal_default() {
        let workload = Pvp6Workload::new(5, 64, 2, Pvp6PopulationProfile::LowPopcount);
        let selection = workload.select().expect("valid workload");

        assert_eq!(
            selection.architecture,
            Pvp6Architecture::PascalAnfMaterialize
        );
        assert_eq!(
            selection.rationale,
            Pvp6SelectionRationale::ConservativeUnmeasuredPascalDefault
        );
        assert!(!selection.measured_on_thor);
        assert!(selection.diagnostic_only);
    }

    #[test]
    fn invalid_workloads_fail_closed() {
        assert_eq!(
            Pvp6Workload::new(0, 32, 1, Pvp6PopulationProfile::AffineUniform).select(),
            Err(Pvp6SelectionError::ZeroDensity)
        );
        assert_eq!(
            Pvp6Workload::new(4, 0, 1, Pvp6PopulationProfile::AffineUniform).select(),
            Err(Pvp6SelectionError::ZeroQueryCount)
        );
        assert_eq!(
            Pvp6Workload::new(4, 32, 0, Pvp6PopulationProfile::AffineUniform).select(),
            Err(Pvp6SelectionError::ZeroReuse)
        );
    }

    #[test]
    fn protocol_and_architecture_names_are_stable() {
        assert_eq!(PVP6_PROTOCOL_V1, "sml-pvp6-anf-fixture/v1");
        assert_eq!(
            Pvp6Architecture::PascalAnfMaterialize.as_str(),
            "PASCAL_ANF_MATERIALIZE"
        );
        assert_eq!(THOR_MEASURED_CELLS.len(), 20);
    }
}
