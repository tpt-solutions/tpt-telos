//! tpt-telos command-line interface.
//!
//! Usage:
//!   telos parse     <file.telos>   pretty-print the parsed AST
//!   telos verify    <file.telos>   run formal verification and report pass/fail
//!   telos transpile <file.telos>   run the agentic transpiler and emit Rust
//!   telos build     <file.telos>   transpile then compile the generated Rust

use clap::{Parser, Subcommand};
use std::fs;
use std::process::ExitCode;

use telos_agent::{StaticAgent, transpile_module};
use telos_codegen::generate_program;
use telos_parser::ast::*;
use telos_parser::parse;
use telos_verifier::verify;

#[cfg(feature = "llm")]
use telos_agent::llm_agent::LlmAgent;

#[derive(Parser)]
#[command(name = "telos", version, about = "tpt-telos compiler frontend (Phase 2)")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Parse a .telos file and print its AST.
    Parse {
        /// Path to the .telos source file.
        file: String,
    },
    /// Run formal verification on a .telos file (pass/fail report).
    Verify {
        /// Path to the .telos source file.
        file: String,
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
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Parse { file } => match run_parse(&file) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("parse error: {e}");
                ExitCode::FAILURE
            }
        },
        Command::Verify { file } => match run_verify(&file) {
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
        Command::Build { file, out_dir, llm } => match run_build(&file, &out_dir, llm) {
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
    }
}

fn load_modules(file: &str) -> Result<Vec<Module>, String> {
    let src = fs::read_to_string(file).map_err(|e| format!("cannot read `{file}`: {e}"))?;
    parse(&src)
}

fn run_parse(file: &str) -> Result<(), String> {
    let modules = load_modules(file)?;
    for m in &modules {
        println!("{}", render_module(m));
    }
    Ok(())
}

fn run_verify(file: &str) -> Result<bool, String> {
    let modules = load_modules(file)?;
    let problems = telos_ir::extract(&modules)?;

    if problems.is_empty() {
        eprintln!("warning: no functions found to verify in `{file}`");
    }

    let mut overall = true;
    println!("Verifying {}\n", file);
    for problem in &problems {
        let result = verify(problem);
        println!("  function {}:", result.func_name);
        for check in &result.checks {
            let tag = if check.passed { "PASS" } else { "FAIL" };
            let kind = if check.is_ensures { "ensures " } else { "" };
            println!("    [{}] {}{}", tag, kind, check.description);
            if !check.passed {
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
}

fn make_agent(llm: bool) -> Result<Box<dyn telos_agent::CodeAgent>, String> {
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
            println!("  {} :: {}  [target: {}]", m.name, o.func_name, o.target.as_str());
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

fn run_build(file: &str, out_dir: &str, llm: bool) -> Result<bool, String> {
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

    let rust = generate_program(&modules, &outcomes);
    let crate_dir = std::path::Path::new(out_dir);
    fs::create_dir_all(crate_dir.join("src"))
        .map_err(|e| format!("cannot create {out_dir}: {e}"))?;
    fs::write(crate_dir.join("src/lib.rs"), &rust)
        .map_err(|e| format!("cannot write lib.rs: {e}"))?;
    fs::write(
        crate_dir.join("Cargo.toml"),
        format!(
            "[package]\nname = \"generated\"\nversion = \"0.1.0\"\n\
             edition = \"2021\"\n\n[dependencies]\n\n[workspace]\n"
        ),
    )
    .map_err(|e| format!("cannot write Cargo.toml: {e}"))?;

    println!("Generated crate written to {out_dir}/");
    if !all_verified {
        println!("WARNING: some functions were not mathematically verified.");
    }

    // Compile the generated crate.
    println!("Compiling generated Rust with cargo...\n");
    let status = std::process::Command::new("cargo")
        .arg("build")
        .arg("--manifest-path")
        .arg(crate_dir.join("Cargo.toml"))
        .status();

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
    }
}

fn render_assign(a: &Assign) -> String {
    let op = match a.op {
        AssignOp::Set => "=",
        AssignOp::Add => "+=",
        AssignOp::Sub => "-=",
    };
    format!("{} {} {};", pretty_expr(&a.target), op, pretty_expr(&a.value))
}

fn render_type(t: &Type) -> String {
    match t {
        Type::Named(s) => s.clone(),
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
