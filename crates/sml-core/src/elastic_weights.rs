//! Deterministic elastic-weight page planning for SML.
//!
//! Representation precision, residency, and activation change only at explicit
//! plan/commit boundaries. The planner never invents richer learned values when
//! promoting precision: materializing a Boolean page as ternary or residual
//! remains the caller's responsibility.

use core::fmt;

use crate::{PageId, ParameterBudget};

/// Parts-per-million scale used by page importance scores.
pub const IMPORTANCE_SCALE_PPM: u32 = 1_000_000;
/// Versioned self-contained contract exported by elastic weight plans.
pub const ELASTIC_WEIGHT_PLAN_SCHEMA_V1: &str = "sml.elastic-weight-plan@1.0.0";

/// Physical representation class for one logical learned-weight page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum WeightPrecision {
    Boolean,
    Ternary,
    Residual4,
}

impl WeightPrecision {
    #[must_use]
    pub const fn bits_per_parameter(self) -> u8 {
        match self {
            Self::Boolean => 1,
            Self::Ternary => 2,
            Self::Residual4 => 4,
        }
    }

    #[must_use]
    pub const fn parameter_budget(self, parameters: u64) -> ParameterBudget {
        match self {
            Self::Boolean => ParameterBudget {
                boolean: parameters,
                ternary: 0,
                residual4: 0,
            },
            Self::Ternary => ParameterBudget {
                boolean: 0,
                ternary: parameters,
                residual4: 0,
            },
            Self::Residual4 => ParameterBudget {
                boolean: 0,
                ternary: 0,
                residual4: parameters,
            },
        }
    }
}

/// Exclusive residency tier for the reference planner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResidencyTier {
    Disk,
    Ram,
    Vram,
}

/// Runtime metadata for one logical learned-weight page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ElasticWeightPage {
    pub page_id: PageId,
    pub parameters: u64,
    pub precision: WeightPrecision,
    pub residency: ResidencyTier,
    pub active: bool,
    pub importance_ppm: u32,
    pub access_count: u64,
    pub last_access_epoch: u64,
    pub representation_version: u64,
}

impl ElasticWeightPage {
    pub fn new(
        page_id: PageId,
        parameters: u64,
        precision: WeightPrecision,
        residency: ResidencyTier,
        importance_ppm: u32,
    ) -> Result<Self, ElasticWeightError> {
        if parameters == 0 {
            return Err(ElasticWeightError::ZeroParameters(page_id));
        }
        validate_importance(page_id, importance_ppm)?;
        Ok(Self {
            page_id,
            parameters,
            precision,
            residency,
            active: false,
            importance_ppm,
            access_count: 0,
            last_access_epoch: 0,
            representation_version: 0,
        })
    }

    #[must_use]
    pub const fn parameter_budget(self) -> ParameterBudget {
        self.precision.parameter_budget(self.parameters)
    }

    #[must_use]
    pub const fn payload_bits(self) -> u128 {
        self.parameters as u128 * self.precision.bits_per_parameter() as u128
    }

    pub fn payload_bytes_ceil(self) -> Result<u64, ElasticWeightError> {
        let bytes = self.payload_bits().div_ceil(8);
        u64::try_from(bytes).map_err(|_| ElasticWeightError::ArithmeticOverflow)
    }
}

/// Deterministic precision, residency, and active-set limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ElasticWeightPolicy {
    pub ram_limit_bytes: u64,
    pub vram_limit_bytes: u64,
    pub max_active_parameters: u64,
    pub ternary_threshold_ppm: u32,
    pub residual4_threshold_ppm: u32,
    pub vram_threshold_ppm: u32,
}

