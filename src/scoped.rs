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
//! FOO.with(|slot| {
//!     assert_eq!(slot, None);
//! });
//!
//! // When inserting a value into TLS, the value is only in place for the
//! // duration of the closure specified.
//! FOO.set(&1, || {
//!     FOO.with(|slot| {
//!         assert_eq!(slot.map(|x| *x), Some(1));
//!     });
//! });
//! # }
//! ```

#![macro_escape]

pub use self::imp::TlsInner;

pub struct Tls<T> { pub inner: TlsInner<T> }

/// Declare a new scoped TLS key.
///
/// This macro declares a `static` item on which methods are used to get and
/// set the TLS value stored within.
#[macro_export]
macro_rules! scoped_tls(
    (static $name:ident: $t:ty) => (
        scoped_tls_inner!(static $name: $t)
    );
)

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
    ///     let val = FOO.with(|v| *v.unwrap());
    ///     assert_eq!(val, 100);
    ///
    ///     // set can be called recursively
    ///     FOO.set(&101, || {
    ///         // ...
    ///     });
    ///
    ///     // Recursive calls restore the previous value.
    ///     let val = FOO.with(|v| *v.unwrap());
    ///     assert_eq!(val, 100);
    /// });
    /// # }
    /// ```
    pub fn set<R>(&'static self, t: &T, cb: || -> R) -> R {
        self.inner.set(t, cb)
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
    /// FOO.with(|slot| {
    ///     // work with `slot`
    /// });
    /// # }
    /// ```
    pub fn with<R>(&'static self, cb: |Option<&T>| -> R) -> R {
        self.inner.with(cb)
    }
}

#[cfg(feature = "thread-local")]
#[macro_escape]
mod imp {

    use std::cell::UnsafeCell;

    // TODO: Should be a `Cell`, but that's not `Sync`
    pub struct TlsInner<T> { pub inner: UnsafeCell<*mut T> }

    #[macro_export]
    macro_rules! scoped_tls_inner(
        (static $name:ident: $t:ty) => (
            #[thread_local]
            static $name: ::tls::ScopedTls<$t> = ::tls::ScopedTls {
                inner: ::tls::scoped::TlsInner {
                    inner: ::std::cell::UnsafeCell { value: 0 as *mut _ },
                }
            };
        );
    )

    impl<T> TlsInner<T> {
        pub fn set<R>(&'static self, t: &T, cb: || -> R) -> R {
            struct Reset<'a, T: 'a> {
                key: &'a UnsafeCell<*mut T>,
                val: *mut T,
            }
            #[unsafe_destructor]
            impl<'a, T> Drop for Reset<'a, T> {
                fn drop(&mut self) {
                    unsafe { *self.key.get() = self.val; }
                }
            }

            let prev = unsafe {
                let prev = *self.inner.get();
                *self.inner.get() = t as *const T as *mut T;
                prev
            };

            let _reset = Reset { key: &self.inner, val: prev };
            cb()
        }

        pub fn with<R>(&'static self, cb: |Option<&T>| -> R) -> R {
            unsafe {
                let ptr: *mut T = *self.inner.get();
                if ptr.is_null() {
                    cb(None)
                } else {
                    cb(Some(&*ptr))
                }
            }
        }
    }
}

#[cfg(not(feature = "thread-local"))]
#[macro_escape]
mod imp {
    use std::kinds::marker;
    use os::StaticTls as OsStaticTls;

    pub struct TlsInner<T> {
        pub inner: OsStaticTls,
        pub marker: marker::InvariantType<T>,
    }

    #[macro_export]
    macro_rules! scoped_tls(
        (static $name:ident: $t:ty) => (
            static $name: ::tls::ScopedTls<$t> = ::tls::ScopedTls {
                inner: ::tls::scoped::TlsInner {
                    inner: ::tls::os::INIT,
                    marker: ::std::kinds::marker::InvariantType,
                }
            };
        );
    )

    impl<T> TlsInner<T> {
        pub fn set<R>(&'static self, t: &T, cb: || -> R) -> R {
            struct Reset<'a> {
                key: &'a OsStaticTls,
                val: *mut u8,
            }
            #[unsafe_destructor]
            impl<'a> Drop for Reset<'a> {
                fn drop(&mut self) {
                    unsafe { self.key.set(self.val); }
                }
            }

            let prev = unsafe {
                let prev = self.inner.get();
                self.inner.set(t as *const T as *mut u8);
                prev
            };

            let _reset = Reset { key: &self.inner, val: prev };
            cb()
        }

        pub fn with<R>(&'static self, cb: |Option<&T>| -> R) -> R {
            unsafe {
                let ptr = self.inner.get() as *mut T;
                if ptr.is_null() {
                    cb(None)
                } else {
                    cb(Some(&*ptr))
                }
            }
        }
    }
}


#[cfg(test)]
mod tests {
    #[test]
    fn smoke() {
        scoped_tls!(static BAR: uint)

        BAR.with(|slot| {
            assert_eq!(slot, None);
        });
        BAR.set(&1, || {
            BAR.with(|slot| {
                assert_eq!(slot.map(|x| *x), Some(1));
            });
        });
        BAR.with(|slot| {
            assert_eq!(slot, None);
        });
    }
}
