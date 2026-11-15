use std::{collections::BTreeMap, fs::File, path::PathBuf};

use csv::ReaderBuilder;
use proc_macro2::Span;
use quote::quote;
use serde::Deserialize;
use serde_json::Value;
use syn::{Fields, Ident, Variant};

#[derive(Debug, Deserialize)]
struct CodeMapping {
    #[serde(rename = "_alpha3")]
    alpha3: Option<String>,
    #[serde(rename = "_numeric")]
    numeric: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LanguageRecord {
    alpha3_bibliographic: String,
    alpha3_terminologic: Option<String>,
    alpha2: Option<String>,
    english_name: String,
    _french_name: String,
}

fn main() {
    generate_3166();
    generate_639();
}

fn read_iso_639_2() -> Vec<LanguageRecord> {
    let file = File::open("data/loc/ISO-639-2_utf-8.txt").expect("Could not open ISO 639-2 list");

    let mut reader = ReaderBuilder::new()
        .delimiter(b'|')
        .has_headers(false)
        .from_reader(file);

    reader
        .deserialize()
        .map(|result| result.expect("ISO 639-2 list parse error"))
        .filter(|record: &LanguageRecord| record.alpha3_bibliographic != "qaa-qtz")
        .collect()
}

fn iso_639_2_bibliographic_code<'a>(records: &'a [LanguageRecord], code: &str) -> &'a str {
    records
        .iter()
        .find(|record| {
            record.alpha2.as_deref() == Some(code)
                || record.alpha3_bibliographic == code
                || record.alpha3_terminologic.as_deref() == Some(code)
        })
        .map(|record| record.alpha3_bibliographic.as_str())
        .unwrap_or_else(|| panic!("Language {code} is not in the ISO 639-2 list"))
}

fn read_json_section<T: for<'de> Deserialize<'de>>(path: &str, pointer: &str) -> T {
    let file = File::open(path).unwrap_or_else(|error| panic!("Could not open {path}: {error}"));

    let mut document: Value = serde_json::from_reader(file)
        .unwrap_or_else(|error| panic!("Could not parse {path}: {error}"));

    let section = document
        .pointer_mut(pointer)
        .unwrap_or_else(|| panic!("{path} has no {pointer}"))
        .take();

    serde_json::from_value(section)
        .unwrap_or_else(|error| panic!("Unexpected shape at {pointer} in {path}: {error}"))
}

fn is_user_assigned_country_code(code: &str) -> bool {
    let mut letters = code.chars();
    let first = letters.next();
    let second = letters.next();

    matches!(
        (first, second),
        (Some('A'), Some('A'))
            | (Some('Q'), Some('M'..='Z'))
            | (Some('X'), _)
            | (Some('Z'), Some('Z'))
    )
}

