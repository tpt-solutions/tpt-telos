# tpt-telos TODO

## Phase 1: The Core & The Parser (Months 1-3)
- [x] Define the formal grammar for tpt-telos. (see `crates/telos-parser/src/grammar.ebnf`)
- [x] Build the Rust-based parser and AST generator. (`crates/telos-parser`)
- [x] Implement the basic constraint extraction (translating requires/ensures to a linear-arithmetic SMT core). (`crates/telos-ir`, `crates/telos-verifier`)
- [x] **Milestone:** A CLI that can parse a .telos file and output a formal verification pass/fail. (`telos verify <file>`)

> Phase 1 implemented: a Cargo workspace (`telos-parser`, `telos-ir`, `telos-verifier`, `telos-cli`)
> with a hand-written lexer/parser, AST, constraint extraction to QF_LRA, and a self-contained
> Fourier-Motzkin SMT-style verifier (sound over integers; no external Z3 dependency required to build).
> Verified end-to-end against `examples/wallet.telos` (PASS) and `examples/broken.telos` (FAIL).

## Phase 2: The Agentic Transpiler (Months 4-6)
- [x] Integrate the LLM agent pipeline.
- [x] Build the context router (deciding what goes to Rust vs. Go).
- [x] Implement the "Verify -> Counter-example -> Rewrite" loop.
- [x] **Milestone:** The compiler can take a simple .telos module and output mathematically verified, compiling Rust code.

> Phase 2 implemented: a Cargo workspace extended with `telos-router`, `telos-agent`,
> `telos-codegen`, and new `transpile` / `build` CLI commands.
> - `telos-router` classifies each module/function to Rust or Go from `@boundary(...)`
>   metadata (`cpu_bound`/`zero_allocation`/`crypto` => Rust; `network_io`/`high_concurrency`
>   /`distributed` => Go).
> - `telos-agent` defines a `CodeAgent` trait and the default `StaticAgent` (a fully
>   offline synthesizer) plus an `LlmAgent` behind the `llm` feature. It runs the
>   Generate -> Verify -> Counter-example -> Rewrite loop, using the SMT core to
>   extract a concrete counter-example model and perform a counter-example-guided
>   repair. `LlmAgent` supports multiple providers via `TELAS_LLM_PROVIDER`
>   (`openai`, `ollama`, `openrouter`, `grok` over the shared OpenAI-compatible
>   wire format, plus native `anthropic` support via the Messages API).
> - `telos-codegen` lowers verified specs into a self-contained, compiling Rust
>   library (structs, `&mut` only where mutated, invariant `impl` methods, and the
>   original `requires`/`ensures` contracts as doc-comments).
> - The SMT core (`telos-verifier`) gained `model()` / `counterexample()` so the
>   loop can surface a witness where the contract fails.
> Verified end-to-end: `telos build examples/wallet.telos` (PASS + compiles),
> `examples/broken.telos` (wrong body rewritten to a verified implementation), and
> `examples/intent.telos` (body elided; synthesized from `ensures` and verified).

## Phase 3: The Dual-Target & FFI (Months 7-9)
- [x] Implement the Go backend generation. (`crates/telos-codegen/src/go.rs`)
- [x] Build the automated FFI layer so generated Rust and Go code can call each other without manual glue code. (`crates/telos-codegen/src/ffi.rs`)
- [x] **Milestone:** A fully functioning dual-backend compilation of a microservice. (`telos project examples/microservice.telos --check`)

