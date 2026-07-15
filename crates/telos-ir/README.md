# tpt-telos-ir

**Intermediate representation and constraint extraction for tpt-telos.**

Part of the [tpt-telos](https://github.com/tpt-solutions/tpt-telos) compiler workspace.

## Overview

`tpt-telos-ir` sits between the parser and the verifier. It lowers the typed AST produced by
`tpt-telos-parser` into `VerificationProblem`s — a flat, linear-arithmetic representation of each
function's `requires`/`ensures` contracts and invariants in QF_LRA (quantifier-free linear rational
arithmetic) form. This IR is the contract between the parser-side of the pipeline and the
solver-side.

## Usage

```rust
use tpt_telos_parser::parse;
use tpt_telos_ir::extract;

let modules = parse(src).unwrap();
let problems = extract(&modules);

for problem in &problems {
    println!("{}: {} premise(s), {} conclusion(s)",
        problem.func_name,
        problem.premises.len(),
        problem.conclusions.len());
}
```

## Key types

| Type | Description |
|------|-------------|
| `VerificationProblem` | Per-function record: `func_name`, `premises`, `conclusions` |
| `Constraint` | A single `Linear` expression with a `Relation` (`≤`, `<`, `=`, …) |
| `Linear` | Sum of `(variable, coefficient)` pairs plus a constant |
| `Conclusion` | One thing to prove: `description`, `constraint`, `is_ensures` flag |
| `Relation` | `Le`, `Lt`, `Ge`, `Gt`, `Eq`, `Ne` |

## License

Licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE) at your option.
