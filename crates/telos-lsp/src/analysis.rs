//! Source analysis for the tpt-telos language server.
//!
//! Pure functions over document text -- parse + verify a `.telos` document and
//! surface diagnostics and hover information. Kept free of any I/O or JSON so it
//! is directly unit-testable.

use tpt_telos_parser::ast::*;
use tpt_telos_parser::parse;

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
    /// Counterexamples for failed checks: `(description, model)`.
    pub counterexamples: Vec<(String, std::collections::HashMap<String, i64>)>,
    pub line: usize,
}

/// Analyse a document, producing one report per function, or a parse/extraction
/// error string.
///
/// # Examples
///
/// ```
/// use tpt_telos_lsp::analysis::analyze;
///
/// let src = r#"
///     module Bank {
///         invariant Wallet { balance >= 0 }
///         func deposit(w: Wallet, amount: PositiveInt)
///             ensures w.balance == old(w.balance) + amount
///         { mutate state { w.balance += amount } }
///     }
/// "#;
///
/// let reports = analyze(src).unwrap();
/// assert_eq!(reports.len(), 1);
/// assert_eq!(reports[0].name, "deposit");
/// assert!(reports[0].verified);
/// ```
pub fn analyze(text: &str) -> Result<Vec<FuncReport>, String> {
    let modules = parse(text)?;
    let problems = tpt_telos_ir::extract(&modules)?;

    let mut reports = Vec::new();
    for m in &modules {
        let target = tpt_telos_router::route(&m.attributes).target.as_str();
        for item in &m.items {
            if let Item::Func(f) = item {
                let problem = problems.iter().find(|p| p.func_name == f.name);
                let (verified, failures, counterexamples) = match problem {
                    Some(p) => {
                        let r = tpt_telos_verifier::verify(p);
                        let fails: Vec<String> = r
                            .checks
                            .iter()
                            .filter(|c| !c.passed)
                            .map(|c| c.description.clone())
                            .collect();
                        let ces: Vec<(String, std::collections::HashMap<String, i64>)> = r
                            .checks
                            .iter()
                            .filter(|c| !c.passed)
                            .filter_map(|c| {
                                c.counterexample
                                    .as_ref()
                                    .map(|ce| (c.description.clone(), ce.clone()))
                            })
                            .collect();
                        (r.all_passed, fails, ces)
                    }
                    None => (true, Vec::new(), Vec::new()),
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
                    counterexamples,
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
///
/// # Examples
///
/// A correct implementation produces no diagnostics:
///
/// ```
/// use tpt_telos_lsp::analysis::diagnostics;
///
/// let src = r#"
///     module Bank {
///         invariant Wallet { balance >= 0 }
///         func deposit(w: Wallet, amount: PositiveInt)
///             ensures w.balance == old(w.balance) + amount
///         { mutate state { w.balance += amount } }
///     }
/// "#;
///
/// assert!(diagnostics(src).is_empty());
/// ```
///
/// A parse error yields one error diagnostic:
///
/// ```
/// use tpt_telos_lsp::analysis::{diagnostics, SEVERITY_ERROR};
///
/// let diags = diagnostics("module {");
/// assert!(!diags.is_empty());
/// assert_eq!(diags[0].severity, SEVERITY_ERROR);
/// ```
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
                // Find counterexample descriptions that match this failure.
                let mut ce_iter = r.counterexamples.iter();
                for fail in &r.failures {
                    let mut msg = format!("{}: {}", prefix, fail);
                    if let Some((_, ce)) = ce_iter.find(|(desc, _)| desc == fail) {
                        let bindings: Vec<_> =
                            ce.iter().map(|(k, v)| format!("{}={}", k, v)).collect();
                        msg.push_str(&format!(" — counterexample: {{{}}}", bindings.join(", ")));
                    }
                    diags.push(Diagnostic {
                        line: r.line,
                        character: 0,
                        end_line: r.line,
                        end_character: end,
                        severity,
                        message: msg,
                    });
                }
            }
            diags
        }
    }
}

/// Markdown hover text for the identifier at `(line, character)`, if it names a
/// function (or the cursor is on a function's definition line).
///
/// # Examples
///
/// ```
/// use tpt_telos_lsp::analysis::hover_markdown;
///
/// let src = "module M {\n    func noop(w: Wallet) ;\n}";
///
/// // Line 1 (0-based), column 9 is within "noop".
/// let md = hover_markdown(src, 1, 9);
/// assert!(md.is_some());
/// let text = md.unwrap();
/// assert!(text.contains("noop"));
/// ```
///
/// Returns `None` when the cursor is not over a known function:
///
/// ```
/// use tpt_telos_lsp::analysis::hover_markdown;
///
/// let md = hover_markdown("module M {}", 0, 0);
/// assert!(md.is_none());
/// ```
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
        if !report.counterexamples.is_empty() {
            md.push_str("\n**counterexamples**\n");
            for (desc, ce) in &report.counterexamples {
                let bindings: Vec<_> = ce.iter().map(|(k, v)| format!("{}={}", k, v)).collect();
                md.push_str(&format!("- `{}` => {{{}}}\n", desc, bindings.join(", ")));
            }
        }
    }
    Some(md)
}

/// A suggested quick-fix: insert a new `requires` clause into a function that
/// rules out a concrete counterexample witness for one of its failing checks.
/// This is a starting point for the developer to refine, not a guaranteed fix
/// (it only excludes the exact witness the solver found).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuickFix {
    pub title: String,
    /// 0-based line to insert `new_text` before.
    pub line: usize,
    pub new_text: String,
}

