use std::{env, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-env-changed=NUTTX_INCLUDE_DIR");
    println!("cargo::rustc-check-cfg=cfg(cdcacm_console)");
    println!("cargo::rustc-check-cfg=cfg(video_fb)");

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
        .clang_macro_fallback()
        // bindgen often has difficulty representing this type in Rust, so we have to blocklist it
        .blocklist_type("max_align_t")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate()
        .unwrap();

    // FIXME: This is a quite terrible way for config feature direction
    let bindings_source = bindings.to_string();

    if bindings_source.contains("pub const CONFIG_CDCACM_CONSOLE: u32 = 1;") {
        println!("cargo::rustc-cfg=cdcacm_console");
    }

    if bindings_source.contains("pub const CONFIG_VIDEO_FB: u32 = 1;") {
        println!("cargo::rustc-cfg=video_fb");
    }

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings.write_to_file(out_dir.join("bindings.rs")).unwrap();
}
