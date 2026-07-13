//! tpt-telos formal verifier (QF_LRA).
//!
//! Provides a self-contained SMT-style engine that decides whether a set of
//! linear arithmetic constraints is unsatisfiable, and uses it to prove that
//! each `ensures` clause and invariant of a function follows from its
//! `requires` and `mutate state` assignments.

pub mod solver;
pub mod verify;

pub use solver::{entails, unsat, model, satisfies_model, counterexample, Model};
pub use verify::{verify, VerificationResult, CheckResult, is_unsat};
