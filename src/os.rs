#![allow(non_camel_case_types)]

use std::kinds::marker;
use std::sync::atomic::{mod, AtomicUint};

pub struct StaticTls {
    key: AtomicUint,
    nc: marker::NoCopy,
}

pub struct Tls {
    inner: StaticTls,
}

pub const INIT: StaticTls = StaticTls {
    key: atomic::INIT_ATOMIC_UINT,
    nc: marker::NoCopy,
};

impl StaticTls {
    pub unsafe fn get(&self) -> *mut u8 { imp::get(self.key()) }
    pub unsafe fn set(&self, val: *mut u8) { imp::set(self.key(), val) }

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

impl Tls {
    pub fn new() -> Tls {
        Tls {
            inner: StaticTls {
                key: AtomicUint::new(unsafe { imp::create() as uint }),
                nc: marker::NoCopy
            }
        }
    }

    pub fn get(&self) -> *mut u8 { unsafe { self.inner.get() } }
    pub fn set(&self, val: *mut u8) { unsafe { self.inner.set(val) } }
}

impl Drop for Tls {
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
