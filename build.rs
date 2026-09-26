use std::{env, path::PathBuf};

fn main() {
    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());

    println!("cargo:rustc-link-search={}", manifest_dir.display());
    println!("cargo:rustc-link-arg=-Tlinkall.x");
    println!("cargo:rerun-if-changed=build.rs");
}
