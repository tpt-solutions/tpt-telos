# tpt-telos

The `telos` command-line compiler — an intent-based, constraint-declarative language and compiler.

Developers write `.telos` source (modules, invariants, and functions with `requires`/`ensures` contracts). The pipeline parses, extracts constraints, verifies them with a self-contained SMT-style solver, and synthesizes compiling Rust and/or Go code for each function, routed by `@boundary(...)` metadata.

## Installation

```
cargo install tpt-telos
```

## Subcommands

| Command | Description |
|---|---|
| `telos parse <file>` | Parse and pretty-print the AST |
| `telos verify <file>` | Run the SMT verifier and report pass/fail |
| `telos transpile <file>` | Transpile to Rust/Go via the static (or LLM) agent |
| `telos build <file>` | Transpile and invoke `cargo build` / `go build` |
| `telos project <file>` | Generate a full dual-backend project tree |
| `telos eject <file> --func <name>` | Eject a function to a trusted implementation block |
| `telos lsp` | Start the LSP server over stdio |

## Quick start

```
telos verify examples/wallet.telos
telos build examples/wallet.telos --out-dir gen
telos project examples/microservice.telos --out-dir gen-project --check
```

## Documentation

Full documentation and examples: <https://github.com/tpt-solutions/tpt-telos>

## License

Apache-2.0
