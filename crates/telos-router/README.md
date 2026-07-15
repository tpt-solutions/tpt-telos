# tpt-telos-router

**Context router that classifies tpt-telos modules to Rust or Go backends.**

Part of the [tpt-telos](https://github.com/tpt-solutions/tpt-telos) compiler workspace.

## Overview

`tpt-telos-router` is a pure classification crate with no side effects. It reads the `@boundary(...)`
attribute flags on a module or function and decides whether the code should be emitted to the Rust
backend or the Go backend. The result is consumed by `tpt-telos-agent` (to record the target in each
`FuncOutcome`) and by `tpt-telos-codegen` (to drive Rust vs. Go emission and FFI bridge generation).

**Routing rules:**

| Flag | Target |
|------|--------|
| `cpu_bound`, `zero_allocation`, `crypto`, `real_time` | Rust |
| `network_io`, `high_concurrency`, `distributed`, `high_latency` | Go |

Go wins when any Go flag is present. Rust is the default when no flags match.

## Usage

```rust
use tpt_telos_parser::parse;
use tpt_telos_router::route;

let modules = parse(src).unwrap();
for module in &modules {
    for func in &module.funcs {
        let r = route(&func.attrs);
        println!("{} -> {} ({})", func.name, r.target.as_str(), r.reason);
    }
}
```

## Key API

| Item | Description |
|------|-------------|
| `route(attrs: &[Attribute]) -> Route` | Classify a function's attributes |
| `Target` | `Rust` or `Go`; `.as_str()` returns `"rust"` / `"go"` |
| `Route` | `{ target: Target, reason: String }`; `.is_rust() -> bool` |

## License

Licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE) at your option.
