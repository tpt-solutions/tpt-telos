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
# Parse and type/contract-check a .telos file
telos parse  examples/wallet.telos

# Run formal verification (requires/ensures → QF_LRA)
telos verify examples/wallet.telos

# Transpile to a single self-contained Rust file
telos transpile examples/wallet.telos --out wallet.rs

# Build a verified, compiling Rust crate
telos build examples/wallet.telos --out-dir ./gen

# Generate a dual Rust+Go project with the FFI bridge
telos project examples/microservice.telos --out-dir ./gen-project --check

# Eject a function to a hand-tunable opaque block wrapped by a contract guard
telos eject examples/microservice.telos --func withdraw

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
| `tpt-telos`     | The `telos` binary and CLI surface.                           |
| `tpt-telos-parser`  | Lexer, parser, and AST.                                       |
| `tpt-telos-ir`      | AST → IR lowering + QF_LRA constraint extraction.             |
| `tpt-telos-verifier`| Self-contained Fourier–Motzkin SMT-style solver.              |
| `tpt-telos-router`  | Classifies modules to Rust/Go from `@boundary(...)`.          |
| `tpt-telos-agent`   | `CodeAgent` trait: `StaticAgent` + `LlmAgent` (behind `llm`). |
| `tpt-telos-codegen` | Rust/Go backends, FFI bridge, eject, project assembly.        |
| `tpt-telos-lsp`     | JSON-RPC 2.0 language server over stdio.                      |

## License

Licensed under the Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE)).

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
licensed as above, without any additional terms or conditions.