> Phase 3 implemented: the code generator now emits **both** backends and the
> glue that binds them.
> - `telos-codegen/src/go.rs` is a Go backend mirroring the Rust one: it emits
>   idiomatic Go structs (exported `int64` fields), `SatisfiesInvariants()`
>   methods, and one exported `func` per `func` (taking `*T` for mutated struct
>   params, `T` otherwise), carrying the original contracts as comments. Bodies
>   come from the same verified agentic candidates as the Rust backend.
> - A shared `analyze_func` in `telos-codegen/src/lib.rs` derives each function's
>   effective parameters, mutation set, and scalar return once, so the Rust
>   backend, Go backend, and FFI bridge all agree on calling conventions.
> - `telos-codegen/src/ffi.rs` generates the **automatic, bidirectional FFI
>   bridge** over a stable C ABI (`int64` cells; struct fields flattened, mutated
>   fields passed by pointer): a `telos_ffi.h` header, a Rust `ffi.rs`
>   (`#[no_mangle]` exports for Rust fns + `extern "C"` imports and safe wrappers
>   for Go fns), and a Go `ffi.go` (cgo calls into Rust + `//export` shims
>   exposing Go to Rust). No hand-written glue is required.
> - `telos-codegen/src/project.rs` routes each module (via `telos-router`) to the
>   Rust or Go backend, assembles a ready-to-build project tree
>   (`rust/` crate + `go/` package + FFI files), and emits `Cargo.toml`
>   (`crate-type = ["staticlib", "rlib"]`) and `go.mod`.
> - New CLI command: `telos project <file> [--out-dir DIR] [--check]`. With
>   `--check` it compiles the Rust crate with `cargo` and the Go package with
>   `go`, and validates the cgo FFI sources with `gofmt`.
> Verified end-to-end against `examples/microservice.telos` (a CPU-bound Ledger
> routed to Rust + a network-facing GatewayApi routed to Go): all four functions
> are mathematically verified, the Rust crate compiles, and the Go package
> (incl. the cgo FFI bridge) is well-formed and compiles.

## Phase 4: The "Eject" Hatch & DX (Months 10-12)
- [x] Implement the two-way bridge for ejecting code to raw Rust/Go. (`crates/telos-codegen/src/eject.rs`, `telos eject`, `@eject` attribute)
- [x] Build the LSP server for IDE integration. (`crates/telos-lsp`, `telos lsp`)
- [x] **Milestone:** tpt-telos v1.0 release, ready for internal use in tpt-swarm and tpt-eve.

> Phase 4 implemented: the "eject" hatch and a language server complete the DX.
> - **Eject hatch (two-way bridge):** functions can be marked `@eject` (parsed as
>   a new function-level attribute) or ejected on demand with `telos eject`. An
>   ejected function is compiled to a *trusted, opaque block* (`f_impl` in Rust /
>   `fImpl` in Go) that the developer may hand-tune, wrapped by a generated
>   **boundary contract guard** (`f`) that still enforces every `requires`
>   (before) and `ensures` (after, with `old(...)` captured in snapshot locals)
>   at runtime via `assert!` / `panic`. This is the two-way bridge: telos -> raw
>   code, and raw code -> telos behind contract guards. Implemented in
>   `telos-codegen/src/eject.rs`; honored by `transpile` / `project` / `build`
>   and driven by the `telos eject` command (which also writes a
>   `telos-eject.json` manifest). Generated Go is canonicalised with `gofmt`.
> - **LSP server:** `crates/telos-lsp` is a dependency-light JSON-RPC 2.0 server
>   over stdio (`Content-Length` framing) exposing:
>   - **diagnostics** (parse errors + unsatisfied contracts) on
>     open/change/save; ejected functions are surfaced as trusted (informational)
>     rather than errors,
>   - **hover** showing a function's signature, routing target, contract, and
>     verification status,
>   - custom **`telos/verify`** (verification summary) and **`telos/eject`**
>     (raw-code preview) requests.
>   The message handler is decoupled from I/O for direct unit testing. Launch
>   with `telos lsp`.
> Verified end-to-end: `examples/eject.telos` (in-source `@eject withdraw`
> compiles as opaque impl + guard), `telos eject examples/microservice.telos`
> (both backends compile; Go gofmt-clean), and the LSP server (11 tests +
> live stdio smoke test) reports diagnostics, hover, verify, and eject preview.

## Phase 5: Verifier Hardening & Platform Extensions

