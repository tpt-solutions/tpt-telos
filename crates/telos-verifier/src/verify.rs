//! Top-level verification driver over a `VerificationProblem`.

use crate::solver::{entails, unsat};
use tpt_telos_ir::{Constraint, VerificationProblem};

/// The outcome of verifying one conclusion (an `ensures` clause or invariant).
#[derive(Debug, Clone)]
pub struct CheckResult {
    pub description: String,
    pub passed: bool,
    pub is_ensures: bool,
    /// Mirrors [`tpt_telos_ir::Conclusion::is_approximation`]: true when the
    /// constraint was proved via interval-arithmetic bounding of a nonlinear
    /// product rather than exact linear arithmetic.
    pub is_approximation: bool,
}

/// The aggregate verification result for a single function.
#[derive(Debug, Clone)]
pub struct VerificationResult {
    pub func_name: String,
    pub checks: Vec<CheckResult>,
    pub all_passed: bool,
}

/// Verify every conclusion of a single function.
///
/// # Examples
///
/// ```
/// use tpt_telos_parser::parse;
/// use tpt_telos_ir::extract;
/// use tpt_telos_verifier::verify;
///
/// let src = r#"
///     module Bank {
///         invariant Wallet { balance >= 0 }
///         func deposit(w: Wallet, amount: PositiveInt)
///             requires amount > 0
///             ensures w.balance == old(w.balance) + amount
///         { mutate state { w.balance += amount } }
///     }
/// "#;
///
/// let modules = parse(src).unwrap();
/// let problems = extract(&modules).unwrap();
/// let result = verify(&problems[0]);
///
/// assert_eq!(result.func_name, "deposit");
/// assert!(result.all_passed);
/// assert!(result.checks.iter().all(|c| c.passed));
/// ```
pub fn verify(problem: &VerificationProblem) -> VerificationResult {
    use std::collections::HashMap;

    let mut checks = Vec::new();
    let mut all_passed = true;

    // First pass: check independent conclusions (or_group: None).
    for concl in &problem.conclusions {
        if concl.or_group.is_some() {
            continue; // handled in second pass
        }
        let passed = entails(&problem.premises, &concl.constraint);
        if !passed {
            all_passed = false;
        }
        checks.push(CheckResult {
            description: concl.description.clone(),
            passed,
            is_ensures: concl.is_ensures,
            is_approximation: concl.is_approximation,
        });
    }

    // Second pass: handle disjunction groups. For each group, at least one
    // conclusion must be entailed.
    let mut groups: HashMap<usize, Vec<&tpt_telos_ir::Conclusion>> = HashMap::new();
    for concl in &problem.conclusions {
        if let Some(g) = concl.or_group {
            groups.entry(g).or_default().push(concl);
    }
    }
    for (_group_id, group_conclusions) in &groups {
        let mut any_passed = false;
        let mut group_results: Vec<CheckResult> = Vec::new();
        for concl in group_conclusions {
            let passed = entails(&problem.premises, &concl.constraint);
            if passed {
                any_passed = true;
            }
            group_results.push(CheckResult {
                description: concl.description.clone(),
                passed,
                is_ensures: concl.is_ensures,
                is_approximation: concl.is_approximation,
            });
        }
        if !any_passed {
            all_passed = false;
        }
        checks.extend(group_results);
    }

    VerificationResult {
        func_name: problem.func_name.clone(),
        checks,
        all_passed,
    }
}

/// Convenience: is this constraint set already contradictory?
///
/// # Examples
///
/// ```
/// use tpt_telos_ir::{Constraint, Linear, Relation};
/// use tpt_telos_verifier::is_unsat;
///
/// let ge1 = Constraint(Linear::var("x").sub(&Linear::constant_only(1)), Relation::Ge);
/// let le0 = Constraint(Linear::var("x"), Relation::Le);
/// assert!(is_unsat(&[ge1, le0]));
/// ```
pub fn is_unsat(cs: &[Constraint]) -> bool {
    unsat(cs)
}
