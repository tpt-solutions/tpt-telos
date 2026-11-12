//! Regression tests for statement lowering.
//!
//! 1. A bare expression statement (such as the trailing value of a function) has no effect on state.
//!    It used to be lowered as an assignment of `0`, which made a false claim `tokens == 0` pass.
//! 2. A field assigned more than once on one path is refused, because the repeated equations are not
//!    verified soundly.

use tpt_telos_ir::extract;
use tpt_telos_parser::parse;
use tpt_telos_verifier::verify;

fn verify_src(src: &str) -> bool {
    let modules = parse(src).expect("parse");
    let problems = extract(&modules).expect("extract");
    problems.iter().all(|p| verify(p).all_passed)
}

fn with_ensures(ensures: &str) -> String {
    format!(
        r#"
@boundary(cpu_bound)
@state(persistent)
module M {{
    invariant Bucket {{
        tokens >= 0
    }}

    func consume(mut bucket: Bucket, cost: i64) -> i64
        requires cost <= bucket.tokens
        ensures {ensures}
    {{
        mutate state {{
            bucket.tokens = old(bucket.tokens) - cost;
        }}
        bucket.tokens
    }}
}}
"#
    )
}

#[test]
fn true_claim_about_the_assignment_verifies() {
    assert!(verify_src(&with_ensures("bucket.tokens == old(bucket.tokens) - cost")));
}

#[test]
fn false_claim_is_not_made_true_by_the_trailing_expression() {
    // The trailing `bucket.tokens` is a value, not an assignment. `tokens == 0` is false in general.
    assert!(!verify_src(&with_ensures("bucket.tokens == 0")));
}

#[test]
fn repeated_field_assignment_on_one_path_is_refused() {
    let src = r#"
module M {
    invariant Account {
        balance >= 0
    }

    func f(mut acc: Account, amount: i64)
        requires amount >= 0
        requires acc.balance >= amount
        ensures acc.balance >= 0
    {
        mutate state {
            acc.balance = old(acc.balance) - amount;
        }
        mutate state {
            acc.balance = old(acc.balance) + amount + 1;
        }
    }
}
"#;
    let modules = parse(src).expect("parse");
    let err = extract(&modules).expect_err("repeated assignment must be refused");
    assert!(err.contains("assigned more than once"), "unexpected error: {err}");
}