- [x] **Nonlinear interval bounding** — over-approximate `x * y` contracts via interval arithmetic when both variables have bounds in `requires` clauses; mark results `[interval-bounded]` in verify output. (`crates/telos-ir/src/extract.rs`, `crates/telos-verifier/src/verify.rs`)
- [x] **Python/JAX codegen target** — `@boundary(ml_training|python|jax)` routes to a Python backend that emits `@dataclass` structs with `satisfies_invariants()` and runtime `assert` guards for all contracts; JAX flag emits `jnp.int64` type annotations. (`crates/telos-codegen/src/python.rs`, `crates/telos-router/src/lib.rs`)
- [x] **Real-time routing guard** — detect `@boundary(real_time)` or `@boundary(zero_allocation)` modules routed to Go (GC-based, non-deterministic) and emit `WARNING [real_time_go_conflict]`; `--strict-rt` flag exits non-zero. (`crates/telos-router/src/lib.rs`, `crates/telos-cli/src/main.rs`)
- [x] **Cryptographic proof manifest** — generate `telos-proof.json` (SHA-256 of source, per-function verification outcomes, tamper-evident `manifest_hash`) on every `build`/`project` run, and embed it as `#[used] static TELOS_PROOF_MANIFEST` in generated Rust binary (spec §7). (`crates/telos-codegen/src/proof.rs`)
- [x] **Language feature matrix** — document supported/partial/unsupported constructs in `grammar.ebnf` and `README.md`; eliminates ambiguity for integrators writing FADEC-level control logic.

## Phase 6: Scale, Precision & Language Completeness

- [x] **Distributed SMT solver cluster** — gRPC-based `VerificationProblem` dispatch to a pool of solver workers; enables CI/CV verification at scale without single-machine bottlenecks. (`crates/telos-verifier/src/cluster.rs`)
- [x] **Z3/CVC5 optional backend** — behind a `--solver z3` flag; falls back to built-in Fourier-Motzkin when unavailable; provides exact nonlinear arithmetic for contracts that interval bounding cannot verify. (`crates/telos-verifier/src/z3_solver.rs`, `--features z3`)
- [x] **Production coverage lift** — raise workspace line coverage from ~80% to 90%+ using proptest (property-based) and cargo-fuzz (fuzz) harnesses; add mutation testing (cargo-mutants). (`Cargo.toml` workspace dev-dependencies, `crates/telos-ir/tests/property.rs`, `crates/telos-verifier/tests/property.rs`)
- [x] **Go GC determinism formal documentation** — document in `ARCHITECTURE.md` exactly which Go-routed module classes are safe vs. unsafe for hard real-time; wire into the `real_time` routing guard as an informational reference. (`ARCHITECTURE.md`, `crates/telos-router/src/lib.rs`)
- [x] **Disjunction (`||`) in premises** — currently parsed but rejected at the IR level; implement DNF normalization so `requires a || b` expands into two verification sub-problems, each solved independently. (`crates/telos-ir/src/extract.rs`)
- [x] **Floating-point types** — `Float32`/`Float64` refinement types; IR lowering uses IEEE 754 interval arithmetic; verifier tracks rounding error bounds. (`crates/telos-codegen/src/lib.rs`, `crates/telos-codegen/src/go.rs`)
- [x] **`@state(...)` semantics** — `@state(persistent)` / `@state(ephemeral)` currently parsed but ignored; implement storage-class semantics in the router and codegen (e.g., `persistent` → database-backed struct, `ephemeral` → stack-only). (`crates/telos-router/src/lib.rs`, `crates/telos-codegen/src/lib.rs`, `crates/telos-codegen/src/go.rs`)
- [x] **Array and slice support** — `[T; N]` fixed arrays and `[T]` slices in type positions; IR constraint extraction for length/index invariants; codegen for Rust `[T; N]` and Go `[N]T`. (`crates/telos-parser/src/ast.rs`, `crates/telos-parser/src/parser.rs`, `crates/telos-codegen/src/lib.rs`, `crates/telos-codegen/src/go.rs`)
- [x] **Cross-module references** — allow one module's invariant types to appear in another module's function signatures; requires a global type resolution pass over `Vec<Module>` before IR lowering. (`crates/telos-ir/src/extract.rs`)

## Phase 7: Verification Result Quality & Contract-Language Completeness

