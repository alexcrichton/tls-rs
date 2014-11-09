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
    ///
    /// See `Key::new` for information about when the destructor runs and how
    /// it runs.
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
    ///
    /// Note that this does *not* run the user-provided destructor if one was
    /// specified at definition time. Doing so must be done manually.
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
    ///
    /// The argument provided is an optionally-specified destructor for the
    /// value of this TLS key. When a thread exits and the value for this key
    /// is non-null the destructor will be invoked. The TLS value will be reset
    /// to null before the destructor is invoked.
    ///
    /// Note that the destructor will not be run when the `Key` goes out of
    /// scope.
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
}

fn register_key(key: imp::Key) {
    INIT_KEYS.doit(init_keys);
    let mut keys = unsafe { (*KEYS).lock() };
    keys.push(key);
}

fn unregister_key(key: imp::Key) {
    INIT_KEYS.doit(init_keys);
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
        let r = pthread_setspecific(key, value);
        debug_assert_eq!(r, 0);
    }

    pub unsafe fn get(key: Key) -> *mut u8 {
        pthread_getspecific(key)
    }

    pub unsafe fn destroy(key: Key) {
        let r = pthread_key_delete(key);
        debug_assert_eq!(r, 0);
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
#[allow(dead_code)]
mod imp {
    use std::mem;
    use std::rt;
    use std::rt::exclusive::Exclusive;
    use std::sync::{ONCE_INIT, Once};
    use libc::types::os::arch::extra::{DWORD, LPVOID, BOOL};

    pub type Key = DWORD;
    pub type Dtor = unsafe extern fn(*mut u8);

    // Turns out, like pretty much everything, Windows is pretty close the
    // functionality that Unix provides, but slightly different! In the case of
    // TLS, Windows does not provide an API to provide a destructor for a TLS
    // variable. This ends up being pretty crucial to this implementation, so we
    // need a way around this.
    //
    // The solution here ended up being a little obscure, but fear not, the
    // internet has informed me [1][2] that this solution is not unique (no way
    // I could have thought of it as well!). The key idea is to insert some hook
    // somewhere to run arbitrary code on thread termination. With this in place
    // we'll be able to run anything we like, including all TLS destructors!
    //
    // To accomplish this feat, we perform a number of tasks, all contained
    // within this module:
    //
    // * All TLS destructors are tracked by *us*, not the windows runtime. This
    //   means that we have a global list of destructors for each TLS key that
    //   we know about.
    // * When a TLS key is destroyed, we're sure to remove it from the dtor list
    //   if it's in there.
    // * When a thread exits, we run over the entire list and run dtors for all
    //   non-null keys. This attempts to match Unix semantics in this regard.
    //
    // This ends up having the overhead of using a global list, having some
    // locks here and there, and in general just adding some more code bloat. We
    // attempt to optimize runtime by forgetting keys that don't have
    // destructors, but this only gets us so far.
    //
    // For more details and nitty-gritty, see the code sections below!
    //
    // [1]: http://www.codeproject.com/Articles/8113/Thread-Local-Storage-The-C-Way
    // [2]: https://github.com/ChromiumWebApps/chromium/blob/master/base
    //                        /threading/thread_local_storage_win.cc#L42

    static INIT_DTORS: Once = ONCE_INIT;
    static mut DTORS: *mut Exclusive<Vec<(Key, Dtor)>> = 0 as *mut _;

    // -------------------------------------------------------------------------
    // Native bindings
    //
    // This section is just raw bindings to the native functions that Windows
    // provides, There's a few extra calls to deal with destructors.

    pub unsafe fn create(dtor: Option<Dtor>) -> Key {
        const TLS_OUT_OF_INDEXES: DWORD = 0xFFFFFFFF;
        let key = TlsAlloc();
        assert!(key != TLS_OUT_OF_INDEXES);
        match dtor {
            Some(f) => register_dtor(key, f),
            None => {}
        }
        return key;
    }

    pub unsafe fn set(key: Key, value: *mut u8) {
        let r = TlsSetValue(key, value as LPVOID);
        debug_assert!(r != 0);
    }

    pub unsafe fn get(key: Key) -> *mut u8 {
        TlsGetValue(key) as *mut u8
    }

    pub unsafe fn destroy(key: Key) {
        if unregister_dtor(key) {
            // FIXME: Currently if a key has a destructor associated with it we
            //        can't actually ever unregister it. If we were to
            //        unregister it, then any key destruction would have to be
            //        serialized with respect to actually running destructors.
            //
            //        We want to avoid a race where right before run_dtors runs
            //        some destructors TlsFree is called. Allowing the call to
            //        TlsFree would imply that the caller understands that *all
            //        known threads* are not exiting, which is quite a difficult
            //        thing to know!
            //
            //        For now we just leak all keys with dtors to "fix" this.
            //        Note that source [2] above shows precedent for this sort
            //        of strategy.
        } else {
            let r = TlsFree(key)
            debug_assert!(r != 0);
        }
    }

    extern "system" {
        fn TlsAlloc() -> DWORD;
        fn TlsFree(dwTlsIndex: DWORD) -> BOOL;
        fn TlsGetValue(dwTlsIndex: DWORD) -> LPVOID;
        fn TlsSetValue(dwTlsIndex: DWORD, lpTlsvalue: LPVOID) -> BOOL;
    }

    // -------------------------------------------------------------------------
    // Dtor registration
    //
    // These functions are associated with registering and unregistering
    // destructors. They're pretty simple, they just push onto a vector and scan
    // a vector currently.
    //
    // FIXME: This could probably be at least a little faster with a BTree.

    fn init_dtors() {
        let dtors = box Exclusive::new(Vec::<(Key, Dtor)>::new());
        unsafe {
            DTORS = mem::transmute(dtors);
        }

        rt::at_exit(proc() unsafe {
            mem::transmute::<_, Box<Exclusive<Vec<(Key, Dtor)>>>>(DTORS);
            DTORS = 0 as *mut _;
        });
    }

    unsafe fn register_dtor(key: Key, dtor: Dtor) {
        INIT_DTORS.doit(init_dtors);
        let mut dtors = (*DTORS).lock();
        dtors.push((key, dtor));
    }

    unsafe fn unregister_dtor(key: Key) -> bool {
        if DTORS.is_null() { return false }
        let mut dtors = (*DTORS).lock();
        let before = dtors.len();
        dtors.retain(|&(k, _)| k != key);
        dtors.len() != before
    }

    // -------------------------------------------------------------------------
    // Where the Magic (TM) Happens
    //
    // If you're looking at this code, and wondering "what is this doing?",
    // you're not alone! I'll try to break this down step by step:
    //
    // # What's up with CRT$XLB?
    //
    // For anything about TLS destructors to work on Windows, we have to be able
    // to run *something* when a thread exits. To do so, we place a very special
    // static in a very special location. If this is encoded in just the right
    // way, the kernel's loader is apparently nice enough to run some function
    // of ours whenever a thread exits! How nice of the kernel!
    //
    // Lots of detailed information can be found in source [1] above, but the
    // gist of it is that this is leveraging a feature of Microsoft's PE format
    // (executable format) which is not actually used by any compilers today.
    // This apparently translates to any callbacks in the ".CRT$XLB" section
    // being run on certain events.
    //
    // So after all that, we use the compiler's #[link_section] feature to place
    // a callback pointer into the magic section so it ends up being called.
    //
    // # What's up with this callback?
    //
    // The callback specified receives a number of parameters from... someone!
    // (the kernel? the runtime? I'm not qute sure!) There are a few events that
    // this gets invoked for, but we're currentl only interested on when a
    // thread or a process "detaches" (exits). The process part happens for the
    // last thread and the thread part happens for any normal thread.
    //
    // # Ok, what's up with running all these destructors?
    //
    // This will likely need to be improved over time, but this function
    // attempts a "poor man's" destructor callback system. To do this we clone a
    // local copy of the dtor list to start out with. This is our fudgy attempt
    // to not hold the lock while destructors run and not worry about the list
    // changing while we're looking at it.
    //
    // Once we've got a list of what to run, we iterate over all keys, check
    // their values, and then run destructors if the values turn out to be non
    // null (setting them to null just beforehand). We do this a few times in a
    // loop to basically match Unix semantics. If we don't reach a fixed point
    // after a short while then we just inevitably leak something most likely.
    //
    // # The article mentions crazy stuff about "/INCLUDE"?
    //
    // It sure does! This seems to work for now, so maybe we'll just run into
    // that if we start linking with msvc?

    #[link_section = ".CRT$XLB"]
    #[linkage = "extern"]
    #[allow(warnings)]
    pub static p_thread_callback: unsafe extern "system" fn(LPVOID, DWORD,
                                                            LPVOID) =
            on_tls_callback;

    #[allow(warnings)]
    unsafe extern "system" fn on_tls_callback(h: LPVOID,
                                              dwReason: DWORD,
                                              pv: LPVOID) {
        const DLL_THREAD_DETACH: DWORD = 3;
        const DLL_PROCESS_DETACH: DWORD = 0;
        if dwReason == DLL_THREAD_DETACH || dwReason == DLL_PROCESS_DETACH {
            run_dtors();
        }
    }

    unsafe fn run_dtors() {
        if DTORS.is_null() { return }
        let mut any_run = true;
        for _ in range(0, 5i) {
            if !any_run { break }
            any_run = false;
            let dtors = (*DTORS).lock().iter().map(|p| *p).collect::<Vec<_>>();
            for &(key, dtor) in dtors.iter() {
                let ptr = TlsGetValue(key);
                if !ptr.is_null() {
                    TlsSetValue(key, 0 as *mut _);
                    dtor(ptr as *mut _);
                    any_run = true;
                }
            }
        }
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
