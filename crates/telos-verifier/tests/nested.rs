//! Integration test for `.telos` fixtures beyond `wallet`/`broken`.
//!
//! Exercises `nested.telos`, which contains a function with nested arithmetic
//! (constant multiplication, `old()` inside `ensures`) and another using `&&`
//! flattening in `requires`. Both are expected to verify.

use tpt_telos_ir::extract;
use tpt_telos_parser::parse;
use tpt_telos_verifier::verify;

#[test]
fn nested_example_passes() {
    let src = std::fs::read_to_string("../../examples/nested.telos").unwrap();
    let modules = parse(&src).unwrap();
    let problems = extract(&modules).unwrap();
    assert_eq!(problems.len(), 2);

    let mut by_name: std::collections::HashMap<_, _> = problems
        .into_iter()
        .map(|p| (p.func_name.clone(), p))
        .collect();

    for name in ["compound", "guarded"] {
        let p = by_name
            .remove(name)
            .unwrap_or_else(|| panic!("missing {name}"));
        let r = verify(&p);
        assert!(r.all_passed, "{name} should verify, got {:?}", r);
    }
}

#[test]
fn compound_has_expected_problem_shape() {
    let src = std::fs::read_to_string("../../examples/nested.telos").unwrap();
    let modules = parse(&src).unwrap();
    let problems = extract(&modules).unwrap();
    let compound = problems
        .iter()
        .find(|p| p.func_name == "compound")
        .expect("compound function present");
    // requires c.value >= 0, plus the PositiveInt constraints on `a`/`b`.
    assert!(compound.premises.len() >= 3);
    // one `ensures` clause (plus the maintained `Counter` invariant conclusion).
    assert_eq!(
        compound.conclusions.iter().filter(|c| c.is_ensures).count(),
        1
    );
    assert!(compound.conclusions.len() >= 2);
}

#[test]
fn disjunction_example_passes() {
    let src = std::fs::read_to_string("../../examples/disjunction.telos").unwrap();
    let modules = parse(&src).unwrap();
    let problems = extract(&modules).unwrap();
    // Disjunction in `requires` produces 2 premise branches (`flag == 1` /
    // `flag == 2`); the function body's own `if flag == 1 { .. } else { .. }`
    // produces 2 more (guarded by the condition and its negation). Every
    // premise branch is paired with every body branch, so this is 2 * 2 = 4
    // problems — two of them (`flag == 1` paired with the mutation-`else`
    // guard, and vice versa) have contradictory premises and verify
    // vacuously, since this extraction layer doesn't prune infeasible
    // branches (that would require calling back into the solver, which
    // `telos-ir` doesn't depend on). All 4 must still verify.
    assert_eq!(
        problems.len(),
        4,
        "expected 4 problems: 2 requires-branches * 2 body if/else-branches"
    );

    for p in &problems {
        eprintln!("=== {} ===", p.func_name);
        eprintln!("Premises ({}):", p.premises.len());
        for c in &p.premises {
            eprintln!("  {:?}", c);
        }
        eprintln!("Conclusions ({}):", p.conclusions.len());
        for c in &p.conclusions {
            eprintln!("  or_group={:?} {}", c.or_group, c.description);
            eprintln!("    {:?}", c.constraint);
        }
        let r = verify(p);
        eprintln!("Result: all_passed={}", r.all_passed);
        for check in &r.checks {
            eprintln!("  {} passed={}", check.description, check.passed);
        }
        eprintln!();
        assert!(
            r.all_passed,
            "{} should verify (disjunction ensures), got {:?}",
            p.func_name, r
        );
    }
}
