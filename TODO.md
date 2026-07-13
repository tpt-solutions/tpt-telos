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
- [ ] Integrate the LLM agent pipeline.
- [ ] Build the context router (deciding what goes to Rust vs. Go).
- [ ] Implement the "Verify -> Counter-example -> Rewrite" loop.
- [ ] **Milestone:** The compiler can take a simple .telos module and output mathematically verified, compiling Rust code.

## Phase 3: The Dual-Target & FFI (Months 7-9)
- [ ] Implement the Go backend generation.
- [ ] Build the automated FFI layer so generated Rust and Go code can call each other without manual glue code.
- [ ] **Milestone:** A fully functioning dual-backend compilation of a microservice.

## Phase 4: The "Eject" Hatch & DX (Months 10-12)
- [ ] Implement the two-way bridge for ejecting code to raw Rust/Go.
- [ ] Build the LSP server for IDE integration.
- [ ] **Milestone:** tpt-telos v1.0 release, ready for internal use in tpt-swarm and tpt-eve.