impl ElasticWeightPolicy {
    pub fn validate(self) -> Result<Self, ElasticWeightError> {
        if self.ternary_threshold_ppm > IMPORTANCE_SCALE_PPM
            || self.residual4_threshold_ppm > IMPORTANCE_SCALE_PPM
            || self.vram_threshold_ppm > IMPORTANCE_SCALE_PPM
            || self.ternary_threshold_ppm > self.residual4_threshold_ppm
        {
            return Err(ElasticWeightError::InvalidPolicyThresholds);
        }
        Ok(self)
    }
}

/// One page transition proposed by an immutable cycle plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ElasticWeightTransition {
    pub page_id: PageId,
    pub parameters: u64,
    pub from_representation_version: u64,
    pub to_representation_version: u64,
    pub from_precision: WeightPrecision,
    pub to_precision: WeightPrecision,
    pub from_residency: ResidencyTier,
    pub to_residency: ResidencyTier,
    pub from_active: bool,
    pub to_active: bool,
    pub target_payload_bytes: u64,
}

/// Immutable result of PLAN/VALIDATE. Applying it requires an explicit commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElasticWeightPlan {
    pub contract: &'static str,
    pub base_generation: u64,
    pub epoch: u64,
    pub ram_limit_bytes: u64,
    pub vram_limit_bytes: u64,
    pub max_active_parameters: u64,
    pub transitions: Vec<ElasticWeightTransition>,
    pub ram_bytes: u64,
    pub vram_bytes: u64,
    pub active_parameters: u64,
    pub active_storage_bits: u128,
    pub bytes_moved_to_ram: u64,
    pub bytes_moved_to_vram: u64,
    pub precision_changes: u64,
}

/// Aggregate fields independently recomputed by ElasticWeightPlan::validate_contract_v1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElasticWeightPlanField {
    RamBytes,
    VramBytes,
    ActiveParameters,
    ActiveStorageBits,
    BytesMovedToRam,
    BytesMovedToVram,
    PrecisionChanges,
}

