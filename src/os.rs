//! OS-based thread local storage
//!
//! This module provides an implementation of OS-based thread local storage,
//! using the native OS-provided facilities. The interface of this differs from
//! the other types of thread-local-storage provided in this crate in that
//! OS-based TLS can only get/set pointers,
//!
//! This modules also provides two flavors of TLS. One is intended for static
//! initialization, and does not contain a `Drop` implementation to deallocate
//! the OS-TLS key. The other is a type which does implement `Drop` and hence
//! has a safe interface.
//!
//! # Usage
//!
//! This module should likely not be used directly unless other primitives are
//! being built on. types such as `tls::scoped::Key` are likely much more useful
//! in practice than this OS-based version which likely requires unsafe code to
//! interoperate with.
//!
//! # Example
//!
//! Using a dynamically allocated TLS key. Note that this key can be shared
//! among many threads via an `Arc`.
//!
//! ```rust
//! use tls::os::Key;
//!
//! let key = Key::new();
//! assert!(key.get().is_null());
//! key.set(1 as *mut u8);
//! assert!(!key.get().is_null());
//!
//! drop(key); // deallocate this TLS slot.
//! ```
//!
//! Sometimes a statically allocated key is either required or easier to work
//! with, however.
//!
//! ```rust
//! use tls::os::{StaticKey, INIT};
//!
//! static KEY: StaticKey = INIT;
//!
//! unsafe {
//!     assert!(KEY.get().is_null());
//!     KEY.set(1 as *mut u8);
//!
//!     // static keys must be manually deallocated
//!     KEY.destroy();
//! }
//! ```

#![allow(non_camel_case_types)]

use std::kinds::marker;
use std::sync::atomic::{mod, AtomicUint};

/// A type for TLS keys that are statically allocated.
///
/// This type is entirely `unsafe` to use as it does not ensure that it is
/// deallocated properly and it does not protect against use-after-deallocation
/// or use-during-deallocation.
///
/// The actual OS-TLS key is lazily allocated when this is used for the first
/// time.
pub struct StaticKey {
    key: AtomicUint,
    nc: marker::NoCopy,
}

/// A type for a safely managed OS-based TLS slot.
///
/// This type allocates an OS TLS key when it is initialized and will deallocate
/// the key when it falls out of scope. When compared with `StaticKey`, this
/// type is entirely safe to use.
///
/// Implementations will likely, however, contain unsafe code as this type only
/// operates on `*mut u8`, an unsafe pointer.
pub struct Key {
    inner: StaticKey,
}

/// Constant initialization value for static TLS keys.
pub const INIT: StaticKey = StaticKey {
    key: atomic::INIT_ATOMIC_UINT,
    nc: marker::NoCopy,
};

impl StaticKey {
    /// Gets the value associated with this TLS key
    ///
    /// This will lazily allocate a TLS key from the OS if one has not already
    /// been allocated.
    pub unsafe fn get(&self) -> *mut u8 { imp::get(self.key()) }

    /// Sets this TLS key to a new value.
    ///
    /// This will lazily allocate a TLS key from the OS if one has not already
    /// been allocated.
    pub unsafe fn set(&self, val: *mut u8) { imp::set(self.key(), val) }

    /// Deallocates this OS TLS key.
    ///
    /// This function is unsafe as there is no guarantee that the key is not
    /// currently in use by other threads or will not ever be used again.
    pub unsafe fn destroy(&self) {
        match self.key.swap(0, atomic::SeqCst) {
            0 => {}
            n => imp::destroy(n as imp::Key),
        }
    }

    unsafe fn key(&self) -> imp::Key {
        match self.key.load(atomic::SeqCst) {
            0 => self.lazy_init() as imp::Key,
            n => n as imp::Key
        }
    }

    unsafe fn lazy_init(&self) -> uint {
        let key = imp::create();
        assert!(key != 0);
        match self.key.compare_and_swap(0, key as uint, atomic::SeqCst) {
            // The CAS succeeded, so we've created the actual key
            0 => key as uint,
            // If someone beat us to the punch, use their key instead
            n => { imp::destroy(key); n }
        }
    }
}

impl Key {
    /// Create a new managed OS TLS key.
    ///
    /// This key will be deallocated when the key falls out of scope.
    pub fn new() -> Key {
        Key {
            inner: StaticKey {
                key: AtomicUint::new(unsafe { imp::create() as uint }),
                nc: marker::NoCopy
            }
        }
    }

    /// See StaticKey::get
    pub fn get(&self) -> *mut u8 { unsafe { self.inner.get() } }

    /// See StaticKey::set
    pub fn set(&self, val: *mut u8) { unsafe { self.inner.set(val) } }
}

impl Drop for Key {
    fn drop(&mut self) {
        unsafe { self.inner.destroy() }
    }
}

#[cfg(unix)]
mod imp {
    use libc::c_int;
    use std::ptr::null;

    pub type Key = pthread_key_t;

    pub unsafe fn create() -> Key {
        let mut key = 0;
        assert_eq!(pthread_key_create(&mut key, null()), 0);
        return key;
    }

    pub unsafe fn set(key: Key, value: *mut u8) {
        assert_eq!(pthread_setspecific(key, value), 0);
    }

    pub unsafe fn get(key: Key) -> *mut u8 {
        pthread_getspecific(key)
    }

    pub unsafe fn destroy(key: Key) {
        assert_eq!(pthread_key_delete(key), 0);
    }

    #[cfg(target_os = "macos")]
    type pthread_key_t = ::libc::c_ulong;

    #[cfg(not(target_os = "macos"))]
    type pthread_key_t = ::libc::c_uint;

    extern {
        fn pthread_key_create(key: *mut pthread_key_t, dtor: *const u8) -> c_int;
        fn pthread_key_delete(key: pthread_key_t) -> c_int;
        fn pthread_getspecific(key: pthread_key_t) -> *mut u8;
        fn pthread_setspecific(key: pthread_key_t, value: *mut u8) -> c_int;
    }
}

#[cfg(windows)]
mod imp {
    use libc::types::os::arch::extra::{DWORD, LPVOID, BOOL};

    pub type Key = DWORD;

    pub unsafe fn create() -> Key {
        const TLS_OUT_OF_INDEXES: DWORD = 0xFFFFFFFF;
        let key = TlsAlloc();
        assert!(key != TLS_OUT_OF_INDEXES);
        return key;
    }

    pub unsafe fn set(key: Key, value: *mut u8) {
        assert!(TlsSetValue(key, value as LPVOID) != 0)
    }

    pub unsafe fn get(key: Key) -> *mut u8 {
        TlsGetValue(key) as *mut u8
    }

    pub unsafe fn destroy(key: Key) {
        assert!(TlsFree(key) != 0);
    }

    extern "system" {
        fn TlsAlloc() -> DWORD;
        fn TlsFree(dwTlsIndex: DWORD) -> BOOL;
        fn TlsGetValue(dwTlsIndex: DWORD) -> LPVOID;
        fn TlsSetValue(dwTlsIndex: DWORD, lpTlsvalue: LPVOID) -> BOOL;
    }
}
