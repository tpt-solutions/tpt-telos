//! Source analysis for the tpt-telos language server.
//!
//! Pure functions over document text -- parse + verify a `.telos` document and
//! surface diagnostics and hover information. Kept free of any I/O or JSON so it
//! is directly unit-testable.

use telos_parser::ast::*;
use telos_parser::parse;

/// LSP diagnostic severity codes.
pub const SEVERITY_ERROR: u8 = 1;
#[allow(dead_code)]
pub const SEVERITY_WARNING: u8 = 2;
pub const SEVERITY_INFO: u8 = 3;

/// A location-tagged diagnostic (0-based line/character, LSP convention).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub line: usize,
    pub character: usize,
    pub end_line: usize,
    pub end_character: usize,
    pub severity: u8,
    pub message: String,
}

/// A per-function report combining its contract with its verification status.
#[derive(Debug, Clone)]
pub struct FuncReport {
    pub module: String,
    pub name: String,
    pub signature: String,
    pub requires: Vec<String>,
    pub ensures: Vec<String>,
    pub target: &'static str,
    pub ejected: bool,
    pub verified: bool,
    pub failures: Vec<String>,
    pub line: usize,
}

/// Analyse a document, producing one report per function, or a parse/extraction
/// error string.
pub fn analyze(text: &str) -> Result<Vec<FuncReport>, String> {
    let modules = parse(text)?;
    let problems = telos_ir::extract(&modules)?;

    let mut reports = Vec::new();
    for m in &modules {
        let target = telos_router::route(&m.attributes).target.as_str();
        for item in &m.items {
            if let Item::Func(f) = item {
                let problem = problems.iter().find(|p| p.func_name == f.name);
                let (verified, failures) = match problem {
                    Some(p) => {
                        let r = telos_verifier::verify(p);
                        (
                            r.all_passed,
                            r.checks
                                .iter()
                                .filter(|c| !c.passed)
                                .map(|c| c.description.clone())
                                .collect::<Vec<_>>(),
                        )
                    }
                    None => (true, Vec::new()),
                };
                reports.push(FuncReport {
                    module: m.name.clone(),
                    name: f.name.clone(),
                    signature: signature(f),
                    requires: f.requires.iter().map(pretty_expr).collect(),
                    ensures: f.ensures.iter().map(pretty_expr).collect(),
                    target,
                    ejected: f.is_ejected(),
                    verified,
                    failures,
                    line: find_func_line(text, &f.name),
                });
            }
        }
    }
    Ok(reports)
}

/// Produce diagnostics for a document: parse errors, and unsatisfied contracts.
/// Ejected functions are trusted opaque blocks, so their internal verification
/// is reported as an informational note rather than an error.
pub fn diagnostics(text: &str) -> Vec<Diagnostic> {
    match analyze(text) {
        Err(e) => {
            let (line, character) = error_position(text, &e);
            vec![Diagnostic {
                line,
                character,
                end_line: line,
                end_character: character + 1,
                severity: SEVERITY_ERROR,
                message: e,
            }]
        }
        Ok(reports) => {
            let mut diags = Vec::new();
            for r in &reports {
                if r.verified {
                    continue;
                }
                let (severity, prefix) = if r.ejected {
                    (SEVERITY_INFO, "ejected (trusted) — boundary guard enforces")
                } else {
                    (SEVERITY_ERROR, "contract not satisfied")
                };
                let end = line_len(text, r.line);
                for fail in &r.failures {
                    diags.push(Diagnostic {
                        line: r.line,
                        character: 0,
                        end_line: r.line,
                        end_character: end,
                        severity,
                        message: format!("{}: {}", prefix, fail),
                    });
                }
            }
            diags
        }
    }
}

