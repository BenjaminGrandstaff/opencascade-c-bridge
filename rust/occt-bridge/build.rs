use std::{env, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-env-changed=OCCT_BRIDGE_LIB_DIR");
    let library_dir = env::var_os("OCCT_BRIDGE_LIB_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"))
                .join("../..")
                .join("build")
        });
    println!("cargo:rustc-link-search=native={}", library_dir.display());
    println!("cargo:rustc-link-lib=dylib=occt_bridge");
}
