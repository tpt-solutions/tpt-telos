//! Extraction of verification problems from the AST.
//!
//! For each `func`, we build:
//!   * `premises`     -- requires, type constraints, entry invariants, frame
//!                       axioms, and `mutate state` assignments (which define
//!                       post-state variables).
//!   * `conclusions`  -- `ensures` clauses and invariants that must hold in the
//!                       post-state.
//!
//! State variables are named `"<base>.<field>"` in the pre-state and
//! `"<base>.<field>'"` in the post-state. `old(e)` always resolves to the
//! pre-state name.

use crate::ir::*;
use std::collections::{HashMap, HashSet};
use telos_parser::ast::*;

type Naming = dyn Fn(&str, &str) -> String;

fn pre_field(base: &str, field: &str) -> String {
    format!("{}.{}", base, field)
}

fn post_field(base: &str, field: &str) -> String {
    format!("{}.{}'", base, field)
}

/// Recursively replace bare `Var` identifiers with `Field { base, var }`.
/// Used to specialise a type-level invariant (e.g. `balance >= 0`) for a
/// particular binding (e.g. `from`).
fn instantiate(expr: &Expr, base: &str) -> Expr {
    match expr {
        Expr::Var(v) => Expr::Field {
            base: base.to_string(),
            field: v.clone(),
        },
        Expr::Field { .. } => expr.clone(),
        Expr::Int(_) | Expr::Old(_) => expr.clone(),
        Expr::Unary { op, expr } => Expr::Unary {
            op: *op,
            expr: Box::new(instantiate(expr, base)),
        },
        Expr::Bin { op, lhs, rhs } => Expr::Bin {
            op: *op,
            lhs: Box::new(instantiate(lhs, base)),
            rhs: Box::new(instantiate(rhs, base)),
        },
    }
}

fn linearize(
    expr: &Expr,
    field_fn: &Naming,
    var_fn: &dyn Fn(&str) -> String,
) -> Result<Linear, String> {
    match expr {
        Expr::Int(n) => Ok(Linear::constant_only(*n)),
        Expr::Var(name) => Ok(Linear::var(&var_fn(name))),
        Expr::Field { base, field } => Ok(Linear::var(&field_fn(base, field))),
        Expr::Old(e) => linearize(e, &|b, f| pre_field(b, f), var_fn),
        Expr::Unary { op, expr } => match op {
            UnOp::Neg => Ok(linearize(expr, field_fn, var_fn)?.neg()),
        },
        Expr::Bin { op, lhs, rhs } => {
            let l = linearize(lhs, field_fn, var_fn)?;
            let r = linearize(rhs, field_fn, var_fn)?;
            match op {
                BinOp::Add => Ok(l.add(&r)),
                BinOp::Sub => Ok(l.sub(&r)),
                BinOp::Mul => {
                    if let Some(k) = as_int(rhs) {
                        Ok(l.scale(k))
                    } else if let Some(k) = as_int(lhs) {
                        Ok(r.scale(k))
                    } else {
                        Err("non-linear multiplication is not supported in constraints".into())
                    }
                }
                BinOp::Div => {
                    if let Some(k) = as_int(rhs) {
                        if k == 0 {
                            Err("division by zero".into())
                        } else {
                            Ok(scale_exact(&l, k)?)
                        }
                    } else {
                        Err("division must be by a constant in constraints".into())
                    }
                }
                _ => Err("expected arithmetic expression".into()),
            }
        }
    }
}

fn as_int(expr: &Expr) -> Option<i64> {
    match expr {
        Expr::Int(n) => Some(*n),
        _ => None,
    }
}

fn scale_exact(l: &Linear, k: i64) -> Result<Linear, String> {
    if l.constant % k != 0 {
        return Err("non-integer division in constraint".into());
    }
    let mut terms = Vec::new();
    for (v, c) in &l.terms {
        if c % k != 0 {
            return Err("non-integer division in constraint".into());
        }
        terms.push((v.clone(), c / k));
    }
    Ok(Linear {
        terms,
        constant: l.constant / k,
    })
}

fn relation_of(op: BinOp) -> Option<Relation> {
    match op {
        BinOp::Eq => Some(Relation::Eq),
        BinOp::Ne => Some(Relation::Ne),
        BinOp::Lt => Some(Relation::Lt),
        BinOp::Le => Some(Relation::Le),
        BinOp::Gt => Some(Relation::Gt),
        BinOp::Ge => Some(Relation::Ge),
        _ => None,
    }
}