impl ElasticWeightPlan {
    /// Independently validates every self-contained SML elastic-weight plan field.
    ///
    /// This is deliberately callable without an ElasticWeightPlanner so a
    /// downstream runtime can reject malformed or internally inconsistent plans
    /// before considering physical actuation.
    pub fn validate_contract_v1(&self) -> Result<(), ElasticWeightError> {
        if self.contract != ELASTIC_WEIGHT_PLAN_SCHEMA_V1 {
            return Err(ElasticWeightError::InvalidPlanContract);
        }

        let mut seen = Vec::with_capacity(self.transitions.len());
        let mut ram_bytes = 0_u64;
        let mut vram_bytes = 0_u64;
        let mut active_parameters = 0_u64;
        let mut active_storage_bits = 0_u128;
        let mut bytes_moved_to_ram = 0_u64;
        let mut bytes_moved_to_vram = 0_u64;
        let mut precision_changes = 0_u64;

        for transition in &self.transitions {
            if transition.parameters == 0 {
                return Err(ElasticWeightError::ZeroParameters(transition.page_id));
            }
            if seen.contains(&transition.page_id) {
                return Err(ElasticWeightError::DuplicatePlanPageId(transition.page_id));
            }
            seen.push(transition.page_id);

            let expected_payload =
                payload_bytes(transition.parameters, transition.to_precision)?;
            if transition.target_payload_bytes != expected_payload {
                return Err(ElasticWeightError::PlanPayloadBytes {
                    page_id: transition.page_id,
                    expected: expected_payload,
                    actual: transition.target_payload_bytes,
                });
            }

            let expected_version = if transition.from_precision != transition.to_precision {
                transition
                    .from_representation_version
                    .checked_add(1)
                    .ok_or(ElasticWeightError::RepresentationVersionOverflow(
                        transition.page_id,
                    ))?
            } else {
                transition.from_representation_version
            };
            if transition.to_representation_version != expected_version {
                return Err(ElasticWeightError::PlanRepresentationVersion {
                    page_id: transition.page_id,
                    expected: expected_version,
                    actual: transition.to_representation_version,
                });
            }

            if transition.to_active && transition.to_residency == ResidencyTier::Disk {
                return Err(ElasticWeightError::ActivePageOnDisk(transition.page_id));
            }

            match transition.to_residency {
                ResidencyTier::Disk => {}
                ResidencyTier::Ram => {
                    ram_bytes = checked_add(ram_bytes, transition.target_payload_bytes)?;
                }
                ResidencyTier::Vram => {
                    vram_bytes = checked_add(vram_bytes, transition.target_payload_bytes)?;
                }
            }

            if transition.to_active {
                active_parameters = checked_add(active_parameters, transition.parameters)?;
                active_storage_bits = active_storage_bits
                    .checked_add(
                        transition.parameters as u128
                            * transition.to_precision.bits_per_parameter() as u128,
                    )
                    .ok_or(ElasticWeightError::ArithmeticOverflow)?;
            }

            if transition.from_precision != transition.to_precision {
                precision_changes = precision_changes.saturating_add(1);
            }
            if transition.from_residency != transition.to_residency {
                match transition.to_residency {
                    ResidencyTier::Disk => {}
                    ResidencyTier::Ram => {
                        bytes_moved_to_ram =
                            checked_add(bytes_moved_to_ram, transition.target_payload_bytes)?;
                    }
                    ResidencyTier::Vram => {
                        bytes_moved_to_vram =
                            checked_add(bytes_moved_to_vram, transition.target_payload_bytes)?;
                    }
                }
            }
        }

        if ram_bytes > self.ram_limit_bytes {
            return Err(ElasticWeightError::RamBudget {
                actual: ram_bytes,
                maximum: self.ram_limit_bytes,
            });
        }
        if vram_bytes > self.vram_limit_bytes {
            return Err(ElasticWeightError::VramBudget {
                actual: vram_bytes,
                maximum: self.vram_limit_bytes,
            });
        }
        if active_parameters > self.max_active_parameters {
            return Err(ElasticWeightError::ActiveParameterBudget {
                actual: active_parameters,
                maximum: self.max_active_parameters,
            });
        }

        validate_plan_field(
            ElasticWeightPlanField::RamBytes,
            ram_bytes as u128,
            self.ram_bytes as u128,
        )?;
        validate_plan_field(
            ElasticWeightPlanField::VramBytes,
            vram_bytes as u128,
            self.vram_bytes as u128,
        )?;
        validate_plan_field(
            ElasticWeightPlanField::ActiveParameters,
            active_parameters as u128,
            self.active_parameters as u128,
        )?;
        validate_plan_field(
            ElasticWeightPlanField::ActiveStorageBits,
            active_storage_bits,
            self.active_storage_bits,
        )?;
        validate_plan_field(
            ElasticWeightPlanField::BytesMovedToRam,
            bytes_moved_to_ram as u128,
            self.bytes_moved_to_ram as u128,
        )?;
        validate_plan_field(
            ElasticWeightPlanField::BytesMovedToVram,
            bytes_moved_to_vram as u128,
            self.bytes_moved_to_vram as u128,
        )?;
        validate_plan_field(
            ElasticWeightPlanField::PrecisionChanges,
            precision_changes as u128,
            self.precision_changes as u128,
        )?;

        Ok(())
    }
}

/// Exact current logical/physical accounting across all page descriptors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ElasticWeightAccounting {
    pub total_parameters: u64,
    pub boolean_parameters: u64,
    pub ternary_parameters: u64,
    pub residual4_parameters: u64,
    pub disk_bytes: u64,
    pub ram_bytes: u64,
    pub vram_bytes: u64,
    pub active_parameters: u64,
    pub active_storage_bits: u128,
}

/// Deterministic elastic-weight control plane.
///
/// Selected pages are the only active pages after commit. High-importance pages
/// prefer VRAM, then RAM; lower-importance selected pages prefer RAM and may use
/// spare VRAM as a deterministic fallback. Unselected pages become inactive and
/// disk-resident in this reference policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElasticWeightPlanner {
    pages: Vec<ElasticWeightPage>,
    policy: ElasticWeightPolicy,
    generation: u64,
}

