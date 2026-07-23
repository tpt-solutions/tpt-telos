//! Extraction of verification problems from the AST.
//!
//! For each `func`, we build:
//! * `premises` -- requires, type constraints, entry invariants, frame
//!   axioms, and `mutate state` assignments (which define post-state
//!   variables).
//! * `conclusions` -- `ensures` clauses and invariants that must hold in the
//!   post-state.
//!
//! State variables are named `"<base>.<field>"` in the pre-state and
//! `"<base>.<field>'"` in the post-state. `old(e)` always resolves to the
//! pre-state name.

use crate::ir::*;
use std::collections::{HashMap, HashSet};
use tpt_telos_parser::ast::*;

/// Per-variable lower and upper bounds derived from premise constraints.
/// `None` means the bound is unknown.
type VarBounds = HashMap<String, (Option<i64>, Option<i64>)>;

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
        // New expression kinds are cloned as-is (they are not lowered to IR
        // constraints; they appear only in codegen or runtime paths).
        other => other.clone(),
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
        // New expression kinds (Call, If, Match, etc.) cannot be lowered to
        // linear arithmetic. They are handled at the codegen level only.
        other => Err(format!(
            "expression `{}` cannot be lowered to linear arithmetic",
            pretty(other)
        )),
    }
}

/// Scan premise constraints to extract per-variable lower and upper bounds.
///
/// Only single-variable constraints with coefficient ±1 are processed; multi-
/// variable constraints are ignored because they don't isolate a single bound.
fn collect_var_bounds(premises: &[Constraint]) -> VarBounds {
    let mut bounds: VarBounds = HashMap::new();
    for Constraint(lin, rel) in premises {
        if lin.terms.len() != 1 {
            continue;
        }
        let (var, coeff) = &lin.terms[0];
        let c = lin.constant;
        // The constraint is `coeff * var + c  rel  0`.
        let (lo_delta, hi_delta): (Option<i64>, Option<i64>) = match (*coeff, rel) {
            // var + c >= 0  →  var >= -c
            (1, Relation::Ge) => (Some(-c), None),
            // var + c > 0   →  var > -c  →  var >= -c + 1 (integers)
            (1, Relation::Gt) => (Some(-c + 1), None),
            // var + c <= 0  →  var <= -c
            (1, Relation::Le) => (None, Some(-c)),
            // var + c < 0   →  var < -c  →  var <= -c - 1
            (1, Relation::Lt) => (None, Some(-c - 1)),
            // -var + c >= 0  →  var <= c
            (-1, Relation::Ge) => (None, Some(c)),
            // -var + c > 0   →  var < c  →  var <= c - 1
            (-1, Relation::Gt) => (None, Some(c - 1)),
            // -var + c <= 0  →  var >= c
            (-1, Relation::Le) => (Some(c), None),
            // -var + c < 0   →  var > c  →  var >= c + 1
            (-1, Relation::Lt) => (Some(c + 1), None),
            // var + c == 0  →  var == -c  (both lo and hi)
            (1, Relation::Eq) => (Some(-c), Some(-c)),
            _ => continue,
        };
        let entry = bounds.entry(var.clone()).or_insert((None, None));
        if let Some(lo) = lo_delta {
            entry.0 = Some(entry.0.map_or(lo, |e: i64| e.max(lo)));
        }
        if let Some(hi) = hi_delta {
            entry.1 = Some(entry.1.map_or(hi, |e: i64| e.min(hi)));
        }
    }
    bounds
}

/// Look up bounds for `var`, falling back to the pre-state version when
/// `var` is a post-state name (ends with `'`). Frame axioms make pre-state
/// bounds valid for unmodified post-state variables.
fn lookup_bounds<'a>(bounds: &'a VarBounds, var: &str) -> Option<&'a (Option<i64>, Option<i64>)> {
    bounds.get(var).or_else(|| {
        if let Some(pre) = var.strip_suffix('\'') {
            bounds.get(pre)
        } else {
            None
        }
    })
}

/// Return the resolved variable name for a simple variable expression, or
/// `None` for complex sub-expressions we cannot name.
fn expr_var_name(
    expr: &Expr,
    field_fn: &Naming,
    var_fn: &dyn Fn(&str) -> String,
) -> Option<String> {
    match expr {
        Expr::Var(name) => Some(var_fn(name)),
        Expr::Field { base, field } => Some(field_fn(base, field)),
        _ => None,
    }
}

