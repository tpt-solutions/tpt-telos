//! A self-contained decision procedure for the quantifier-free linear
//! arithmetic fragment (QF_LRA) over the reals, which is a sound
//! over-approximation of integer arithmetic: if a set of constraints is
//! *unsatisfiable* over the reals, it is also unsatisfiable over the integers.
//!
//! This gives **sound** verification -- the solver will never report a
//! constraint as provable when it is not. (It may occasionally fail to prove a
//! true integer fact, i.e. it is incomplete, which is acceptable for Phase 1.)
//!
//! Elimination uses Fourier-Motzkin variable elimination.

use telos_ir::{Constraint, Relation};
use std::collections::HashMap;

#[derive(Clone, Debug)]
struct LinIneq {
    /// coefficients; the inequality is  sum(coeff * var) <= c
    coeffs: HashMap<String, i128>,
    c: i128,
}

fn neg_terms(terms: &[(String, i64)]) -> HashMap<String, i128> {
    terms
        .iter()
        .map(|(v, c)| (v.clone(), -(*c as i128)))
        .collect()
}

/// Convert a constraint into one or two `<= 0` style inequalities.
fn to_inequalities(cs: &[Constraint]) -> Vec<LinIneq> {
    let mut out = Vec::new();
    for Constraint(lin, rel) in cs {
        let const_i128 = lin.constant as i128;
        let base: HashMap<String, i128> = lin
            .terms
            .iter()
            .map(|(v, c)| (v.clone(), *c as i128))
            .collect();
        // All forms are normalised to `sum(coeff * var) <= c`.
        // Given `sum(t_i * var) + K  (rel)  0`:
        //   Le : sum t_i var <= -K
        //   Lt : sum t_i var <= -K - 1
        //   Ge : sum(-t_i) var <= K
        //   Gt : sum(-t_i) var <= K - 1
        //   Eq : (sum t_i var <= -K) and (sum(-t_i) var <= K)
        match rel {
            Relation::Le => out.push(LinIneq {
                coeffs: base,
                c: -const_i128,
            }),
            Relation::Lt => out.push(LinIneq {
                coeffs: base,
                c: -const_i128 - 1,
            }),
            Relation::Ge => out.push(LinIneq {
                coeffs: neg_terms(&lin.terms),
                c: const_i128,
            }),
            Relation::Gt => out.push(LinIneq {
                coeffs: neg_terms(&lin.terms),
                c: const_i128 - 1,
            }),
            Relation::Eq => {
                out.push(LinIneq {
                    coeffs: base.clone(),
                    c: -const_i128,
                });
                out.push(LinIneq {
                    coeffs: neg_terms(&lin.terms),
                    c: const_i128,
                });
            }
            // `!=` is only produced at the conclusion level; ignore it as a premise.
            Relation::Ne => {}
        }
    }
    out
}

fn remove_var(coeffs: &HashMap<String, i128>, v: &str) -> HashMap<String, i128> {
    coeffs
        .iter()
        .filter(|(k, _)| k.as_str() != v)
        .map(|(k, c)| (k.clone(), *c))
        .collect()
}

fn merge_keys(a: &HashMap<String, i128>, b: &HashMap<String, i128>) -> Vec<String> {
    let mut keys: Vec<String> = a.keys().cloned().collect();
    for k in b.keys() {
        if !keys.contains(k) {
            keys.push(k.clone());
        }
    }
    keys
}

