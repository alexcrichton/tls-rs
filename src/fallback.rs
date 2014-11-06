#![macro_escape]
#![allow(non_camel_case_types)]

use std::kinds::marker;
use std::mem;
use std::rt;
use std::sync::atomic::{mod, AtomicUint};

use super::{os, Ref, RefMut};

pub struct Tls<T> {
    pub inner: T,
    pub key: AtomicUint,
    pub nc: marker::NoCopy,
}

#[macro_export]
macro_rules! tls(
    (static $name:ident: $t:ty = $init:expr) => (
        static $name: ::tls::StaticTls<$t> = tls!($init);
    );
    (static mut $name:ident: $t:ty = $init:expr) => (
        static mut $name: ::tls::StaticTls<$t> = tls!($init);
    );
    ($init:expr) => (
        ::tls::StaticTls {
            inner: $init,
            key: ::std::sync::atomic::INIT_ATOMIC_UINT,
            nc: ::std::kinds::marker::NoCopy,
        }
    );
)

impl<T> Tls<T> {
    pub fn get(&'static self) -> Ref<T> {
        unsafe { Ref { inner: &*self.ptr() } }
    }

    pub fn get_mut(&'static self) -> RefMut<T> {
        unsafe { RefMut { inner: &mut *self.ptr() } }
    }

    unsafe fn ptr(&self) -> *mut T {
        // If our key is still 0, then we need to do a lazy init
        let key = match self.key.load(atomic::SeqCst) {
            0 => self.lazy_init(),
            n => n
        } as os::Key;

        // Actually perform the TLS lookup
        let value = os::get(key);
        if !value.is_null() {
            return value as *mut T
        }

        // If the lookup returned null, we haven't initialized our own local
        // copy, so do that now.
        let value: Box<T> = box mem::transmute_copy(&self.inner);
        let value: *mut T = mem::transmute(value);
        os::set(key, value as *mut u8);
        value as *mut T
    }

    unsafe fn lazy_init(&self) -> uint {
        let key = os::create();
        assert!(key != 0);
        match self.key.compare_and_swap(0, key as uint, atomic::SeqCst) {
            // The CAS succeeded, schedule this key to be deleted
            0 => {
                rt::at_exit(proc() os::destroy(key));
                key as uint
            }
            // If someone beat us to the punch, use their key instead
            n => { os::destroy(key); n }
        }
    }
}
