//! Owning TLS

#![macro_escape]

use std::kinds::marker;

#[doc(hidden)]
pub use self::imp::Key as KeyInner;
#[doc(hidden)]
pub use self::imp::destroy_value;

/// A TLS key which owns its contents.
///
/// This TLS key uses the fastest possible TLS implementation available to it
/// for the target platform. It is instantiated with the `tls!` macro and the
/// primary method is the `get` method.
///
/// The `get` method returns an object which represents a shared reference to
/// the TLS value for the current thread. The object cannot be sent across
/// tasks.
///
/// # Initialization and Destruction
///
/// Currently initialization and interior mutability must be done through
/// `UnsafeCell`, but hopefully this will change!
///
/// Values stored into TLS support destructors, and their destructors will be
/// run when a thread exits.
///
/// # Example
///
/// ```
/// # #![feature(phase)]
/// # #[phase(plugin, link)] extern crate tls;
/// # fn main() {
/// use std::cell::UnsafeCell;
///
/// tls!(static FOO: UnsafeCell<uint> = UnsafeCell { value: 1 });
///
/// unsafe {
///     let f = FOO.get();
///     assert_eq!(*f.get(), 1);
///     *f.get() = 2;
///
///     // each thread starts out with the initial value of 1
///     spawn(proc() {
///         assert_eq!(*FOO.get().get(), 1);
///         *FOO.get().get() = 3;
///     });
///
///     // we retain our original value of 2 despite the child thread
///     assert_eq!(*FOO.get().get(), 2);
/// }
/// # }
/// ```
pub struct Key<T> {
    #[doc(hidden)]
    pub inner: KeyInner<T>,
}

/// A structure representing a reference to a TLS value.
///
/// This structure implements `Deref` to the inner contents.
pub struct Ref<T: 'static> {
    inner: &'static T,
    marker1: marker::NoSend,
    marker2: marker::NoSync,
}

impl<T: 'static> Key<T> {
    /// Acquire a reference to the value in this TLS key.
    ///
    /// This may lazily initialize an OS-based TLS key, or the value itself.
    /// This method in general is quite cheap to call, however.
    pub fn get(&'static self) -> Ref<T> {
        self.inner.get()
    }
}

impl<T> Ref<T> {
    fn new(ptr: &'static T) -> Ref<T> {
        Ref {
            inner: ptr,
            marker1: marker::NoSend,
            marker2: marker::NoSync,
        }
    }
}

impl<T> Deref<T> for Ref<T> {
    fn deref<'a>(&'a self) -> &'a T { self.inner }
}

#[cfg(feature = "thread-local")]
mod imp {
    #![macro_escape]

    use std::cell::UnsafeCell;
    use std::intrinsics;
    use std::kinds::marker;
    use std::ptr;

    use super::Ref;

    #[doc(hidden)]
    pub struct Key<T> {
        pub inner: UnsafeCell<T>,
        pub dtor_registered: UnsafeCell<bool>, // should be Cell
        pub dtor_running: UnsafeCell<bool>,
        pub nc: marker::NoCopy,
    }

    #[macro_export]
    macro_rules! tls(
        (static $name:ident: $t:ty = $init:expr) => (
            #[thread_local]
            static $name: ::tls::statik::Key<$t> = tls!($init);
        );
        (static mut $name:ident: $t:ty = $init:expr) => (
            #[thread_local]
            static mut $name: ::tls::statik::Key<$t> = tls!($init);
        );
        ($init:expr) => (
            ::tls::statik::Key {
                inner: ::tls::statik::KeyInner {
                    inner: ::std::cell::UnsafeCell { value: $init },
                    nc: ::std::kinds::marker::NoCopy,
                    dtor_registered: ::std::cell::UnsafeCell { value: false },
                    dtor_running: ::std::cell::UnsafeCell { value: false },
                },
            }
        );
    )

    #[doc(hidden)]
    impl<T> Key<T> {
        pub fn get(&'static self) -> Ref<T> {
            unsafe {
                if *self.dtor_running.get() {
                    panic!("cannot access a TLS variable after it has been \
                            destroyed");
                }
                self.register_dtor();
                Ref::new(&*self.inner.get())
            }
        }

        unsafe fn register_dtor(&self) {
            if !intrinsics::needs_drop::<T>() || *self.dtor_registered.get() {
                return
            }

            register_dtor::<T>(self as *const _ as *mut u8,
                               destroy_value::<T>);
            *self.dtor_registered.get() = true;
        }
    }

