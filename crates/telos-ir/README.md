# tpt-telos-ir

Intermediate representation and constraint extraction for the [tpt-telos](https://github.com/tpt-solutions/tpt-telos) compiler.

Lowers the parsed `Vec<Module>` AST into `VerificationProblem`s, translating `requires`/`ensures` contracts and invariants into a linear-arithmetic constraint model (QF_LRA-ish) ready for the verifier.

## Part of the tpt-telos workspace

See the [main repository](https://github.com/tpt-solutions/tpt-telos) for the full pipeline, examples, and documentation.

## License

Apache-2.0
