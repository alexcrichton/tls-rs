//! Bindings to native thread-local-storage in a variety of flavors.

#![feature(macro_rules, unsafe_destructor)]
#![deny(missing_docs)]

extern crate libc;

mod statik;
pub mod os;
pub mod scoped;

// woohoo macro hygiene
mod tls {
    pub use {os, scoped};
}
