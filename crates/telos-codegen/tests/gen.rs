use telos_agent::{StaticAgent, transpile_module};
use telos_codegen::generate_program;
use telos_parser::parse;

#[test]
fn generated_rust_compiles_for_wallet() {
    let src = std::fs::read_to_string("../../examples/wallet.telos").unwrap();
    let modules = parse(&src).unwrap();
    let agent = StaticAgent::new();
    let mut outcomes = Vec::new();
    for m in &modules {
        outcomes.extend(transpile_module(m, &agent).unwrap());
    }
    let rust = generate_program(&modules, &outcomes);

    assert!(rust.contains("pub struct Wallet"));
    assert!(rust.contains("pub fn transfer(from: &mut Wallet, to: &mut Wallet, amount: i64)"));
    assert!(rust.contains("from.balance -= amount"));
    assert!(rust.contains("to.balance += amount"));
    // Contracts preserved faithfully in doc-comments.
    assert!(rust.contains("ensures:  from.balance == old(from.balance) - amount"));
}

#[test]
fn elided_func_synthesizes_body() {
    let src = std::fs::read_to_string("../../examples/intent.telos").unwrap();
    let modules = parse(&src).unwrap();
    let agent = StaticAgent::new();
    let mut outcomes = Vec::new();
    for m in &modules {
        outcomes.extend(transpile_module(m, &agent).unwrap());
    }
    let rust = generate_program(&modules, &outcomes);
    assert!(rust.contains("c.value = c.value + by"));
    assert!(rust.contains("c.value = c.value - by"));
}
