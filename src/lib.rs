//! Bindings to native thread-local-storage in a variety of flavors.

// TODO
//  * the `statik` module does not support dynamic initialization
//  * for `statik`, think about failure in destructors
//  * destructors for windows
//  * application exit does not run pthread TLS dtors
//
//  * detect __cxa_thread_atexit_impl at runtime, linuxes are all over the place
//
// WISHLIST
//
// * #[thread_local] => static can be non-Sync
// * too many fields are public for static initialization


#![feature(macro_rules, unsafe_destructor)]
#![deny(missing_docs)]

extern crate libc;

pub mod statik;
pub mod os;
pub mod scoped;

// woohoo macro hygiene
mod tls {
    pub use {os, scoped, statik};
}
