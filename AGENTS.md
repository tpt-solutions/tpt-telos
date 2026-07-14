# AGENTS.md — tpt-telos

Rust Cargo workspace. The `telos` binary (crate `tpt-telos`) is the compiler frontend. The full pipeline narrative lives in `TODO.md`.

## Build & verify (these are the CI gates)
- `cargo fmt --all -- --check` — format gate.
- `cargo clippy --workspace --all-targets -- -D warnings` — warnings denied.
- `cargo test --workspace` — all tests.
- `cargo test -p tpt-telos-verifier` or `cargo test -p tpt-telos-verifier <name>` — single crate / single test.
- `cargo llvm-cov --workspace --fail-under-lines 75` — coverage gate. Do NOT lower the 75% floor (it is the project's agreed minimum; raise it as coverage improves).

CI sets `RUSTFLAGS=-D warnings`, so keep the build warning-clean locally too — a warning breaks CI.

## Architecture (pipeline order)
`tpt-telos-parser` (grammar of record: `crates/telos-parser/src/grammar.ebnf`) → `tpt-telos-ir` (AST→IR, `requires`/`ensures`→QF_LRA) → `tpt-telos-verifier` (self-contained Fourier–Motzkin SMT-style solver, sound over integers, **no external Z3 dependency** — do not add one) → `tpt-telos-agent` (agentic transpiler) → `tpt-telos-router` (Rust/Go selection) → `tpt-telos-codegen` (Rust + Go backends, FFI bridge, eject) → `tpt-telos-lsp`.

The CLI needs no network by default: the offline `StaticAgent` runs unless `--llm` is passed.

## Crate ownership
- `tpt-telos-parser` — lexer/parser/AST. Grammar lives in `grammar.ebnf`.
- `tpt-telos-ir` — AST→IR lowering + QF_LRA constraint extraction.
- `tpt-telos-verifier` — solver; exposes `model()`/`counterexample()` for the rewrite loop.
- `tpt-telos-router` — classifies a module to Rust/Go from `@boundary(...)`.
- `tpt-telos-agent` — `CodeAgent` trait; `StaticAgent` (offline) and `LlmAgent` (behind the `llm` feature).
- `tpt-telos-codegen` — `generate_program` (Rust), `generate_project` (dual backend + FFI), `eject.rs`.
- `tpt-telos-lsp` — JSON-RPC 2.0 server over stdio (`Content-Length` framing).
- `tpt-telos` — the `telos` binary.

## Routing & attributes
- `@boundary(...)` on a module picks the backend: `cpu_bound` / `zero_allocation` / `crypto` / `real_time` → Rust; `network_io` / `high_concurrency` / `distributed` / `high_latency` → Go. Any Go flag wins; unannotated defaults to **Rust**.
- `@eject` marks a function as a trusted opaque block (`f_impl`/`fImpl`) wrapped by a generated `requires`/`ensures` contract guard.

## CLI commands (binary: `telos`)
`parse`, `verify`, `transpile [--out PATH]`, `build [--out-dir DIR]`, `project [--out-dir DIR] [--check]`, `eject [--func NAME]`, `lsp`.
- `build` runs `cargo` on the generated crate (`cargo`/`rustc` must be on PATH).
- `project --check` additionally needs `go` and `gofmt` on PATH. `go build` skips cgo files, so the FFI bridge is validated with `gofmt -l`, not `go build`.
- `--llm` requires the `llm` feature: `cargo run -p tpt-telos --features llm -- <cmd> --llm`. Without it, `--llm` errors with "requires building telos with the `llm` feature". At runtime it needs `TELAS_LLM_KEY` + `TELAS_LLM_PROVIDER` (`openai` default | `ollama` | `openrouter` | `grok` | `anthropic`); optional `TELAS_LLM_MODEL` / `TELAS_LLM_URL` / `TELAS_LLM_MAX_TOKENS`.

## Examples are regression fixtures
`examples/*.telos` are wired into integration tests (e.g. `nested.telos` → `tpt-telos-verifier/tests/nested.rs`). Add a new fixture there for each bug found.
