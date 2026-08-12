use std::{env, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-env-changed=NUTTX_INCLUDE_DIR");

    let Ok(nuttx_include_dirs) = env::var("NUTTX_INCLUDE_DIR") else {
        return;
    };
    let mut builder = bindgen::Builder::default();

    for dir in nuttx_include_dirs.split(':') {
        builder = builder.clang_arg(format!("-I{dir}"));
    }

    let bindings = builder
        .header("wrapper.h")
        .clang_arg("-nostdinc")
        .clang_arg("-nostdlib")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate()
        .unwrap();

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings.write_to_file(out_dir.join("bindings.rs")).unwrap();
}
