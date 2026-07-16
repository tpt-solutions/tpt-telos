# tpt-telos-router

Backend router for the [tpt-telos](https://github.com/tpt-solutions/tpt-telos) compiler.

Reads `@boundary(...)` attributes on modules and functions and classifies each as `Target::Rust` or `Target::Go`:

- `cpu_bound` / `zero_allocation` / `crypto` / `real_time` → Rust
- `network_io` / `high_concurrency` / `distributed` / `high_latency` → Go

## Part of the tpt-telos workspace

See the [main repository](https://github.com/tpt-solutions/tpt-telos) for the full pipeline, examples, and documentation.

## License

Apache-2.0
