#![allow(non_camel_case_types)]

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
