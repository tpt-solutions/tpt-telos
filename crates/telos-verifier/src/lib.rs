//! tpt-telos formal verifier (QF_LRA).
//!
//! Provides a self-contained SMT-style engine that decides whether a set of
//! linear arithmetic constraints is unsatisfiable, and uses it to prove that
//! each `ensures` clause and invariant of a function follows from its
//! `requires` and `mutate state` assignments.

pub mod solver;
pub mod verify;

pub use solver::{counterexample, entails, model, satisfies_model, unsat, Model};
pub use verify::{is_unsat, verify, CheckResult, VerificationResult};