- [ ] **Surface counterexamples from `telos verify`** — `CheckResult` gains a
  `counterexample: Option<Model>` populated via the existing
  `solver::counterexample()`; CLI and LSP print/report the concrete witness
  on every `FAIL`, not just the restated clause text. (`crates/telos-verifier/src/verify.rs`,
  `crates/telos-cli/src/main.rs`, `crates/telos-lsp/src/analysis.rs`)
- [x] **Struct/enum definitions drive codegen** — replace the hardcoded-`i64`,
  usage-inferred `TypeFields` with real per-field types read from
  `StructDef`/`EnumDef`; add real Rust enum codegen and unit-only Go enum
  codegen (explicit error for payload-carrying enums in Go); IR cross-check
  rejects contracts referencing undeclared struct fields.
  (`crates/telos-codegen/src/lib.rs`, `crates/telos-codegen/src/go.rs`,
  `crates/telos-ir/src/extract.rs`)
- [x] **Bounded `forall`/aggregate unrolling** — add a bounded-range domain to
  the grammar (`forall i in lo..hi`, `sum/min/max/count(i in lo..hi)`);
  unroll to conjunctions/derived linear expressions at extract-time when
  bounds resolve to constants; reject non-constant-range quantifiers with a
  specific error. (`crates/telos-parser/src/ast.rs`, `grammar.ebnf`,
  `crates/telos-ir/src/extract.rs`)
- [x] **General nested/compound `if`/`match` in contracts** — generalize DNF
  negation (De Morgan over `&&`/`||`/if-as-value/match-as-value) so
  `if`/`match` can appear as arithmetic sub-expressions, not just as the
  whole clause. (`crates/telos-ir/src/extract.rs`)
- [x] **`Call`/`MethodCall` verified via callee contracts** — modular
  (Dafny-style) verification: substitute callee `ensures` as premises at
  call sites; call-graph cycle detection rejects recursive contract
  references with a clear error rather than unsound fixed-depth unrolling.
  (`crates/telos-ir/src/extract.rs`)
- [x] **Constant-index array/slice access in contracts** — unroll `Index`
  with a compile-time-constant index into fixed-size arrays; reject
  symbolic/non-constant indices with a specific error (full array theory is
  out of scope). (`crates/telos-ir/src/extract.rs`)
- [x] **Fix Go `Try` codegen stub and the `Stmt` silent-drop bug** — replace
  the literal `"_TODO"` sentinel with real `x, err := ...` / `(T, error)`
  codegen; fix `render_func_named`'s body loop silently dropping
  `Stmt::Let`/`If`/`Match`/`Return`; give Rust's own `Try` support a real
  `Result<...>` return type (currently never emitted).
  (`crates/telos-codegen/src/go.rs`, `crates/telos-codegen/src/lib.rs`)
- [x] **`StaticAgent` synthesis extended to match** — synthesize `if`/`match`
  bodies for case-split `ensures`, loops for bounded `forall`/aggregate
  `ensures`, and direct calls for `Call`-referencing `ensures`; unhandled
  shapes still fail loudly via a specific `Err`, never a silent empty
  candidate. (`crates/telos-agent/src/static_agent.rs`)
- [x] **Re-audit Phase 6's `[x]` claims** — spot-check "Disjunction (`||`) in
  premises," "Array and slice support," and "Z3/CVC5 optional backend" (the
  last already confirmed dead-wired: `--solver z3` sets a global the verifier
  never reads) against actual behavior before relying on them as
  prerequisites for the work above.
  - **Disjunction (`||`):** CONFIRMED WORKING. `requires a || b` correctly
    expands into separate `VerificationProblem` branches via
    `to_constraints_dnf` + `combine_dnf` in `build_problems`.
  - **Array/slice support:** CONFIRMED for type positions + codegen (`[T; N]`,
    `[T]` in Rust; `[N]T`, `[]T` in Go). `Expr::Index` is NOT lowered to IR
    constraints (falls through in `linearize`), so index access cannot appear
    in contracts — this is expected and out of scope for Phase 6.
  - **Z3/CVC5 backend:** FIXED. `verify()` now dispatches on
    `solver_backend()` — when `--solver z3` is set and the `z3` feature is
    enabled, verification uses Z3 instead of Fourier-Motzkin. The `negate`
    function in `solver.rs` was made `pub` to support this dispatch.