impl ElasticWeightPlanner {
    pub fn new(
        pages: Vec<ElasticWeightPage>,
        policy: ElasticWeightPolicy,
    ) -> Result<Self, ElasticWeightError> {
        let policy = policy.validate()?;
        for (index, page) in pages.iter().copied().enumerate() {
            validate_importance(page.page_id, page.importance_ppm)?;
            if page.parameters == 0 {
                return Err(ElasticWeightError::ZeroParameters(page.page_id));
            }
            if page.active && page.residency == ResidencyTier::Disk {
                return Err(ElasticWeightError::ActivePageOnDisk(page.page_id));
            }
            if pages[..index]
                .iter()
                .any(|other| other.page_id == page.page_id)
            {
                return Err(ElasticWeightError::DuplicatePageId(page.page_id));
            }
        }

        let planner = Self {
            pages,
            policy,
            generation: 0,
        };
        planner.validate_current_budgets()?;
        Ok(planner)
    }

    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    #[must_use]
    pub fn pages(&self) -> &[ElasticWeightPage] {
        &self.pages
    }

    /// Updates caller-owned importance evidence without changing representation.
    pub fn record_access(
        &mut self,
        page_id: PageId,
        importance_ppm: u32,
        epoch: u64,
    ) -> Result<(), ElasticWeightError> {
        validate_importance(page_id, importance_ppm)?;
        let page = self.page_mut(page_id)?;
        page.importance_ppm = importance_ppm;
        page.access_count = page.access_count.saturating_add(1);
        page.last_access_epoch = epoch;
        Ok(())
    }

