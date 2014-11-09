//! Bindings to native thread-local-storage in a variety of flavors.

#![feature(macro_rules, unsafe_destructor, linkage)]
#![deny(missing_docs)]

extern crate libc;

pub mod statik;
pub mod os;
pub mod scoped;
pub mod dynamic;

// woohoo macro hygiene
mod tls {
    pub use {os, scoped, statik, dynamic};
}