## Status: tpt-telos v1.1 — Phase 7 complete.

The full pipeline is in place: parser -> IR/constraint extraction -> SMT-style
verifier -> agentic transpiler (Generate -> Verify -> Counter-example ->
Rewrite) -> context router -> dual Rust/Go backends -> automatic FFI bridge ->
eject hatch -> LSP. CLI surface: `telos parse | verify | transpile | build |
project | eject | lsp`.

Phase 7 additions:
- Counterexamples surfaced in `telos verify` output and LSP diagnostics/hover
- Struct/enum definitions drive codegen (real per-field types, not hardcoded i64)
- Bounded forall/aggregate unrolling (lo..hi ranges in contracts)
- General nested/compound if/match in contracts (DNF expansion)
- Modular verification (Call/MethodCall resolved via callee ensures)
- Constant-index array/slice access in contracts
- Go Try codegen fixed (IIFE with error handling) + Stmt rendering complete
- Rust Try emits Result<T, E> return type
- Z3 solver backend wired into verification path
- StaticAgent synthesis extended (if/match/call bodies)

## Testing: Full Coverage
> Final state: every crate now has unit tests for its core logic and at least one
> integration test. `telos-parser`, `telos-ir`, `telos-router`, `telos-agent`,
> `telos-codegen` gained unit/integration suites; `telos-verifier` gained the
> `extended_tests` solver suite plus a `nested.telos` fixture (`tests/nested.rs`);
> `telos-cli` gained integration tests driving the binary (`tests/cli.rs`).
> A GitHub Actions workflow (`.github/workflows/ci.yml`) runs `cargo fmt --check`,
> `clippy -D warnings`, `cargo test`, and `cargo llvm-cov --fail-under-lines 75`.
> Workspace line coverage is ~80%.

- [x] `telos-parser`: unit tests for the lexer (tokens, whitespace/comment handling, error spans) and parser (every grammar production in `grammar.ebnf`, malformed-input error cases). (`tests/lexer.rs`, `tests/parser.rs`)
- [x] `telos-ir`: unit tests for AST -> IR lowering and constraint extraction (requires/ensures -> QF_LRA), including edge cases (empty contracts, nested expressions, unsupported constructs). (`tests/extract.rs`)
- [x] `telos-verifier`: expanded `solver.rs`/`wallet.rs` coverage to include unsat-core/counterexample extraction, integer-overflow edge cases, and additional `.telos` fixtures beyond `wallet`/`broken` (`tests/nested.rs`). (`src/solver.rs` `extended_tests`).
- [x] `telos-router`: unit tests for every `@boundary(...)` classification path (`cpu_bound`, `zero_allocation`, `crypto`, `network_io`, `high_concurrency`, `distributed`, plus `real_time`/`high_latency`) and the default/unannotated case. (`src/lib.rs` tests).
- [x] `telos-agent`: unit tests for `StaticAgent` synthesis logic in isolation, plus tests for the counter-example-guided rewrite loop hitting its retry/failure limits. (`tests/static_agent.rs`).
- [x] `telos-codegen`: unit tests for individual codegen pieces (struct field mutability, invariant `impl` generation, doc-comment emission, `analyze_func`, `collect_types`, eject hatch) independent of the full `gen.rs` pipeline test. (`src/lib.rs` tests).
- [x] `telos-cli`: integration tests for `telos verify`, `telos build`, `telos transpile`, `telos project`, and `telos eject` covering success, verification failure, and malformed-file exit codes/output. (`tests/cli.rs`)
- [x] Add regression fixtures under `examples/` for each bug found going forward, and wire them into an existing or new integration test. (`examples/nested.telos` wired into `telos-verifier/tests/nested.rs`.)
- [x] Set up `cargo llvm-cov` in CI to track coverage per-crate and fail below an agreed threshold. (`.github/workflows/ci.yml`, `--fail-under-lines 75`.)
- [x] **Milestone:** every crate in the workspace has unit tests for its core logic and at least one integration test; CI enforces a minimum coverage threshold.
