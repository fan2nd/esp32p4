use std::{env, path::PathBuf};

fn main() {
    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());

    #[cfg(feature = "slint-ui")]
    {
        slint_build::compile_with_config(
            "assets/ui/desktop.slint",
            slint_build::CompilerConfiguration::new()
                .with_style("fluent".into())
                .embed_resources(slint_build::EmbedResourcesKind::EmbedForSoftwareRenderer),
        )
        .unwrap();

        // #[path] requires a literal path. Keep the generated module in the
        // ignored workspace target directory, even with CARGO_TARGET_DIR overridden.
        // The compiler still registers Slint's input/resource dependencies above.
        let generated_dir = manifest_dir.join("target/generated");
        std::fs::create_dir_all(&generated_dir).unwrap();
        let source = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("desktop.rs");
        let destination = generated_dir.join("slint.rs");
        let bytes = std::fs::read(source).unwrap();
        if std::fs::read(&destination).ok().as_deref() != Some(bytes.as_slice()) {
            std::fs::write(&destination, bytes).unwrap();
        }
        // Regenerate if just this derived file is deleted while Cargo caches survive.
        println!("cargo:rerun-if-changed={}", destination.display());
    }

    println!("cargo:rustc-link-search={}", manifest_dir.display());
    println!("cargo:rustc-link-arg=-Tlinkall.x");
    println!("cargo:rerun-if-changed=build.rs");
}