/// Lower a boolean expression into one or more linear constraints.
/// `&&` is flattened; `||` is rejected (handled at the conclusion level later
/// if ever needed).
fn to_constraints(
    expr: &Expr,
    field_fn: &Naming,
    var_fn: &dyn Fn(&str) -> String,
) -> Result<Vec<Constraint>, String> {
    match expr {
        Expr::Bin { op, lhs, rhs } if *op == BinOp::And => {
            let mut out = to_constraints(lhs, field_fn, var_fn)?;
            out.extend(to_constraints(rhs, field_fn, var_fn)?);
            Ok(out)
        }
        Expr::Bin { op, lhs, rhs } if relation_of(*op).is_some() => {
            let rel = relation_of(*op).unwrap();
            let l = linearize(lhs, field_fn, var_fn)?;
            let r = linearize(rhs, field_fn, var_fn)?;
            let diff = l.sub(&r);
            Ok(vec![Constraint(diff, rel)])
        }
        _ => Err("expected a boolean constraint (comparison or `&&`)".into()),
    }
}

fn assign_constraint(
    a: &Assign,
    field_fn: &Naming,
    var_fn: &dyn Fn(&str) -> String,
) -> Result<Constraint, String> {
    let (base, field) = match &a.target {
        Expr::Field { base, field } => (base.clone(), field.clone()),
        other => {
            return Err(format!(
                "assignment target must be a field, found {:?}",
                other
            ))
        }
    };
    let post = Linear::var(&post_field(&base, &field));
    let pre = Linear::var(&pre_field(&base, &field));
    let value = linearize(&a.value, field_fn, var_fn)?;
    let (terms, rel) = match a.op {
        AssignOp::Set => (post.sub(&value), Relation::Eq),
        AssignOp::Add => (post.sub(&pre).sub(&value), Relation::Eq),
        AssignOp::Sub => (post.sub(&pre).add(&value), Relation::Eq),
    };
    Ok(Constraint(terms, rel))
}

/// Collect every `Field { base, field }` referenced in an expression.
fn collect_fields(expr: &Expr, out: &mut HashSet<(String, String)>) {
    match expr {
        Expr::Field { base, field } => {
            out.insert((base.clone(), field.clone()));
        }
        Expr::Old(e) => collect_fields(e, out),
        Expr::Unary { expr, .. } => collect_fields(expr, out),
        Expr::Bin { lhs, rhs, .. } => {
            collect_fields(lhs, out);
            collect_fields(rhs, out);
        }
        _ => {}
    }
}

fn pretty(expr: &Expr) -> String {
    match expr {
        Expr::Int(n) => n.to_string(),
        Expr::Var(v) => v.clone(),
        Expr::Field { base, field } => format!("{}.{}", base, field),
        Expr::Old(e) => format!("old({})", pretty(e)),
        Expr::Unary { op, expr } => match op {
            UnOp::Neg => format!("-{}", pretty(expr)),
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
            format!("{} {} {}", pretty(lhs), s, pretty(rhs))
        }
    }
}

/// Extract all verification problems from a parsed program.
pub fn extract(modules: &[Module]) -> Result<Vec<VerificationProblem>, String> {
    let mut invariants: HashMap<String, &Invariant> = HashMap::new();
    // Fields declared by each invariant-bearing type, derived from the bare
    // variable names used in the invariant's constraints (e.g. `Wallet {
    // balance >= 0 }` => field `balance`).
    let mut invariant_fields: HashMap<String, HashSet<String>> = HashMap::new();
    for m in modules {
        for item in &m.items {
            if let Item::Invariant(inv) = item {
                invariants.insert(inv.name.clone(), inv);
                let entry = invariant_fields.entry(inv.name.clone()).or_default();
                for c in &inv.constraints {
                    collect_vars(c, entry);
                }
            }
        }
    }

    let mut problems = Vec::new();
    for m in modules {
        for item in &m.items {
            if let Item::Func(f) = item {
                problems.push(build_problem(f, &invariants, &invariant_fields)?);
            }
        }
    }
    Ok(problems)
}

/// Collect bare variable names referenced in an expression (used to discover an
/// invariant-bearing type's fields).
fn collect_vars(expr: &Expr, out: &mut HashSet<String>) {
    match expr {
        Expr::Var(v) => {
            out.insert(v.clone());
        }
        Expr::Old(e) => collect_vars(e, out),
        Expr::Unary { expr, .. } => collect_vars(expr, out),
        Expr::Bin { lhs, rhs, .. } => {
            collect_vars(lhs, out);
            collect_vars(rhs, out);
        }
        _ => {}
    }
}

