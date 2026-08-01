//! tpt-telos formal verifier (QF_LRA).
//!
//! Provides a self-contained SMT-style engine that decides whether a set of
//! linear arithmetic constraints is unsatisfiable, and uses it to prove that
//! each `ensures` clause and invariant of a function follows from its
//! `requires` and `mutate state` assignments.
//!
//! The default solver is an internal Fourier-Motzkin variable elimination
//! engine. When the `z3` feature is enabled, an alternative Z3-backed solver
//! is available via [`SolverBackend::Z3`].

pub mod cluster;
pub mod solver;
pub mod verify;

#[cfg(feature = "z3")]
pub mod z3_solver;

pub use solver::{counterexample, entails, model, negate, satisfies_model, unsat, Model};
pub use verify::{is_unsat, verify, CheckResult, VerificationResult};

/// The solver backend to use for verification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SolverBackend {
    /// Built-in Fourier-Motzkin variable elimination (default, no external
    /// dependencies).
    #[default]
    FourierMotzkin,
    /// Z3 SMT solver (requires the `z3` feature and the Z3 shared library
    /// to be installed).
    #[cfg(feature = "z3")]
    Z3,
}

/// Global solver backend setting. Defaults to Fourier-Motzkin.
static mut SOLVER_BACKEND: SolverBackend = SolverBackend::FourierMotzkin;

/// Set the global solver backend.
///
/// # Safety
///
/// This uses a static mutable variable and is not thread-safe. Call only
/// during single-threaded initialization (e.g., at CLI startup).
pub fn set_solver_backend(backend: SolverBackend) {
    // SAFETY: called during single-threaded CLI initialization.
    unsafe {
        SOLVER_BACKEND = backend;
    }
}

/// Get the current solver backend.
pub fn solver_backend() -> SolverBackend {
    // SAFETY: read is atomic on all supported platforms for single-word enums.
    unsafe { SOLVER_BACKEND }
}
