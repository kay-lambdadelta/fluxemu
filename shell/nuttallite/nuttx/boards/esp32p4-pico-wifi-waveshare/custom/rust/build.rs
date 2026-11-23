use std::{env, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-env-changed=NUTTX_INCLUDE_DIR");

    let Ok(nuttx_include_directories) = env::var("NUTTX_INCLUDE_DIR") else {
        return;
    };
    let mut builder = bindgen::Builder::default();

    for directory in nuttx_include_directories.split(':') {
        builder = builder.clang_arg(format!("-I{directory}"));
    }

    let nuttx_directory = PathBuf::from(nuttx_include_directories.split(':').next().unwrap())
        .parent()
        .unwrap()
        .to_path_buf();

    let arch_include_directories = [
        nuttx_directory.join("arch/risc-v/src/common"),
        nuttx_directory.join("boards/risc-v/esp32p4/common/include"),
    ];

    for directory in arch_include_directories {
        builder = builder.clang_arg(format!("-I{}", directory.display()));
    }

    let bindings = builder
        .header("wrapper.h")
        .use_core()
        .wrap_unsafe_ops(true)
        .prepend_enum_name(false)
        .clang_arg("-nostdinc")
        .clang_arg("-D__NuttX__")
        .clang_arg("-D__KERNEL__")
        .clang_macro_fallback()
        .blocklist_type("max_align_t")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate()
        .unwrap();

    let out_directory = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_directory.join("bindings.rs"))
        .unwrap();
}
