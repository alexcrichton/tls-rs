//! Scoped thread-local storage
//!
//! This module provides the ability to generate *scoped* thread-local
//! variables. In this sense, scoped indicates that TLS actually stores a
//! reference to a value, and this reference is only placed in TLS for a scoped
//! amount of time.
//!
//! There are no restrictions on what types can be placed into a scoped TLS
//! variable, but all scoped variables are initialized to the equivalent of
//! null. Scoped TLS is useful when a value is present in TLS for a known period
//! of time and it is not required for TLS to take ownership of the contents.
//!
//! # Example
//!
//! ```
//! # #![feature(phase)]
//! # #[phase(plugin, link)] extern crate tls;
//! # fn main() {
//! scoped_tls!(static FOO: uint)
//!
//! // Initially each scoped TLS slot is empty.
//! FOO.get(|slot| {
//!     assert_eq!(slot, None);
//! });
//!
//! // When inserting a value into TLS, the value is only in place for the
//! // duration of the closure specified.
//! FOO.set(&1, || {
//!     FOO.get(|slot| {
//!         assert_eq!(slot.map(|x| *x), Some(1));
//!     });
//! });
//! # }
//! ```

#![macro_escape]

use super::StaticTls;
use std::cell::UnsafeCell;

pub struct Tls<T: 'static> {
    pub inner: StaticTls<UnsafeCell<*mut T>>,
}

/// Declare a new scoped TLS key.
///
/// This macro declares a `static` item on which methods are used to get and set
/// the TLS value stored within.
#[macro_export]
macro_rules! scoped_tls(
    (static $name:ident: $t:ty) => (
        static $name: ::tls::ScopedTls<$t> = ::tls::ScopedTls {
            inner: tls!(::std::cell::UnsafeCell { value: 0 as *mut $t })
        };
    );
)

struct Reset<T: 'static> {
    key: &'static StaticTls<UnsafeCell<*mut T>>,
    val: *mut T,
}

impl<T: 'static> Tls<T> {
    /// Insert a value into this scoped TLS slot for a duration of a closure.
    ///
    /// While `cb` is running, the value `t` will be returned by `get` unless
    /// this function is called recursively inside of `cb`.
    ///
    /// Upon return, this function will restore the previous TLS value, if any
    /// was available.
    ///
    /// # Example
    ///
    /// ```
    /// # #![feature(phase)]
    /// # #[phase(plugin, link)] extern crate tls;
    /// # fn main() {
    /// scoped_tls!(static FOO: uint)
    ///
    /// FOO.set(&100, || {
    ///     let val = FOO.get(|v| *v.unwrap());
    ///     assert_eq!(val, 100);
    ///
    ///     // set can be called recursively
    ///     FOO.set(&101, || {
    ///         // ...
    ///     });
    ///
    ///     // Recursive calls restore the previous value.
    ///     let val = FOO.get(|v| *v.unwrap());
    ///     assert_eq!(val, 100);
    /// });
    /// # }
    /// ```
    pub fn set<R>(&'static self, t: &T, cb: || -> R) -> R {
        let prev = unsafe {
            let cell = self.inner.get();
            let prev = *cell.get();
            *cell.get() = t as *const T as *mut T;
            prev
        };
        let _reset = Reset { key: &self.inner, val: prev };
        cb()
    }

    /// Get a value out of this scoped TLS variable.
    ///
    /// This function takes a closure which receives the value of this TLS
    /// variable, if any is available. If this variable has not yet been set,
    /// then `None` is yielded.
    ///
    /// # Example
    ///
    /// ```
    /// # #![feature(phase)]
    /// # #[phase(plugin, link)] extern crate tls;
    /// # fn main() {
    /// scoped_tls!(static FOO: uint)
    ///
    /// FOO.get(|slot| {
    ///     // work with `slot`
    /// });
    /// # }
    /// ```
    pub fn get<R>(&'static self, cb: |Option<&T>| -> R) -> R {
        unsafe {
            let ptr: *mut T = self.inner.get().value;
            if ptr.is_null() {
                cb(None)
            } else {
                cb(Some(&*ptr))
            }
        }
    }
}


#[unsafe_destructor]
impl<T: 'static> Drop for Reset<T> {
    fn drop(&mut self) {
        unsafe {
            *self.key.get().get() = self.val;
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn smoke() {
        scoped_tls!(static BAR: uint)

        BAR.get(|slot| {
            assert_eq!(slot, None);
        });
        BAR.set(&1, || {
            BAR.get(|slot| {
                assert_eq!(slot.map(|x| *x), Some(1));
            });
        });
        BAR.get(|slot| {
            assert_eq!(slot, None);
        });
    }
}
