# tpt-telos-codegen

**Rust/Go dual-backend code generation and FFI bridge for tpt-telos.**

Part of the [tpt-telos](https://github.com/tpt-solutions/tpt-telos) compiler workspace.

## Overview

`tpt-telos-codegen` is the final emission stage of the tpt-telos pipeline. It consumes the verified
`FuncOutcome`s produced by `tpt-telos-agent` and emits human-readable, compilable source code.

- **`generate_program`** — emits a single self-contained Rust library file (structs, invariant
  `impl` methods, contract doc-comments, one `fn` per function).
- **`generate_project`** — assembles a full dual-backend project tree: a Rust crate, a Go module,
  and a C-ABI FFI bridge between them, with each module routed to Rust or Go by `tpt-telos-router`.
- **eject hatch** — functions marked `@eject` are compiled to a trusted opaque `f_impl` stub wrapped
  by a generated guard that enforces `requires`/`ensures` at runtime via `assert!`.

## Usage

```rust
use tpt_telos_codegen::{generate_program, generate_project};

// Single Rust file
let rust_src = generate_program(&modules, &outcomes);
std::fs::write("output.rs", rust_src).unwrap();

// Full dual-backend project
let project = generate_project(&modules, &outcomes);
for file in &project.files {
    let path = out_dir.join(&file.path);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, &file.contents).unwrap();
}
```

## Key API

| Item | Description |
|------|-------------|
| `generate_program(modules, outcomes) -> String` | Single Rust library source |
| `generate_project(modules, outcomes) -> Project` | Full Rust+Go project tree |
| `Project` | Collection of `GeneratedFile { path, contents }` |
| `eject::render_rust_ejected` | Opaque stub + contract-guarded wrapper |
| `go` module | Go backend emitter |
| `ffi` module | C-ABI FFI bridge generator |

## License

Licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE) at your option.
