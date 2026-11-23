#![allow(
    non_upper_case_globals,
    suspicious_runtime_symbol_definitions,
    non_camel_case_types,
    non_snake_case,
    unused,
    clippy::upper_case_acronyms,
    clippy::ptr_offset_with_cast,
    clippy::manual_div_ceil,
    clippy::useless_transmute
)]

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
