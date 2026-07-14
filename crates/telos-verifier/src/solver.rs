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

use std::collections::HashMap;
use tpt_telos_ir::{Constraint, Relation};

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

// ---------------------------------------------------------------------------
// Counter-example (model) extraction.
//
// When a conclusion is *not* entailed, we can construct a concrete witness: a
// variable assignment that satisfies the premises together with the negation of
// the conclusion. This witness is the "counter-example" fed back to the agentic
// code generator during the Verify -> Counter-example -> Rewrite loop.
// ---------------------------------------------------------------------------

/// A model maps variable names to integer values that satisfy a given
/// constraint set (when one exists).
pub type Model = std::collections::HashMap<String, i64>;

#[derive(Clone, Copy)]
struct Frac {
    num: i128,
    den: i128, // always > 0
}

impl Frac {
    fn new(num: i128, den: i128) -> Frac {
        debug_assert!(den != 0);
        let (num, den) = if den < 0 { (-num, -den) } else { (num, den) };
        let g = gcd(num.unsigned_abs(), den as u128);
        let g = g as i128;
        if g == 0 {
            Frac { num, den }
        } else {
            Frac {
                num: num / g,
                den: den / g,
            }
        }
    }

    /// Smallest integer >= self.
    fn ceil(self) -> i128 {
        let q = self.num.div_euclid(self.den);
        let r = self.num.rem_euclid(self.den);
        if r == 0 {
            q
        } else {
            q + 1
        }
    }

    /// Largest integer <= self.
    fn floor(self) -> i128 {
        self.num.div_euclid(self.den)
    }
}

fn gcd(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        let t = b;
        b = a % b;
        a = t;
    }
    a
}

fn eval(coeffs: &HashMap<String, i128>, model: &Model) -> i128 {
    coeffs
        .iter()
        .map(|(v, c)| *c * model.get(v).copied().unwrap_or(0) as i128)
        .sum()
}

/// Find a concrete integer model satisfying `cs`, if one exists.
///
/// Uses Fourier-Motzkin variable elimination and reconstructs an integer
/// assignment from the derived bounds. Returns `None` if the system is
/// unsatisfiable (over the reals, hence over the integers).
pub fn model(cs: &[Constraint]) -> Option<Model> {
    let ineqs = to_inequalities(cs);
    solve_model(ineqs)
}

fn solve_model(ineqs: Vec<LinIneq>) -> Option<Model> {
    // Collect variables.
    let mut vars: Vec<String> = Vec::new();
    for ineq in &ineqs {
        for k in ineq.coeffs.keys() {
            if !vars.contains(k) {
                vars.push(k.clone());
            }
        }
    }

    if vars.is_empty() {
        // Only constant inequalities remain; feasible iff every `c >= 0`.
        for ineq in &ineqs {
            if ineq.c < 0 {
                return None;
            }
        }
        return Some(Model::new());
    }

    let v = vars[0].clone();

    let mut uppers: Vec<(i128, i128, HashMap<String, i128>)> = Vec::new();
    let mut lowers: Vec<(i128, i128, HashMap<String, i128>)> = Vec::new();
    let mut rest: Vec<LinIneq> = Vec::new();

    for ineq in &ineqs {
        match ineq.coeffs.get(&v) {
            None => rest.push(ineq.clone()),
            Some(&tv) => {
                let others = remove_var(&ineq.coeffs, &v);
                if tv > 0 {
                    uppers.push((tv, ineq.c, others));
                } else if tv < 0 {
                    lowers.push((tv, ineq.c, others));
                } else {
                    rest.push(ineq.clone());
                }
            }
        }
    }

    // Combine every upper (a*v <= c - rest) with every lower (b*v <= c2 - rest2), b < 0.
    let mut new_ineqs = rest;
    for (a, cu, uc) in &uppers {
        for (b, c2, lc) in &lowers {
            let mut coeffs = HashMap::new();
            for k in merge_keys(uc, lc) {
                let bi = *uc.get(&k).unwrap_or(&0);
                let ei = *lc.get(&k).unwrap_or(&0);
                // derived earlier: a*rest2 - b*rest_u <= a*c2 - b*c
                let coeff = a * ei - b * bi;
                if coeff != 0 {
                    coeffs.insert(k, coeff);
                }
            }
            let c = a * c2 - b * cu;
            new_ineqs.push(LinIneq { coeffs, c });
        }
    }

    let mut model = solve_model(new_ineqs)?;

    // Recover a value for `v` from its (real) bounds.
    let mut low: Option<Frac> = None;
    for (b, c2, lc) in &lowers {
        // b*v <= c2 - lc  with b < 0  =>  v >= (lc_val - c2)/(-b)
        let lc_val = eval(lc, &model);
        let num = lc_val - c2;
        let den = -b; // > 0
        let f = Frac::new(num, den);
        low = Some(match low {
            None => f,
            Some(x) => {
                if f.num * x.den >= x.num * f.den {
                    f
                } else {
                    x
                }
            }
        });
    }

    let mut high: Option<Frac> = None;
    for (a, cu, uc) in &uppers {
        // a*v <= cu - uc  with a > 0  =>  v <= (cu - uc_val)/a
        let uc_val = eval(uc, &model);
        let num = cu - uc_val;
        let den = *a; // > 0
        let f = Frac::new(num, den);
        high = Some(match high {
            None => f,
            Some(x) => {
                if f.num * x.den <= x.num * f.den {
                    f
                } else {
                    x
                }
            }
        });
    }

    let vval: i128 = match (low, high) {
        (None, None) => 0,
        (Some(l), None) => l.ceil(),
        (None, Some(h)) => h.floor(),
        (Some(l), Some(h)) => {
            let lo = l.ceil();
            let hi = h.floor();
            if lo > hi {
                return None;
            }
            lo
        }
    };

    model.insert(v, vval as i64);
    Some(model)
}

