//! tpt-telos intermediate representation and constraint extraction.

pub mod extract;
pub mod ir;

pub use extract::{exact_nonlinear, extract, set_exact_nonlinear};
pub use ir::*;