/// Markdown hover text for the identifier at `(line, character)`, if it names a
/// function (or the cursor is on a function's definition line).
pub fn hover_markdown(text: &str, line: usize, character: usize) -> Option<String> {
    let reports = analyze(text).ok()?;
    let word = word_at(text, line, character);
    let report = word
        .as_deref()
        .and_then(|w| reports.iter().find(|r| r.name == w))
        .or_else(|| reports.iter().find(|r| r.line == line))?;

    let status = if report.ejected {
        "EJECTED (trusted opaque block; boundary guard enforces the contract)"
    } else if report.verified {
        "VERIFIED — contract mathematically proven"
    } else {
        "UNVERIFIED — contract NOT satisfied"
    };

    let mut md = String::new();
    md.push_str(&format!("### func `{}`\n\n", report.name));
    md.push_str(&format!("```\nfunc {}\n```\n\n", report.signature));
    md.push_str(&format!(
        "- **module:** `{}`\n- **target:** `{}`\n- **status:** {}\n",
        report.module, report.target, status
    ));
    if !report.requires.is_empty() {
        md.push_str("\n**requires**\n");
        for r in &report.requires {
            md.push_str(&format!("- `{}`\n", r));
        }
    }
    if !report.ensures.is_empty() {
        md.push_str("\n**ensures**\n");
        for e in &report.ensures {
            md.push_str(&format!("- `{}`\n", e));
        }
    }
    if !report.failures.is_empty() {
        md.push_str("\n**unsatisfied**\n");
        for f in &report.failures {
            md.push_str(&format!("- `{}`\n", f));
        }
    }
    Some(md)
}

// ---------------------------------------------------------------- helpers

fn signature(f: &Func) -> String {
    let params = f
        .params
        .iter()
        .map(|p| format!("{}: {}", p.name, p.ty.name()))
        .collect::<Vec<_>>()
        .join(", ");
    format!("{}({})", f.name, params)
}

fn find_func_line(text: &str, name: &str) -> usize {
    for (i, line) in text.lines().enumerate() {
        let t = line.trim_start();
        if let Some(rest) = t.strip_prefix("func ") {
            // match `func <name>(` or `func <name> `
            let rest = rest.trim_start();
            if rest == name
                || rest.starts_with(&format!("{}(", name))
                || rest.starts_with(&format!("{} ", name))
            {
                return i;
            }
        }
    }
    0
}

fn line_len(text: &str, line: usize) -> usize {
    text.lines()
        .nth(line)
        .map(|l| l.chars().count())
        .unwrap_or(0)
}

/// Best-effort mapping of an error message to a source position. The lexer
/// reports `offset N`; otherwise we default to the top of the document.
fn error_position(text: &str, msg: &str) -> (usize, usize) {
    if let Some(idx) = msg.find("offset ") {
        let num: String = msg[idx + "offset ".len()..]
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect();
        if let Ok(off) = num.parse::<usize>() {
            return offset_to_line_col(text, off);
        }
    }
    (0, 0)
}

fn offset_to_line_col(text: &str, offset: usize) -> (usize, usize) {
    let mut line = 0;
    let mut col = 0;
    for (i, c) in text.chars().enumerate() {
        if i == offset {
            break;
        }
        if c == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    (line, col)
}

/// Extract the identifier under the cursor at 0-based `(line, character)`.
fn word_at(text: &str, line: usize, character: usize) -> Option<String> {
    let line_str = text.lines().nth(line)?;
    let chars: Vec<char> = line_str.chars().collect();
    if chars.is_empty() {
        return None;
    }
    let is_ident = |c: char| c.is_ascii_alphanumeric() || c == '_';
    let mut start = character.min(chars.len());
    // If the cursor is just past the identifier, step back one.
    if start > 0 && (start >= chars.len() || !is_ident(chars[start])) && is_ident(chars[start - 1])
    {
        start -= 1;
    }
    if start >= chars.len() || !is_ident(chars[start]) {
        return None;
    }
    let mut lo = start;
    while lo > 0 && is_ident(chars[lo - 1]) {
        lo -= 1;
    }
    let mut hi = start;
    while hi + 1 < chars.len() && is_ident(chars[hi + 1]) {
        hi += 1;
    }
    Some(chars[lo..=hi].iter().collect())
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
