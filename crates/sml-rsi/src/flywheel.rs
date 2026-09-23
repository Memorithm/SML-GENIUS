use core::fmt;
use std::fmt::Write as _;

use crate::{
    portfolio::{CurriculumPortfolioReport, CurriculumSource},
    post_selection::{PostSelectionOutcome, PostSelectionReport},
};

pub const RSI_FLYWHEEL_SCHEMA_V1: &str = "sml.rsi-flywheel@1.0.0";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlywheelSignal {
    HoldoutQualityImproved,
    EquivalentQualityLowerTrainingWork,
    EquivalentQualityNoEfficiencyGain,
    HoldoutQualityRegressed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RsiFlywheelRecord {
    pub schema: String,
    pub selected_id: String,
    pub selected_source: CurriculumSource,
    pub fixed_control_id: String,
    pub selection_tasks: u64,
    pub selected_selection_exact: u64,
    pub fixed_selection_exact: u64,
    pub holdout_tasks: u64,
    pub selected_holdout_exact: u64,
    pub fixed_holdout_exact: u64,
    pub selected_training_events: u64,
    pub fixed_training_events: u64,
    pub conjunction_table_code: u8,
    pub overwrite_table_code: u8,
    pub recall_table_code: u8,
    pub copy_table_code: u8,
    pub signal: FlywheelSignal,
}

impl RsiFlywheelRecord {
    pub fn from_reports(
        portfolio: &CurriculumPortfolioReport,
        post: &PostSelectionReport,
    ) -> Result<Self, FlywheelError> {
        validate_text(&portfolio.selected_id)?;
        validate_text(&post.selected_id)?;
        validate_text(&post.fixed_control_id)?;
        if portfolio.selected_id != post.selected_id {
            return Err(FlywheelError::SelectedBindingMismatch);
        }
        let selected = portfolio
            .entries
            .get(portfolio.selected_index)
            .ok_or(FlywheelError::InvalidPortfolio)?;
        if selected.candidate_id != portfolio.selected_id {
            return Err(FlywheelError::InvalidPortfolio);
        }
        let fixed = portfolio
            .entries
            .iter()
            .find(|entry| entry.candidate_id == post.fixed_control_id)
            .ok_or(FlywheelError::FixedControlBindingMismatch)?;
        if fixed.source != CurriculumSource::FixedControl {
            return Err(FlywheelError::FixedControlBindingMismatch);
        }

        validate_panel(
            post.holdout_tasks,
            post.selected_holdout.tasks,
            post.selected_holdout.exact_tasks,
        )?;
        validate_panel(
            post.holdout_tasks,
            post.fixed_holdout.tasks,
            post.fixed_holdout.exact_tasks,
        )?;
        if post.selected_training_events != selected.training_events
            || post.fixed_training_events != fixed.training_events
        {
            return Err(FlywheelError::TrainingWorkMismatch);
        }

        let delta = i64::try_from(post.selected_holdout.exact_tasks)
            .and_then(|left| {
                i64::try_from(post.fixed_holdout.exact_tasks).map(|right| left - right)
            })
            .map_err(|_| FlywheelError::Overflow)?;
        if delta != post.exact_task_delta {
            return Err(FlywheelError::OutcomeMismatch);
        }
        let expected_outcome = match delta.cmp(&0) {
            core::cmp::Ordering::Greater => PostSelectionOutcome::SelectedBetter,
            core::cmp::Ordering::Equal => PostSelectionOutcome::Tie,
            core::cmp::Ordering::Less => PostSelectionOutcome::SelectedWorse,
        };
        if post.outcome != expected_outcome {
            return Err(FlywheelError::OutcomeMismatch);
        }

        let signal = match post.outcome {
            PostSelectionOutcome::SelectedBetter => FlywheelSignal::HoldoutQualityImproved,
            PostSelectionOutcome::SelectedWorse => FlywheelSignal::HoldoutQualityRegressed,
            PostSelectionOutcome::Tie
                if post.selected_training_events < post.fixed_training_events =>
            {
                FlywheelSignal::EquivalentQualityLowerTrainingWork
            }
            PostSelectionOutcome::Tie => FlywheelSignal::EquivalentQualityNoEfficiencyGain,
        };

        Ok(Self {
            schema: RSI_FLYWHEEL_SCHEMA_V1.to_string(),
            selected_id: selected.candidate_id.clone(),
            selected_source: selected.source,
            fixed_control_id: fixed.candidate_id.clone(),
            selection_tasks: portfolio.selection_tasks,
            selected_selection_exact: selected.selection.exact_tasks,
            fixed_selection_exact: fixed.selection.exact_tasks,
            holdout_tasks: post.holdout_tasks,
            selected_holdout_exact: post.selected_holdout.exact_tasks,
            fixed_holdout_exact: post.fixed_holdout.exact_tasks,
            selected_training_events: post.selected_training_events,
            fixed_training_events: post.fixed_training_events,
            conjunction_table_code: selected.training.conjunction_table.code(),
            overwrite_table_code: selected.training.overwrite_table.code(),
            recall_table_code: selected.training.recall_table.code(),
            copy_table_code: selected.training.copy_table.code(),
            signal,
        })
    }

    /// Aggregate-only deterministic representation suitable for a future
    /// flywheel dataset. It intentionally contains no holdout examples, case
    /// IDs, raw inputs or expected answers.
    pub fn canonical_text(&self) -> Result<String, FlywheelError> {
        validate_text(&self.selected_id)?;
        validate_text(&self.fixed_control_id)?;
        let mut out = String::new();
        writeln!(&mut out, "schema={}", self.schema).map_err(|_| FlywheelError::Format)?;
        writeln!(&mut out, "selected={}", escape(&self.selected_id))
            .map_err(|_| FlywheelError::Format)?;
        writeln!(&mut out, "selected_source={:?}", self.selected_source)
            .map_err(|_| FlywheelError::Format)?;
        writeln!(&mut out, "fixed={}", escape(&self.fixed_control_id))
            .map_err(|_| FlywheelError::Format)?;
        writeln!(&mut out, "selection_tasks={}", self.selection_tasks)
            .map_err(|_| FlywheelError::Format)?;
        writeln!(
            &mut out,
            "selection_exact={},{}",
            self.selected_selection_exact, self.fixed_selection_exact
        )
        .map_err(|_| FlywheelError::Format)?;
        writeln!(&mut out, "holdout_tasks={}", self.holdout_tasks)
            .map_err(|_| FlywheelError::Format)?;
        writeln!(
            &mut out,
            "holdout_exact={},{}",
            self.selected_holdout_exact, self.fixed_holdout_exact
        )
        .map_err(|_| FlywheelError::Format)?;
        writeln!(
            &mut out,
            "training_events={},{}",
            self.selected_training_events, self.fixed_training_events
        )
        .map_err(|_| FlywheelError::Format)?;
        writeln!(
            &mut out,
            "tables={},{},{},{}",
            self.conjunction_table_code,
            self.overwrite_table_code,
            self.recall_table_code,
            self.copy_table_code
        )
        .map_err(|_| FlywheelError::Format)?;
        writeln!(&mut out, "signal={:?}", self.signal).map_err(|_| FlywheelError::Format)?;
        Ok(out)
    }
}

fn validate_panel(expected_tasks: u64, tasks: u64, exact: u64) -> Result<(), FlywheelError> {
    if expected_tasks == 0 || tasks != expected_tasks || exact > tasks {
        return Err(FlywheelError::InvalidPanelAccounting);
    }
    Ok(())
}

fn validate_text(value: &str) -> Result<(), FlywheelError> {
    if value.trim().is_empty()
        || value.contains('\0')
        || value.contains('\n')
        || value.contains('\r')
    {
        return Err(FlywheelError::InvalidIdentifier);
    }
    Ok(())
}

fn escape(value: &str) -> String {
    value
        .replace('%', "%25")
        .replace('=', "%3d")
        .replace(',', "%2c")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlywheelError {
    InvalidIdentifier,
    InvalidPortfolio,
    SelectedBindingMismatch,
    FixedControlBindingMismatch,
    TrainingWorkMismatch,
    InvalidPanelAccounting,
    OutcomeMismatch,
    Overflow,
    Format,
}

impl fmt::Display for FlywheelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for FlywheelError {}
