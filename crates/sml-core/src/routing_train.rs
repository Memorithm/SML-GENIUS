//! Trainable direct-address routing with a hard Boolean deployment form.
//!
//! Training uses one continuous latent scalar per eventual learned route bit.
//! Frozen inference is discrete and delegates to the existing O(1)
//! `DirectRouteTable`. The current reference table stores `PageId(u32)` entries;
//! learned-bit accounting is semantic and does not claim a bit-packed runtime.

use core::fmt;

use crate::{DirectRouteTable, PageId, ParameterBudget, RouteError};

/// Explicit structural limits for one trainable direct router.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RouteLearningBudget {
    pub max_address_width: u8,
    pub max_pages: u32,
    pub max_learned_bits: u64,
}

impl Default for RouteLearningBudget {
    fn default() -> Self {
        Self {
            max_address_width: 12,
            max_pages: 4_096,
            max_learned_bits: 49_152,
        }
    }
}

/// Deterministic optimization settings for latent route bits.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RouteTrainingConfig {
    pub epochs: u32,
    pub learning_rate: f64,
    pub epsilon: f64,
}

impl Default for RouteTrainingConfig {
    fn default() -> Self {
        Self {
            epochs: 64,
            learning_rate: 0.5,
            epsilon: 1.0e-9,
        }
    }
}

impl RouteTrainingConfig {
    fn validate(self) -> Result<Self, RouteLearningError> {
        if self.epochs == 0 {
            return Err(RouteLearningError::ZeroEpochs);
        }
        if !self.learning_rate.is_finite() || self.learning_rate <= 0.0 {
            return Err(RouteLearningError::InvalidLearningRate);
        }
        if !self.epsilon.is_finite() || self.epsilon <= 0.0 || self.epsilon >= 0.5 {
            return Err(RouteLearningError::InvalidEpsilon);
        }
        Ok(self)
    }
}

/// Exact semantic hard-model and training-only accounting for a learned router.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RouteLearningAccounting {
    pub entry_count: u64,
    pub page_count: u32,
    pub bits_per_route: u8,
    pub learned_boolean_bits: u64,
    pub latent_training_scalars: u64,
}

impl RouteLearningAccounting {
    #[must_use]
    pub const fn parameter_budget(self) -> ParameterBudget {
        ParameterBudget {
            boolean: self.learned_boolean_bits,
            ternary: 0,
            residual4: 0,
        }
    }
}

/// Deterministic work counters for one complete route-training run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RouteTrainingCounters {
    pub epochs: u32,
    pub addresses_seen: u64,
    pub latent_bit_updates: u64,
}

/// Evidence returned by complete-oracle route training.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RouteTrainingReport {
    pub initial_loss: f64,
    pub final_loss: f64,
    pub hard_correct: u64,
    pub address_count: u64,
    pub counters: RouteTrainingCounters,
}

/// Direct-address router whose route bits are continuous only while training.
#[derive(Debug, Clone, PartialEq)]
pub struct LatentDirectRouter {
    address_width: u8,
    page_count: u32,
    bits_per_route: u8,
    logits: Vec<f64>,
}

impl LatentDirectRouter {
    pub fn new(
        address_width: u8,
        page_count: u32,
        budget: RouteLearningBudget,
    ) -> Result<Self, RouteLearningError> {
        if address_width == 0 {
            return Err(RouteLearningError::ZeroAddressWidth);
        }
        if address_width > budget.max_address_width {
            return Err(RouteLearningError::AddressWidthBudget {
                actual: address_width,
                maximum: budget.max_address_width,
            });
        }
        if address_width as u32 >= usize::BITS {
            return Err(RouteLearningError::AddressWidthTooLarge(address_width));
        }
        if page_count < 2 {
            return Err(RouteLearningError::PageCount(page_count));
        }
        if page_count > budget.max_pages {
            return Err(RouteLearningError::PageCountBudget {
                actual: page_count,
                maximum: budget.max_pages,
            });
        }

        let bits_per_route = bits_required(page_count);
        let entry_count = 1_usize << address_width;
        let learned_bits = (entry_count as u64)
            .checked_mul(bits_per_route as u64)
            .ok_or(RouteLearningError::ArithmeticOverflow)?;
        if learned_bits > budget.max_learned_bits {
            return Err(RouteLearningError::LearnedBitsBudget {
                actual: learned_bits,
                maximum: budget.max_learned_bits,
            });
        }

        let latent_count =
            usize::try_from(learned_bits).map_err(|_| RouteLearningError::ArithmeticOverflow)?;
        Ok(Self {
            address_width,
            page_count,
            bits_per_route,
            logits: vec![0.0; latent_count],
        })
    }

    #[must_use]
    pub fn accounting(&self) -> RouteLearningAccounting {
        RouteLearningAccounting {
            entry_count: self.entry_count() as u64,
            page_count: self.page_count,
            bits_per_route: self.bits_per_route,
            learned_boolean_bits: self.logits.len() as u64,
            latent_training_scalars: self.logits.len() as u64,
        }
    }

    #[must_use]
    pub fn entry_count(&self) -> usize {
        1_usize << self.address_width
    }

