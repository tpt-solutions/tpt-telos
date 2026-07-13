//! Context router for tpt-telos.
//!
//! Decides, from `@boundary(...)` architectural metadata, whether a module or
//! function should be compiled to the Rust backend (CPU-bound, zero-allocation,
//! cryptographic paths) or the Go backend (network I/O, high-concurrency,
//! distributed paths).
//!
//! In Phase 2 only the Rust backend is generated; the router nonetheless
//! records the intended target so the dual-backend work (Phase 3) can consume
//! it without further parsing changes.

use telos_parser::ast::{Attribute, Arg};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Rust,
    Go,
}

impl Target {
    pub fn as_str(&self) -> &'static str {
        match self {
            Target::Rust => "rust",
            Target::Go => "go",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Route {
    pub target: Target,
    /// Human-readable justification, surfaced by the CLI for transparency.
    pub reason: String,
}

impl Route {
    pub fn is_rust(&self) -> bool {
        self.target == Target::Rust
    }
}

/// Boundary flags that imply a Rust target.
const RUST_FLAGS: &[&str] = &["cpu_bound", "zero_allocation", "crypto", "real_time"];

/// Boundary flags that imply a Go target.
const GO_FLAGS: &[&str] = &["network_io", "high_concurrency", "distributed", "high_latency"];

fn boundary_flags(attrs: &[Attribute]) -> Vec<String> {
    let mut flags = Vec::new();
    for attr in attrs {
        if attr.name != "boundary" {
            continue;
        }
        for arg in &attr.args {
            if let Arg::Flag(f) = arg {
                flags.push(f.clone());
            }
        }
    }
    flags
}

/// Route a module from its own `@boundary` attributes (and any inherited
/// function-level attributes if supplied).
pub fn route(attrs: &[Attribute]) -> Route {
    let flags = boundary_flags(attrs);

    let mut go_hits = Vec::new();
    let mut rust_hits = Vec::new();
    for f in &flags {
        if GO_FLAGS.contains(&f.as_str()) {
            go_hits.push(f.clone());
        } else if RUST_FLAGS.contains(&f.as_str()) {
            rust_hits.push(f.clone());
        }
    }

    // Go backend wins when any distributed/network flag is present, otherwise
    // Rust is the default compute backend.
    if !go_hits.is_empty() {
        Route {
            target: Target::Go,
            reason: format!("boundary flags {:?} routed to Go backend", go_hits),
        }
    } else if !rust_hits.is_empty() {
        Route {
            target: Target::Rust,
            reason: format!("boundary flags {:?} routed to Rust backend", rust_hits),
        }
    } else {
        Route {
            target: Target::Rust,
            reason: "no explicit boundary flags; defaulting to Rust compute backend".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use telos_parser::ast::{Arg, Attribute};

    fn attr(flags: &[&str]) -> Attribute {
        Attribute {
            name: "boundary".to_string(),
            args: flags.iter().map(|f| Arg::Flag(f.to_string())).collect(),
        }
    }

    #[test]
    fn network_io_routes_to_go() {
        let r = route(&[attr(&["network_io"])]);
        assert_eq!(r.target, Target::Go);
    }

    #[test]
    fn cpu_bound_routes_to_rust() {
        let r = route(&[attr(&["cpu_bound"])]);
        assert_eq!(r.target, Target::Rust);
    }

    #[test]
    fn no_flags_default_rust() {
        let r = route(&[]);
        assert_eq!(r.target, Target::Rust);
    }
}
