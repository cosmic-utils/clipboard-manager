use std::env;

fn main() {
    println!("cargo:rerun-if-env-changed=MIGRATIONS_FOLDER");

    let var = env::var("MIGRATIONS_FOLDER").unwrap_or_else(|_| "./migrations".to_string());

    println!("cargo:rustc-env=MIGRATIONS_FOLDER={}", var);
}
