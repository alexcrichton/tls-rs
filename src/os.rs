//! OS-based thread local storage
//!
//! This module provides an implementation of OS-based thread local storage,
//! using the native OS-provided facilities (think `TlsAlloc` or
//! `pthread_setspecific`). The interface of this differs from the other types
//! of thread-local-storage provided in this crate in that OS-based TLS can only
//! get/set pointers,
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
//! let key = Key::new(None);
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
//! }
//! ```

#![allow(non_camel_case_types)]

use std::kinds::marker;
use std::mem;
use std::rt::exclusive::Exclusive;
use std::rt;
use std::sync::atomic::{mod, AtomicUint};
use std::sync::{Once, ONCE_INIT};

/// A type for TLS keys that are statically allocated.
///
/// This type is entirely `unsafe` to use as it does not protect against
/// use-after-deallocation or use-during-deallocation.
///
/// The actual OS-TLS key is lazily allocated when this is used for the first
/// time. The key is also deallocated when the Rust runtime exits or `destroy`
/// is called, whichever comes first.
///
/// # Example
///
/// ```
/// use tls::os::{StaticKey, INIT};
///
/// static KEY: StaticKey = INIT;
///
/// unsafe {
///     assert!(KEY.get().is_null());
///     KEY.set(1 as *mut u8);
/// }
/// ```
pub struct StaticKey {
    /// Inner static TLS key (internals), created with by `INIT_INNER` in this
    /// module.
    pub inner: StaticKeyInner,
    /// Destructor for the TLS value.
    pub dtor: Option<unsafe extern fn(*mut u8)>,
}

/// Inner contents of `StaticKey`, created by the `INIT_INNER` constant.
pub struct StaticKeyInner {
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
///
/// # Example
///
/// ```rust
/// use tls::os::Key;
///
/// let key = Key::new(None);
/// assert!(key.get().is_null());
/// key.set(1 as *mut u8);
/// assert!(!key.get().is_null());
///
/// drop(key); // deallocate this TLS slot.
/// ```
pub struct Key {
    key: imp::Key,
}

/// Constant initialization value for static TLS keys.
///
/// This value specifies no destructor by default.
pub const INIT: StaticKey = StaticKey {
    inner: INIT_INNER,
    dtor: None,
};

/// Constant initialization value for the inner part of static TLS keys.
///
/// This value allos specific configuration of the destructor for a TLS key.
pub const INIT_INNER: StaticKeyInner = StaticKeyInner {
    key: atomic::INIT_ATOMIC_UINT,
    nc: marker::NoCopy,
};

static INIT_KEYS: Once = ONCE_INIT;
static mut KEYS: *mut Exclusive<Vec<imp::Key>> = 0 as *mut _;

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
        match self.inner.key.swap(0, atomic::SeqCst) {
            0 => {}
            n => { unregister_key(n as imp::Key); imp::destroy(n as imp::Key) }
        }
    }

    unsafe fn key(&self) -> imp::Key {
        match self.inner.key.load(atomic::SeqCst) {
            0 => self.lazy_init() as imp::Key,
            n => n as imp::Key
        }
    }

    unsafe fn lazy_init(&self) -> uint {
        let key = imp::create(self.dtor);
        assert!(key != 0);
        match self.inner.key.compare_and_swap(0, key as uint, atomic::SeqCst) {
            // The CAS succeeded, so we've created the actual key
            0 => {
                register_key(key);
                key as uint
            }
            // If someone beat us to the punch, use their key instead
            n => { imp::destroy(key); n }
        }
    }
}

impl Key {
    /// Create a new managed OS TLS key.
    ///
    /// This key will be deallocated when the key falls out of scope.
    pub fn new(dtor: Option<unsafe extern fn(*mut u8)>) -> Key {
        Key { key: unsafe { imp::create(dtor) } }
    }

    /// See StaticKey::get
    pub fn get(&self) -> *mut u8 {
        unsafe { imp::get(self.key) }
    }

    /// See StaticKey::set
    pub fn set(&self, val: *mut u8) {
        unsafe { imp::set(self.key, val) }
    }
}

