use std::{env, ops::RangeInclusive, path::PathBuf};

use read_fonts::{FontRef, collections::IntSet};
use skera::{Plan, SubsetFlags, subset_font};

const UNSCII_FONT_PATH: &str = "../../external/unscii/fontfiles/unscii-16.otf";
const LAST_CODEPOINT: u32 = 0x10ffff;
const PRIVATE_USE_AREAS: [RangeInclusive<u32>; 3] =
    [0xe000..=0xf8ff, 0xf0000..=0xffffd, 0x100000..=0x10fffd];

fn main() {
    println!("cargo:rerun-if-env-changed=NUTTX_INCLUDE_DIR");
    println!("cargo::rustc-check-cfg=cfg(cdcacm_console)");
    println!("cargo::rustc-check-cfg=cfg(video_fb)");

    write_bindings();

    if env::var_os("CARGO_FEATURE_EGUI").is_some() {
        write_unscii_without_private_use();
    }
}

fn write_bindings() {
    let Ok(nuttx_include_directories) = env::var("NUTTX_INCLUDE_DIR") else {
        return;
    };
    let mut builder = bindgen::Builder::default();

    for directory in nuttx_include_directories.split(':') {
        builder = builder.clang_arg(format!("-I{directory}"));
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

/// We do not use the private use areas, and the conflict with lucide is difficult to resolve
///
/// This also saves a couple dozen kilobytes anyway
fn write_unscii_without_private_use() {
    println!("cargo:rerun-if-changed={UNSCII_FONT_PATH}");

    let font_bytes = std::fs::read(UNSCII_FONT_PATH).unwrap();
    let font = FontRef::new(&font_bytes).unwrap();

    let mut codepoints = IntSet::empty();
    codepoints.insert_range(0..=LAST_CODEPOINT);

    for area in PRIVATE_USE_AREAS {
        codepoints.remove_range(area);
    }

    let plan = Plan::new(
        &IntSet::empty(),
        &codepoints,
        &font,
        SubsetFlags::SUBSET_FLAGS_DEFAULT,
        &IntSet::empty(),
        &IntSet::empty(),
        &IntSet::empty(),
        &IntSet::empty(),
        &IntSet::empty(),
    );
    let subset = subset_font(&font, &plan).unwrap();

    let out_directory = PathBuf::from(env::var("OUT_DIR").unwrap());
    std::fs::write(out_directory.join("unscii-16.otf"), subset).unwrap();
}
