# tpt-telos-agent

Agentic transpiler backend for [tpt-telos](https://github.com/tpt-solutions/tpt-telos).

Runs the Generate → Verify → Counter-example → Rewrite loop per function via the `CodeAgent` trait:

- **`StaticAgent`** (default, always available) — fully offline, deterministic synthesizer; translates the developer's body when present, otherwise derives one from `ensures` contracts.
- **`LlmAgent`** (behind the `llm` Cargo feature) — calls a real LLM over an OpenAI-compatible or native Anthropic wire format.

## Features

| Feature | Description |
|---|---|
| `llm` | Enables the real LLM-backed agent (requires network + an API key at runtime) |

## Part of the tpt-telos workspace

See the [main repository](https://github.com/tpt-solutions/tpt-telos) for the full pipeline, examples, and documentation.

## License

Apache-2.0