/// Returns true iff the constraint set is unsatisfiable (over the reals).
pub fn unsat(cs: &[Constraint]) -> bool {
    let mut ineqs = to_inequalities(cs);

    // collect variable names
    let mut vars: Vec<String> = Vec::new();
    for ineq in &ineqs {
        for k in ineq.coeffs.keys() {
            if !vars.contains(k) {
                vars.push(k.clone());
            }
        }
    }

    for v in &vars {
        let mut uppers: Vec<(i128, i128, HashMap<String, i128>)> = Vec::new();
        let mut lowers: Vec<(i128, i128, HashMap<String, i128>)> = Vec::new();
        let mut rest: Vec<LinIneq> = Vec::new();

        for ineq in &ineqs {
            match ineq.coeffs.get(v) {
                None => rest.push(ineq.clone()),
                Some(&tv) if tv > 0 => {
                    uppers.push((tv, ineq.c, remove_var(&ineq.coeffs, v)));
                }
                Some(&tv) if tv < 0 => {
                    lowers.push((-tv, -ineq.c, {
                        let mut m = remove_var(&ineq.coeffs, v);
                        for c in m.values_mut() {
                            *c = -*c;
                        }
                        m
                    }));
                }
                Some(_) => rest.push(ineq.clone()),
            }
        }

        let mut new_ineqs = rest;
        for (a, b, uc) in &uppers {
            for (e, d, lc) in &lowers {
                let keys = merge_keys(uc, lc);
                let mut coeffs = HashMap::new();
                for k in keys {
                    let bi = *uc.get(&k).unwrap_or(&0);
                    let ei = *lc.get(&k).unwrap_or(&0);
                    let coeff = e * bi - a * ei;
                    if coeff != 0 {
                        coeffs.insert(k, coeff);
                    }
                }
                let c = e * b - a * d;
                new_ineqs.push(LinIneq { coeffs, c });
            }
        }
        ineqs = new_ineqs;
    }

    for ineq in &ineqs {
        if ineq.coeffs.is_empty() && ineq.c < 0 {
            return true;
        }
    }
    false
}

/// Negate a conclusion into one or more branches (each a conjunction of
/// constraints) that, conjoined with the premises, must each be unsatisfiable
/// for the conclusion to be entailed.
fn negate(concl: &Constraint) -> Vec<Vec<Constraint>> {
    let Constraint(lin, rel) = concl;
    let branch = |r: Relation| vec![Constraint(lin.clone(), r)];
    match rel {
        Relation::Eq => vec![branch(Relation::Lt), branch(Relation::Gt)],
        Relation::Ne => vec![branch(Relation::Eq)],
        Relation::Le => vec![branch(Relation::Gt)],
        Relation::Lt => vec![branch(Relation::Ge)],
        Relation::Ge => vec![branch(Relation::Lt)],
        Relation::Gt => vec![branch(Relation::Le)],
    }
}

/// Does `premises` entail `conclusion`?
pub fn entails(premises: &[Constraint], concl: &Constraint) -> bool {
    for branch in negate(concl) {
        let mut combined: Vec<Constraint> = premises.to_vec();
        combined.extend(branch);
        if !unsat(&combined) {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use telos_ir::Linear;

    fn c(terms: &[(&str, i64)], k: i64, rel: Relation) -> Constraint {
        Constraint(
            Linear {
                terms: terms.iter().map(|(v, c)| (v.to_string(), *c)).collect(),
                constant: k,
            },
            rel,
        )
    }

    #[test]
    fn unsat_obvious() {
        // x <= 0 && x >= 1
        let cs = vec![c(&[("x", 1)], 0, Relation::Le), c(&[("x", 1)], -1, Relation::Ge)];
        assert!(unsat(&cs));
    }

    #[test]
    fn sat_consistent_bounds() {
        // 0 <= x <= 5
        let cs = vec![c(&[("x", 1)], 0, Relation::Ge), c(&[("x", 1)], -5, Relation::Le)];
        assert!(!unsat(&cs));
    }

    #[test]
    fn entails_simple() {
        // premises: x >= 1  =>  x >= 0
        let pre = vec![c(&[("x", 1)], -1, Relation::Ge)];
        let concl = c(&[("x", 1)], 0, Relation::Ge);
        assert!(entails(&pre, &concl));
    }

    #[test]
    fn entails_negative() {
        // premises: x >= 1  does NOT entail x >= 2
        let pre = vec![c(&[("x", 1)], -1, Relation::Ge)];
        let concl = c(&[("x", 1)], -2, Relation::Ge);
        assert!(!entails(&pre, &concl));
    }

    #[test]
    fn entails_affine_postcondition() {
        // y == x + 1, x >= 0  =>  y >= 1
        let pre = vec![
            c(&[("y", 1), ("x", -1)], -1, Relation::Eq),
            c(&[("x", 1)], 0, Relation::Ge),
        ];
        let concl = c(&[("y", 1)], -1, Relation::Ge);
        assert!(entails(&pre, &concl));
    }
}
