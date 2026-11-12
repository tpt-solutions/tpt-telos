//! Regression tests for disjunctive `ensures` (`||`).
//!
//! A DNF branch is a conjunction. The group holds if some branch has every one of its checks
//! entailed. Before this fix, each constraint was an alternative, so `(A && B) || C` passed on
//! `A` alone. These tests pin the correct behaviour, under the default solver.

use tpt_telos_ir::extract;
use tpt_telos_parser::parse;
use tpt_telos_verifier::verify;

/// The first branch is `x >= 1 && y >= 1`, and the second is `z == 0`. Only `x >= 1` is required,
/// so that single conjunct is entailed. `y >= 1` and `z == 0` are not, so the group must FAIL.
/// The old rule passed here because it checked each conjunct as an alternative.
const CONJUNCTIVE_BRANCH_FAILS: &str = r#"
module M {
    func f(x: Int, y: Int, z: Int)
        requires x >= 1
        ensures (x >= 1 && y >= 1) || z == 0
    {
    }
}
"#;

/// Same shape, but the branch `x >= 0 && y >= 1` is provable from the requires alone:
/// `x >= 0` gives branch one's first conjunct, and `y >= 1` is required, so branch one holds.
const CONJUNCTIVE_BRANCH_HOLDS: &str = r#"
module M {
    func f(x: Int, y: Int)
        requires x >= 0
        requires y >= 1
        ensures (x >= 0 && y >= 1) || x == 7
    {
    }
}
"#;

fn verify_src(src: &str) -> bool {
    let modules = parse(src).expect("parse");
    let problems = extract(&modules).expect("extract");
    assert!(!problems.is_empty());
    problems.iter().all(|p| verify(p).all_passed)
}

#[test]
fn conjunctive_branch_is_not_proven_by_one_conjunct() {
    // Only `x >= 1` could come from the clause, and `x == 0` is not implied, so the group fails.
    assert!(!verify_src(CONJUNCTIVE_BRANCH_FAILS));
}

#[test]
fn conjunctive_branch_holds_when_all_its_conjuncts_are_entailed() {
    assert!(verify_src(CONJUNCTIVE_BRANCH_HOLDS));
}
