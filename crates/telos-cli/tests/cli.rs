//! Integration tests for the `telos` CLI binary.
//!
//! Drives the compiled binary (`CARGO_BIN_EXE_telos`) for `verify`, `transpile`,
//! and `build`, covering success, verification-failure, and malformed-file exit
//! codes and output.

use std::path::PathBuf;
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_telos");
const WALLET: &str = "../../examples/wallet.telos";
const BROKEN: &str = "../../examples/broken.telos";

fn run(args: &[&str]) -> (bool, String, String) {
    let out = Command::new(BIN)
        .args(args)
        .output()
        .expect("failed to run telos");
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    (out.status.success(), stdout, stderr)
}

fn write_temp(name: &str, contents: &str) -> PathBuf {
    let path = std::env::temp_dir().join(name);
    std::fs::write(&path, contents).unwrap();
    path
}

#[test]
fn verify_succeeds_on_valid_module() {
    let (ok, stdout, _) = run(&["verify", WALLET]);
    assert!(ok, "verify should exit 0 on a valid module");
    assert!(
        stdout.contains("PASS"),
        "expected PASS in output:\n{stdout}"
    );
    assert!(
        stdout.contains("all constraints satisfied"),
        "expected success summary:\n{stdout}"
    );
}

#[test]
fn verify_fails_on_broken_module() {
    let (ok, stdout, _) = run(&["verify", BROKEN]);
    assert!(!ok, "verify should exit non-zero on a failing module");
    assert!(
        stdout.contains("FAIL"),
        "expected FAIL in output:\n{stdout}"
    );
    assert!(
        stdout.contains("verification failed"),
        "expected failure summary:\n{stdout}"
    );
}

#[test]
fn verify_reports_error_on_malformed_file() {
    let path = write_temp(
        "telos_cli_malformed.telos",
        "module M { func f(x: T) requires @ }",
    );
    let (ok, _, stderr) = run(&["verify", path.to_str().unwrap()]);
    assert!(!ok, "verify should exit non-zero on a malformed file");
    assert!(
        stderr.contains("error"),
        "expected an error on stderr:\n{stderr}"
    );
}

#[test]
fn transpile_emits_rust_to_stdout() {
    let (ok, stdout, stderr) = run(&["transpile", WALLET]);
    assert!(ok, "transpile should exit 0: {stderr}");
    assert!(
        stdout.contains("pub fn transfer"),
        "expected generated Rust:\n{stdout}"
    );
}

#[test]
fn transpile_writes_to_output_file() {
    let path = std::env::temp_dir().join("telos_cli_transpile_out.rs");
    let (ok, _, stderr) = run(&["transpile", WALLET, "--out", path.to_str().unwrap()]);
    assert!(ok, "transpile --out should exit 0: {stderr}");
    let written = std::fs::read_to_string(&path).unwrap();
    assert!(
        written.contains("pub fn transfer"),
        "output file missing generated Rust:\n{written}"
    );
}

#[test]
fn build_compiles_generated_rust() {
    // Use a unique temp dir so parallel test runs don't collide.
    let dir = std::env::temp_dir().join(format!("telos_cli_build_{}", std::process::id()));
    let (ok, stdout, stderr) = run(&["build", WALLET, "--out-dir", dir.to_str().unwrap()]);
    assert!(ok, "build should exit 0:\n{stderr}");
    assert!(
        stdout.contains("compiles successfully"),
        "expected successful compile:\n{stdout}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn parse_reports_error_on_malformed_file() {
    let path = write_temp("telos_cli_malformed2.telos", "module M { func f(@) }");
    let (ok, _, stderr) = run(&["parse", path.to_str().unwrap()]);
    assert!(!ok, "parse should exit non-zero on a malformed file");
    assert!(stderr.contains("error"), "expected an error: {stderr}");
}

#[test]
fn project_generates_dual_backend_tree() {
    let dir = std::env::temp_dir().join(format!("telos_cli_project_{}", std::process::id()));
    let (ok, stdout, stderr) = run(&[
        "project",
        "../../examples/microservice.telos",
        "--out-dir",
        dir.to_str().unwrap(),
    ]);
    assert!(ok, "project should exit 0: {stderr}");
    assert!(
        stdout.contains("Backends: rust=true go=true"),
        "expected dual-backend summary:\n{stdout}"
    );
    assert!(dir.join("rust/src/lib.rs").exists(), "missing rust lib.rs");
    assert!(dir.join("go/service.go").exists(), "missing go service.go");
    assert!(dir.join("go/go.mod").exists(), "missing go.mod");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn eject_writes_manifest_and_project() {
    let dir = std::env::temp_dir().join(format!("telos_cli_eject_{}", std::process::id()));
    let (ok, _, stderr) = run(&[
        "eject",
        "../../examples/microservice.telos",
        "--out-dir",
        dir.to_str().unwrap(),
    ]);
    assert!(ok, "eject should exit 0: {stderr}");
    let manifest = std::fs::read_to_string(dir.join("telos-eject.json")).unwrap();
    assert!(
        manifest.contains("ejected"),
        "manifest should list ejected fns"
    );
    assert!(dir.join("rust/src/lib.rs").exists(), "missing rust lib.rs");
    assert!(dir.join("go/service.go").exists(), "missing go service.go");
    let _ = std::fs::remove_dir_all(&dir);
}
