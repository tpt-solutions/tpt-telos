# tpt-telos-verifier

Self-contained SMT-style verifier for [tpt-telos](https://github.com/tpt-solutions/tpt-telos) contracts.

Implements a Fourier-Motzkin-based solver over the quantifier-free linear real arithmetic (QF_LRA) fragment — no external Z3/CVC5 dependency. Produces pass/fail results plus, on failure, a concrete counter-example `Model` used to drive agent rewrites.

## Part of the tpt-telos workspace

See the [main repository](https://github.com/tpt-solutions/tpt-telos) for the full pipeline, examples, and documentation.

## License

Apache-2.0