/// Like [`linearize`] but attempts interval-arithmetic bounding for nonlinear
/// products when both operands have known bounds in `bounds`. Sets
/// `*approximated = true` when a product was replaced by its interval bound.
fn linearize_bounded(
    expr: &Expr,
    field_fn: &Naming,
    var_fn: &dyn Fn(&str) -> String,
    bounds: &VarBounds,
    approximated: &mut bool,
) -> Result<Linear, String> {
    match expr {
        Expr::Int(n) => Ok(Linear::constant_only(*n)),
        Expr::Var(name) => Ok(Linear::var(&var_fn(name))),
        Expr::Field { base, field } => Ok(Linear::var(&field_fn(base, field))),
        Expr::Old(e) => linearize_bounded(e, &|b, f| pre_field(b, f), var_fn, bounds, approximated),
        Expr::Unary { op, expr } => match op {
            UnOp::Neg => Ok(linearize_bounded(expr, field_fn, var_fn, bounds, approximated)?.neg()),
        },
        Expr::Bin { op, lhs, rhs } => {
            let l = linearize_bounded(lhs, field_fn, var_fn, bounds, approximated)?;
            let r = linearize_bounded(rhs, field_fn, var_fn, bounds, approximated)?;
            match op {
                BinOp::Add => Ok(l.add(&r)),
                BinOp::Sub => Ok(l.sub(&r)),
                BinOp::Mul => {
                    if let Some(k) = as_int(rhs) {
                        Ok(l.scale(k))
                    } else if let Some(k) = as_int(lhs) {
                        Ok(r.scale(k))
                    } else {
                        // Try interval bounding: replace a * b with a constant
                        // derived from the worst-case product of their bounds.
                        let a_name = expr_var_name(lhs, field_fn, var_fn);
                        let b_name = expr_var_name(rhs, field_fn, var_fn);
                        if let (Some(ref a), Some(ref b)) = (a_name, b_name) {
                            let ab = lookup_bounds(bounds, a);
                            let bb = lookup_bounds(bounds, b);
                            if let (
                                Some((Some(a_lo), Some(a_hi))),
                                Some((Some(b_lo), Some(b_hi))),
                            ) = (ab, bb)
                            {
                                let corners = [
                                    a_lo.saturating_mul(*b_lo),
                                    a_lo.saturating_mul(*b_hi),
                                    a_hi.saturating_mul(*b_lo),
                                    a_hi.saturating_mul(*b_hi),
                                ];
                                let product_bound = *corners.iter().max().unwrap();
                                *approximated = true;
                                return Ok(Linear::constant_only(product_bound));
                            }
                        }
                        Err(format!(
                            "non-linear multiplication ({} * {}) cannot be verified; \
                             add `requires` bounds on both variables to enable \
                             interval-bounding approximation",
                            pretty(lhs),
                            pretty(rhs)
                        ))
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
        // New expression kinds (Call, If, Match, etc.) cannot be lowered to
        // linear arithmetic. They are handled at the codegen level only.
        other => Err(format!(
            "expression `{}` cannot be lowered to linear arithmetic",
            pretty(other)
        )),
    }
}

/// Like [`to_constraints`] but uses [`linearize_bounded`] to handle nonlinear
/// products via interval-arithmetic bounding. Sets `*approximated = true` when
/// any product was replaced by an interval-bounded constant.
fn to_constraints_bounded(
    expr: &Expr,
    field_fn: &Naming,
    var_fn: &dyn Fn(&str) -> String,
    bounds: &VarBounds,
    approximated: &mut bool,
) -> Result<Vec<Constraint>, String> {
    match expr {
        Expr::Bin { op, lhs, rhs } if *op == BinOp::And => {
            let mut out = to_constraints_bounded(lhs, field_fn, var_fn, bounds, approximated)?;
            out.extend(to_constraints_bounded(
                rhs,
                field_fn,
                var_fn,
                bounds,
                approximated,
            )?);
            Ok(out)
        }
        Expr::Bin { op, lhs, rhs } if relation_of(*op).is_some() => {
            let rel = relation_of(*op).unwrap();
            let l = linearize_bounded(lhs, field_fn, var_fn, bounds, approximated)?;
            let r = linearize_bounded(rhs, field_fn, var_fn, bounds, approximated)?;
            let diff = l.sub(&r);
            Ok(vec![Constraint(diff, rel)])
        }
        _ => Err("expected a boolean constraint (comparison or `&&`)".into()),
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
        Expr::Call(c) => {
            let args: Vec<_> = c.args.iter().map(pretty).collect();
            format!("{}({})", c.func, args.join(", "))
        }
        Expr::MethodCall(m) => {
            let args: Vec<_> = m.args.iter().map(pretty).collect();
            format!("{}.{}({})", pretty(&m.receiver), m.method, args.join(", "))
        }
        Expr::Index(i) => format!("{}[{}]", pretty(&i.receiver), pretty(&i.index)),
        Expr::If(i) => format!(
            "if {} {{ {} }} else {{ {} }}",
            pretty(&i.condition),
            pretty(&i.then_expr),
            pretty(&i.else_expr)
        ),
        Expr::Match(m) => {
            let arms: Vec<_> = m
                .arms
                .iter()
                .map(|a| format!("... => {}", pretty(&a.expr)))
                .collect();
            format!("match {} {{ {} }}", pretty(&m.scrutinee), arms.join(", "))
        }
        Expr::Try(e) => format!("{}?", pretty(e)),
        Expr::Forall(f) => format!(
            "forall {}: {} {{ {} }}",
            f.var,
            f.var_ty.name(),
            pretty(&f.body)
        ),
        Expr::Aggregate(a) => {
            let args: Vec<_> = a.args.iter().map(pretty).collect();
            let op = match a.op {
                AggregateOp::Sum => "sum",
                AggregateOp::Min => "min",
                AggregateOp::Max => "max",
                AggregateOp::Count => "count",
            };
            format!("{}({})", op, args.join(", "))
        }
    }
}

/// Extract all verification problems from a parsed program.
///
/// # Examples
///
/// ```
/// use tpt_telos_parser::parse;
/// use tpt_telos_ir::extract;
///
/// let src = r#"
///     module Bank {
///         invariant Wallet { balance >= 0 }
///
///         func deposit(w: Wallet, amount: PositiveInt)
///             requires amount > 0
///             ensures w.balance == old(w.balance) + amount
///         { mutate state { w.balance += amount } }
///     }
/// "#;
///
/// let modules = parse(src).unwrap();
/// let problems = extract(&modules).unwrap();
///
/// assert_eq!(problems.len(), 1);
/// assert_eq!(problems[0].func_name, "deposit");
/// // Premises include the requires clause and the entry invariant.
/// assert!(!problems[0].premises.is_empty());
/// // Conclusions include the ensures clause and the exit invariant.
/// assert!(!problems[0].conclusions.is_empty());
/// ```
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
            // Let, If, Match, Return are not lowered to IR constraints; they
            // are handled at the codegen/runtime level only.
            _ => continue,
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
            _ => continue,
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

    // Derive per-variable bounds from the now-complete premise set.
    // These are used to linearize nonlinear products in conclusions via
    // interval-arithmetic bounding (see `linearize_bounded`).
    let premise_bounds = collect_var_bounds(&premises);

    // conclusions: ensures + exit invariants
    let mut conclusions: Vec<Conclusion> = Vec::new();
    for e in &func.ensures {
        let mut approximated = false;
        let cs = to_constraints_bounded(e, &post_fn, &var_fn, &premise_bounds, &mut approximated)?;
        for c in cs {
            conclusions.push(Conclusion {
                description: format!("ensures: {}", pretty(e)),
                constraint: c,
                is_ensures: true,
                is_approximation: approximated,
            });
        }
    }
    for p in &func.params {
        if let Some(inv) = invariants.get(p.ty.name()) {
            for c in &inv.constraints {
                let inst = instantiate(c, &p.name);
                let mut approximated = false;
                let cs = to_constraints_bounded(
                    &inst,
                    &post_fn,
                    &var_fn,
                    &premise_bounds,
                    &mut approximated,
                )?;
                for c2 in cs {
                    conclusions.push(Conclusion {
                        description: format!(
                            "invariant {} maintained: {}",
                            inv.name,
                            pretty(&inst)
                        ),
                        constraint: c2,
                        is_ensures: false,
                        is_approximation: approximated,
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
