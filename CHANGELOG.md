# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-07-14

### Added
- Initial public release of the tpt-telos compiler workspace.
- `telos-parser`: hand-written lexer, parser, and AST for the tpt-telos language.
- `telos-ir`: AST → IR lowering and `requires`/`ensures` → QF_LRA constraint extraction.
- `telos-verifier`: self-contained Fourier–Motzkin SMT-style solver (sound over
  integers, no external Z3 dependency) with `model()` / `counterexample()` support.
- `telos-router`: classifies modules to Rust/Go backends from `@boundary(...)`.
- `telos-agent`: `CodeAgent` trait with the offline `StaticAgent` and an `LlmAgent`
  behind the `llm` feature (OpenAI-compatible + native Anthropic providers).
- `telos-codegen`: dual Rust/Go backends, automatic FFI bridge, eject hatch, and
  project assembly.
- `telos-lsp`: JSON-RPC 2.0 language server over stdio (diagnostics, hover,
  `telos/verify`, `telos/eject`).
- `telos-cli`: the `telos` binary exposing `parse`, `verify`, `transpile`,
  `build`, `project`, `eject`, and `lsp`.

[0.1.0]: https://github.com/tpt-solutions/tpt-telos/releases/tag/v0.1.0