    pub fn hard_page(&self, address: usize) -> Result<PageId, RouteLearningError> {
        if address >= self.entry_count() {
            return Err(RouteLearningError::AddressOutOfRange {
                address,
                entries: self.entry_count(),
            });
        }

        let offset = address * self.bits_per_route as usize;
        let mut page = 0_u32;
        for bit in 0..self.bits_per_route as usize {
            if self.logits[offset + bit] > 0.0 {
                page |= 1_u32 << bit;
            }
        }
        if page >= self.page_count {
            return Err(RouteLearningError::InvalidHardPage {
                address,
                page,
                page_count: self.page_count,
            });
        }
        Ok(PageId(page))
    }

    /// Freezes the learned Boolean route bits into the hard O(1) runtime table.
    pub fn freeze(&self) -> Result<DirectRouteTable, RouteLearningError> {
        let mut entries = Vec::with_capacity(self.entry_count());
        for address in 0..self.entry_count() {
            entries.push(self.hard_page(address)?);
        }
        DirectRouteTable::new(self.address_width, entries).map_err(RouteLearningError::Route)
    }

    /// Trains against exactly one caller-owned target page for every address.
    pub fn train_complete(
        &mut self,
        targets: &[PageId],
        config: RouteTrainingConfig,
    ) -> Result<RouteTrainingReport, RouteLearningError> {
        let config = config.validate()?;
        self.validate_targets(targets)?;
        let initial_loss = self.loss(targets, config.epsilon)?;
        let mut counters = RouteTrainingCounters::default();

        for _ in 0..config.epochs {
            for (address, target) in targets.iter().copied().enumerate() {
                let offset = address * self.bits_per_route as usize;
                for bit in 0..self.bits_per_route as usize {
                    let target_bit = ((target.0 >> bit) & 1) as f64;
                    let logit = &mut self.logits[offset + bit];
                    let gradient = sigmoid(*logit) - target_bit;
                    *logit -= config.learning_rate * gradient;
                    if !logit.is_finite() {
                        return Err(RouteLearningError::NonFiniteLatent);
                    }
                }
            }
            counters.epochs = counters.epochs.saturating_add(1);
            counters.addresses_seen = counters.addresses_seen.saturating_add(targets.len() as u64);
            counters.latent_bit_updates = counters
                .latent_bit_updates
                .saturating_add(self.logits.len() as u64);
        }

        let mut hard_correct = 0_u64;
        for (address, expected) in targets.iter().copied().enumerate() {
            if self.hard_page(address)? == expected {
                hard_correct = hard_correct.saturating_add(1);
            }
        }

        Ok(RouteTrainingReport {
            initial_loss,
            final_loss: self.loss(targets, config.epsilon)?,
            hard_correct,
            address_count: targets.len() as u64,
            counters,
        })
    }

    fn validate_targets(&self, targets: &[PageId]) -> Result<(), RouteLearningError> {
        if targets.len() != self.entry_count() {
            return Err(RouteLearningError::OracleLength {
                expected: self.entry_count(),
                actual: targets.len(),
            });
        }
        for (address, target) in targets.iter().copied().enumerate() {
            if target.0 >= self.page_count {
                return Err(RouteLearningError::TargetPage {
                    address,
                    page: target.0,
                    page_count: self.page_count,
                });
            }
        }
        Ok(())
    }

    fn loss(&self, targets: &[PageId], epsilon: f64) -> Result<f64, RouteLearningError> {
        self.validate_targets(targets)?;
        let mut loss = 0.0_f64;
        for (address, target) in targets.iter().copied().enumerate() {
            let offset = address * self.bits_per_route as usize;
            for bit in 0..self.bits_per_route as usize {
                let expected = ((target.0 >> bit) & 1) as f64;
                let probability = sigmoid(self.logits[offset + bit]).clamp(epsilon, 1.0 - epsilon);
                loss -= expected * probability.ln() + (1.0 - expected) * (1.0 - probability).ln();
            }
        }
        Ok(loss / self.logits.len() as f64)
    }
}

fn sigmoid(value: f64) -> f64 {
    if value >= 0.0 {
        1.0 / (1.0 + (-value).exp())
    } else {
        let exp = value.exp();
        exp / (1.0 + exp)
    }
}

fn bits_required(cardinality: u32) -> u8 {
    (u32::BITS - (cardinality - 1).leading_zeros()) as u8
}

/// Construction, budget, training, or hard-freeze failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteLearningError {
    ZeroAddressWidth,
    AddressWidthTooLarge(u8),
    AddressWidthBudget {
        actual: u8,
        maximum: u8,
    },
    PageCount(u32),
    PageCountBudget {
        actual: u32,
        maximum: u32,
    },
    LearnedBitsBudget {
        actual: u64,
        maximum: u64,
    },
    ArithmeticOverflow,
    ZeroEpochs,
    InvalidLearningRate,
    InvalidEpsilon,
    OracleLength {
        expected: usize,
        actual: usize,
    },
    TargetPage {
        address: usize,
        page: u32,
        page_count: u32,
    },
    AddressOutOfRange {
        address: usize,
        entries: usize,
    },
    InvalidHardPage {
        address: usize,
        page: u32,
        page_count: u32,
    },
    NonFiniteLatent,
    Route(RouteError),
}

impl fmt::Display for RouteLearningError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for RouteLearningError {}
