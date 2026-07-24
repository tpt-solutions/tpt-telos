//! tpt-telos command-line interface.
//!
//! Usage:
//!   telos parse     <file.telos>   pretty-print the parsed AST
//!   telos verify    <file.telos>   run formal verification and report pass/fail
//!   telos transpile <file.telos>   run the agentic transpiler and emit Rust
//!   telos build     <file.telos>   transpile then compile the generated Rust
//!   telos project   <file.telos>   generate a dual-backend (Rust + Go) project
//!                                   with an automatic FFI bridge
//!   telos eject     <file.telos>   eject functions to raw Rust/Go opaque blocks
//!                                   guarded by their contracts
//!   telos lsp                       run the language server (LSP over stdio)

use clap::{Parser, Subcommand};
use serde::Serialize;
use std::fs;
use std::process::ExitCode;

use tpt_telos_agent::{transpile_module, StaticAgent};
use tpt_telos_codegen::{generate_program, generate_project, proof};
use tpt_telos_parser::ast::*;
use tpt_telos_parser::parse;
use tpt_telos_verifier::verify;

#[cfg(feature = "llm")]
use tpt_telos_agent::llm_agent::LlmAgent;

#[derive(Parser)]
#[command(
    name = "telos",
    version,
    about = "tpt-telos compiler frontend (Phase 4)"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Scaffold a starter .telos module file.
    Init {
        /// Name of the module to generate (default: "MyModule").
        #[arg(long, default_value = "MyModule")]
        module: String,
        /// Output file path (default: <module>.telos).
        #[arg(long)]
        out: Option<String>,
    },
    /// Parse a .telos file and print its AST.
    Parse {
        /// Path to the .telos source file.
        file: String,
    },
    /// Run formal verification on a .telos file (pass/fail report).
    Verify {
        /// Path to the .telos source file.
        file: String,
        /// Solver backend to use. Default: fourier-motzkin (built-in).
        /// Use `z3` for exact nonlinear arithmetic (requires the `z3` feature).
        #[arg(long, default_value = "fourier-motzkin")]
        solver: String,
        /// Emit machine-readable JSON instead of human-readable text.
        #[arg(long)]
        json: bool,
        /// Re-run verification automatically when the source file changes.
        #[arg(long)]
        watch: bool,
    },
    /// Run the agentic transpiler and print generated Rust.
    Transpile {
        /// Path to the .telos source file.
        file: String,
        /// Use the LLM-backed agent instead of the offline static synthesizer.
        #[arg(long)]
        llm: bool,
        /// Write the generated Rust to this path instead of stdout.
        #[arg(long)]
        out: Option<String>,
    },
    /// Transpile then compile the generated Rust (requires cargo/rustc).
    Build {
        /// Path to the .telos source file.
        file: String,
        /// Emit the generated crate into this directory.
        #[arg(long, default_value = "gen")]
        out_dir: String,
        /// Use the LLM-backed agent instead of the offline static synthesizer.
        #[arg(long)]
        llm: bool,
        /// Solver backend to use.
        #[arg(long, default_value = "fourier-motzkin")]
        solver: String,
        /// Emit machine-readable JSON instead of human-readable text.
        #[arg(long)]
        json: bool,
    },
    /// Generate a dual-backend project (Rust + Go) with an automatic FFI bridge.
    Project {
        /// Path to the .telos source file.
        file: String,
        /// Emit the generated project into this directory.
        #[arg(long, default_value = "gen-project")]
        out_dir: String,
        /// Use the LLM-backed agent instead of the offline static synthesizer.
        #[arg(long)]
        llm: bool,
        /// After generating, compile the Rust crate (cargo) and vet the Go
        /// package (go) to prove both backends build.
        #[arg(long)]
        check: bool,
        /// Exit non-zero if any module has a real_time or zero_allocation
        /// routing conflict with Go (GC is non-deterministic).
        #[arg(long)]
        strict_rt: bool,
        /// Emit machine-readable JSON instead of human-readable text.
        #[arg(long)]
        json: bool,
    },
    /// Eject functions to raw Rust/Go opaque blocks guarded by their contracts.
    Eject {
        /// Path to the .telos source file.
        file: String,
        /// Emit the ejected project into this directory.
        #[arg(long, default_value = "ejected")]
        out_dir: String,
        /// Eject only this function (by name). Default: eject every function.
        #[arg(long)]
        func: Option<String>,
        /// Use the LLM-backed agent instead of the offline static synthesizer.
        #[arg(long)]
        llm: bool,
        /// Exit non-zero if any module has a real_time or zero_allocation
        /// routing conflict with Go (GC is non-deterministic).
        #[arg(long)]
        strict_rt: bool,
    },
    /// Verify a proof manifest against the current source file.
    VerifyManifest {
        /// Path to the telos-proof.json manifest.
        manifest: String,
        /// Path to the original .telos source file.
        source: String,
    },
    /// Run the tpt-telos language server (LSP over stdio) for IDE integration.
    Lsp,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Init { module, out } => match run_init(&module, out.as_deref()) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("init error: {e}");
                ExitCode::FAILURE
            }
        },
        Command::Parse { file } => match run_parse(&file) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("parse error: {e}");
                ExitCode::FAILURE
            }
        },
        Command::Verify {
            file,
            solver,
            json,
            watch,
        } => match run_verify(&file, &solver, json, watch) {
            Ok(passed) => {
                if passed {
                    ExitCode::SUCCESS
                } else {
                    ExitCode::FAILURE
                }
            }
            Err(e) => {
                eprintln!("verify error: {e}");
                ExitCode::FAILURE
            }
        },
        Command::Transpile { file, llm, out } => match run_transpile(&file, llm, out) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("transpile error: {e}");
                ExitCode::FAILURE
            }
        },
        Command::Build {
            file,
            out_dir,
            llm,
            solver,
            json,
        } => match run_build(&file, &out_dir, llm, &solver, json) {
            Ok(passed) => {
                if passed {
                    ExitCode::SUCCESS
                } else {
                    ExitCode::FAILURE
                }
            }
            Err(e) => {
                eprintln!("build error: {e}");
                ExitCode::FAILURE
            }
        },
        Command::Project {
            file,
            out_dir,
            llm,
            check,
            strict_rt,
            json,
        } => match run_project(&file, &out_dir, llm, check, strict_rt, json) {
            Ok(passed) => {
                if passed {
                    ExitCode::SUCCESS
                } else {
                    ExitCode::FAILURE
                }
            }
            Err(e) => {
                eprintln!("project error: {e}");
                ExitCode::FAILURE
            }
        },
        Command::Eject {
            file,
            out_dir,
            func,
            llm,
            strict_rt,
        } => match run_eject(&file, &out_dir, func.as_deref(), llm, strict_rt) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("eject error: {e}");
                ExitCode::FAILURE
            }
        },
        Command::VerifyManifest { manifest, source } => {
            match run_verify_manifest(&manifest, &source) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("verify-manifest error: {e}");
                    ExitCode::FAILURE
                }
            }
        }
        Command::Lsp => match tpt_telos_lsp::run_stdio() {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("lsp error: {e}");
                ExitCode::FAILURE
            }
        },
    }
}

