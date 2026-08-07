# tpt-telos-sdk

**Programmatic orchestration API over the tpt-telos parse → transpile → codegen → attest pipeline.**

Part of the [tpt-telos](https://github.com/tpt-solutions/tpt-telos) compiler workspace.

## Overview

`telos`'s library crates (`parser`, `ir`, `verifier`, `router`, `agent`, `codegen`) are individually
excellent but require a consumer to wire them together by hand. `tpt-telos-sdk` provides a single
entry point for integration harnesses (e.g. `tpt-nexus`) that want the whole pipeline as one call,
plus the missing "agent_hint" surface (human/LLM-readable counterexample formatting) and a build
step that reads back compiled artifact bytes.

## Usage

```rust
use tpt_telos_sdk::{compile, format_outcome_hints, StaticAgent};

let src = r#"
    module Bank {
        invariant Wallet { balance >= 0 }
        func deposit(w: Wallet, amount: PositiveInt)
            ensures w.balance == old(w.balance) + amount
        ;
    }
"#;

// One call: parse -> agentic transpile -> codegen -> attest.
let artifact = compile(src, &StaticAgent::new()).unwrap();
if !artifact.all_verified {
    // Verification failure is *not* an error — surface the hints instead.
    for outcome in &artifact.outcomes {
        eprintln!("{}", format_outcome_hints(outcome));
    }
}

// artifacts:
//   artifact.modules   — parsed AST
//   artifact.outcomes  — per-function transpilation results
//   artifact.project   — assembled Rust/Go/FFI sources (write + build via compile_project)
//   artifact.manifest  — attestation manifest (source hash + per-fn record)
//   artifact.all_verified
```

`compile_static` accepts raw bytes; `compile_project` / `compile_project_tempdir` write the
[`Project`](crate::Project) to disk and shell `cargo build` / `go build`, returning the compiled
Rust artifact bytes.

## Errors

`SdkError` distinguishes pipeline failures (`Parse`, `Transpile`, `Codegen`, `Io`,
`ToolNotFound`) from genuine verification failure, which is reported via
[`VerifiedArtifact::all_verified`](crate::VerifiedArtifact::all_verified) rather than as an error.
