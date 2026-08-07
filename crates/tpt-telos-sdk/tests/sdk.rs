//! End-to-end integration tests for the `tpt-telos-sdk` pipeline.
//!
//! These exercise the headless `compile` / `compile_static` path only (no
//! `cargo`/`go` builds), mirroring how `tpt-nexus` drives the SDK.

use tpt_telos_sdk::{compile, compile_static, format_outcome_hints, StaticAgent};

fn fixture(name: &str) -> String {
    std::fs::read_to_string(format!("../../examples/{name}"))
        .unwrap_or_else(|e| panic!("cannot read examples/{name}: {e}"))
}

#[test]
fn wallet_verifies_end_to_end() {
    let src = fixture("wallet.telos");
    let artifact = compile(&src, &StaticAgent::new()).unwrap();
    assert!(
        artifact.all_verified,
        "wallet.telos should be fully verified"
    );
}

#[test]
fn unsatisfiable_fails_with_contradiction_hint() {
    let src = fixture("unsatisfiable.telos");
    let artifact = compile(&src, &StaticAgent::new()).unwrap();
    assert!(
        !artifact.all_verified,
        "unsatisfiable.telos must not verify"
    );
    let hints = format_outcome_hints(&artifact.outcomes[0]);
    assert!(
        hints.contains("FAILED"),
        "hint should report the failure: {hints}"
    );
    assert!(
        hints.contains("counterexample"),
        "hint should surface the contradiction: {hints}"
    );
}

#[test]
fn broken_is_silently_repaired() {
    let src = fixture("broken.telos");
    let artifact = compile(&src, &StaticAgent::new()).unwrap();
    assert!(
        artifact.all_verified,
        "broken.telos should be silently repaired by the agentic loop"
    );
}

#[test]
fn compile_static_accepts_bytes() {
    let bytes = b"module Bank { func noop() ; }";
    let artifact = compile_static(bytes, &StaticAgent::new()).unwrap();
    assert!(artifact.all_verified);
}
