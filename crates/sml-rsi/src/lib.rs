#![forbid(unsafe_code)]

//! Native, dependency-free governance primitives for SML-GENIUS recursive
//! self-improvement.
//!
//! This crate is intentionally independent from Memorithm/RSI. It may implement
//! similar ideas, but it has no build/runtime/orchestration dependency on that
//! repository.

pub mod curriculum;
pub mod cycle;
pub mod flywheel;
pub mod manifest;
pub mod matched_training;
pub mod multi_task_training;
pub mod portfolio;
pub mod post_selection;
pub mod promotion;
