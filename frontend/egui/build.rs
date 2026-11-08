use std::path::PathBuf;

use lucide_icons::{Icon, LUCIDE_FONT_BYTES};
use proc_macro2::Literal;
use quote::{format_ident, quote};
use read_fonts::{FontRef, collections::IntSet, types::NameId};
use skera::{Plan, SubsetFlags, subset_font};

const ICONS: &[Icon] = &[
    Icon::ArrowDown,
    Icon::ArrowUp,
    Icon::Bug,
    Icon::Camera,
    Icon::CircleX,
    Icon::Eye,
    Icon::Folder,
    Icon::FolderOpen,
    Icon::Footprints,
    Icon::Info,
    Icon::Lock,
    Icon::Pencil,
    Icon::Play,
    Icon::RefreshCw,
    Icon::Save,
    Icon::Settings,
    Icon::TriangleAlert,
];

const ENGLISH_UNITED_STATES: u16 = 0x0409;

fn main() {
    println!("cargo:rerun-if-changed=locales");
    println!("cargo:rerun-if-changed=build.rs");

    let out_directory = PathBuf::from(std::env::var("OUT_DIR").unwrap());

    let mut constants = Vec::new();
    let mut codepoints = IntSet::<u32>::empty();

    for icon in ICONS {
        let codepoint = char::from(*icon);
        let name = format_ident!("{}", icon.to_string().replace('-', "_").to_uppercase());
        let value = Literal::string(&codepoint.to_string());

        codepoints.insert(codepoint as u32);
        constants.push(quote! {
            pub const #name: &str = #value;
        });
    }

    let constants = quote! { #(#constants)* };

    let font = FontRef::new(LUCIDE_FONT_BYTES).unwrap();

    let mut name_ids = IntSet::empty();
    name_ids.insert_range(NameId::from(0)..=NameId::from(6));
    let mut name_languages = IntSet::empty();
    name_languages.insert(ENGLISH_UNITED_STATES);

    let plan = Plan::new(
        &IntSet::empty(),
        &codepoints,
        &font,
        SubsetFlags::SUBSET_FLAGS_NO_HINTING,
        &IntSet::empty(),
        &IntSet::empty(),
        &IntSet::empty(),
        &name_ids,
        &name_languages,
    );
    let subset = subset_font(&font, &plan).unwrap();

    std::fs::write(out_directory.join("lucide.ttf"), subset).unwrap();
    std::fs::write(out_directory.join("icon.rs"), constants.to_string()).unwrap();
}
