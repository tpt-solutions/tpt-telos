//! A deterministic, fully offline [`CodeAgent`].
//!
//! - When the developer supplied a body, the first candidate *is* that body
//!   (the loop then verifies it; if it is wrong the agent repairs it).
//! - When the body is elided (`func ... ;`), the agent synthesizes an
//!   implementation directly from the `ensures` clauses.
//! - [`StaticAgent::rewrite`] performs a concrete counter-example-guided
//!   repair: it evaluates each `ensures` clause under the counter-example,
//!   finds the field whose post-state disagrees with the contract, and replaces
//!   just that assignment with the contract-derived one.

use std::collections::HashMap;

use tpt_telos_parser::ast::*;

use crate::{Candidate, CodeAgent, FuncSpec, Model};

/// The default, fully offline code-generation agent for tpt-telos.
///
/// When the developer supplies a function body, `StaticAgent` uses it as the
/// first candidate. When the body is elided (`func ... ;`), it synthesizes one
/// directly from the `ensures` clauses. Rewriting is guided by the
/// counter-example returned by the solver.
///
/// # Examples
///
/// ```
/// use tpt_telos_agent::{CodeAgent, StaticAgent};
///
/// let agent = StaticAgent::new();
/// assert_eq!(agent.name(), "static-synth");
/// ```
pub struct StaticAgent;

impl StaticAgent {
    pub fn new() -> Self {
        StaticAgent
    }
}

impl Default for StaticAgent {
    fn default() -> Self {
        Self::new()
    }
}

impl CodeAgent for StaticAgent {
    fn name(&self) -> &str {
        "static-synth"
    }

    fn generate(&self, spec: &FuncSpec) -> Result<Candidate, String> {
        if spec.func.elided || spec.func.body.is_empty() {
            Ok(synthesize_from_ensures(spec))
        } else {
            Ok(Candidate {
                stmts: spec.func.body.clone(),
            })
        }
    }

    fn rewrite(&self, spec: &FuncSpec, prev: &Candidate, ce: &Model) -> Result<Candidate, String> {
        if ce.is_empty() {
            // No concrete counter-example to guide the repair: fall back to a
            // full re-synthesis from the contract.
            return Ok(synthesize_from_ensures(spec));
        }

        // Find every post-state field whose value in the counter-example
        // disagrees with what the corresponding `ensures` clause demands.
        let mut fixes: HashMap<(String, String), Expr> = HashMap::new();
        for e in &spec.func.ensures {
            if let Expr::Bin {
                op: BinOp::Eq,
                lhs,
                rhs,
            } = e
            {
                if let Expr::Field { base, field } = &**lhs {
                    let expected = eval_post(rhs, ce);
                    let actual = ce.get(&post_name(base, field)).copied().unwrap_or(i64::MIN);
                    if expected != actual {
                        fixes.insert((base.clone(), field.clone()), resolve_old(rhs));
                    }
                }
            }
        }

        if fixes.is_empty() {
            // Counter-example did not isolate a single broken field; re-synthesize.
            return Ok(synthesize_from_ensures(spec));
        }

        // Replace only the offending assignments; keep the rest of the body.
        let mut new_stmts = prev.stmts.clone();
        apply_fixes(&mut new_stmts, &fixes);
        Ok(Candidate { stmts: new_stmts })
    }
}

fn post_name(base: &str, field: &str) -> String {
    format!("{}.{}'", base, field)
}

fn pre_name(base: &str, field: &str) -> String {
    format!("{}.{}", base, field)
}

/// Replace `old(e)` with `e` so the expression refers to the current state.
fn resolve_old(e: &Expr) -> Expr {
    match e {
        Expr::Old(inner) => resolve_old(inner),
        Expr::Unary { op, expr } => Expr::Unary {
            op: *op,
            expr: Box::new(resolve_old(expr)),
        },
        Expr::Bin { op, lhs, rhs } => Expr::Bin {
            op: *op,
            lhs: Box::new(resolve_old(lhs)),
            rhs: Box::new(resolve_old(rhs)),
        },
        other => other.clone(),
    }
}