fn build_problem(
    func: &Func,
    invariants: &HashMap<String, &Invariant>,
    invariant_fields: &HashMap<String, HashSet<String>>,
) -> Result<VerificationProblem, String> {
    let var_fn = |name: &str| name.to_string();
    let pre_fn = |b: &str, f: &str| pre_field(b, f);
    let post_fn = |b: &str, f: &str| post_field(b, f);

    let mut premises: Vec<Constraint> = Vec::new();

    // requires
    for r in &func.requires {
        premises.extend(to_constraints(r, &pre_fn, &var_fn)?);
    }

    // type constraints (e.g. PositiveInt => x >= 1)
    for p in &func.params {
        if p.ty.name().starts_with("Positive") {
            let ge = Linear::var(&p.name).sub(&Linear::constant_only(1));
            premises.push(Constraint(ge, Relation::Ge));
        }
    }

    // entry invariants for parameters whose type has an invariant
    for p in &func.params {
        if let Some(inv) = invariants.get(p.ty.name()) {
            for c in &inv.constraints {
                let inst = instantiate(c, &p.name);
                premises.extend(to_constraints(&inst, &pre_fn, &var_fn)?);
            }
        }
    }

    // mutate state assignments -> equality premises defining post vars
    let mut assigned: HashSet<(String, String)> = HashSet::new();
    for stmt in &func.body {
        let assigns = match stmt {
            Stmt::MutateState(a) => a,
            Stmt::Assign(a) => {
                premises.push(assign_constraint(a, &pre_fn, &var_fn)?);
                std::slice::from_ref(a)
            }
        };
        for a in assigns {
            premises.push(assign_constraint(a, &pre_fn, &var_fn)?);
            if let Expr::Field { base, field } = &a.target {
                assigned.insert((base.clone(), field.clone()));
            }
        }
    }

    // frame axioms: any referenced field not explicitly assigned keeps its value
    let mut referenced: HashSet<(String, String)> = HashSet::new();
    for r in &func.requires {
        collect_fields(r, &mut referenced);
    }
    for e in &func.ensures {
        collect_fields(e, &mut referenced);
    }
    for stmt in &func.body {
        let assigns = match stmt {
            Stmt::MutateState(a) => a,
            Stmt::Assign(a) => std::slice::from_ref(a),
        };
        for a in assigns {
            collect_fields(&a.value, &mut referenced);
            if let Expr::Field { base, field } = &a.target {
                referenced.insert((base.clone(), field.clone()));
            }
        }
    }

    // Fields of invariant-bearing parameters are also framed: a `&mut` parameter
    // of a type with an invariant keeps its invariant unless explicitly mutated,
    // even if the function body never reads or writes it.
    for p in &func.params {
        if let Some(fields) = invariant_fields.get(p.ty.name()) {
            for f in fields {
                referenced.insert((p.name.clone(), f.clone()));
            }
        }
    }

    for (base, field) in &referenced {
        if !assigned.contains(&(base.clone(), field.clone())) {
            let post = Linear::var(&post_field(base, field));
            let pre = Linear::var(&pre_field(base, field));
            premises.push(Constraint(post.sub(&pre), Relation::Eq));
        }
    }

    // conclusions: ensures + exit invariants
    let mut conclusions: Vec<Conclusion> = Vec::new();
    for e in &func.ensures {
        for c in to_constraints(e, &post_fn, &var_fn)? {
            conclusions.push(Conclusion {
                description: format!("ensures: {}", pretty(e)),
                constraint: c,
                is_ensures: true,
            });
        }
    }
    for p in &func.params {
        if let Some(inv) = invariants.get(p.ty.name()) {
            for c in &inv.constraints {
                let inst = instantiate(c, &p.name);
                for c2 in to_constraints(&inst, &post_fn, &var_fn)? {
                    conclusions.push(Conclusion {
                        description: format!(
                            "invariant {} maintained: {}",
                            inv.name,
                            pretty(&inst)
                        ),
                        constraint: c2,
                        is_ensures: false,
                    });
                }
            }
        }
    }

    Ok(VerificationProblem {
        func_name: func.name.clone(),
        premises,
        conclusions,
    })
}
