fn main() {
    let library = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/web/js/opfs_shared_access_hack.js"
    );

    println!("cargo::rerun-if-changed={library}");
    println!("cargo::rustc-link-arg=--js-library={library}");

    if std::env::var_os("CARGO_FEATURE_WEBGPU").is_some() {
        println!("cargo::rustc-link-arg=-sMAX_WEBGL_VERSION=2");
        println!("cargo::rustc-link-arg=-sERROR_ON_UNDEFINED_SYMBOLS=0");
    }
}
