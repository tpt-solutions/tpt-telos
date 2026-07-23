# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

tpt-telos is an intent-based, constraint-declarative language and compiler. Developers write `.telos`
source (module/invariant/func with `requires`/`ensures` contracts); the pipeline parses it, extracts
mathematical constraints, verifies them with a self-contained SMT-style solver, and uses an agentic
"Generate -> Verify -> Counter-example -> Rewrite" loop to synthesize compiling Rust and/or Go code for
each function, routed by `@boundary(...)` metadata. The full pipeline (all 4 planned phases) is
implemented; see `TODO.md` for what each phase built and `spec.txt` for the original design doc (note:
`spec.txt`'s aspirational directory layout — `compiler/`, `verifier/`, `ai-orchestrator/` — was **not**
what was built; the actual layout is the Cargo workspace under `crates/` described below).

## Commands

```
cargo build --workspace                       # build everything
cargo test --workspace                        # run all unit + integration tests
cargo test -p tpt-telos-verifier                  # test a single crate
cargo test -p tpt-telos-verifier extended_tests    # run a specific test module/fn (cargo test filters by name)
cargo test --workspace --features llm         # include the LLM-agent feature in the build/tests
cargo fmt --all -- --check                    # CI formatting check
cargo clippy --workspace --all-targets -- -D warnings   # CI lint check (matches RUSTFLAGS=-D warnings in CI)
cargo llvm-cov --workspace --fail-under-lines 75         # CI coverage gate (75% floor; don't lower it)
```

Running the compiled CLI directly (subcommands: `parse | verify | transpile | build | project | eject | lsp`):

```
cargo run -p tpt-telos -- verify examples/wallet.telos
cargo run -p tpt-telos -- build examples/wallet.telos --out-dir gen
cargo run -p tpt-telos -- project examples/microservice.telos --out-dir gen-project --check
cargo run -p tpt-telos -- eject examples/eject.telos --func withdraw
cargo run -p tpt-telos -- transpile examples/wallet.telos --llm   # requires --features llm and TELAS_LLM_* env vars
```

