#![feature(generic_const_exprs)]
#![feature(iter_array_chunks)]
#![feature(int_roundings)]
#![feature(inline_const)]
#![feature(never_type)]
#![feature(type_alias_impl_trait)]
#![feature(core_intrinsics)]
#![feature(const_caller_location)]
#![feature(const_location_fields)]
#![feature(const_trait_impl)]
#![feature(generic_arg_infer)]
#![feature(result_flattening)]
#![feature(negative_impls)]
#![feature(auto_traits)]
#![feature(associated_type_bounds)]
#![feature(specialization)]
#![deny(unused_import_braces)]
#![allow(incomplete_features)]

pub mod message;
#[cfg(feature = "server")]
pub mod db;
#[cfg(feature = "client")]
pub mod client;
#[cfg(feature = "server")]
pub mod server;
mod handle;
mod stream;
pub mod serialization;
pub mod logger;

impl message::Message for String {}

#[derive(Copy, Clone)]
pub struct Uninitialized;
