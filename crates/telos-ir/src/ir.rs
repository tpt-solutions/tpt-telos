//! Intermediate representation for verification.
//!
//! All constraints are lowered into *linear* form: a sum of
//! `coefficient * variable` plus a constant, compared against zero.
//! This is the fragment handled by the QF_LRA solver in `telos-verifier`.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Relation {
    Le, // <= 0
    Lt, // <  0
    Ge, // >= 0
    Gt, // >  0
    Eq, // == 0
    Ne, // != 0
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Linear {
    /// `(variable, coefficient)` pairs.
    pub terms: Vec<(String, i64)>,
    pub constant: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Constraint(pub Linear, pub Relation);

impl Linear {
    pub fn var(name: &str) -> Linear {
        Linear {
            terms: vec![(name.to_string(), 1)],
            constant: 0,
        }
    }

    pub fn constant_only(c: i64) -> Linear {
        Linear {
            terms: vec![],
            constant: c,
        }
    }

    pub fn add(&self, other: &Linear) -> Linear {
        let mut terms = self.terms.clone();
        for (v, c) in &other.terms {
            if let Some(existing) = terms.iter_mut().find(|(vv, _)| vv == v) {
                existing.1 += *c;
            } else {
                terms.push((v.clone(), *c));
            }
        }
        terms.retain(|(_, c)| *c != 0);
        Linear {
            terms,
            constant: self.constant + other.constant,
        }
    }

    pub fn sub(&self, other: &Linear) -> Linear {
        let neg = Linear {
            terms: other.terms.iter().map(|(v, c)| (v.clone(), -*c)).collect(),
            constant: -other.constant,
        };
        self.add(&neg)
    }

    pub fn neg(&self) -> Linear {
        Linear {
            terms: self.terms.iter().map(|(v, c)| (v.clone(), -*c)).collect(),
            constant: -self.constant,
        }
    }

    pub fn scale(&self, k: i64) -> Linear {
        Linear {
            terms: self.terms.iter().map(|(v, c)| (v.clone(), c * k)).collect(),
            constant: self.constant * k,
        }
    }
}

/// A single function's verification problem.
#[derive(Debug, Clone)]
pub struct VerificationProblem {
    pub func_name: String,
    /// Facts assumed/known: pre-conditions, type constraints, entry invariants,
    /// and `mutate state` assignments (which define post-state variables).
    pub premises: Vec<Constraint>,
    /// Properties that must be proven from the premises.
    pub conclusions: Vec<Conclusion>,
}

#[derive(Debug, Clone)]
pub struct Conclusion {
    /// Human-readable description, e.g. the source clause.
    pub description: String,
    pub constraint: Constraint,
    /// True for post-condition `ensures`, false for a maintained invariant.
    pub is_ensures: bool,
}