`telos project --check` shells out to `cargo` and `go`; `eject`/`project` also invoke `gofmt` to
canonicalize generated Go (falls back to a warning, not a failure, if `gofmt`/`go` aren't on PATH).

CI (`.github/workflows/ci.yml`) runs two jobs: format+clippy+test, and a separate `cargo-llvm-cov`
coverage job with an enforced 75% line-coverage floor.

## Workspace layout

Eight crates under `crates/`, each with a focused responsibility in the pipeline:

- **tpt-telos-parser** — hand-written lexer/parser/AST for `.telos` source. Grammar is the source of truth
  at `crates/telos-parser/src/grammar.ebnf`; keep it in sync with `lexer.rs`/`parser.rs`/`ast.rs` when
  the language changes. The AST also covers generics, tuples, `struct`/`enum` definitions, calls,
  and control-flow expressions (`if`/`match`/`forall`/aggregates/`?`) — see the caveat below: this
  parses today, but `tpt-telos-ir`/`tpt-telos-codegen` lowering of it is a work-in-progress tracked as
  Phase 7 in `TODO.md`, not a finished feature.
- **tpt-telos-ir** (`extract.rs`) — lowers the AST into `VerificationProblem`s, translating `requires`/
  `ensures`/invariants into a linear-arithmetic constraint model (QF_LRA-ish).
- **tpt-telos-verifier** (`solver.rs`, `verify.rs`) — a self-contained Fourier-Motzkin-based SMT-style
  solver (no external Z3/CVC5 dependency). Sound over integers. Produces pass/fail plus, on failure, a
  concrete counter-example `Model` used to drive agent rewrites.
- **tpt-telos-router** — pure classification: reads `@boundary(...)` attributes and decides `Target::Rust`
  vs `Target::Go` per module/function (`cpu_bound`/`zero_allocation`/`crypto`/`real_time` -> Rust;
  `network_io`/`high_concurrency`/`distributed`/`high_latency` -> Go).
- **tpt-telos-agent** — the `CodeAgent` trait plus the Generate -> Verify -> Counter-example -> Rewrite
  loop (`transpile_module`). `StaticAgent` (`static_agent.rs`) is the default, fully offline,
  deterministic synthesizer (translates the developer's body when present, else derives one from
  `ensures`). `llm_agent.rs` (behind the `llm` Cargo feature) calls a real LLM over an
  OpenAI-compatible or native Anthropic wire format — see env vars below.
- **tpt-telos-codegen** — lowers verified `Candidate`s into target source: `lib.rs`/`eject.rs` for the Rust
  backend (structs, invariant `impl` methods, contracts as doc comments), `go.rs` for the Go backend
  (mirrors the Rust one), `ffi.rs` for the bidirectional C-ABI FFI bridge between them, and
  `project.rs` to assemble a full buildable project tree (`Cargo.toml` + `go.mod` + FFI glue) routed
  per-module via `tpt-telos-router`.
- **tpt-telos-lsp** — dependency-light JSON-RPC 2.0 LSP server over stdio (`Content-Length` framing):
  diagnostics, hover, and custom `telos/verify` / `telos/eject` requests. Message handling
  (`analysis.rs`) is decoupled from stdio I/O for unit testing.
- **tpt-telos** — the `telos` binary (clap). Thin orchestration layer over the crates above; also
  contains the AST pretty-printer used by `telos parse`.

### Pipeline data flow

`.telos` source -> `telos_parser::parse` (`Vec<Module>`) -> `telos_ir::extract` (`VerificationProblem`s)
-> `telos_verifier::verify` (pass/fail + counterexample) -> `telos_agent::transpile_module` (runs the
verify/rewrite loop per function, using `telos_router::route` for target selection) -> `telos_codegen::
generate_program` (single Rust output) or `generate_project` (dual-backend tree with FFI) ->
optional `cargo build` / `go build` / `gofmt` invocation by the CLI.

### Key language semantics (see `grammar.ebnf` for full grammar)

- `@boundary(...)` / `@state(...)` are architectural metadata attributes on modules/functions.
- `invariant T { c }` must hold at function entry and after every `mutate state` block.
- `requires`/`ensures` are pre-/post-conditions; `old(e)` refers to the pre-state value of `e`.
- `@eject` marks a function to compile to a trusted opaque block (`f_impl`/`fImpl`) wrapped by a
  generated guard function that still enforces `requires`/`ensures` at runtime via `assert!`/`panic`.
- No implicit type coercion, no hidden allocation — every operation is explicitly named (by design,
  to keep the language easy for both humans and the LLM agent to reason about).

### Known gaps (do not assume these work without checking; tracked in `TODO.md` Phase 7)

- `telos verify` does not yet print a counterexample on `FAIL` — only the restated clause text
  (the verifier already computes one internally via `telos_verifier::solver::counterexample`, used by
  the agent's rewrite loop, but `CheckResult`/CLI/LSP output don't surface it yet).
- `struct`/`enum` definitions parse but are not yet consumed by `telos-ir::extract` or driven into
  codegen's emitted field types (codegen still infers fields from usage and hardcodes `i64`).
- Contract expressions using `Call`/`MethodCall`/`Index`/`If`/`Match`/`Forall`/`Aggregate` are rejected
  by `telos-ir::extract`'s `linearize`/`linearize_bounded` with one generic error today; full/partial
  lowering for these is planned, not implemented.
- The `--solver z3` CLI flag (behind the `z3` feature) sets a global `SolverBackend` that
  `telos-verifier::verify`/`unsat` never actually reads — it is currently a no-op, not a working
  alternate backend, despite `TODO.md`'s Phase 6 marking it `[x]` complete. Don't trust a Phase
  `[x]` mark alone; spot-check against the code.

### LLM agent environment variables (only relevant behind the `llm` feature)

`TELAS_LLM_PROVIDER` (`openai` default, or `ollama`/`openrouter`/`grok`/`anthropic`), `TELAS_LLM_URL`,
`TELAS_LLM_KEY` (required), `TELAS_LLM_MODEL`, `TELAS_LLM_MAX_TOKENS` (Anthropic only).

## Testing conventions

Every crate has unit tests colocated in `src/` plus at least one integration test under `tests/`.
`tpt-telos/tests/cli.rs` drives the actual binary end-to-end. `examples/*.telos` are fixtures used by
integration tests (`wallet.telos`/`broken.telos` for pass/fail verification, `nested.telos` for nested
struct fields, `microservice.telos` for dual-backend + FFI, `eject.telos` for the eject hatch). When
fixing a bug, add a regression fixture under `examples/` and wire it into an existing or new
integration test, matching the existing pattern.