pub fn generate_3166() {
    let code_mappings: BTreeMap<String, CodeMapping> =
        read_json_section("data/cldr/codeMappings.json", "/supplemental/codeMappings");

    let display_names: BTreeMap<String, String> = read_json_section(
        "data/cldr/territories.json",
        "/main/en/localeDisplayNames/territories",
    );

    let mut countries: Vec<_> = code_mappings
        .iter()
        .filter_map(|(alpha2, mapping)| {
            let alpha3 = mapping.alpha3.as_deref()?;
            mapping.numeric.as_ref()?;
            let english_name = display_names.get(alpha2)?;

            let is_real_country = alpha2.len() == 2
                && !is_user_assigned_country_code(alpha2)
                && !is_user_assigned_country_code(alpha3);

            is_real_country.then_some((alpha2.as_str(), alpha3, english_name.as_str()))
        })
        .collect();
    countries.sort_by_key(|(_, _, english_name)| english_name.to_lowercase());

    let likely_subtags: BTreeMap<String, String> = read_json_section(
        "data/cldr/likelySubtags.json",
        "/supplemental/likelySubtags",
    );
    let languages = read_iso_639_2();

    // Construct variants
    let mut variants_2 = Vec::new();
    let mut variants_3 = Vec::new();
    let mut mappings_2to3 = Vec::new();
    let mut mappings_3to2 = Vec::new();
    let mut english_names = BTreeMap::new();
    let mut likely_languages = Vec::new();

    for (alpha2, alpha3, _) in &countries {
        let ident2 = Ident::new(&alpha2.to_uppercase(), Span::call_site());

        let likely_tag = likely_subtags
            .get(&format!("und-{alpha2}"))
            .or_else(|| likely_subtags.get("und"))
            .expect("likelySubtags.json has no und entry");
        let likely_language = likely_tag.split('-').next().unwrap_or_default();
        let language_ident = Ident::new(
            &iso_639_2_bibliographic_code(&languages, likely_language).to_uppercase(),
            Span::call_site(),
        );
        likely_languages.push((ident2.clone(), language_ident));

        variants_2.push(Variant {
            attrs: Vec::new(),
            ident: ident2.clone(),
            fields: Fields::Unit,
            discriminant: None,
        });

        let ident3 = Ident::new(&alpha3.to_uppercase(), Span::call_site());

        variants_3.push(Variant {
            attrs: Vec::new(),
            ident: ident3.clone(),
            fields: Fields::Unit,
            discriminant: None,
        });

        let name_keys = display_names
            .keys()
            .filter(|key| *key == alpha2 || key.starts_with(&format!("{alpha2}-alt-")));

        for key in name_keys {
            let name = display_names[key].trim().to_lowercase();
            let ascii_apostrophe = name.replace('\u{2019}', "'");
            let ampersand_spelled_out = ascii_apostrophe.replace(" & ", " and ");

            for spelling in [ampersand_spelled_out, ascii_apostrophe, name] {
                english_names
                    .entry(spelling)
                    .or_insert_with(|| ident2.clone());
            }
        }

        mappings_2to3.push((ident2.clone(), ident3.clone()));
        mappings_3to2.push((ident3, ident2));
    }

    let derives: Vec<_> = [
        "Copy",
        "Clone",
        "Hash",
        "PartialEq",
        "Eq",
        "PartialOrd",
        "Ord",
        "Debug",
    ]
    .into_iter()
    .map(|ident| Ident::new(ident, Span::call_site()))
    .collect();

    let arms_2to3 = mappings_2to3.iter().map(|(k, v)| {
        quote! { Self::#k => Iso3166Alpha3::#v, }
    });
    let english_name_arms = english_names.iter().map(|(name, ident)| {
        quote! { #name => Some(Self::#ident), }
    });
    let likely_language_arms = likely_languages.iter().map(|(country, language)| {
        quote! { Self::#country => Iso639Alpha3::#language, }
    });
    let arms_3to2 = mappings_3to2.iter().map(|(k, v)| {
        quote! { Self::#k => Iso3166Alpha2::#v, }
    });
    let display_arms_2 = variants_2.iter().map(|variant| {
        let ident = variant.ident.clone();
        let s = ident.to_string().to_lowercase();
        quote! { Self::#ident => #s, }
    });
    let fromstr_arms_2 = variants_2.iter().map(|variant| {
        let ident = variant.ident.clone();
        let s = ident.to_string().to_lowercase();
        quote! { #s => Ok(Self::#ident), }
    });
    let display_arms_3 = variants_3.iter().map(|variant| {
        let ident = variant.ident.clone();
        let s = ident.to_string().to_lowercase();
        quote! { Self::#ident => #s, }
    });
    let fromstr_arms_3 = variants_3.iter().map(|variant| {
        let ident = variant.ident.clone();
        let s = ident.to_string().to_lowercase();
        quote! { #s => Ok(Self::#ident), }
    });

    // Build the enum AST
    let file = quote! {
        #[derive(#(#derives),*)]
        pub enum Iso3166Alpha2 {
            #(#variants_2),*
        }

        impl Iso3166Alpha2 {
            pub fn to_alpha3(&self) -> Iso3166Alpha3 {
                match self {
                    #(#arms_2to3)*
                }
            }

            pub fn from_english_name(name: &str) -> Option<Self> {
                match name.to_lowercase().as_str() {
                    #(#english_name_arms)*
                    _ => None,
                }
            }

            pub fn likely_language(&self) -> Iso639Alpha3 {
                match self {
                    #(#likely_language_arms)*
                }
            }
        }

        impl core::fmt::Display for Iso3166Alpha2 {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                let s = match self {
                    #(#display_arms_2)*
                };
                write!(f, "{}", s)
            }
        }
        impl core::str::FromStr for Iso3166Alpha2 {
            type Err = String;

                fn from_str(s: &str) -> Result<Self, Self::Err> {
                match s.to_lowercase().as_str() {
                    #(#fromstr_arms_2)*
                    _ => Err(format!("unknown ISO3166 code: {}", s)),
                }
            }
        }

        impl serde::Serialize for Iso3166Alpha2 {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
            {
                serializer.serialize_str(&self.to_string())
            }
        }
        impl<'de> serde::Deserialize<'de> for Iso3166Alpha2 {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: serde::Deserializer<'de>,
            {
                let s = String::deserialize(deserializer)?;
                s.parse().map_err(serde::de::Error::custom)
            }
        }

        #[derive(#(#derives),*)]
        pub enum Iso3166Alpha3 {
            #(#variants_3),*
        }

        impl Iso3166Alpha3 {
            pub fn to_alpha2(&self) -> Iso3166Alpha2 {
                match self {
                    #(#arms_3to2)*
                }
            }
        }

        impl core::fmt::Display for Iso3166Alpha3 {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                let s = match self {
                    #(#display_arms_3)*
                };
                write!(f, "{}", s)
            }
        }
        impl core::str::FromStr for Iso3166Alpha3 {
            type Err = String;

                fn from_str(s: &str) -> Result<Self, Self::Err> {
                match s.to_lowercase().as_str() {
                    #(#fromstr_arms_3)*
                    _ => Err(format!("unknown ISO3166 code: {}", s)),
                }
            }
        }

        impl serde::Serialize for Iso3166Alpha3 {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
            {
                serializer.serialize_str(&self.to_string())
            }
        }
        impl<'de> serde::Deserialize<'de> for Iso3166Alpha3 {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: serde::Deserializer<'de>,
            {
                let s = String::deserialize(deserializer)?;
                s.parse().map_err(serde::de::Error::custom)
            }
        }
    };

    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR not set"));
    let dest_path = out_dir.join("iso3166.rs");
    std::fs::write(dest_path, file.to_string()).expect("Failed to write generated Rust source");
}

pub fn generate_639() {
    // Construct variants
    let mut variants_2 = Vec::new();
    let mut variants_3 = Vec::new();
    let mut mappings_2to3 = Vec::new();
    let mut mappings_3to2 = Vec::new();
    let mut english_names = Vec::new();

    for record in read_iso_639_2() {
        let ident2 = record.alpha2.map(|alpha2| {
            let ident_str = alpha2.to_uppercase();
            let ident2 = Ident::new(&ident_str, Span::call_site());

            variants_2.push(Variant {
                attrs: Vec::new(),
                ident: ident2.clone(),
                fields: Fields::Unit,
                discriminant: None,
            });

            ident2
        });

        let ident_str = record.alpha3_bibliographic.to_uppercase();
        let ident3 = Ident::new(&ident_str, Span::call_site());

        variants_3.push(Variant {
            attrs: Vec::new(),
            ident: ident3.clone(),
            fields: Fields::Unit,
            discriminant: None,
        });

        let english_name = record
            .english_name
            .split(';')
            .next()
            .unwrap_or_default()
            .trim()
            .to_string();
        english_names.push((ident3.clone(), english_name));

        if let Some(ident2) = ident2 {
            mappings_2to3.push((ident2.clone(), ident3.clone()));
            mappings_3to2.push((ident3, ident2));
        }
    }

    let derives: Vec<_> = [
        "Copy",
        "Clone",
        "Hash",
        "PartialEq",
        "Eq",
        "PartialOrd",
        "Ord",
        "Debug",
    ]
    .into_iter()
    .map(|ident| Ident::new(ident, Span::call_site()))
    .collect();

    let arms_2to3 = mappings_2to3.iter().map(|(k, v)| {
        quote! { Self::#k => Iso639Alpha3::#v, }
    });
    let arms_3to2 = mappings_3to2.iter().map(|(k, v)| {
        quote! { Self::#k => Some(Iso639Alpha2::#v), }
    });
    let display_arms_2 = variants_2.iter().map(|variant| {
        let ident = variant.ident.clone();
        let s = ident.to_string().to_lowercase();
        quote! { Self::#ident => #s, }
    });
    let fromstr_arms_2 = variants_2.iter().map(|variant| {
        let ident = variant.ident.clone();
        let s = ident.to_string().to_lowercase();
        quote! { #s => Ok(Self::#ident), }
    });
    let display_arms_3 = variants_3.iter().map(|variant| {
        let ident = variant.ident.clone();
        let s = ident.to_string().to_lowercase();
        quote! { Self::#ident => #s, }
    });
    let english_name_arms_3 = english_names.iter().map(|(ident, name)| {
        quote! { Self::#ident => #name, }
    });
    let fromstr_arms_3 = variants_3.iter().map(|variant| {
        let ident = variant.ident.clone();
        let s = ident.to_string().to_lowercase();
        quote! { #s => Ok(Self::#ident), }
    });

    // Build the enum AST
    let file = quote! {
        #[derive(#(#derives),*)]
        pub enum Iso639Alpha2 {
            #(#variants_2),*
        }

        impl Iso639Alpha2 {
            pub fn to_alpha3(&self) -> Iso639Alpha3 {
                match self {
                    #(#arms_2to3)*
                }
            }
        }

        impl core::fmt::Display for Iso639Alpha2 {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                let s = match self {
                    #(#display_arms_2)*
                };
                write!(f, "{}", s)
            }
        }
        impl core::str::FromStr for Iso639Alpha2 {
            type Err = String;

                fn from_str(s: &str) -> Result<Self, Self::Err> {
                match s.to_lowercase().as_str() {
                    #(#fromstr_arms_2)*
                    _ => Err(format!("unknown IS639 code: {}", s)),
                }
            }
        }

        impl serde::Serialize for Iso639Alpha2 {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
            {
                serializer.serialize_str(&self.to_string())
            }
        }
        impl<'de> serde::Deserialize<'de> for Iso639Alpha2 {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: serde::Deserializer<'de>,
            {
                let s = String::deserialize(deserializer)?;
                s.parse().map_err(serde::de::Error::custom)
            }
        }

        #[derive(#(#derives),*)]
        pub enum Iso639Alpha3 {
            #(#variants_3),*
        }
        impl Iso639Alpha3 {
            pub fn to_alpha2(&self) -> Option<Iso639Alpha2> {
                match self {
                    #(#arms_3to2)*
                    _ => None,
                }
            }

            pub fn english_name(&self) -> &'static str {
                match self {
                    #(#english_name_arms_3)*
                }
            }
        }

        impl core::fmt::Display for Iso639Alpha3 {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                let s = match self {
                    #(#display_arms_3)*
                };
                write!(f, "{}", s)
            }
        }
        impl core::str::FromStr for Iso639Alpha3 {
            type Err = String;

                fn from_str(s: &str) -> Result<Self, Self::Err> {
                match s.to_lowercase().as_str() {
                    #(#fromstr_arms_3)*
                    _ => Err(format!("unknown IS639 code: {}", s)),
                }
            }
        }

        impl serde::Serialize for Iso639Alpha3 {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
            {
                serializer.serialize_str(&self.to_string())
            }
        }
        impl<'de> serde::Deserialize<'de> for Iso639Alpha3 {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: serde::Deserializer<'de>,
            {
                let s = String::deserialize(deserializer)?;
                s.parse().map_err(serde::de::Error::custom)
            }
        }
    };

    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR not set"));
    let dest_path = out_dir.join("iso639.rs");
    std::fs::write(dest_path, file.to_string()).expect("Failed to write generated Rust source");
}
