#![forbid(unsafe_code)]

//! SML-TUTOR massive continual-learning engine (P1 foundation).
//!
//! The crate is std-only (plus `sml-core` for SHA-256) and separates four roles
//! that must never be merged:
//!
//! * the **scheduler** ([`scheduler`], [`executor`]) decides *what* runs and
//!   *where*; it can never certify anything;
//! * **teachers** ([`teacher`]) propose lessons and are never trusted;
//! * the **examiner** ([`mastery`]) is the only code that can mint a
//!   [`mastery::Certificate`], and its admission rule (every cohort exactly
//!   100 %, abstention counted as failure) has no tunable threshold;
//! * the **consolidator** ([`store`]) only accepts certified entries and
//!   publishes them with compare-and-swap transactions.
//!
//! What this crate does *not* claim: it does not by itself teach French,
//! English, Mathematics or Rust. It is the orchestration and memory substrate
//! that native learners plug into. The built-in [`anf_lesson`] learner is a
//! native GF(2) ANF induction workload used to qualify the engine.

pub mod anf_lesson;
pub mod domain;
pub mod engine;
pub mod executor;
pub mod graph;
pub mod hash;
pub mod host_probe;
pub mod journal;
pub mod lesson;
pub mod mastery;
pub mod registry;
pub mod resources;
pub mod scheduler;
pub mod store;
pub mod supervisor;
pub mod teacher;
pub mod telemetry;
