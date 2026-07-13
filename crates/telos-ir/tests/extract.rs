//! Constraint-extraction (AST -> IR) unit tests for tpt-telos.
//!
//! Covers the [`Linear`] arithmetic helpers, and [`extract`] for a range of
//! inputs: `requires`/`ensures` lowering, `Positive*` parameter constraints,
//! invariant entry/maintenance, frame axioms, `old(...)` and nested arithmetic,
//! and the rejection of unsupported (non-linear) constructs.

use telos_ir::{extract, Constraint, Linear, Relation};
use telos_parser::ast::*;
use telos_parser::parse;

// ---- helpers -------------------------------------------------------------

#[test]
fn linear_add_and_sub() {
    // (x + y) - (x + y) = 0  =>  empty terms
    let a = Linear::var("x").add(&Linear::var("y")); // x + y
    let b = Linear::var("x").add(&Linear::var("y")); // x + y
    let diff = a.sub(&b);
    assert!(diff.terms.is_empty());
    assert_eq!(diff.constant, 0);

    // x - (x + y) = -y
    let diff2 = Linear::var("x").sub(&a);
    assert_eq!(diff2.terms, vec![("y".to_string(), -1)]);
    assert_eq!(diff2.constant, 0);
}

#[test]
fn linear_scale_and_neg() {
    let s = Linear::var("x").scale(3);
    assert_eq!(s.terms, vec![("x".to_string(), 3)]);

    let n = Linear::var("x").neg();
    assert_eq!(n.terms, vec![("x".to_string(), -1)]);

    // scale distributes over the constant too
    let c = Linear::var("x").add(&Linear::constant_only(4)).scale(2);
    assert_eq!(c.terms, vec![("x".to_string(), 2)]);
    assert_eq!(c.constant, 8);

    // negating removes zero-coefficient terms
    let z = Linear::var("x").add(&Linear::var("x").neg());
    assert!(z.terms.is_empty());
}

#[test]
fn linear_constant_only() {
    let c = Linear::constant_only(7);
    assert!(c.terms.is_empty());
    assert_eq!(c.constant, 7);
}

// ---- extract --------------------------------------------------------------

fn first_problem(src: &str) -> telos_ir::VerificationProblem {
    let modules = parse(src).unwrap();
    let mut probs = extract(&modules).unwrap();
    assert_eq!(probs.len(), 1, "expected exactly one function problem");
    probs.remove(0)
}

#[test]
fn extract_requires_premise() {
    // requires x >= 0  =>  (x) - 0 >= 0
    let p = first_problem("module M { func f(x: i64) requires x >= 0 { } }");
    assert!(p.premises.iter().any(|Constraint(l, r)| {
        *r == Relation::Ge && l.terms == vec![("x".to_string(), 1)] && l.constant == 0
    }));
}

#[test]
fn extract_positive_param_constraint() {
    // A `Positive*` parameter adds the premise `x >= 1`.
    let p = first_problem("module M { func f(x: PositiveInt) { } }");
    assert!(p.premises.iter().any(|Constraint(l, r)| {
        *r == Relation::Ge && l.terms == vec![("x".to_string(), 1)] && l.constant == -1
    }));
}

#[test]
fn extract_invariant_premise_for_param() {
    // A parameter whose type has an invariant gets that invariant as a premise.
    let p = first_problem(
        "module M { invariant Wallet { balance >= 0 } func f(w: Wallet) { } }",
    );
    assert!(p.premises.iter().any(|Constraint(l, r)| {
        *r == Relation::Ge && l.terms == vec![("w.balance".to_string(), 1)] && l.constant == 0
    }));
}

#[test]
fn extract_maintained_invariant_conclusion() {
    // The invariant must be re-established in the post-state (is_ensures=false).
    let p = first_problem(
        "module M { invariant Wallet { balance >= 0 } func f(w: Wallet) { } }",
    );
    assert!(!p.conclusions.is_empty());
    assert!(p.conclusions.iter().any(|c| !c.is_ensures));
    assert!(p.conclusions.iter().any(|c| {
        matches!(c.constraint, Constraint(Linear { terms, .. }, Relation::Ge)
            if terms == vec![("w.balance'".to_string(), 1)])
    }));
}

