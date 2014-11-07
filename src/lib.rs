//! Bindings to native thread-local-storage in a variety of flavors.

// TODO
//  * too many fields are public for static initialization
//  * the `statik` module leaks memory in the fallback implementation
//      * maybe we get this for free with dtors implemented?
//
//  * the `statik` module does not support dynamic initialization
//  * the `statik` module does not support dynamic destruction
//      * hm, ManuallyDrop seems like it would be... perfect here.
//


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
