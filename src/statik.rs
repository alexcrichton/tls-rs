#![macro_escape]
#![allow(dead_code, missing_docs)]

pub use self::imp::Key;

pub struct Ref<T: 'static> { inner: &'static T }
pub struct RefMut<T: 'static> { inner: &'static mut T }

impl<T> Deref<T> for Ref<T> {
    fn deref<'a>(&'a self) -> &'a T { self.inner }
}
impl<T> Deref<T> for RefMut<T> {
    fn deref<'a>(&'a self) -> &'a T { &*self.inner }
}
impl<T> DerefMut<T> for RefMut<T> {
    fn deref_mut<'a>(&'a mut self) -> &'a mut T { &mut *self.inner }
}

#[cfg(feature = "thread-local")]
mod imp {
    #![macro_escape]

    use std::kinds::marker;

    use super::{Ref, RefMut};

    pub struct Key<T> {
        pub inner: T,
        pub nc: marker::NoCopy,
    }

    #[macro_export]
    macro_rules! tls(
        (static $name:ident: $t:ty = $init:expr) => (
            #[thread_local]
            static $name: ::tls::StaticKey<$t> = tls!($init);
        );
        (static mut $name:ident: $t:ty = $init:expr) => (
            #[thread_local]
            static mut $name: ::tls::StaticKey<$t> = tls!($init);
        );
        ($init:expr) => (
            ::tls::StaticKey {
                inner: $init,
                nc: ::std::kinds::marker::NoCopy,
            }
        );
    )

    impl<T> Key<T> {
        pub fn get(&'static self) -> Ref<T> {
            Ref { inner: &self.inner }
        }

        pub fn get_mut(&'static mut self) -> RefMut<T> {
            RefMut { inner: &mut self.inner }
        }
    }
}

#[cfg(not(feature = "thread-local"))]
mod imp {
    #![macro_escape]

    use std::mem;

    use super::{Ref, RefMut};
    use os::StaticKey as OsStaticKey;

    pub struct Key<T> {
        pub inner: T,
        pub os: OsStaticKey,
    }

    #[macro_export]
    macro_rules! tls(
        (static $name:ident: $t:ty = $init:expr) => (
            static $name: ::tls::StaticKey<$t> = tls!($init);
        );
        (static mut $name:ident: $t:ty = $init:expr) => (
            static mut $name: ::tls::StaticKey<$t> = tls!($init);
        );
        ($init:expr) => (
            ::tls::StaticKey {
                inner: $init,
                os: ::tls::os::INIT,
            }
        );
    )

    impl<T> Key<T> {
        pub fn get(&'static self) -> Ref<T> {
            unsafe { Ref { inner: &*self.ptr() } }
        }

        pub fn get_mut(&'static self) -> RefMut<T> {
            unsafe { RefMut { inner: &mut *self.ptr() } }
        }

        unsafe fn ptr(&self) -> *mut T {
            let ptr = self.os.get();
            if !ptr.is_null() {
                return ptr as *mut T
            }

            // If the lookup returned null, we haven't initialized our own local
            // copy, so do that now.
            let value: Box<T> = box mem::transmute_copy(&self.inner);
            let value: *mut T = mem::transmute(value);
            self.os.set(value as *mut u8);
            value as *mut T
        }
    }
}