    /// Plans the next frozen active set without mutating current page state.
    pub fn plan_cycle(
        &self,
        selected_pages: &[PageId],
        epoch: u64,
    ) -> Result<ElasticWeightPlan, ElasticWeightError> {
        let mut selected_indices: Vec<usize> = Vec::with_capacity(selected_pages.len());
        for page_id in selected_pages.iter().copied() {
            if selected_indices
                .iter()
                .any(|index| self.pages[*index].page_id == page_id)
            {
                return Err(ElasticWeightError::DuplicateSelection(page_id));
            }
            selected_indices.push(self.page_index(page_id)?);
        }

        let active_parameters = selected_indices.iter().try_fold(0_u64, |total, index| {
            total
                .checked_add(self.pages[*index].parameters)
                .ok_or(ElasticWeightError::ArithmeticOverflow)
        })?;
        if active_parameters > self.policy.max_active_parameters {
            return Err(ElasticWeightError::ActiveParameterBudget {
                actual: active_parameters,
                maximum: self.policy.max_active_parameters,
            });
        }

        selected_indices.sort_by(|left, right| {
            let left_page = self.pages[*left];
            let right_page = self.pages[*right];
            right_page
                .importance_ppm
                .cmp(&left_page.importance_ppm)
                .then_with(|| right_page.access_count.cmp(&left_page.access_count))
                .then_with(|| left_page.page_id.0.cmp(&right_page.page_id.0))
        });

        let mut target_precision = Vec::with_capacity(self.pages.len());
        let mut target_residency = vec![ResidencyTier::Disk; self.pages.len()];
        let mut target_active = vec![false; self.pages.len()];
        let mut target_bytes = Vec::with_capacity(self.pages.len());

        for page in self.pages.iter().copied() {
            let precision = precision_for_importance(page.importance_ppm, self.policy);
            target_precision.push(precision);
            target_bytes.push(payload_bytes(page.parameters, precision)?);
        }

        let mut ram_bytes = 0_u64;
        let mut vram_bytes = 0_u64;
        for index in selected_indices.iter().copied() {
            let page = self.pages[index];
            let bytes = target_bytes[index];
            let prefer_vram = page.importance_ppm >= self.policy.vram_threshold_ppm;
            let destination =
                if prefer_vram && fits(vram_bytes, bytes, self.policy.vram_limit_bytes) {
                    vram_bytes = checked_add(vram_bytes, bytes)?;
                    ResidencyTier::Vram
                } else if fits(ram_bytes, bytes, self.policy.ram_limit_bytes) {
                    ram_bytes = checked_add(ram_bytes, bytes)?;
                    ResidencyTier::Ram
                } else if fits(vram_bytes, bytes, self.policy.vram_limit_bytes) {
                    vram_bytes = checked_add(vram_bytes, bytes)?;
                    ResidencyTier::Vram
                } else {
                    return Err(ElasticWeightError::ActiveResidencyBudget {
                        page_id: page.page_id,
                        payload_bytes: bytes,
                        ram_remaining: self.policy.ram_limit_bytes.saturating_sub(ram_bytes),
                        vram_remaining: self.policy.vram_limit_bytes.saturating_sub(vram_bytes),
                    });
                };
            target_residency[index] = destination;
            target_active[index] = true;
        }

        let mut transitions = Vec::with_capacity(self.pages.len());
        let mut active_storage_bits = 0_u128;
        let mut bytes_moved_to_ram = 0_u64;
        let mut bytes_moved_to_vram = 0_u64;
        let mut precision_changes = 0_u64;

        for (index, page) in self.pages.iter().copied().enumerate() {
            let to_precision = target_precision[index];
            let to_residency = target_residency[index];
            let to_active = target_active[index];
            let bytes = target_bytes[index];

            if page.precision != to_precision {
                precision_changes = precision_changes.saturating_add(1);
            }
            if page.residency != to_residency {
                match to_residency {
                    ResidencyTier::Ram => {
                        bytes_moved_to_ram = checked_add(bytes_moved_to_ram, bytes)?;
                    }
                    ResidencyTier::Vram => {
                        bytes_moved_to_vram = checked_add(bytes_moved_to_vram, bytes)?;
                    }
                    ResidencyTier::Disk => {}
                }
            }
            if to_active {
                active_storage_bits = active_storage_bits
                    .checked_add(
                        page.parameters as u128 * to_precision.bits_per_parameter() as u128,
                    )
                    .ok_or(ElasticWeightError::ArithmeticOverflow)?;
            }

            let to_representation_version = if page.precision != to_precision {
                page.representation_version
                    .checked_add(1)
                    .ok_or(ElasticWeightError::RepresentationVersionOverflow(page.page_id))?
            } else {
                page.representation_version
            };

            transitions.push(ElasticWeightTransition {
                page_id: page.page_id,
                parameters: page.parameters,
                from_representation_version: page.representation_version,
                to_representation_version,
                from_precision: page.precision,
                to_precision,
                from_residency: page.residency,
                to_residency,
                from_active: page.active,
                to_active,
                target_payload_bytes: bytes,
            });
        }

        let plan = ElasticWeightPlan {
            contract: ELASTIC_WEIGHT_PLAN_SCHEMA_V1,
            base_generation: self.generation,
            epoch,
            ram_limit_bytes: self.policy.ram_limit_bytes,
            vram_limit_bytes: self.policy.vram_limit_bytes,
            max_active_parameters: self.policy.max_active_parameters,
            transitions,
            ram_bytes,
            vram_bytes,
            active_parameters,
            active_storage_bits,
            bytes_moved_to_ram,
            bytes_moved_to_vram,
            precision_changes,
        };
        plan.validate_contract_v1()?;
        Ok(plan)
    }

