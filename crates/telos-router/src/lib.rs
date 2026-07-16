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

use tpt_telos_parser::ast::{Arg, Attribute};

/// The compilation backend a module or function is routed to.
///
/// Determined by `@boundary(...)` architectural metadata.
/// Defaults to [`Target::Rust`] when no recognised flags are present.
///
/// # Examples
///
/// ```
/// use tpt_telos_router::{route, Target};
/// use tpt_telos_parser::ast::{Arg, Attribute};
///
/// let attr = Attribute {
///     name: "boundary".to_string(),
///     args: vec![Arg::Flag("network_io".to_string())],
/// };
/// let r = route(&[attr]);
/// assert_eq!(r.target, Target::Go);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Rust,
    Go,
}

impl Target {
    /// Returns the target as a lowercase string: `"rust"` or `"go"`.
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_telos_router::Target;
    ///
    /// assert_eq!(Target::Rust.as_str(), "rust");
    /// assert_eq!(Target::Go.as_str(), "go");
    /// ```
    pub fn as_str(&self) -> &'static str {
        match self {
            Target::Rust => "rust",
            Target::Go => "go",
        }
    }
}

/// The routing decision for a module or function.
///
/// Contains the chosen [`Target`] backend and a human-readable `reason`
/// string for transparency in CLI output.
#[derive(Debug, Clone)]
pub struct Route {
    pub target: Target,
    /// Human-readable justification, surfaced by the CLI for transparency.
    pub reason: String,
}

impl Route {
    /// Returns `true` when the target is [`Target::Rust`].
    ///
    /// # Examples
    ///
    /// ```
    /// use tpt_telos_router::{route, Target};
    /// use tpt_telos_parser::ast::{Arg, Attribute};
    ///
    /// let cpu_attr = Attribute {
    ///     name: "boundary".to_string(),
    ///     args: vec![Arg::Flag("cpu_bound".to_string())],
    /// };
    /// let r = route(&[cpu_attr]);
    /// assert!(r.is_rust());
    /// assert_eq!(r.target, Target::Rust);
    /// ```
    pub fn is_rust(&self) -> bool {
        self.target == Target::Rust
    }
}

/// Boundary flags that imply a Rust target.
const RUST_FLAGS: &[&str] = &["cpu_bound", "zero_allocation", "crypto", "real_time"];

/// Boundary flags that imply a Go target.
const GO_FLAGS: &[&str] = &[
    "network_io",
    "high_concurrency",
    "distributed",
    "high_latency",
];

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
///
/// # Examples
///
/// ```
/// use tpt_telos_router::{route, Target};
/// use tpt_telos_parser::ast::{Arg, Attribute};
///
/// // No boundary flags → Rust (default).
/// assert_eq!(route(&[]).target, Target::Rust);
///
/// // cpu_bound → Rust.
/// let rust_attr = Attribute {
///     name: "boundary".to_string(),
///     args: vec![Arg::Flag("cpu_bound".to_string())],
/// };
/// assert_eq!(route(&[rust_attr]).target, Target::Rust);
///
/// // network_io → Go.
/// let go_attr = Attribute {
///     name: "boundary".to_string(),
///     args: vec![Arg::Flag("network_io".to_string())],
/// };
/// let r = route(&[go_attr]);
/// assert_eq!(r.target, Target::Go);
/// assert!(!r.is_rust());
/// assert!(r.reason.contains("network_io"));
/// ```
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
    use tpt_telos_parser::ast::{Arg, Attribute};

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

    // ---- every Rust-bound boundary flag ----

    #[test]
    fn zero_allocation_routes_to_rust() {
        let r = route(&[attr(&["zero_allocation"])]);
        assert_eq!(r.target, Target::Rust);
    }

    #[test]
    fn crypto_routes_to_rust() {
        let r = route(&[attr(&["crypto"])]);
        assert_eq!(r.target, Target::Rust);
    }

    #[test]
    fn real_time_routes_to_rust() {
        let r = route(&[attr(&["real_time"])]);
        assert_eq!(r.target, Target::Rust);
    }

    // ---- every Go-bound boundary flag ----

    #[test]
    fn high_concurrency_routes_to_go() {
        let r = route(&[attr(&["high_concurrency"])]);
        assert_eq!(r.target, Target::Go);
    }

    #[test]
    fn distributed_routes_to_go() {
        let r = route(&[attr(&["distributed"])]);
        assert_eq!(r.target, Target::Go);
    }

    #[test]
    fn high_latency_routes_to_go() {
        let r = route(&[attr(&["high_latency"])]);
        assert_eq!(r.target, Target::Go);
    }

    // ---- combinations ----

    #[test]
    fn multiple_rust_flags_still_rust() {
        let r = route(&[attr(&["cpu_bound", "zero_allocation", "crypto"])]);
        assert_eq!(r.target, Target::Rust);
    }

    #[test]
    fn multiple_go_flags_still_go() {
        let r = route(&[attr(&["network_io", "high_concurrency"])]);
        assert_eq!(r.target, Target::Go);
    }

    #[test]
    fn go_flag_wins_over_rust_flag() {
        // A mixed annotation is treated as a Go (distributed/network) target.
        let r = route(&[attr(&["cpu_bound", "network_io"])]);
        assert_eq!(r.target, Target::Go);
    }

    #[test]
    fn unrecognised_flag_defaults_to_rust() {
        // Unknown flags contribute nothing; the default compute backend applies.
        let r = route(&[attr(&["some_future_flag"])]);
        assert_eq!(r.target, Target::Rust);
    }

    #[test]
    fn route_records_justification() {
        let r = route(&[attr(&["network_io"])]);
        assert!(r.reason.contains("network_io"));
        assert!(r.is_rust() == (r.target == Target::Rust));
    }
}
