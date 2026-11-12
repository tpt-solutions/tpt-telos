//! Z3-backed solver for tpt-telos constraints.
//!
//! This module is only available when the `z3` feature is enabled. It provides
//! an alternative solver backend that uses the Z3 SMT solver for exact
//! nonlinear arithmetic verification.
//!
//! Falls back to the built-in Fourier-Motzkin solver if Z3 is unavailable
//! at runtime.

use tpt_telos_ir::{Constraint, Linear, Relation};
use z3::ast::{Ast, Int};
use z3::{Config, Context, SatResult, Solver};

/// Check if Z3 is available at runtime by attempting to create a context.
pub fn is_z3_available() -> bool {
    // z3 0.12: Context::new is infallible once Z3 is linked; constructing one is the probe.
    let _ctx = Context::new(&Config::new());
    true
}

/// Convert a linear expression, including its monomials, into a Z3 integer AST.
fn linear_to_z3<'a>(ctx: &'a Context, lin: &Linear) -> Int<'a> {
    let mut expr = Int::from_i64(ctx, lin.constant);
    for (var, coeff) in &lin.terms {
        let var_ast = Int::new_const(ctx, var.as_str());
        expr = &expr + &(&var_ast * &Int::from_i64(ctx, *coeff));
    }
    for (a, b, k) in &lin.products {
        let prod = &linear_to_z3(ctx, a) * &linear_to_z3(ctx, b);
        expr = &expr + &(&prod * &Int::from_i64(ctx, *k));
    }
    expr
}

/// Convert a tpt-telos constraint into a Z3 integer AST.
fn constraint_to_z3<'a>(ctx: &'a Context, c: &Constraint) -> Int<'a> {
    let Constraint(lin, _rel) = c;
    linear_to_z3(ctx, lin)
}

/// Collect every variable name in a linear expression, including inside monomials.
fn collect_linear_vars(lin: &Linear, out: &mut Vec<String>) {
    for (v, _) in &lin.terms {
        if !out.contains(v) {
            out.push(v.clone());
        }
    }
    for (a, b, _) in &lin.products {
        collect_linear_vars(a, out);
        collect_linear_vars(b, out);
    }
}

/// Check if a set of constraints is unsatisfiable using Z3.
pub fn z3_unsat(cs: &[Constraint]) -> bool {
    let cfg = Config::new();
    let ctx = Context::new(&cfg);
    let solver = Solver::new(&ctx);

    for c in cs {
        let ast = constraint_to_z3(&ctx, c);
        let zero = Int::from_i64(&ctx, 0);
        match c.1 {
            Relation::Le => solver.assert(&ast.le(&zero)),
            Relation::Lt => solver.assert(&ast.lt(&zero)),
            Relation::Ge => solver.assert(&ast.ge(&zero)),
            Relation::Gt => solver.assert(&ast.gt(&zero)),
            Relation::Eq => solver.assert(&ast._eq(&zero)),
            Relation::Ne => solver.assert(&ast._eq(&zero).not()),
        }
    }

    solver.check() == SatResult::Unsat
}

/// Find a model satisfying the constraints using Z3.
pub fn z3_model(cs: &[Constraint]) -> Option<std::collections::HashMap<String, i64>> {
    let cfg = Config::new();
    let ctx = Context::new(&cfg);
    let solver = Solver::new(&ctx);

    // Collect variable names.
    let mut var_names: Vec<String> = Vec::new();
    for c in cs {
        collect_linear_vars(&c.0, &mut var_names);
    }

    for c in cs {
        let ast = constraint_to_z3(&ctx, c);
        let zero = Int::from_i64(&ctx, 0);
        match c.1 {
            Relation::Le => solver.assert(&ast.le(&zero)),
            Relation::Lt => solver.assert(&ast.lt(&zero)),
            Relation::Ge => solver.assert(&ast.ge(&zero)),
            Relation::Gt => solver.assert(&ast.gt(&zero)),
            Relation::Eq => solver.assert(&ast._eq(&zero)),
            Relation::Ne => solver.assert(&ast._eq(&zero).not()),
        }
    }

    if solver.check() != SatResult::Sat {
        return None;
    }

    let model = solver.get_model()?;
    let mut result = std::collections::HashMap::new();
    for v in &var_names {
        let var_ast = Int::new_const(&ctx, v.as_str());
        if let Some(val) = model.eval(&var_ast, true) {
            if let Some(i) = val.as_i64() {
                result.insert(v.clone(), i);
            }
        }
    }
    Some(result)
}

/// Does `premises` entail `conclusion` using Z3?
pub fn z3_entails(premises: &[Constraint], concl: &Constraint) -> bool {
    use crate::solver::negate;
    for branch in negate(concl) {
        let mut combined: Vec<Constraint> = premises.to_vec();
        combined.extend(branch);
        if !z3_unsat(&combined) {
            return false;
        }
    }
    true
}
