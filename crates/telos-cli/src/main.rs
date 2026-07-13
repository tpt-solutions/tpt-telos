//! tpt-telos command-line interface.
//!
//! Usage:
//!   telos parse   <file.telos>     pretty-print the parsed AST
//!   telos verify  <file.telos>     run formal verification and report pass/fail

use clap::{Parser, Subcommand};
use std::fs;
use std::process::ExitCode;
use telos_ir::extract;
use telos_parser::ast::*;
use telos_parser::parse;
use telos_verifier::verify;

#[derive(Parser)]
#[command(name = "telos", version, about = "tpt-telos compiler frontend (Phase 1)")]
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
    }
}

fn run_parse(file: &str) -> Result<(), String> {
    let src = fs::read_to_string(file).map_err(|e| format!("cannot read `{file}`: {e}"))?;
    let modules = parse(&src)?;
    for m in &modules {
        println!("{}", render_module(m));
    }
    Ok(())
}

fn run_verify(file: &str) -> Result<bool, String> {
    let src = fs::read_to_string(file).map_err(|e| format!("cannot read `{file}`: {e}"))?;
    let modules = parse(&src)?;
    let problems = extract(&modules)?;

    if problems.is_empty() {
        eprintln!("warning: no functions found to verify in `{file}`");
    }

    let mut overall = true;
    println!("Verifying {}\n", file);
    for problem in &problems {
        let result = verify(problem);
        println!("  function {}:", result.func_name);
        for check in &result.checks {
            let tag = if check.passed { "✓" } else { "✗" };
            let kind = if check.is_ensures { "ensures " } else { "" };
            println!("    {} {}{}", tag, kind, check.description);
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
        println!("RESULT: verification failed (see ✗ above).");
    }
    Ok(overall)
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
    out += "\n{\n";
    out += &indent(&body);
    out += "\n}";
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
