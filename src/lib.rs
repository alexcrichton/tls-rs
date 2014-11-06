#![feature(macro_rules, unsafe_destructor)]

extern crate libc;

pub use statik::Tls as StaticTls;
pub use scoped::Tls as ScopedTls;

#[cfg(feature = "thread-local")] #[path = "thread_local.rs"] pub mod statik;
#[cfg(not(feature = "thread-local"))] #[path = "fallback.rs"] pub mod statik;

#[cfg(unix)] #[path = "unix.rs"] pub mod os;
#[cfg(windows)] #[path = "windows.rs"] pub mod os;

pub mod scoped;

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

// woohoo macro hygiene
mod tls {
    pub use {StaticTls, ScopedTls};
}
