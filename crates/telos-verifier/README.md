# tpt-telos-verifier

**Self-contained SMT-style verifier for tpt-telos contracts — no Z3 required.**

Part of the [tpt-telos](https://github.com/tpt-solutions/tpt-telos) compiler workspace.

## Overview

`tpt-telos-verifier` is a fully self-contained formal reasoning engine over QF_LRA (quantifier-free
linear rational arithmetic). It uses Fourier–Motzkin variable elimination to decide whether a set of
linear premises entails a linear conclusion, with no dependency on Z3, CVC5, or any other external
solver.

Given a `VerificationProblem` from `tpt-telos-ir`, `verify` checks every `ensures` clause and
invariant. On failure it extracts a concrete counter-example `Model` — a variable assignment that
satisfies the premises but violates the conclusion — which the agent in `tpt-telos-agent` uses to
drive the rewrite step.

## Usage

```rust
use tpt_telos_parser::parse;
use tpt_telos_ir::extract;
use tpt_telos_verifier::verify;

let modules = parse(src).unwrap();
let problems = extract(&modules);

for problem in &problems {
    let result = verify(problem);
    if result.all_passed {
        println!("{}: all checks passed", result.func_name);
    } else {
        for check in result.checks.iter().filter(|c| !c.passed) {
            println!("FAIL {}: {}", result.func_name, check.description);
        }
    }
}
```

## Key API

| Item | Description |
|------|-------------|
| `verify(problem) -> VerificationResult` | Check all conclusions for one function |
| `entails(premises, conclusion) -> bool` | Core Fourier–Motzkin entailment check |
| `counterexample(premises, conclusion) -> Option<Model>` | Concrete failing assignment |
| `VerificationResult` | `func_name`, `checks`, `all_passed` |
| `CheckResult` | `description`, `passed`, `is_ensures` |
| `Model` | Map of variable name → rational value |

## License

Licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE) at your option.