    /// Atomically applies a plan if its generation and source states still match.
    pub fn commit(&mut self, plan: &ElasticWeightPlan) -> Result<(), ElasticWeightError> {
        plan.validate_contract_v1()?;
        if plan.base_generation != self.generation {
            return Err(ElasticWeightError::StalePlan {
                expected_generation: self.generation,
                actual_generation: plan.base_generation,
            });
        }
        if plan.transitions.len() != self.pages.len() {
            return Err(ElasticWeightError::PlanPageCount {
                expected: self.pages.len(),
                actual: plan.transitions.len(),
            });
        }

        for transition in &plan.transitions {
            let page = self.page(transition.page_id)?;
            if page.parameters != transition.parameters
                || page.representation_version != transition.from_representation_version
                || page.precision != transition.from_precision
                || page.residency != transition.from_residency
                || page.active != transition.from_active
            {
                return Err(ElasticWeightError::PlanStateMismatch(transition.page_id));
            }
        }

        for transition in &plan.transitions {
            let page = self.page_mut(transition.page_id)?;
            page.representation_version = transition.to_representation_version;
            page.precision = transition.to_precision;
            page.residency = transition.to_residency;
            page.active = transition.to_active;
            if transition.to_active {
                page.last_access_epoch = plan.epoch;
            }
        }

        self.generation = self.generation.saturating_add(1);
        self.validate_current_budgets()?;
        Ok(())
    }

    /// Computes exact current storage and active-set accounting.
    pub fn accounting(&self) -> Result<ElasticWeightAccounting, ElasticWeightError> {
        let mut accounting = ElasticWeightAccounting::default();

        for page in self.pages.iter().copied() {
            accounting.total_parameters =
                checked_add(accounting.total_parameters, page.parameters)?;
            match page.precision {
                WeightPrecision::Boolean => {
                    accounting.boolean_parameters =
                        checked_add(accounting.boolean_parameters, page.parameters)?;
                }
                WeightPrecision::Ternary => {
                    accounting.ternary_parameters =
                        checked_add(accounting.ternary_parameters, page.parameters)?;
                }
                WeightPrecision::Residual4 => {
                    accounting.residual4_parameters =
                        checked_add(accounting.residual4_parameters, page.parameters)?;
                }
            }

            let bytes = page.payload_bytes_ceil()?;
            match page.residency {
                ResidencyTier::Disk => {
                    accounting.disk_bytes = checked_add(accounting.disk_bytes, bytes)?;
                }
                ResidencyTier::Ram => {
                    accounting.ram_bytes = checked_add(accounting.ram_bytes, bytes)?;
                }
                ResidencyTier::Vram => {
                    accounting.vram_bytes = checked_add(accounting.vram_bytes, bytes)?;
                }
            }

            if page.active {
                if page.residency == ResidencyTier::Disk {
                    return Err(ElasticWeightError::ActivePageOnDisk(page.page_id));
                }
                accounting.active_parameters =
                    checked_add(accounting.active_parameters, page.parameters)?;
                accounting.active_storage_bits = accounting
                    .active_storage_bits
                    .checked_add(page.payload_bits())
                    .ok_or(ElasticWeightError::ArithmeticOverflow)?;
            }
        }

        Ok(accounting)
    }

    fn page_index(&self, page_id: PageId) -> Result<usize, ElasticWeightError> {
        self.pages
            .iter()
            .position(|page| page.page_id == page_id)
            .ok_or(ElasticWeightError::UnknownPage(page_id))
    }

    fn page(&self, page_id: PageId) -> Result<&ElasticWeightPage, ElasticWeightError> {
        let index = self.page_index(page_id)?;
        Ok(&self.pages[index])
    }

    fn page_mut(&mut self, page_id: PageId) -> Result<&mut ElasticWeightPage, ElasticWeightError> {
        let index = self.page_index(page_id)?;
        Ok(&mut self.pages[index])
    }

