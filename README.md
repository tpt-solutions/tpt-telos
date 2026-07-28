# tpt-telos

**tpt-telos** is a compiler for the *tpt-telos* language: a formally-specified,
verification-first source language that lowers to both **Rust** and **Go** behind
an automatic FFI bridge.

The pipeline is:

```
parser → IR / constraint extraction → SMT-style verifier → agentic transpiler
       → context router → Rust/Go codegen → FFI bridge → eject hatch → LSP
```

Every `func` carries `requires`/`ensures` contracts that are extracted to
**QF_LRA** linear arithmetic and discharged by a self-contained
Fourier–Motzkin SMT-style solver — **no external Z3 dependency** is required to
build or run.

> **Status:** v1.2, Phase 7 and Phase 8 complete (see [`TODO.md`](TODO.md)).
> Counterexamples are surfaced by `telos verify`/`build` and the LSP; structs,
> enums, bounded `forall`/aggregates, disjunction, and modular `Call`/
> `MethodCall` verification are all wired into `telos-ir`/`telos-codegen`.

## Install

```sh
cargo install tpt-telos
```

Or build from this workspace:

```sh
cargo build --release -p tpt-telos
```

## Usage

```sh
# Scaffold a starter module
telos init --module MyModule --out my_module.telos

# Parse and type/contract-check a .telos file
telos parse  examples/wallet.telos

# Run formal verification (requires/ensures → QF_LRA); prints a counterexample on FAIL
telos verify examples/wallet.telos

# Machine-readable output for CI/editors, or watch the file and re-verify on save
telos verify examples/wallet.telos --json
telos verify examples/wallet.telos --watch

# Exact nonlinear arithmetic via Z3 (requires building with --features z3)
telos verify examples/wallet.telos --solver z3

# Transpile to a single self-contained Rust file
telos transpile examples/wallet.telos --out wallet.rs

# Build a verified, compiling Rust crate (writes telos-proof.json alongside it)
telos build examples/wallet.telos --out-dir ./gen

# Generate a dual Rust+Go project with the FFI bridge
telos project examples/microservice.telos --out-dir ./gen-project --check

# Fail the build if a real_time/zero_allocation module got routed to Go
telos project examples/microservice.telos --check --strict-rt

# Eject a function to a hand-tunable opaque block wrapped by a contract guard
telos eject examples/microservice.telos --func withdraw

# Re-hash source against a previously generated telos-proof.json to detect drift
telos verify-manifest gen/telos-proof.json examples/wallet.telos

# Run the language server (JSON-RPC 2.0 over stdio)
telos lsp
```

### LLM-backed agent

By default the agentic transpiler runs the fully offline `StaticAgent`. To use a
real LLM backend, build with the `llm` feature and pass `--llm`:

```sh
cargo run -p tpt-telos --features llm -- transpile examples/intent.telos --llm
```

At runtime it needs `TELAS_LLM_KEY` and `TELAS_LLM_PROVIDER`
(`openai` default | `ollama` | `openrouter` | `grok` | `anthropic`); optionally
`TELAS_LLM_MODEL` / `TELAS_LLM_URL` / `TELAS_LLM_MAX_TOKENS`.

## Crates

| Crate           | Purpose                                                       |
|-----------------|---------------------------------------------------------------|
| `tpt-telos`     | The `telos` binary and CLI surface (incl. `init`/`verify-manifest`, `--json`/`--watch`/`--strict-rt`/`--solver`). |
| `tpt-telos-parser`  | Lexer, parser, and AST.                                       |
| `tpt-telos-ir`      | AST → IR lowering + QF_LRA constraint extraction, disjunction/DNF, bounded `forall`/aggregates, modular `Call`/`MethodCall` verification. |
| `tpt-telos-verifier`| Self-contained Fourier–Motzkin SMT-style solver, plus an optional Z3 backend (`--features z3`) and a gRPC solver cluster. |
| `tpt-telos-router`  | Classifies modules to Rust/Go/Python from `@boundary(...)`, storage class from `@state(...)`, and real-time/Go conflict warnings. |
| `tpt-telos-agent`   | `CodeAgent` trait: `StaticAgent` + `LlmAgent` (behind `llm`). |
| `tpt-telos-codegen` | Rust/Go/Python backends, FFI bridge, eject, project assembly, cryptographic proof manifest. |
| `tpt-telos-lsp`     | JSON-RPC 2.0 language server over stdio (diagnostics, hover, quick-fix code actions, `telos/verify`, `telos/eject`). |

## Architecture note: divergence from spec.txt

`spec.txt` sketched an aspirational directory layout with top-level `compiler/`,
`verifier/`, and `ai-orchestrator/` sibling directories, and assumed Z3/CVC5 as the
primary solver, a LangGraph-style orchestration layer, and vLLM for local inference.

What was built instead is a flat **Cargo workspace** under `crates/` with eight
focused crates (parser, ir, verifier, router, agent, codegen, lsp, cli). The reasons:

- A single workspace makes cross-crate refactoring, CI, and coverage tooling
  straightforward with standard Cargo tooling and no per-directory build wiring.
- The self-contained Fourier–Motzkin solver eliminates the Z3 C-library build
  dependency for the common case; Z3 is available behind `--features z3` for
  exact nonlinear arithmetic when needed.
- `StaticAgent` (fully offline, deterministic synthesis) satisfies the
  Generate → Verify → Counter-example → Rewrite loop without requiring a running
  LLM service; `LlmAgent` behind `--features llm` adds real-LLM support without
  making it a hard dependency.

The `compiler/` / `verifier/` / `ai-orchestrator/` names from spec.txt are
**not** present on disk; the mapping is: parser+ir ≈ compiler front-end,
verifier ≈ verifier, agent+router+codegen ≈ ai-orchestrator.

## Crate documentation

Individual crate READMEs have not yet been written; the table in the [Crates](#crates)
section above is the authoritative per-crate summary. The source-of-truth for the
grammar is `crates/telos-parser/src/grammar.ebnf`; for the full feature and phase
history see [`TODO.md`](TODO.md); for example `.telos` files see [`examples/README.md`](examples/README.md).

## Troubleshooting

**`gofmt: command not found` / `go: command not found`**
`telos project --check` and `telos eject` shell out to `go build` and `gofmt` to
compile and canonicalize generated Go. If Go is not on your `PATH`, these commands
fall back to a warning rather than a hard failure; the generated source is still
written to disk. Install Go ≥ 1.21 and ensure `$GOPATH/bin` (or the Go install
`bin/`) is on your `PATH`.

**`--features z3` fails to build**
The `z3` Cargo feature links against the Z3 C library via `z3-sys`, which requires
`z3.h` on the compiler's include path and `libz3` at link time. Z3 is not vendored.
Install Z3 development headers (e.g. `apt install libz3-dev` or `brew install z3`)
before building with `--features z3`. The default feature set builds without any
external C dependencies.

**`cargo llvm-cov` not found**
Coverage reporting requires `cargo-llvm-cov`. Install it with:
```sh
cargo install cargo-llvm-cov
```

## License

Licensed under either of:

- [MIT License](LICENSE-MIT)
- [Apache License, Version 2.0](LICENSE-APACHE)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual-licensed as above, without any additional terms or conditions.
