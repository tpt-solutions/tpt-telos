# examples/

Fixture `.telos` files used by integration tests and to demonstrate language features.

| File | Demonstrates |
|------|--------------|
| `wallet.telos` | Core pass case: `@boundary(cpu_bound)` module with a non-negative balance invariant, `transfer` with `requires`/`ensures` and `old(...)`. |
| `broken.telos` | Core fail case: intentionally wrong mutation body (adds instead of subtracts) so `telos verify` exits non-zero with a counterexample. |
| `nested.telos` | Nested arithmetic in `ensures` (`old(c.value) * 2 + a + b`), `&&`-flattened `requires`, and scalar equality post-conditions. |
| `microservice.telos` | Dual-backend compilation: `@boundary(cpu_bound, zero_allocation)` Ledger routed to Rust + `@boundary(network_io, high_concurrency)` GatewayApi routed to Go, with the automatic FFI bridge. |
| `eject.telos` | The eject hatch: `@eject(rust)` marks a function as a trusted opaque block (`*_impl`) wrapped by a generated contract guard enforcing `requires`/`ensures` at runtime. |
| `float.telos` | `Float32`/`Float64` type parsing and codegen; documents that the IR currently treats floats as integers (QF_LRA). |
| `array_test.telos` | `[T; N]` fixed-array type in function signatures; shows parser/codegen support and documents the absence of IR length-constraint extraction. |
| `disjunction.telos` | `requires a \|\| b` DNF expansion into independent verification sub-problems; `ensures` disjunction satisfied when at least one branch holds. |
| `intent.telos` | Body-elided (intent-only) functions: the compiler synthesizes implementations from `ensures` contracts alone, then verifies them. |
| `interval.telos` | Nonlinear interval bounding: `ensures x * y <= 50` over-approximated via interval arithmetic when both variables have bounded `requires`; result tagged `[interval-bounded]`. |
| `cross_module.telos` | Cross-module invariant references: `Operations` uses `Counter` declared in `Core`; demonstrates the global type-resolution pass in `tpt-telos-ir`. |
| `physics.telos` | `@boundary(ml_training)` routed to the Python/JAX backend; demonstrates the Python codegen target with `@dataclass` structs and runtime `assert` guards. |
