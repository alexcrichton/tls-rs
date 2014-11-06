#![macro_escape]
#![allow(dead_code)]

pub use self::imp::Tls;

#[cfg(feature = "thread-local")]
mod imp {
    #![macro_escape]

    use std::kinds::marker;

    use {Ref, RefMut};

    pub struct Tls<T> {
        pub inner: T,
        pub nc: marker::NoCopy,
    }

    #[macro_export]
    macro_rules! tls(
        (static $name:ident: $t:ty = $init:expr) => (
            #[thread_local]
            static $name: ::tls::StaticTls<$t> = tls!($init);
        );
        (static mut $name:ident: $t:ty = $init:expr) => (
            #[thread_local]
            static mut $name: ::tls::StaticTls<$t> = tls!($init);
        );
        ($init:expr) => (
            ::tls::StaticTls {
                inner: $init,
                nc: ::std::kinds::marker::NoCopy,
            }
        );
    )

    impl<T> Tls<T> {
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

    use {Ref, RefMut};
    use os::StaticTls as OsStaticTls;

    pub struct Tls<T> {
        pub inner: T,
        pub os: OsStaticTls,
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
                os: ::tls::os::INIT,
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
