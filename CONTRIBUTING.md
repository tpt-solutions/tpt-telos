# Contributing to tpt-telos

## Building

```
cargo build --workspace
```

## Testing

```
cargo test --workspace
```

Run a single crate's tests:

```
cargo test -p tpt-telos-verifier
```

Run a specific test by name (cargo filters by substring):

```
cargo test -p tpt-telos-verifier extended_tests
```

## Linting and formatting

```
cargo fmt --all -- --check          # check formatting
cargo clippy --workspace --all-targets -- -D warnings   # lint (matches CI)
```

Fix formatting in place: `cargo fmt --all`.

## Coverage

CI enforces a 75% line-coverage floor. Check locally before opening a PR:

```
cargo llvm-cov --workspace --fail-under-lines 75
```

Do not lower the threshold.

## Crate layout

The workspace contains eight crates under `crates/`. Each has a focused role in the pipeline:

| Crate | Role |
|---|---|
| `tpt-telos-parser` | Lexer / parser / AST for `.telos` source |
| `tpt-telos-ir` | AST → `VerificationProblem` lowering |
| `tpt-telos-verifier` | Fourier-Motzkin SMT-style solver |
| `tpt-telos-router` | `@boundary` → `Target` classification |
| `tpt-telos-agent` | Generate → Verify → Rewrite loop |
| `tpt-telos-codegen` | Verified candidates → Rust / Go / Python source |
| `tpt-telos-lsp` | JSON-RPC 2.0 LSP server |
| `tpt-telos` | CLI binary (`telos`) |

See `CLAUDE.md` for a detailed description of each crate and the full pipeline data flow.

## Adding a `.telos` fixture

1. Add your file under `examples/` following the naming convention of existing files
   (`wallet.telos`, `broken.telos`, `nested.telos`, `microservice.telos`, `eject.telos`).
2. Wire it into an existing or new integration test in `tpt-telos/tests/cli.rs` (or the
   relevant crate's `tests/` directory), matching the existing pattern.
3. For a bug fix, name the fixture after the issue or behaviour it covers and add a
   regression test that would have caught the bug.

## Pull request conventions

- **One commit per logical change.** Split unrelated fixes into separate PRs.
- **Commit message style:** imperative mood, present tense, ≤72 chars in the subject line.
  Example: `verifier: fix off-by-one in Fourier-Motzkin pivot selection`
- All CI checks must pass: format, clippy, tests, and the 75% coverage gate.
- Do not lower the coverage threshold or suppress clippy warnings with `#[allow(...)]`
  without a comment explaining why.
- Keep PRs focused. Large refactors should be discussed in an issue first.
