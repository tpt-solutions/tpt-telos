//! Top-level verification driver over a `VerificationProblem`.

use crate::solver::{entails, unsat};
use telos_ir::{Constraint, VerificationProblem};

#[derive(Debug, Clone)]
pub struct CheckResult {
    pub description: String,
    pub passed: bool,
    pub is_ensures: bool,
}

#[derive(Debug, Clone)]
pub struct VerificationResult {
    pub func_name: String,
    pub checks: Vec<CheckResult>,
    pub all_passed: bool,
}

/// Verify every conclusion of a single function.
pub fn verify(problem: &VerificationProblem) -> VerificationResult {
    let mut checks = Vec::new();
    let mut all_passed = true;
    for concl in &problem.conclusions {
        let passed = entails(&problem.premises, &concl.constraint);
        if !passed {
            all_passed = false;
        }
        checks.push(CheckResult {
            description: concl.description.clone(),
            passed,
            is_ensures: concl.is_ensures,
        });
    }
    VerificationResult {
        func_name: problem.func_name.clone(),
        checks,
        all_passed,
    }
}

/// Convenience: is this constraint set already contradictory?
pub fn is_unsat(cs: &[Constraint]) -> bool {
    unsat(cs)
}
