//! MuleForge core binary — thin wrapper that calls the library.
//! The real CLI is the TypeScript wrapper in `cli/`.

fn main() {
    eprintln!("muleforge-core: use the `muleforge` CLI instead.");
    eprintln!("This binary is an internal implementation detail.");
    std::process::exit(1);
}
