# tpt-telos-lsp

Language Server Protocol (LSP) server for [tpt-telos](https://github.com/tpt-solutions/tpt-telos).

A dependency-light JSON-RPC 2.0 LSP server over stdio with `Content-Length` framing. Provides:

- **Diagnostics** — surface parse and verification errors as you type
- **Hover** — type and contract information on hover
- **`telos/verify`** — custom request to run the full verification pipeline on demand
- **`telos/eject`** — custom request to eject a function to a trusted implementation block

Message handling (`analysis.rs`) is decoupled from stdio I/O for unit testing.

## Part of the tpt-telos workspace

See the [main repository](https://github.com/tpt-solutions/tpt-telos) for the full pipeline, examples, and documentation.

## License

Apache-2.0
