#![macro_escape]

use std::kinds::marker;

use super::{Ref, RefMut};

pub struct Tls<T> {
    pub inner: T,
    pub nc: marker::NoCopy,
}

#[macro_export]
macro_rules! tls(
    (static $name:ident: $t:ty = $init:expr) => (
        #[thread_local]
        static $name: ::tls::StaticTls<$t> = ::tls::StaticTls {
            inner: $init,
            nc: ::std::kinds::marker::NoCopy,
        };
    );
    (static mut $name:ident: $t:ty = $init:expr) => (
        #[thread_local]
        static mut $name: ::tls::StaticTls<$t> = ::tls::StaticTls {
            inner: $init,
            nc: ::std::kinds::marker::NoCopy,
        };
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