fn load_modules(file: &str) -> Result<Vec<Module>, String> {
    let src = fs::read_to_string(file).map_err(|e| format!("cannot read `{file}`: {e}"))?;
    parse(&src)
}

/// Canonicalise generated Go with `gofmt -w`. Go's printer tightens operator
/// spacing by precedence (e.g. `a == b+c`), which a string-based generator
/// cannot easily reproduce, so we defer final formatting to the real tool.
/// No-ops (with a note) when gofmt is not installed.
fn canonicalize_go(go_dir: &std::path::Path) {
    let out = std::process::Command::new("gofmt")
        .arg("-w")
        .arg(".")
        .current_dir(go_dir)
        .output();
    match out {
        Ok(o) if o.status.success() => {}
        Ok(o) => eprintln!(
            "note: gofmt could not format generated Go: {}",
            String::from_utf8_lossy(&o.stderr).trim()
        ),
        Err(_) => eprintln!("note: gofmt not found; generated Go left un-formatted."),
    }
}

fn run_init(module_name: &str, out: Option<&str>) -> Result<(), String> {
    let default_path = format!("{}.telos", module_name);
    let path = out.unwrap_or(&default_path);
    let template = format!(
        "@boundary(cpu_bound)\n\
         module {mod} {{\n\
         \n\
             invariant Counter {{\n\
                 count >= 0\n\
             }}\n\
         \n\
             func increment(c: Counter)\n\
                 requires c.count >= 0\n\
                 ensures c.count == old(c.count) + 1\n\
             {{\n\
                 mutate state {{\n\
                     c.count += 1\n\
                 }}\n\
             }}\n\
         \n\
             func get(c: Counter): Int\n\
                 requires c.count >= 0\n\
                 ensures result == c.count\n\
             {{\n\
                 mutate state {{\n\
                     return c.count\n\
                 }}\n\
             }}\n\
         }}\n",
        mod = module_name,
    );
    fs::write(path, &template).map_err(|e| format!("cannot write `{path}`: {e}"))?;
    println!("Scaffolded {path} with module `{module_name}`.");
    println!("Run `telos verify {path}` to verify the contracts.");
    Ok(())
}

