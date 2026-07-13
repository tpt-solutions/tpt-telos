//! Dual-backend project assembly for tpt-telos (Phase 3).
//!
//! [`generate_project`] routes each module to the Rust or Go backend, generates
//! the corresponding sources, and -- when a program spans *both* backends --
//! emits the automatic FFI bridge so the two halves can call each other with no
//! hand-written glue. The result is a ready-to-build project tree:
//!
//! ```text
//! <out>/
//!   rust/Cargo.toml
//!   rust/src/lib.rs
//!   rust/src/ffi.rs      (dual-backend only)
//!   go/go.mod
//!   go/service.go
//!   go/ffi.go            (dual-backend only)
//!   go/telos_ffi.h       (dual-backend only)
//! ```

use std::io;
use std::path::Path;

use telos_agent::FuncOutcome;
use telos_parser::ast::*;
use telos_router::Target;

use crate::{collect_bodies, ffi, go, render_rust};

/// Go package name used for the generated service and its FFI shims.
pub const GO_PACKAGE: &str = "gosvc";

/// A single generated file, addressed by a path relative to the project root.
#[derive(Debug, Clone)]
pub struct GeneratedFile {
    pub path: String,
    pub contents: String,
}

/// A fully assembled dual-backend project.
#[derive(Debug, Clone)]
pub struct Project {
    pub files: Vec<GeneratedFile>,
    pub has_rust: bool,
    pub has_go: bool,
    pub has_ffi: bool,
}

impl Project {
    /// Write every generated file under `root`, creating directories as needed.
    pub fn write(&self, root: &Path) -> io::Result<()> {
        for f in &self.files {
            let full = root.join(&f.path);
            if let Some(parent) = full.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&full, &f.contents)?;
        }
        Ok(())
    }
}

/// Assemble the dual-backend project for a program.
pub fn generate_project(modules: &[Module], outcomes: &[FuncOutcome]) -> Project {
    let bodies = collect_bodies(outcomes);

    let rust_mods: Vec<&Module> = modules
        .iter()
        .filter(|m| telos_router::route(&m.attributes).target == Target::Rust)
        .collect();
    let go_mods: Vec<&Module> = modules
        .iter()
        .filter(|m| telos_router::route(&m.attributes).target == Target::Go)
        .collect();

    let has_rust = !rust_mods.is_empty();
    let has_go = !go_mods.is_empty();
    let has_ffi = has_rust && has_go;

    let mut files = Vec::new();

    if has_rust {
        let mut lib = render_rust(&rust_mods, &bodies);
        if has_ffi {
            lib.push_str("\npub mod ffi;\n");
        }
        files.push(GeneratedFile {
            path: "rust/src/lib.rs".to_string(),
            contents: lib,
        });
        files.push(GeneratedFile {
            path: "rust/Cargo.toml".to_string(),
            contents: rust_cargo_toml(has_ffi),
        });
    }

    if has_go {
        let service = go::generate_go_package(&go_mods, &bodies, GO_PACKAGE);
        files.push(GeneratedFile {
            path: "go/service.go".to_string(),
            contents: service,
        });
        files.push(GeneratedFile {
            path: "go/go.mod".to_string(),
            contents: go_mod(),
        });
    }

    if has_ffi {
        let bridge = ffi::generate_bridge(modules, &bodies, GO_PACKAGE);
        files.push(GeneratedFile {
            path: "rust/src/ffi.rs".to_string(),
            contents: bridge.rust,
        });
        files.push(GeneratedFile {
            path: "go/ffi.go".to_string(),
            contents: bridge.go,
        });
        files.push(GeneratedFile {
            path: "go/telos_ffi.h".to_string(),
            contents: bridge.header,
        });
    }

    Project {
        files,
        has_rust,
        has_go,
        has_ffi,
    }
}

fn rust_cargo_toml(dual: bool) -> String {
    // A dual-backend crate is built as a staticlib (for linking into the Go
    // binary) plus an rlib (so `cargo build` type-checks standalone).
    let lib_section = if dual {
        "\n[lib]\ncrate-type = [\"staticlib\", \"rlib\"]\n"
    } else {
        ""
    };
    format!(
        "[package]\nname = \"generated_rust\"\nversion = \"0.1.0\"\nedition = \"2021\"\n{lib}\n[dependencies]\n\n[workspace]\n",
        lib = lib_section
    )
}

fn go_mod() -> String {
    format!("module telos/{}\n\ngo 1.21\n", GO_PACKAGE)
}
