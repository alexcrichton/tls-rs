//! Bindings to native thread-local-storage in a variety of flavors.
//!
//! This library contains an implementation of TLS keys and values for Rust.
//! There are a few prominent options to choose from when thinking about using a
//! TLS variable:
//!
//! * OS-based TLS. The most `unsafe` of the bunch, this is provided by the `os`
//!   module in this crate and simply contains raw bindings to the system APIs
//!   for TLS storage. Usage of this type will likely require a number of
//!   `unsafe` blocks.
//!
//! * Scoped TLS. Perhaps the "safest" of the bunch, this form of TLS provides a
//!   scoping mechanism where values are placed into TLS for a known period of
//!   time. Provided by the `scoped` module of this crate, borrowed pointers are
//!   stored into TLS for the duration of a closure body, and the borrowed
//!   pointers can be freely borrowed at any time (with shared borrows).
//!
//! * Static TLS. This form of TLS is for data which can be statically
//!   initialized. The data is automatically available for all threads, and
//!   destructors will also be run as necessary for any threads which access the
//!   TLS data. This form is somewhat "unsafe" in terms of destructors, so
//!   please be sure to consult the documentation. This functionality is
//!   provided by the `statik` module in this crate.
//!
//! * Dynamic TLS. This is the closest to C++11's `thread_local` keyword
//!   provided by this crate. This is implemented on top of static TLS, but also
//!   allows for dynamic initialization as well as destruction of values. This
//!   form comes in the `dynamic` module and is also associated with the same
//!   caveats as static TLS.
//!
//! Each module provides a type with the name `Key` which corresponds to that
//! kind of TLS key. With the exception of `os`, each module also attempts to
//! compile to the fastest version available. This is sometimes the `os` module
//! itself, but some platforms can leverage LLVM's specialized support for
//! thread local globals as well.
//!
//! # Usage
//!
//! TLS keys should be thought of as new `static` variables which serve as a
//! handle to acquire a value from. Each module in this crate provides a macro
//! or `const` to create this `static` for consumer crates. Detailed examples
//! can be found on each module, but a general overview looks like:
//!
//! ```
//! #![feature(phase)]
//! #[phase(plugin, link)]
//! extern crate tls;
//!
//! use std::num::Int;
//! use tls::os;
//!
//! static OS_KEY: os::StaticKey = os::INIT;
//! static OS_KEY_WITH_DTOR: os::StaticKey = os::StaticKey {
//!     inner: os::INIT_INNER,
//!     dtor: Some(dtor),
//! };
//!
//! // Creates a `static` of type tls::scoped::Key
//! scoped_tls!(static SCOPED_KEY: uint)
//!
//! // Creates a `static` of type tls::statik::Key
//! tls!(static STATIC_KEY: uint = 5)
//!
//! // Creates a `static` of type tls::dynamic::Key
//! dynamic_tls!(static DYNAMIC_KEY: uint = 5i.count_ones())
//!
//! fn main() {
//!     // see api docs for each respective type
//! }
//!
//! unsafe extern fn dtor(ptr: *mut u8) { /* ... */ }
//! ```

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