#[test]
fn extract_ensures_conclusion() {
    let p = first_problem("module M { func f(x: i64) ensures x == 0 { } }");
    assert!(p.conclusions.iter().any(|c| {
        c.is_ensures
            && matches!(c.constraint, Constraint(Linear { terms, .. }, Relation::Eq)
                if terms.contains(&("x'".to_string(), 1)))
    }));
}

#[test]
fn extract_frame_axiom_for_unmutated_param() {
    // An unassigned scalar keeps its value across the call.
    let p = first_problem("module M { func f(x: i64) { } }");
    assert!(p.premises.iter().any(|Constraint(l, r)| {
        *r == Relation::Eq
            && l.terms.contains(&("x'".to_string(), 1))
            && l.terms.contains(&("x".to_string(), -1))
    }));
}

#[test]
fn extract_body_mutation_premise() {
    // `mutate state { w.balance -= amount }` sets w.balance' = w.balance - amount.
    let p = first_problem(
        "module M { invariant W { balance >= 0 } func f(w: W, amount: i64) { mutate state { w.balance -= amount } } }",
    );
    assert!(p.premises.iter().any(|Constraint(l, r)| {
        *r == Relation::Eq
            && l.terms.contains(&("w.balance'".to_string(), 1))
            && l.terms.contains(&("w.balance".to_string(), -1))
            && l.terms.contains(&("amount".to_string(), 1))
    }));
}

#[test]
fn extract_empty_contracts_ok() {
    // A function with no contracts still lowers and yields no conclusions.
    let p = first_problem("module M { func f(x: i64) { } }");
    assert!(p.conclusions.is_empty());
    // But it still gets a frame axiom for its parameter.
    assert!(!p.premises.is_empty());
}

#[test]
fn extract_old_in_arithmetic() {
    // ensures c.v == old(c.v) * 2  references the post-state field `c.v'`
    // and the pre-state field `c.v`.
    let p = first_problem(
        "module M { invariant C { v >= 0 } \
         func f(c: C) ensures c.v == old(c.v) * 2 { } }",
    );
    assert!(p.conclusions.iter().any(|c| {
        matches!(c.constraint, Constraint(Linear { terms, .. }, Relation::Eq)
            if terms.iter().any(|(n, _)| n == "c.v'"))
    }));
    // No mutation, so a frame axiom keeps c.v' == c.v.
    assert!(p.premises.iter().any(|Constraint(l, r)| {
        *r == Relation::Eq && l.terms.contains(&("c.v'".to_string(), 1))
    }));
}

#[test]
fn extract_multiple_modules() {
    let src = "module A { func f(x: i64) requires x >= 0 { } }
                module B { func g(y: i64) ensures y == 0 { } }";
    let probs = extract(&parse(src).unwrap()).unwrap();
    assert_eq!(probs.len(), 2);
    assert_eq!(probs[0].func_name, "f");
    assert_eq!(probs[1].func_name, "g");
}

#[test]
fn extract_rejects_nonlinear_multiplication() {
    // `x * x` is non-linear and must be rejected during extraction.
    let src = "module M { func f(x: i64) ensures x * x == 0 { } }";
    assert!(extract(&parse(src).unwrap()).is_err());
}

#[test]
fn extract_rejects_division_by_variable() {
    // Division is only supported by a constant in constraints.
    let src = "module M { func f(x: i64, y: i64) ensures x / y == 0 { } }";
    assert!(extract(&parse(src).unwrap()).is_err());
}

#[test]
fn extract_rejects_division_by_zero() {
    let src = "module M { func f(x: i64) ensures x / 0 == 0 { } }";
    assert!(extract(&parse(src).unwrap()).is_err());
}