fn run_parse(file: &str) -> Result<(), String> {
    let modules = load_modules(file)?;
    for m in &modules {
        println!("{}", render_module(m));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Machine-readable JSON output types
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct JsonCheck {
    description: String,
    passed: bool,
    is_ensures: bool,
    is_approximation: bool,
    counterexample: Option<std::collections::HashMap<String, i64>>,
    or_group: Option<usize>,
}

#[derive(Serialize)]
struct JsonFuncResult {
    func_name: String,
    all_passed: bool,
    checks: Vec<JsonCheck>,
}

#[derive(Serialize)]
struct JsonVerifyOutput {
    file: String,
    passed: bool,
    functions: Vec<JsonFuncResult>,
}

#[derive(Serialize)]
struct JsonBuildOutput {
    file: String,
    passed: bool,
    out_dir: String,
    functions: Vec<JsonFuncResult>,
    proof_hash: Option<String>,
}

#[derive(Serialize)]
struct JsonProjectOutput {
    file: String,
    passed: bool,
    out_dir: String,
    functions: Vec<JsonFuncResult>,
    proof_hash: Option<String>,
    has_rust: bool,
    has_go: bool,
    has_python: bool,
    has_ffi: bool,
}

fn collect_verify_output(
    problems: &[tpt_telos_ir::VerificationProblem],
) -> (Vec<JsonFuncResult>, bool) {
    let mut functions = Vec::new();
    let mut overall = true;
    for problem in problems {
        let result = verify(problem);
        let checks: Vec<JsonCheck> = result
            .checks
            .iter()
            .map(|c| JsonCheck {
                description: c.description.clone(),
                passed: c.passed,
                is_ensures: c.is_ensures,
                is_approximation: c.is_approximation,
                counterexample: c.counterexample.clone(),
                or_group: c.or_group,
            })
            .collect();
        if !result.all_passed {
            overall = false;
        }
        functions.push(JsonFuncResult {
            func_name: result.func_name,
            all_passed: result.all_passed,
            checks,
        });
    }
    (functions, overall)
}

fn run_verify(file: &str, solver: &str, json: bool, watch: bool) -> Result<bool, String> {
    // Configure solver backend.
    match solver {
        "fourier-motzkin" => {}
        "z3" => {
            #[cfg(feature = "z3")]
            {
                if !tpt_telos_verifier::z3_solver::is_z3_available() {
                    eprintln!("warning: Z3 not found at runtime; falling back to Fourier-Motzkin");
                } else {
                    tpt_telos_verifier::set_solver_backend(tpt_telos_verifier::SolverBackend::Z3);
                }
            }
            #[cfg(not(feature = "z3"))]
            {
                eprintln!(
                    "warning: Z3 solver requires building with `--features z3`; \
                     falling back to Fourier-Motzkin"
                );
            }
        }
        other => return Err(format!("unknown solver backend: `{other}`")),
    }

    let run_once = |json_mode: bool| -> Result<bool, String> {
        let modules = load_modules(file)?;
        let problems = tpt_telos_ir::extract(&modules)?;

        if problems.is_empty() {
            eprintln!("warning: no functions found to verify in `{file}`");
        }

        if json_mode {
            let (functions, overall) = collect_verify_output(&problems);
            let output = JsonVerifyOutput {
                file: file.to_string(),
                passed: overall,
                functions,
            };
            println!("{}", serde_json::to_string_pretty(&output).unwrap());
            return Ok(overall);
        }

        let mut overall = true;
        println!("Verifying {}\n", file);
        for problem in &problems {
            let result = verify(problem);
            println!("  function {}:", result.func_name);

            let independent: Vec<_> = result
                .checks
                .iter()
                .filter(|c| c.or_group.is_none())
                .collect();
            let mut groups: std::collections::HashMap<usize, Vec<&_>> =
                std::collections::HashMap::new();
            for c in &result.checks {
                if let Some(g) = c.or_group {
                    groups.entry(g).or_default().push(c);
                }
            }

            for check in &independent {
                let tag = if check.passed { "PASS" } else { "FAIL" };
                let kind = if check.is_ensures { "ensures " } else { "" };
                let approx = if check.is_approximation {
                    " [interval-bounded]"
                } else {
                    ""
                };
                println!("    [{}] {}{}{}", tag, kind, check.description, approx);
                if !check.passed {
                    overall = false;
                    if let Some(ce) = &check.counterexample {
                        let bindings: Vec<_> =
                            ce.iter().map(|(k, v)| format!("{}={}", k, v)).collect();
                        println!("      counterexample: {{{}}}", bindings.join(", "));
                    }
                }
            }

            let mut group_keys: Vec<_> = groups.keys().copied().collect();
            group_keys.sort();
            for gk in group_keys {
                let members = &groups[&gk];
                let group_any_passed = members.iter().any(|c| c.passed);
                let group_tag = if group_any_passed { "PASS" } else { "FAIL" };
                println!("    [{}] disjunction group {}:", group_tag, gk);
                for check in members {
                    let tag = if check.passed { "PASS" } else { "FAIL" };
                    let kind = if check.is_ensures { "ensures " } else { "" };
                    let approx = if check.is_approximation {
                        " [interval-bounded]"
                    } else {
                        ""
                    };
                    println!("      [{}] {}{}{}", tag, kind, check.description, approx);
                    if !check.passed {
                        if let Some(ce) = &check.counterexample {
                            let bindings: Vec<_> =
                                ce.iter().map(|(k, v)| format!("{}={}", k, v)).collect();
                            println!("        counterexample: {{{}}}", bindings.join(", "));
                        }
                    }
                }
                if !group_any_passed {
                    overall = false;
                }
            }

            let status = if result.all_passed { "PASS" } else { "FAIL" };
            println!("    => {}\n", status);
        }

        if overall {
            println!("RESULT: all constraints satisfied.");
        } else {
            println!("RESULT: verification failed (see FAIL above).");
        }
        Ok(overall)
    };

    if !watch {
        return run_once(json);
    }

    // Watch mode: poll the file's mtime and re-verify on change.
    let mut last_modified = std::fs::metadata(file).and_then(|m| m.modified()).ok();
    let _overall = run_once(false)?;
    eprintln!("Watching {file} for changes (Ctrl+C to stop)...");
    loop {
        std::thread::sleep(std::time::Duration::from_millis(500));
        let current_modified = std::fs::metadata(file).and_then(|m| m.modified()).ok();
        if current_modified != last_modified {
            last_modified = current_modified;
            println!("\n--- file changed, re-verifying ---\n");
            let _ = run_once(false)?;
        }
    }
}

fn make_agent(llm: bool) -> Result<Box<dyn tpt_telos_agent::CodeAgent>, String> {
    if llm {
        #[cfg(feature = "llm")]
        {
            Ok(Box::new(LlmAgent::from_env()?))
        }
        #[cfg(not(feature = "llm"))]
        {
            Err("the `llm` agent requires building telos with the `llm` feature".to_string())
        }
    } else {
        Ok(Box::new(StaticAgent::new()))
    }
}

fn run_transpile(file: &str, llm: bool, out: Option<String>) -> Result<(), String> {
    let modules = load_modules(file)?;
    let agent = make_agent(llm)?;

    println!("Agentic transpiler (agent: {})\n", agent.name());
    let mut outcomes = Vec::new();
    for m in &modules {
        let funcs = transpile_module(m, agent.as_ref())?;
        for o in &funcs {
            println!(
                "  {} :: {}  [target: {}]",
                m.name,
                o.func_name,
                o.target.as_str()
            );
            for step in &o.iterations {
                let ce = step
                    .counterexample
                    .as_ref()
                    .map(|m| {
                        let mut e: Vec<_> = m.iter().collect();
                        e.sort_by(|a, b| a.0.cmp(b.0));
                        e.iter()
                            .map(|(k, v)| format!("{}={}", k, v))
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .unwrap_or_default();
                println!(
                    "    iter {} [{}] verified={} {}",
                    step.iteration,
                    step.action,
                    step.passed,
                    if ce.is_empty() {
                        String::new()
                    } else {
                        format!("counterexample: {{{}}}", ce)
                    }
                );
            }
            println!(
                "    => {} (agent: {})\n",
                if o.verified { "VERIFIED" } else { "UNVERIFIED" },
                o.agent
            );
        }
        outcomes.extend(funcs);
    }

    let rust = generate_program(&modules, &outcomes);
    match out {
        Some(path) => {
            fs::write(&path, rust).map_err(|e| format!("cannot write `{path}`: {e}"))?;
            println!("Wrote generated Rust to {path}");
        }
        None => {
            println!("----- generated Rust -----\n");
            println!("{rust}");
        }
    }

    Ok(())
}

fn run_build(
    file: &str,
    out_dir: &str,
    llm: bool,
    solver: &str,
    json: bool,
) -> Result<bool, String> {
    // Configure solver backend (same logic as run_verify).
    match solver {
        "fourier-motzkin" => {}
        "z3" => {
            #[cfg(feature = "z3")]
            {
                if !tpt_telos_verifier::z3_solver::is_z3_available() {
                    eprintln!("warning: Z3 not found at runtime; falling back to Fourier-Motzkin");
                } else {
                    tpt_telos_verifier::set_solver_backend(tpt_telos_verifier::SolverBackend::Z3);
                }
            }
            #[cfg(not(feature = "z3"))]
            {
                eprintln!(
                    "warning: Z3 solver requires building with `--features z3`; \
                     falling back to Fourier-Motzkin"
                );
            }
        }
        other => return Err(format!("unknown solver backend: `{other}`")),
    }

    let src_bytes = fs::read(file).map_err(|e| format!("cannot read `{file}`: {e}"))?;
    let modules = load_modules(file)?;
    let agent = make_agent(llm)?;

    let mut outcomes = Vec::new();
    let mut all_verified = true;
    for m in &modules {
        for o in transpile_module(m, agent.as_ref())? {
            if !o.verified {
                all_verified = false;
            }
            outcomes.push(o);
        }
    }

    // Generate proof manifest before appending the static to the Rust source.
    let manifest = proof::generate_manifest(&src_bytes, &outcomes);
    let proof_static = proof::render_rust_proof_static(&manifest);

    let mut rust = generate_program(&modules, &outcomes);
    rust.push_str(&proof_static);

    let crate_dir = std::path::Path::new(out_dir);
    fs::create_dir_all(crate_dir.join("src"))
        .map_err(|e| format!("cannot create {out_dir}: {e}"))?;
    fs::write(crate_dir.join("src/lib.rs"), &rust)
        .map_err(|e| format!("cannot write lib.rs: {e}"))?;
    fs::write(
        crate_dir.join("Cargo.toml"),
        "[package]\nname = \"generated\"\nversion = \"0.1.0\"\n\
         edition = \"2021\"\n\n[dependencies]\n\n[workspace]\n",
    )
    .map_err(|e| format!("cannot write Cargo.toml: {e}"))?;

    // Write proof manifest alongside generated code.
    let proof_json = proof::to_json(&manifest);
    fs::write(crate_dir.join("telos-proof.json"), &proof_json)
        .map_err(|e| format!("cannot write telos-proof.json: {e}"))?;

    // Compile the generated crate.
    let status = std::process::Command::new("cargo")
        .arg("build")
        .arg("--manifest-path")
        .arg(crate_dir.join("Cargo.toml"))
        .status();

    let compile_ok = match &status {
        Ok(s) => s.success(),
        Err(_) => false,
    };

    if json {
        let problems = tpt_telos_ir::extract(&modules)?;
        let (functions, _) = collect_verify_output(&problems);
        let output = JsonBuildOutput {
            file: file.to_string(),
            passed: all_verified && compile_ok,
            out_dir: out_dir.to_string(),
            functions,
            proof_hash: Some(manifest.manifest_hash.clone()),
        };
        println!("{}", serde_json::to_string_pretty(&output).unwrap());
        if !compile_ok {
            return Err("generated Rust failed to compile".into());
        }
        return Ok(all_verified);
    }

    println!("Generated crate written to {out_dir}/");
    println!("Proof manifest written → {out_dir}/telos-proof.json");
    if !all_verified {
        println!("WARNING: some functions were not mathematically verified.");
    }

    println!("Compiling generated Rust with cargo...\n");
    match status {
        Ok(s) if s.success() => {
            println!("\nBUILD: generated Rust compiles successfully.");
            Ok(all_verified)
        }
        Ok(s) => {
            eprintln!("\nBUILD: cargo exited with {s}");
            Err("generated Rust failed to compile".into())
        }
        Err(e) => Err(format!(
            "could not invoke cargo (is it on PATH?): {e}. \
             The generated Rust was still written to {out_dir}/.",
        )),
    }
}

fn run_project(
    file: &str,
    out_dir: &str,
    llm: bool,
    check: bool,
    strict_rt: bool,
    json: bool,
) -> Result<bool, String> {
    let src_bytes = fs::read(file).map_err(|e| format!("cannot read `{file}`: {e}"))?;
    let modules = load_modules(file)?;
    let agent = make_agent(llm)?;

    let mut outcomes = Vec::new();
    let mut all_verified = true;
    for m in &modules {
        let _target = tpt_telos_router::route(&m.attributes).target;
        for o in transpile_module(m, agent.as_ref())? {
            if !o.verified {
                all_verified = false;
            }
            outcomes.push(o);
        }
    }

    let project = generate_project(&modules, &outcomes);

    // --strict-rt: promote routing conflicts to hard errors.
    if strict_rt {
        let has_conflict = project.diagnostics.iter().any(|d| {
            d.kind == tpt_telos_router::DiagnosticKind::RealTimeGoConflict
                || d.kind == tpt_telos_router::DiagnosticKind::ZeroAllocGoConflict
        });
        if has_conflict {
            return Err(
                "strict-rt: routing conflict detected (real_time/zero_allocation \
                 module routed to Go; see warnings above)"
                    .into(),
            );
        }
    }

    let root = std::path::Path::new(out_dir);
    project
        .write(root)
        .map_err(|e| format!("cannot write project to {out_dir}: {e}"))?;
    if project.has_go {
        canonicalize_go(&root.join("go"));
    }

    // Write proof manifest.
    let manifest = proof::generate_manifest(&src_bytes, &outcomes);
    let proof_json = proof::to_json(&manifest);
    fs::write(root.join("telos-proof.json"), &proof_json)
        .map_err(|e| format!("cannot write telos-proof.json: {e}"))?;

    if json {
        let mut ok = all_verified;

        if check {
            if project.has_rust {
                let status = std::process::Command::new("cargo")
                    .arg("build")
                    .arg("--manifest-path")
                    .arg(root.join("rust/Cargo.toml"))
                    .status();
                if let Ok(s) = status {
                    if !s.success() {
                        ok = false;
                    }
                } else {
                    ok = false;
                }
            }
            if project.has_go {
                let status = std::process::Command::new("go")
                    .arg("build")
                    .arg("./...")
                    .current_dir(root.join("go"))
                    .status();
                if let Ok(s) = status {
                    if !s.success() {
                        ok = false;
                    }
                } else {
                    ok = false;
                }
            }
        }

        let problems = tpt_telos_ir::extract(&modules)?;
        let (functions, _) = collect_verify_output(&problems);
        let output = JsonProjectOutput {
            file: file.to_string(),
            passed: ok,
            out_dir: out_dir.to_string(),
            functions,
            proof_hash: Some(manifest.manifest_hash.clone()),
            has_rust: project.has_rust,
            has_go: project.has_go,
            has_python: project.has_python,
            has_ffi: project.has_ffi,
        };
        println!("{}", serde_json::to_string_pretty(&output).unwrap());
        return Ok(ok);
    }

    println!("\nProject written to {out_dir}/");
    for f in &project.files {
        println!("  {}", f.path);
    }
    println!("  telos-proof.json");
    println!(
        "\nBackends: rust={} go={} python={} ffi_bridge={}",
        project.has_rust, project.has_go, project.has_python, project.has_ffi
    );
    println!("Proof manifest written → {out_dir}/telos-proof.json");
    if !all_verified {
        println!("WARNING: some functions were not mathematically verified.");
    }

    if !check {
        return Ok(all_verified);
    }

    let mut ok = all_verified;

    // Compile the Rust crate.
    if project.has_rust {
        println!("\nCompiling Rust backend with cargo...");
        let status = std::process::Command::new("cargo")
            .arg("build")
            .arg("--manifest-path")
            .arg(root.join("rust/Cargo.toml"))
            .status()
            .map_err(|e| format!("could not invoke cargo: {e}"))?;
        if status.success() {
            println!("  Rust backend compiles.");
        } else {
            eprintln!("  Rust backend failed to compile ({status}).");
            ok = false;
        }
    }

    // Vet the Go package.
    if project.has_go {
        println!("\nBuilding Go backend with go...");
        let status = std::process::Command::new("go")
            .arg("build")
            .arg("./...")
            .current_dir(root.join("go"))
            .status()
            .map_err(|e| format!("could not invoke go (is it on PATH?): {e}"))?;
        if status.success() {
            println!("  Go backend compiles.");
        } else {
            eprintln!("  Go backend failed to compile ({status}).");
            ok = false;
        }

        let out = std::process::Command::new("gofmt")
            .arg("-l")
            .arg(".")
            .current_dir(root.join("go"))
            .output();
        match out {
            Ok(o) if o.status.success() => {
                let listed = String::from_utf8_lossy(&o.stdout);
                if listed.trim().is_empty() {
                    println!("  Go sources (incl. cgo FFI) are well-formed.");
                } else {
                    eprintln!("  Go sources not gofmt-clean:\n{}", listed);
                    ok = false;
                }
            }
            Ok(o) => {
                eprintln!(
                    "  gofmt reported errors: {}",
                    String::from_utf8_lossy(&o.stderr)
                );
                ok = false;
            }
            Err(e) => {
                eprintln!("  could not invoke gofmt: {e}");
            }
        }
    }

    if ok {
        println!("\nPROJECT: dual-backend microservice builds successfully.");
    } else {
        println!("\nPROJECT: one or more backends failed.");
    }
    Ok(ok)
}

fn run_verify_manifest(manifest_path: &str, source_path: &str) -> Result<(), String> {
    let manifest_json = fs::read_to_string(manifest_path)
        .map_err(|e| format!("cannot read `{manifest_path}`: {e}"))?;
    let source_bytes =
        fs::read(source_path).map_err(|e| format!("cannot read `{source_path}`: {e}"))?;

    let result = proof::verify_manifest(&manifest_json, &source_bytes);

    if result.tampered {
        eprintln!("MANIFEST TAMPERED");
        if !result.source_hash_valid {
            eprintln!(
                "  source hash MISMATCH: expected {}, got {}",
                result.source_hash_expected, result.source_hash_actual
            );
        }
        if !result.manifest_hash_valid {
            eprintln!(
                "  manifest hash MISMATCH: expected {}, got {}",
                result.manifest_hash_expected, result.manifest_hash_actual
            );
        }
        return Err("manifest verification failed".into());
    }

    println!("MANIFEST OK");
    println!("  source hash:     {}", result.source_hash_expected);
    println!("  manifest hash:   {}", result.manifest_hash_expected);
    println!("  source unchanged: yes");
    println!("  manifest intact:  yes");
    Ok(())
}

fn run_eject(
    file: &str,
    out_dir: &str,
    only: Option<&str>,
    llm: bool,
    strict_rt: bool,
) -> Result<(), String> {
    let mut modules = load_modules(file)?;
    let agent = make_agent(llm)?;

    // Transpile first so ejected opaque blocks are seeded with verified bodies.
    let mut outcomes = Vec::new();
    for m in &modules {
        outcomes.extend(transpile_module(m, agent.as_ref())?);
    }

    // Mark the requested functions as ejected. In-source `@eject` attributes are
    // always honored; here we additionally eject on demand from the CLI.
    let mut ejected: Vec<(String, String, String)> = Vec::new(); // (module, func, lang)
    for m in &mut modules {
        let lang = tpt_telos_router::route(&m.attributes)
            .target
            .as_str()
            .to_string();
        for item in &mut m.items {
            if let Item::Func(f) = item {
                let selected = only.map(|n| n == f.name).unwrap_or(true);
                if selected || f.is_ejected() {
                    if !f.is_ejected() {
                        f.attributes.push(Attribute {
                            name: "eject".to_string(),
                            args: vec![Arg::Flag(lang.clone())],
                        });
                    }
                    ejected.push((m.name.clone(), f.name.clone(), lang.clone()));
                }
            }
        }
    }

    if ejected.is_empty() {
        return Err(format!(
            "no matching function to eject{}",
            only.map(|n| format!(" (looked for `{n}`)"))
                .unwrap_or_default()
        ));
    }

    let project = generate_project(&modules, &outcomes);
    let root = std::path::Path::new(out_dir);

    // Surface routing diagnostics as warnings.
    for diag in &project.diagnostics {
        eprintln!(
            "WARNING [{}] {}",
            format!("{:?}", diag.kind).to_lowercase(),
            diag.message
        );
    }

    // --strict-rt: promote routing conflicts to hard errors.
    if strict_rt {
        let has_conflict = project.diagnostics.iter().any(|d| {
            d.kind == tpt_telos_router::DiagnosticKind::RealTimeGoConflict
                || d.kind == tpt_telos_router::DiagnosticKind::ZeroAllocGoConflict
        });
        if has_conflict {
            return Err(
                "strict-rt: routing conflict detected (real_time/zero_allocation \
                 module routed to Go; see warnings above)"
                    .into(),
            );
        }
    }

    project
        .write(root)
        .map_err(|e| format!("cannot write ejected project to {out_dir}: {e}"))?;
    if project.has_go {
        canonicalize_go(&root.join("go"));
    }

    // Write a manifest recording what was ejected (trusted opaque blocks).
    let mut manifest = String::from("{\n  \"ejected\": [\n");
    for (i, (module, func, lang)) in ejected.iter().enumerate() {
        let comma = if i + 1 < ejected.len() { "," } else { "" };
        manifest.push_str(&format!(
            "    {{ \"module\": \"{}\", \"func\": \"{}\", \"lang\": \"{}\" }}{}\n",
            module, func, lang, comma
        ));
    }
    manifest.push_str("  ]\n}\n");
    std::fs::write(root.join("telos-eject.json"), manifest)
        .map_err(|e| format!("cannot write manifest: {e}"))?;

    println!("Ejected {} function(s) to {}/", ejected.len(), out_dir);
    for (module, func, lang) in &ejected {
        println!(
            "  {}::{} -> raw {} (opaque block + contract guard)",
            module, func, lang
        );
    }
    for f in &project.files {
        println!("  {}", f.path);
    }
    println!("  telos-eject.json");
    println!(
        "\nEjected code is a trusted opaque block; the generated guard enforces the\n\
         original requires/ensures contracts at the boundary."
    );
    Ok(())
}

// ---- lightweight AST rendering ----

fn render_module(m: &Module) -> String {
    let attrs: Vec<String> = m
        .attributes
        .iter()
        .map(|a| {
            if a.args.is_empty() {
                format!("@{}", a.name)
            } else {
                let args = a
                    .args
                    .iter()
                    .map(|arg| match arg {
                        Arg::Flag(f) => f.clone(),
                        Arg::Kv(k, v) => format!("{}={}", k, render_literal(v)),
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("@{}(", a.name) + &args + ")"
            }
        })
        .collect();
    let header = if attrs.is_empty() {
        format!("module {}", m.name)
    } else {
        format!("{} module {}", attrs.join(" "), m.name)
    };
    let items = m
        .items
        .iter()
        .map(render_item)
        .collect::<Vec<_>>()
        .join("\n");
    format!("{}{{\n{}\n}}", header, indent(&items))
}

fn render_item(item: &Item) -> String {
    match item {
        Item::Invariant(i) => render_invariant(i),
        Item::Func(f) => render_func(f),
        Item::Struct(s) => {
            let fields: Vec<_> = s
                .fields
                .iter()
                .map(|f| format!("{}: {}", f.name, render_type(&f.ty)))
                .collect();
            format!("struct {} {{ {} }}", s.name, fields.join(", "))
        }
        Item::Enum(e) => {
            let variants: Vec<_> = e
                .variants
                .iter()
                .map(|v| {
                    if v.fields.is_empty() {
                        v.name.clone()
                    } else {
                        let fields: Vec<_> = v
                            .fields
                            .iter()
                            .map(|f| format!("{}: {}", f.name, render_type(&f.ty)))
                            .collect();
                        format!("{} {{ {} }}", v.name, fields.join(", "))
                    }
                })
                .collect();
            format!("enum {} {{ {} }}", e.name, variants.join(", "))
        }
    }
}

fn render_invariant(i: &Invariant) -> String {
    let body = i
        .constraints
        .iter()
        .map(pretty_expr)
        .collect::<Vec<_>>()
        .join("; ");
    format!("invariant {} {{ {} }}", i.name, body)
}

fn render_func(f: &Func) -> String {
    let params = f
        .params
        .iter()
        .map(|p| format!("{}: {}", p.name, render_type(&p.ty)))
        .collect::<Vec<_>>()
        .join(", ");
    let clauses: Vec<String> = f
        .requires
        .iter()
        .map(|e| format!("requires {}", pretty_expr(e)))
        .chain(
            f.ensures
                .iter()
                .map(|e| format!("ensures {}", pretty_expr(e))),
        )
        .collect();
    let body = f
        .body
        .iter()
        .map(render_stmt)
        .collect::<Vec<_>>()
        .join("\n");
    let mut out = format!("func {}({})", f.name, params);
    if !clauses.is_empty() {
        out += "\n";
        out += &indent(&clauses.join("\n"));
    }
    if f.elided {
        out += ";";
    } else {
        out += "\n{\n";
        out += &indent(&body);
        out += "\n}";
    }
    out
}

fn render_stmt(s: &Stmt) -> String {
    match s {
        Stmt::MutateState(assigns) => {
            let inner = assigns
                .iter()
                .map(render_assign)
                .collect::<Vec<_>>()
                .join("\n");
            format!("mutate state {{\n{}\n}}", indent(&inner))
        }
        Stmt::Assign(a) => render_assign(a),
        Stmt::Let(lb) => {
            let ty = lb
                .ty
                .as_ref()
                .map(|t| format!(": {}", render_type(t)))
                .unwrap_or_default();
            format!("let {}{} = {};", lb.name, ty, pretty_expr(&lb.value))
        }
        Stmt::If(is) => {
            let mut out = format!("if {} {{\n", pretty_expr(&is.condition));
            let then: Vec<_> = is.then_body.iter().map(render_stmt).collect();
            out += &indent(&then.join("\n"));
            out += "\n}";
            if let Some(else_body) = &is.else_body {
                let els: Vec<_> = else_body.iter().map(render_stmt).collect();
                out += " else {\n";
                out += &indent(&els.join("\n"));
                out += "\n}";
            }
            out
        }
        Stmt::Match(ms) => {
            let arms: Vec<_> = ms
                .arms
                .iter()
                .map(|a| {
                    let body: Vec<_> = a.body.iter().map(render_stmt).collect();
                    format!(
                        "{} => {{\n{}\n}}",
                        render_pattern(&a.pattern),
                        indent(&body.join("\n"))
                    )
                })
                .collect();
            format!(
                "match {} {{\n{}\n}}",
                pretty_expr(&ms.scrutinee),
                indent(&arms.join("\n"))
            )
        }
        Stmt::Return(e) => match e {
            Some(expr) => format!("return {};", pretty_expr(expr)),
            None => "return;".to_string(),
        },
    }
}

fn render_assign(a: &Assign) -> String {
    let op = match a.op {
        AssignOp::Set => "=",
        AssignOp::Add => "+=",
        AssignOp::Sub => "-=",
    };
    format!(
        "{} {} {};",
        pretty_expr(&a.target),
        op,
        pretty_expr(&a.value)
    )
}

fn render_type(t: &Type) -> String {
    match t {
        Type::Named(s) => match s.as_str() {
            "Float32" => "f32".to_string(),
            "Float64" => "f64".to_string(),
            "Int" => "i64".to_string(),
            "PositiveInt" => "i64".to_string(),
            other => other.to_string(),
        },
        Type::Generic(name, args) => {
            let args: Vec<_> = args.iter().map(render_type).collect();
            format!("{}<{}>", name, args.join(", "))
        }
        Type::Tuple(elems) => {
            let elems: Vec<_> = elems.iter().map(render_type).collect();
            format!("({})", elems.join(", "))
        }
        Type::Array(elem, len) => format!("[{}; {}]", render_type(elem), len),
        Type::Slice(elem) => format!("[{}]", render_type(elem)),
    }
}

fn render_literal(l: &Literal) -> String {
    match l {
        Literal::Int(n) => n.to_string(),
        Literal::Ident(s) => s.clone(),
    }
}

fn pretty_expr(e: &Expr) -> String {
    match e {
        Expr::Int(n) => n.to_string(),
        Expr::Var(v) => v.clone(),
        Expr::Field { base, field } => format!("{}.{}", base, field),
        Expr::Old(inner) => format!("old({})", pretty_expr(inner)),
        Expr::Unary { op, expr } => match op {
            UnOp::Neg => format!("-{}", pretty_expr(expr)),
        },
        Expr::Bin { op, lhs, rhs } => {
            let s = match op {
                BinOp::Add => "+",
                BinOp::Sub => "-",
                BinOp::Mul => "*",
                BinOp::Div => "/",
                BinOp::Eq => "==",
                BinOp::Ne => "!=",
                BinOp::Lt => "<",
                BinOp::Le => "<=",
                BinOp::Gt => ">",
                BinOp::Ge => ">=",
                BinOp::And => "&&",
                BinOp::Or => "||",
            };
            format!("{} {} {}", pretty_expr(lhs), s, pretty_expr(rhs))
        }
        Expr::Call(c) => {
            let args: Vec<_> = c.args.iter().map(pretty_expr).collect();
            format!("{}({})", c.func, args.join(", "))
        }
        Expr::MethodCall(m) => {
            let args: Vec<_> = m.args.iter().map(pretty_expr).collect();
            format!(
                "{}.{}({})",
                pretty_expr(&m.receiver),
                m.method,
                args.join(", ")
            )
        }
        Expr::Index(i) => format!("{}[{}]", pretty_expr(&i.receiver), pretty_expr(&i.index)),
        Expr::If(i) => format!(
            "if {} {{ {} }} else {{ {} }}",
            pretty_expr(&i.condition),
            pretty_expr(&i.then_expr),
            pretty_expr(&i.else_expr)
        ),
        Expr::Match(m) => {
            let arms: Vec<_> = m
                .arms
                .iter()
                .map(|a| format!("... => {}", pretty_expr(&a.expr)))
                .collect();
            format!(
                "match {} {{ {} }}",
                pretty_expr(&m.scrutinee),
                arms.join(", ")
            )
        }
        Expr::Try(e) => format!("{}?", pretty_expr(e)),
        Expr::Forall(f) => format!(
            "forall {}: {} {{ {} }}",
            f.var,
            f.var_ty.name(),
            pretty_expr(&f.body)
        ),
        Expr::Aggregate(a) => {
            let args: Vec<_> = a.args.iter().map(pretty_expr).collect();
            format!("{}({})", a.op.op_name(), args.join(", "))
        }
        Expr::Range { lo, hi } => format!("{}..{}", pretty_expr(lo), pretty_expr(hi)),
    }
}

fn render_pattern(p: &Pattern) -> String {
    match p {
        Pattern::Literal(n) => n.to_string(),
        Pattern::Var(v) => v.clone(),
        Pattern::Constructor(name, fields) => {
            if fields.is_empty() {
                name.clone()
            } else {
                let fields: Vec<_> = fields.iter().map(render_pattern).collect();
                format!("{}({})", name, fields.join(", "))
            }
        }
        Pattern::Wildcard => "_".to_string(),
    }
}

fn indent(s: &str) -> String {
    s.lines()
        .map(|line| {
            if line.is_empty() {
                line.to_string()
            } else {
                format!("    {}", line)
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}