/// Evaluate an expression in the *post-state* (ensures RHS) under a model.
/// Bare field accesses refer to post-state variables; `old(...)` refers to
/// pre-state variables.
fn eval_post(e: &Expr, ce: &Model) -> i64 {
    match e {
        Expr::Int(n) => *n,
        Expr::Var(v) => ce.get(v).copied().unwrap_or(0),
        Expr::Field { base, field } => ce.get(&post_name(base, field)).copied().unwrap_or(0),
        Expr::Old(inner) => eval_pre(inner, ce),
        Expr::Unary { op, expr } => match op {
            UnOp::Neg => -eval_post(expr, ce),
        },
        Expr::Bin { op, lhs, rhs } => {
            let l = eval_post(lhs, ce);
            let r = eval_post(rhs, ce);
            match op {
                BinOp::Add => l + r,
                BinOp::Sub => l - r,
                BinOp::Mul => l * r,
                BinOp::Div => {
                    if r == 0 {
                        0
                    } else {
                        l / r
                    }
                }
                _ => 0,
            }
        }
    }
}

/// Evaluate an expression in the *pre-state* under a model.
fn eval_pre(e: &Expr, ce: &Model) -> i64 {
    match e {
        Expr::Int(n) => *n,
        Expr::Var(v) => ce.get(v).copied().unwrap_or(0),
        Expr::Field { base, field } => ce.get(&pre_name(base, field)).copied().unwrap_or(0),
        Expr::Old(inner) => eval_pre(inner, ce),
        Expr::Unary { op, expr } => match op {
            UnOp::Neg => -eval_pre(expr, ce),
        },
        Expr::Bin { op, lhs, rhs } => {
            let l = eval_pre(lhs, ce);
            let r = eval_pre(rhs, ce);
            match op {
                BinOp::Add => l + r,
                BinOp::Sub => l - r,
                BinOp::Mul => l * r,
                BinOp::Div => {
                    if r == 0 {
                        0
                    } else {
                        l / r
                    }
                }
                _ => 0,
            }
        }
    }
}

fn apply_fixes(stmts: &mut [Stmt], fixes: &HashMap<(String, String), Expr>) {
    for stmt in stmts.iter_mut() {
        match stmt {
            Stmt::MutateState(assigns) => {
                for a in assigns.iter_mut() {
                    if let Expr::Field { base, field } = &a.target {
                        if let Some(value) = fixes.get(&(base.clone(), field.clone())) {
                            a.value = value.clone();
                            a.op = AssignOp::Set;
                        }
                    }
                }
            }
            Stmt::Assign(a) => {
                if let Expr::Field { base, field } = &a.target {
                    if let Some(value) = fixes.get(&(base.clone(), field.clone())) {
                        a.value = value.clone();
                        a.op = AssignOp::Set;
                    }
                }
            }
        }
    }
}

/// Derive an implementation from the `ensures` clauses. Each clause of the form
/// `lhs == rhs` where `lhs` is a field access becomes a `mutate state`
/// assignment; a clause of the form `var == rhs` (a scalar output) becomes a
/// local binding that is returned.
///
/// # Examples
///
/// ```
/// use tpt_telos_parser::parse;
/// use tpt_telos_agent::{FuncSpec, StaticAgent};
/// use tpt_telos_agent::synthesize_from_ensures;
///
/// let src = r#"
///     module M {
///         func transfer(w: Wallet, amount: PositiveInt)
///             ensures w.balance == old(w.balance) - amount
///         ;
///     }
/// "#;
///
/// let modules = parse(src).unwrap();
/// let m = &modules[0];
/// if let tpt_telos_parser::ast::Item::Func(func) = &m.items[0] {
///     let spec = FuncSpec::new(m.attributes.clone(), func.clone());
///     let candidate = synthesize_from_ensures(&spec);
///     // A body was derived from the ensures clause.
///     assert!(!candidate.stmts.is_empty());
/// }
/// ```
pub fn synthesize_from_ensures(spec: &FuncSpec) -> Candidate {
    let mut field_assigns: Vec<Assign> = Vec::new();
    let mut var_assigns: Vec<Assign> = Vec::new();

    for e in &spec.func.ensures {
        if let Expr::Bin {
            op: BinOp::Eq,
            lhs,
            rhs,
        } = e
        {
            let value = resolve_old(rhs);
            match &**lhs {
                Expr::Field { base, field } => {
                    field_assigns.push(Assign {
                        target: Expr::Field {
                            base: base.clone(),
                            field: field.clone(),
                        },
                        op: AssignOp::Set,
                        value,
                    });
                }
                Expr::Var(v) => {
                    var_assigns.push(Assign {
                        target: Expr::Var(v.clone()),
                        op: AssignOp::Set,
                        value,
                    });
                }
                _ => {}
            }
        }
    }

    let mut stmts = Vec::new();
    if !field_assigns.is_empty() {
        stmts.push(Stmt::MutateState(field_assigns));
    }
    for a in var_assigns {
        stmts.push(Stmt::Assign(a));
    }
    Candidate { stmts }
}
