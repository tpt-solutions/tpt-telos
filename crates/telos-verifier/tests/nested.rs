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