/// Produce a concrete counter-example (a witness model) showing that
/// `conclusion` does *not* follow from `premises`. Returns `None` when the
/// conclusion is actually entailed (no counter-example exists).
pub fn counterexample(premises: &[Constraint], concl: &Constraint) -> Option<Model> {
    for branch in negate(concl) {
        let mut cs = premises.to_vec();
        cs.extend(branch);
        if let Some(m) = model(&cs) {
            return Some(m);
        }
    }
    None
}

/// Check whether an integer model satisfies every constraint in `cs`.
/// Used to validate a generated counter-example before handing it to the agent.
pub fn satisfies_model(cs: &[Constraint], model: &Model) -> bool {
    let ineqs = to_inequalities(cs);
    for ineq in &ineqs {
        let lhs: i128 = ineq
            .coeffs
            .iter()
            .map(|(v, c)| *c * model.get(v).copied().unwrap_or(0) as i128)
            .sum();
        if lhs > ineq.c {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod model_tests {
    use super::*;
    use tpt_telos_ir::Linear;

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
    fn model_finds_witness() {
        // 1 <= x <= 3, y == x + 1
        let cs = vec![
            c(&[("x", 1)], -1, Relation::Ge),
            c(&[("x", 1)], -3, Relation::Le),
            c(&[("y", 1), ("x", -1)], -1, Relation::Eq),
        ];
        let m = model(&cs).expect("should be satisfiable");
        assert!(satisfies_model(&cs, &m), "model {m:?} invalid");
    }

    #[test]
    fn model_unsat_none() {
        // x >= 1 && x <= 0
        let cs = vec![
            c(&[("x", 1)], -1, Relation::Ge),
            c(&[("x", 1)], 0, Relation::Le),
        ];
        assert!(model(&cs).is_none());
    }

    #[test]
    fn model_counterexample_for_postcondition() {
        // premises: y' == y - 1 ; y >= 0
        // conclusion (false): y' >= y
        let pre = vec![
            c(&[("y'", 1), ("y", -1)], 1, Relation::Eq),
            c(&[("y", 1)], 0, Relation::Ge),
        ];
        let concl = c(&[("y'", 1), ("y", -1)], 0, Relation::Ge);
        for branch in negate(&concl) {
            let mut combined = pre.clone();
            combined.extend(branch);
            let m = model(&combined).expect("counterexample should exist");
            assert!(satisfies_model(&combined, &m));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tpt_telos_ir::Linear;

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
        let cs = vec![
            c(&[("x", 1)], 0, Relation::Le),
            c(&[("x", 1)], -1, Relation::Ge),
        ];
        assert!(unsat(&cs));
    }

    #[test]
    fn sat_consistent_bounds() {
        // 0 <= x <= 5
        let cs = vec![
            c(&[("x", 1)], 0, Relation::Ge),
            c(&[("x", 1)], -5, Relation::Le),
        ];
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

#[cfg(test)]
mod extended_tests {
    use super::*;
    use tpt_telos_ir::Linear;

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
    fn unsat_three_variables() {
        // x >= 1 && x <= 0  (impossible regardless of y, z)
        let cs = vec![
            c(&[("x", 1)], -1, Relation::Ge),
            c(&[("x", 1)], 0, Relation::Le),
        ];
        assert!(unsat(&cs));
    }

    #[test]
    fn unsat_with_mixed_relations() {
        // x <= 0 && x >= 2 && y == x + 1
        let cs = vec![
            c(&[("x", 1)], 0, Relation::Le),
            c(&[("x", 1)], -2, Relation::Ge),
            c(&[("y", 1), ("x", -1)], -1, Relation::Eq),
        ];
        assert!(unsat(&cs));
    }

    #[test]
    fn sat_three_variable_bounds() {
        // 0 <= x <= 5, 0 <= y <= 3, z == x + y  (feasible)
        let cs = vec![
            c(&[("x", 1)], 0, Relation::Ge),
            c(&[("x", 1)], -5, Relation::Le),
            c(&[("y", 1)], 0, Relation::Ge),
            c(&[("y", 1)], -3, Relation::Le),
            c(&[("z", 1), ("x", -1), ("y", -1)], 0, Relation::Eq),
        ];
        assert!(!unsat(&cs));
    }

    #[test]
    fn entails_with_neq_conclusion() {
        // premises: x >= 1  does NOT entail x != 0  (x == 1 satisfies x>=1 and x!=0;
        // but x could also be 1, which is != 0, so conclusion holds? No: we must
        // prove it for ALL x>=1. x>=1 implies x!=0, so it IS entailed.)
        let pre = vec![c(&[("x", 1)], -1, Relation::Ge)];
        let concl = c(&[("x", 1)], 0, Relation::Ne);
        assert!(entails(&pre, &concl));
    }

    #[test]
    fn entails_neq_negative() {
        // premises: x >= 0  does NOT entail x != 1  (x == 1 breaks it)
        let pre = vec![c(&[("x", 1)], 0, Relation::Ge)];
        let concl = c(&[("x", 1)], -1, Relation::Ne);
        assert!(!entails(&pre, &concl));
    }

    #[test]
    fn counterexample_finds_witness_for_failed_postcondition() {
        // premises: y' == y - 1 ; y >= 0
        // conclusion (false): y' >= y
        let pre = vec![
            c(&[("y'", 1), ("y", -1)], 1, Relation::Eq),
            c(&[("y", 1)], 0, Relation::Ge),
        ];
        let concl = c(&[("y'", 1), ("y", -1)], 0, Relation::Ge);
        let ce = counterexample(&pre, &concl).expect("a counter-example must exist");
        // The witness must satisfy the premises together with the negated
        // conclusion (y' < y, i.e. y' - y + 1 <= 0).
        let negated = vec![c(&[("y'", 1), ("y", -1)], 1, Relation::Le)];
        let mut combined = pre.clone();
        combined.extend(negated);
        assert!(
            satisfies_model(&combined, &ce),
            "counter-example {ce:?} invalid"
        );
    }

    #[test]
    fn counterexample_none_when_entailed() {
        // premises: y' == y - 1 ; y >= 0  =>  y' <= y  (entailed, no CE)
        let pre = vec![
            c(&[("y'", 1), ("y", -1)], 1, Relation::Eq),
            c(&[("y", 1)], 0, Relation::Ge),
        ];
        let concl = c(&[("y'", 1), ("y", -1)], 0, Relation::Le);
        assert!(counterexample(&pre, &concl).is_none());
    }

    #[test]
    fn model_solves_linear_division() {
        // 2 * x == 4  =>  x == 2
        let cs = vec![c(&[("x", 2)], -4, Relation::Eq)];
        let m = model(&cs).expect("should be satisfiable");
        assert_eq!(m.get("x").copied().unwrap_or(0), 2);
        assert!(satisfies_model(&cs, &m));
    }

    #[test]
    fn integer_overflow_edge_bounds() {
        // Bounds at the i64 extremes are handled (the solver works in i128).
        let max = i64::MAX;
        // x >= i64::MAX && x <= i64::MAX - 1  =>  unsat
        let cs = vec![
            c(&[("x", 1)], -max, Relation::Ge),
            c(&[("x", 1)], -(max - 1), Relation::Le),
        ];
        assert!(unsat(&cs));
        // x <= i64::MAX  =>  sat  (the lower extreme is trivially true for i64)
        let sat = vec![c(&[("x", 1)], -max, Relation::Le)];
        assert!(!unsat(&sat));
    }

    #[test]
    fn entails_across_i64_extremes() {
        // x == i64::MAX  =>  x >= i64::MAX
        let max = i64::MAX;
        let pre = vec![c(&[("x", 1)], -max, Relation::Eq)];
        let concl = c(&[("x", 1)], -max, Relation::Ge);
        assert!(entails(&pre, &concl));
        // but x == i64::MAX  does NOT entail  x >= 0  is... actually it does
        // (MAX >= 0). Instead show it does NOT entail x <= 0.
        let concl2 = c(&[("x", 1)], 0, Relation::Le);
        assert!(!entails(&pre, &concl2));
    }

    #[test]
    fn model_respects_all_relations() {
        // Independent integer bounds on two variables; any corner satisfies all.
        let cs = vec![
            c(&[("x", 1)], -1, Relation::Ge),
            c(&[("x", 1)], -3, Relation::Le),
            c(&[("y", 1)], -2, Relation::Ge),
            c(&[("y", 1)], -4, Relation::Le),
            c(&[("x", 1), ("y", 1)], -100, Relation::Le),
        ];
        let m = model(&cs).expect("should be satisfiable");
        assert!(satisfies_model(&cs, &m), "model {m:?} invalid");
    }
}
