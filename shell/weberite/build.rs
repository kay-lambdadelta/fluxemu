fn main() {
    let library = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/web/js/opfs_shared_access_hack.js"
    );

    println!("cargo::rerun-if-changed={library}");
    println!("cargo::rustc-link-arg=--js-library={library}");
}