/// Derive quick-fix `requires` suggestions from every failing check that has
/// a counterexample. Returns one suggestion per (function, failing check)
/// pair; ejected and fully-verified functions contribute nothing.
///
/// # Examples
///
/// ```
/// use tpt_telos_lsp::analysis::code_actions;
///
/// let src = r#"
///     module Bank {
///         invariant Wallet { balance >= 0 }
///         func withdraw(w: Wallet, amount: Int)
///             ensures w.balance == old(w.balance) - amount
///         { mutate state { w.balance -= amount } }
///     }
/// "#;
///
/// let fixes = code_actions(src);
/// assert!(!fixes.is_empty());
/// assert!(fixes[0].new_text.contains("requires"));
/// ```
pub fn code_actions(text: &str) -> Vec<QuickFix> {
    let reports = match analyze(text) {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };
    let mut fixes = Vec::new();
    for r in &reports {
        if r.verified || r.ejected {
            continue;
        }
        for (desc, ce) in &r.counterexamples {
            if ce.is_empty() {
                continue;
            }
            let mut keys: Vec<_> = ce.keys().cloned().collect();
            keys.sort();
            let clause = keys
                .iter()
                .map(|k| format!("{} == {}", k, ce[k]))
                .collect::<Vec<_>>()
                .join(" && ");
            fixes.push(QuickFix {
                title: format!("Add `requires` excluding counterexample for `{}`", desc),
                line: r.line + 1,
                new_text: format!("    requires !({})\n", clause),
            });
        }
    }
    fixes
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

// ---------------------------------------------------------------- formatter

/// Parse `text` and render it back as canonically-formatted telos source.
///
/// Returns `Err` if the source cannot be parsed (parse errors are surfaced by
/// diagnostics, so callers typically return an empty edit list on failure).
///
/// # Examples
///
/// ```
/// use tpt_telos_lsp::analysis::format_source;
///
/// let src = "module M { func noop(w: Wallet) ; }";
/// let formatted = format_source(src).unwrap();
/// assert!(formatted.contains("module M"));
/// assert!(formatted.contains("func noop"));
/// ```
pub fn format_source(text: &str) -> Result<String, String> {
    let modules = parse(text)?;
    let rendered: Vec<String> = modules.iter().map(render_module).collect();
    let mut out = rendered.join("\n\n");
    if !out.ends_with('\n') {
        out.push('\n');
    }
    Ok(out)
}

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
                format!("@{}({})", a.name, args)
            }
        })
        .collect();
    let header = if attrs.is_empty() {
        format!("module {} ", m.name)
    } else {
        format!("{}\nmodule {} ", attrs.join("\n"), m.name)
    };
    let items = m
        .items
        .iter()
        .map(render_item)
        .collect::<Vec<_>>()
        .join("\n\n");
    format!("{}{{\n{}\n}}", header, fmt_indent(&items))
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
            format!("struct {} {{\n{}\n}}", s.name, fmt_indent(&fields.join(",\n")))
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
                        format!("{} {{\n{}\n}}", v.name, fmt_indent(&fields.join(",\n")))
                    }
                })
                .collect();
            format!("enum {} {{\n{}\n}}", e.name, fmt_indent(&variants.join(",\n")))
        }
    }
}

fn render_invariant(i: &Invariant) -> String {
    let body = i
        .constraints
        .iter()
        .map(pretty_expr)
        .collect::<Vec<_>>()
        .join(";\n");
    format!("invariant {} {{\n{}\n}}", i.name, fmt_indent(&body))
}

fn render_func(f: &Func) -> String {
    let attrs: Vec<String> = f
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
                        Arg::Flag(fl) => fl.clone(),
                        Arg::Kv(k, v) => format!("{}={}", k, render_literal(v)),
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("@{}({})", a.name, args)
            }
        })
        .collect();
    let params = f
        .params
        .iter()
        .map(|p| {
            let mut_prefix = if p.mutability == ParamMutability::Mutable {
                "mut "
            } else {
                ""
            };
            format!("{}{}: {}", mut_prefix, p.name, render_type(&p.ty))
        })
        .collect::<Vec<_>>()
        .join(", ");
    let ret = f
        .return_ty
        .as_ref()
        .map(|t| format!(" -> {}", render_type(t)))
        .unwrap_or_default();
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
    let mut out = String::new();
    if !attrs.is_empty() {
        out.push_str(&attrs.join("\n"));
        out.push('\n');
    }
    out.push_str(&format!("func {}({}){}", f.name, params, ret));
    if !clauses.is_empty() {
        out.push('\n');
        out.push_str(&fmt_indent(&clauses.join("\n")));
    }
    if f.elided {
        out.push(';');
    } else {
        out.push_str("\n{\n");
        out.push_str(&fmt_indent(&body));
        out.push_str("\n}");
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
            format!("mutate state {{\n{}\n}}", fmt_indent(&inner))
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
            out.push_str(&fmt_indent(&then.join("\n")));
            out.push_str("\n}");
            if let Some(else_body) = &is.else_body {
                let els: Vec<_> = else_body.iter().map(render_stmt).collect();
                out.push_str(" else {\n");
                out.push_str(&fmt_indent(&els.join("\n")));
                out.push_str("\n}");
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
                        fmt_indent(&body.join("\n"))
                    )
                })
                .collect();
            format!(
                "match {} {{\n{}\n}}",
                pretty_expr(&ms.scrutinee),
                fmt_indent(&arms.join("\n"))
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
        Type::Named(s) => s.clone(),
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

fn fmt_indent(s: &str) -> String {
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
