//! Abstract Syntax Tree for tpt-telos.
//!
//! The grammar is intentionally "semantically erased": no implicit coercion,
//! every operation is named explicitly. See `grammar.ebnf` for the formal
//! specification.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type {
    /// A user or built-in named type, e.g. `Wallet`, `PositiveInt`.
    Named(String),
}

impl Type {
    pub fn name(&self) -> &str {
        match self {
            Type::Named(s) => s,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Literal {
    Int(i64),
    Ident(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Arg {
    /// `@boundary(cpu_bound)`
    Flag(String),
    /// `@state(replication_factor = 3)`
    Kv(String, Literal),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attribute {
    pub name: String,
    pub args: Vec<Arg>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Module {
    pub attributes: Vec<Attribute>,
    pub name: String,
    pub items: Vec<Item>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    Invariant(Invariant),
    Func(Func),
}

impl Item {
    pub fn func_name(&self) -> String {
        match self {
            Item::Func(f) => f.name.clone(),
            Item::Invariant(i) => i.name.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invariant {
    pub name: String,
    /// One or more boolean constraint expressions that must always hold.
    pub constraints: Vec<Expr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Param {
    pub name: String,
    pub ty: Type,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Func {
    pub name: String,
    pub params: Vec<Param>,
    pub requires: Vec<Expr>,
    pub ensures: Vec<Expr>,
    pub body: Vec<Stmt>,
    /// True when the body was elided with `;` (intent-only). The agentic
    /// synthesizer is responsible for providing an implementation.
    pub elided: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stmt {
    /// `mutate state { ... }`
    MutateState(Vec<Assign>),
    /// A bare assignment outside of `mutate state`.
    Assign(Assign),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Assign {
    pub target: Expr,
    pub op: AssignOp,
    pub value: Expr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignOp {
    Set,
    Add,
    Sub,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    Int(i64),
    /// A bare identifier, e.g. `amount`.
    Var(String),
    /// A field access, e.g. `from.balance`.
    Field { base: String, field: String },
    /// `old(expr)` -- the value of `expr` in the pre-state.
    Old(Box<Expr>),
    Unary { op: UnOp, expr: Box<Expr> },
    Bin {
        op: BinOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Neg,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}