impl Drop for Key {
    fn drop(&mut self) {
        unsafe { imp::destroy(self.key) }
    }
}

fn init_keys() {
    INIT_KEYS.doit(|| {
        let keys = box Exclusive::new(Vec::<imp::Key>::new());
        unsafe {
            KEYS = mem::transmute(keys);
        }

        rt::at_exit(proc() unsafe {
            let keys: Box<Exclusive<Vec<imp::Key>>> = mem::transmute(KEYS);
            KEYS = 0 as *mut _;
            let keys = keys.lock();
            for key in keys.iter() {
                imp::destroy(*key);
            }
        });
    });
}

fn register_key(key: imp::Key) {
    init_keys();
    let mut keys = unsafe { (*KEYS).lock() };
    keys.push(key);
}

fn unregister_key(key: imp::Key) {
    init_keys();
    let mut keys = unsafe { (*KEYS).lock() };
    keys.retain(|k| *k != key);
}

#[cfg(unix)]
mod imp {
    use libc::c_int;

    pub type Key = pthread_key_t;

    pub unsafe fn create(dtor: Option<unsafe extern fn(*mut u8)>) -> Key {
        let mut key = 0;
        assert_eq!(pthread_key_create(&mut key, dtor), 0);
        return key;
    }

    pub unsafe fn set(key: Key, value: *mut u8) {
        debug_assert_eq!(pthread_setspecific(key, value), 0);
    }

    pub unsafe fn get(key: Key) -> *mut u8 {
        pthread_getspecific(key)
    }

    pub unsafe fn destroy(key: Key) {
        debug_assert_eq!(pthread_key_delete(key), 0);
    }

    #[cfg(target_os = "macos")]
    type pthread_key_t = ::libc::c_ulong;

    #[cfg(not(target_os = "macos"))]
    type pthread_key_t = ::libc::c_uint;

    extern {
        fn pthread_key_create(key: *mut pthread_key_t,
                              dtor: Option<unsafe extern fn(*mut u8)>) -> c_int;
        fn pthread_key_delete(key: pthread_key_t) -> c_int;
        fn pthread_getspecific(key: pthread_key_t) -> *mut u8;
        fn pthread_setspecific(key: pthread_key_t, value: *mut u8) -> c_int;
    }
}

#[cfg(windows)]
mod imp {
    use libc::types::os::arch::extra::{DWORD, LPVOID, BOOL};

    pub type Key = DWORD;

    pub unsafe fn create(dtor: Option<unsafe extern fn(*mut u8)>) -> Key {
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
        debug_assert!(TlsFree(key) != 0);
    }

    extern "system" {
        fn TlsAlloc() -> DWORD;
        fn TlsFree(dwTlsIndex: DWORD) -> BOOL;
        fn TlsGetValue(dwTlsIndex: DWORD) -> LPVOID;
        fn TlsSetValue(dwTlsIndex: DWORD, lpTlsvalue: LPVOID) -> BOOL;
    }
}

#[cfg(test)]
mod tests {
    use super::{Key, StaticKey, INIT_INNER};

    fn assert_sync<T: Sync>() {}
    fn assert_send<T: Send>() {}

    #[test]
    fn smoke() {
        assert_sync::<Key>();
        assert_send::<Key>();

        let k1 = Key::new(None);
        let k2 = Key::new(None);
        assert!(k1.get().is_null());
        assert!(k2.get().is_null());
        k1.set(1 as *mut _);
        k2.set(2 as *mut _);
        assert_eq!(k1.get() as uint, 1);
        assert_eq!(k2.get() as uint, 2);
    }

    #[test]
    fn statik() {
        static K1: StaticKey = StaticKey { inner: INIT_INNER, dtor: None };
        static K2: StaticKey = StaticKey { inner: INIT_INNER, dtor: None };

        unsafe {
            assert!(K1.get().is_null());
            assert!(K2.get().is_null());
            K1.set(1 as *mut _);
            K2.set(2 as *mut _);
            assert_eq!(K1.get() as uint, 1);
            assert_eq!(K2.get() as uint, 2);
        }
    }
}
