# tpt-telos-codegen

Rust/Go dual-backend code generation for [tpt-telos](https://github.com/tpt-solutions/tpt-telos).

Lowers verified `Candidate`s into target source:

- **Rust backend** — structs, invariant `impl` methods, contracts as doc comments, eject-hatch wrappers
- **Go backend** — mirrors the Rust output for Go-routed modules
- **FFI bridge** — bidirectional C-ABI glue between the Rust and Go backends
- **Project assembler** — full buildable project tree (`Cargo.toml` + `go.mod` + FFI glue) routed per-module via `tpt-telos-router`

## Part of the tpt-telos workspace

See the [main repository](https://github.com/tpt-solutions/tpt-telos) for the full pipeline, examples, and documentation.

## License

Apache-2.0