    #[cfg(target_os = "linux")]
    unsafe fn register_dtor<T>(t: *mut u8, dtor: unsafe extern fn(*mut u8)) {
        use libc;
        extern {
            static __dso_handle: *mut u8;
            fn __cxa_thread_atexit_impl(dtor: unsafe extern fn(*mut u8),
                                        arg: *mut u8, dso_handle: *mut u8)
                                        -> libc::c_int;
        }
        __cxa_thread_atexit_impl(dtor, t, __dso_handle);
    }

    #[cfg(target_os = "macos")]
    unsafe fn register_dtor<T>(t: *mut u8, dtor: unsafe extern fn(*mut u8)) {
        extern {
            fn _tlv_atexit(dtor: unsafe extern fn(*mut u8),
                           arg: *mut u8);
        }
        _tlv_atexit(dtor, t);
    }

    #[doc(hidden)]
    pub unsafe extern fn destroy_value<T>(ptr: *mut u8) {
        let ptr = ptr as *mut Key<T>;
        *(*ptr).dtor_running.get() = true;
        ptr::read((*ptr).inner.get() as *const T);
    }
}

#[cfg(not(feature = "thread-local"))]
mod imp {
    #![macro_escape]

    use std::mem;

    use super::Ref;
    use os::StaticKey as OsStaticKey;

    #[doc(hidden)]
    pub struct Key<T> {
        pub inner: T,
        pub os: OsStaticKey,
    }

    struct Value<T> {
        key: &'static OsStaticKey,
        value: T,
    }

    #[macro_export]
    macro_rules! tls(
        (static $name:ident: $t:ty = $init:expr) => (
            static $name: ::tls::statik::Key<$t> = tls!($init, $t);
        );
        (static mut $name:ident: $t:ty = $init:expr) => (
            static mut $name: ::tls::statik::Key<$t> = tls!($init, $t);
        );
        ($init:expr, $t:ty) => ({
            unsafe extern fn __destroy(ptr: *mut u8) {
                ::tls::statik::destroy_value::<$t>(ptr);
            }
            ::tls::statik::Key {
                inner: ::tls::statik::KeyInner {
                    inner: $init,
                    os: ::tls::os::StaticKey {
                        inner: ::tls::os::INIT_INNER,
                        dtor: Some(__destroy),
                    },
                },
            }
        });
    )

    #[doc(hidden)]
    impl<T> Key<T> {
        pub fn get(&'static self) -> Ref<T> {
            unsafe { Ref::new(&*self.ptr()) }
        }

        unsafe fn ptr(&'static self) -> *mut T {
            let ptr = self.os.get() as *mut Value<T>;
            if !ptr.is_null() {
                if ptr as uint == 1 {
                    panic!("cannot access a TLS variable after it has been \
                            destroyed");
                }
                return &mut (*ptr).value as *mut T;
            }

            // If the lookup returned null, we haven't initialized our own local
            // copy, so do that now.
            let ptr: Box<Value<T>> = box Value {
                key: &self.os,
                value: mem::transmute_copy(&self.inner),
            };
            let ptr: *mut Value<T> = mem::transmute(ptr);
            self.os.set(ptr as *mut u8);
            &mut (*ptr).value as *mut T
        }
    }

    #[doc(hidden)]
    pub unsafe extern fn destroy_value<T>(ptr: *mut u8) {
        let ptr: Box<Value<T>> = mem::transmute(ptr);
        let key = ptr.key;
        key.set(1 as *mut _);
        drop(ptr);
        key.set(0 as *mut _);
    }
}

#[cfg(test)]
mod tests {
    use std::cell::UnsafeCell;

    struct Foo(Sender<()>);

    impl Drop for Foo {
        fn drop(&mut self) {
            let Foo(ref s) = *self;
            s.send(());
        }
    }

    #[test]
    fn smoke_no_dtor() {
        tls!(static FOO: UnsafeCell<int> = UnsafeCell { value: 1 })

        unsafe {
            let f = FOO.get();
            assert_eq!(*f.get(), 1);
            *f.get() = 2;
            let (tx, rx) = channel();
            spawn(proc() {
                assert_eq!(*FOO.get().get(), 1);
                tx.send(());
            });
            rx.recv();
            assert_eq!(*FOO.get().get(), 2);
        }
    }

    #[test]
    fn smoke_dtor() {
        tls!(static FOO: UnsafeCell<Option<Foo>> = UnsafeCell { value: None })

        let (tx, rx) = channel();
        spawn(proc() unsafe {
            *FOO.get().get() = Some(Foo(tx));
        });
        rx.recv();
    }
}
