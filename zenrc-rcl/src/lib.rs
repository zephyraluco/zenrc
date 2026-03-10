#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

include!(concat!(env!("OUT_DIR"), "/rcl_bindings.rs"));
include!(concat!(env!("OUT_DIR"), "/introspection_maps.rs"));

mod rust_types;