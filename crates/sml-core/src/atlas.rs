//! Exact physical and semantic accounting for directly addressed circuit pages.

use core::fmt;

use crate::{ExecutionCounters, PageId, ParameterBudget};

/// Immutable manifest for one independently addressable circuit-atlas page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AtlasPageManifest {
    pub id: PageId,
    pub parameters: ParameterBudget,
    /// Non-parameter semantic metadata carried by the page.
    pub metadata_bits: u64,
    /// Physical page alignment in bytes.
    pub alignment_bytes: u64,
}

impl AtlasPageManifest {
    /// Constructs a manifest and rejects invalid physical alignment.
    pub fn new(
        id: PageId,
        parameters: ParameterBudget,
        metadata_bits: u64,
        alignment_bytes: u64,
    ) -> Result<Self, AtlasError> {
        if alignment_bytes == 0 || !alignment_bytes.is_power_of_two() {
            return Err(AtlasError::InvalidAlignment(alignment_bytes));
        }
        Ok(Self {
            id,
            parameters,
            metadata_bits,
            alignment_bytes,
        })
    }

    /// Learned parameter count on this page.
    #[must_use]
    pub const fn learned_parameters(self) -> u64 {
        self.parameters.total_parameters()
    }

    /// Packed learned-parameter payload only.
    #[must_use]
    pub const fn parameter_bits(self) -> u128 {
        self.parameters.storage_bits()
    }

    /// Semantic bits required for parameters plus declared metadata.
    #[must_use]
    pub const fn semantic_bits(self) -> u128 {
        self.parameter_bits() + self.metadata_bits as u128
    }

    /// Physical bytes after byte rounding and declared alignment.
    #[must_use]
    pub const fn physical_bytes(self) -> u128 {
        let bytes = self.semantic_bits().div_ceil(8);
        let alignment = self.alignment_bytes as u128;
        bytes.div_ceil(alignment) * alignment
    }
}

/// Exact activation record for one atlas page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageActivation {
    pub id: PageId,
    pub learned_parameters: u64,
    pub semantic_bits: u128,
    pub physical_bytes: u128,
}

/// Dense direct-address atlas.
///
/// Page IDs must equal their vector index. This deliberately makes page lookup
/// O(1) and avoids a hidden scan or nondeterministic hash-map dependency.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CircuitAtlas {
    pages: Vec<AtlasPageManifest>,
}

impl CircuitAtlas {
    /// Validates dense page IDs and constructs the atlas.
    pub fn new(pages: Vec<AtlasPageManifest>) -> Result<Self, AtlasError> {
        for (index, page) in pages.iter().enumerate() {
            let expected = index as u32;
            if page.id.0 != expected {
                return Err(AtlasError::NonDensePageId {
                    index,
                    expected: PageId(expected),
                    actual: page.id,
                });
            }
        }
        Ok(Self { pages })
    }

    /// Number of addressable pages.
    #[must_use]
    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    /// Exact total learned parameters.
    #[must_use]
    pub fn total_parameters(&self) -> u128 {
        self.pages
            .iter()
            .map(|page| page.learned_parameters() as u128)
            .sum()
    }

    /// Exact total semantic bits before physical alignment.
    #[must_use]
    pub fn total_semantic_bits(&self) -> u128 {
        self.pages.iter().map(|page| page.semantic_bits()).sum()
    }

    /// Exact total physical bytes after per-page alignment.
    #[must_use]
    pub fn total_physical_bytes(&self) -> u128 {
        self.pages.iter().map(|page| page.physical_bytes()).sum()
    }

    /// Activates a page by direct index and records exact traffic semantics.
    pub fn activate(
        &self,
        id: PageId,
        counters: &mut ExecutionCounters,
    ) -> Result<PageActivation, AtlasError> {
        let page = *self
            .pages
            .get(id.0 as usize)
            .ok_or(AtlasError::PageNotFound(id))?;

        counters.atlas_page_activations = counters.atlas_page_activations.saturating_add(1);
        counters.atlas_parameter_touches = counters
            .atlas_parameter_touches
            .saturating_add(page.learned_parameters());
        counters.atlas_storage_bits_touched = counters
            .atlas_storage_bits_touched
            .saturating_add(page.semantic_bits());

        Ok(PageActivation {
            id,
            learned_parameters: page.learned_parameters(),
            semantic_bits: page.semantic_bits(),
            physical_bytes: page.physical_bytes(),
        })
    }
}

/// Atlas construction or lookup errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AtlasError {
    InvalidAlignment(u64),
    NonDensePageId {
        index: usize,
        expected: PageId,
        actual: PageId,
    },
    PageNotFound(PageId),
}

impl fmt::Display for AtlasError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for AtlasError {}
