//! Bindings to native thread-local-storage in a variety of flavors.

// TODO
//  * application exit does not run pthread TLS dtors
//
// WISHLIST
//
// * #[thread_local] => static can be non-Sync
// * too many fields are public for static initialization


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