    fn validate_current_budgets(&self) -> Result<(), ElasticWeightError> {
        let accounting = self.accounting()?;
        if accounting.ram_bytes > self.policy.ram_limit_bytes {
            return Err(ElasticWeightError::RamBudget {
                actual: accounting.ram_bytes,
                maximum: self.policy.ram_limit_bytes,
            });
        }
        if accounting.vram_bytes > self.policy.vram_limit_bytes {
            return Err(ElasticWeightError::VramBudget {
                actual: accounting.vram_bytes,
                maximum: self.policy.vram_limit_bytes,
            });
        }
        if accounting.active_parameters > self.policy.max_active_parameters {
            return Err(ElasticWeightError::ActiveParameterBudget {
                actual: accounting.active_parameters,
                maximum: self.policy.max_active_parameters,
            });
        }
        Ok(())
    }
}

fn precision_for_importance(importance_ppm: u32, policy: ElasticWeightPolicy) -> WeightPrecision {
    if importance_ppm >= policy.residual4_threshold_ppm {
        WeightPrecision::Residual4
    } else if importance_ppm >= policy.ternary_threshold_ppm {
        WeightPrecision::Ternary
    } else {
        WeightPrecision::Boolean
    }
}

fn payload_bytes(parameters: u64, precision: WeightPrecision) -> Result<u64, ElasticWeightError> {
    let bits = parameters as u128 * precision.bits_per_parameter() as u128;
    u64::try_from(bits.div_ceil(8)).map_err(|_| ElasticWeightError::ArithmeticOverflow)
}

const fn fits(current: u64, additional: u64, limit: u64) -> bool {
    match current.checked_add(additional) {
        Some(total) => total <= limit,
        None => false,
    }
}

fn checked_add(left: u64, right: u64) -> Result<u64, ElasticWeightError> {
    left.checked_add(right)
        .ok_or(ElasticWeightError::ArithmeticOverflow)
}

fn validate_plan_field(
    field: ElasticWeightPlanField,
    expected: u128,
    actual: u128,
) -> Result<(), ElasticWeightError> {
    if expected != actual {
        return Err(ElasticWeightError::PlanAccounting {
            field,
            expected,
            actual,
        });
    }
    Ok(())
}

fn validate_importance(page_id: PageId, importance_ppm: u32) -> Result<(), ElasticWeightError> {
    if importance_ppm > IMPORTANCE_SCALE_PPM {
        return Err(ElasticWeightError::ImportanceOutOfRange {
            page_id,
            actual: importance_ppm,
        });
    }
    Ok(())
}

/// Invalid elastic-weight metadata, policy, plan, or capacity state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElasticWeightError {
    ZeroParameters(PageId),
    ImportanceOutOfRange {
        page_id: PageId,
        actual: u32,
    },
    InvalidPolicyThresholds,
    InvalidPlanContract,
    DuplicatePageId(PageId),
    DuplicatePlanPageId(PageId),
    DuplicateSelection(PageId),
    UnknownPage(PageId),
    ActivePageOnDisk(PageId),
    ActiveParameterBudget {
        actual: u64,
        maximum: u64,
    },
    ActiveResidencyBudget {
        page_id: PageId,
        payload_bytes: u64,
        ram_remaining: u64,
        vram_remaining: u64,
    },
    RamBudget {
        actual: u64,
        maximum: u64,
    },
    VramBudget {
        actual: u64,
        maximum: u64,
    },
    StalePlan {
        expected_generation: u64,
        actual_generation: u64,
    },
    PlanPageCount {
        expected: usize,
        actual: usize,
    },
    PlanStateMismatch(PageId),
    RepresentationVersionOverflow(PageId),
    PlanPayloadBytes {
        page_id: PageId,
        expected: u64,
        actual: u64,
    },
    PlanRepresentationVersion {
        page_id: PageId,
        expected: u64,
        actual: u64,
    },
    PlanAccounting {
        field: ElasticWeightPlanField,
        expected: u128,
        actual: u128,
    },
    ArithmeticOverflow,
}

impl fmt::Display for ElasticWeightError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for ElasticWeightError {}
