//! Statically initialized, owning TLS
//!
//! This module implements a "flavor" of TLS where the TLS slot owns the data
//! that it contains. All contents, however, must be statically initialized. Any
//! value which does not contain references is permitted inside a key of this
//! type. This means that owning TLS keys also allow for content with
//! destructors.
//!
//! The destructor for TLS values will run when the thread exits. There are some
//! nuances about destructors, however:
//!
//! * A TLS key cannot be accessed while its destructor is running.
//! * A TLS key may not be accessible after its destructor has run.
//! * Repeately setting TLS keys during destruction may cause memory leaks.
//! * A `panic!` in a TLS destructor will result in a process abort.
//! * TLS destructors may not be run when the application exits (the entire
//!   process is exiting anyway).
//!
//! It is generally recommended to avoid TLS from destructors themselves, and if
//! required only doing so in a deterministic, non-cyclic fashion.
//!
//! This form of TLS will also attempt to select the "fastest" implementation of
//! TLS available for the target platform.
//!
//! # Example
//!
//! ```
//! # #![feature(phase)]
//! # #[phase(plugin, link)] extern crate tls;
//! # fn main() {
//! use std::cell::UnsafeCell;
//!
//! tls!(static FOO: UnsafeCell<uint> = UnsafeCell { value: 1 });
//!
//! unsafe {
//!     let f = FOO.get().unwrap();
//!     assert_eq!(*f.get(), 1);
//!     *f.get() = 2;
//!
//!     // each thread starts out with the initial value of 1
//!     spawn(proc() {
//!         let f = FOO.get().unwrap();
//!         assert_eq!(*f.get(), 1);
//!         *f.get() = 3;
//!     });
//!
//!     // we retain our original value of 2 despite the child thread
//!     assert_eq!(*FOO.get().unwrap().get(), 2);
//! }
//! # }
//! ```

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
///     let f = FOO.get().unwrap();
///     assert_eq!(*f.get(), 1);
///     *f.get() = 2;
///
///     // each thread starts out with the initial value of 1
///     spawn(proc() {
///         let f = FOO.get().unwrap();
///         assert_eq!(*f.get(), 1);
///         *f.get() = 3;
///     });
///
///     // we retain our original value of 2 despite the child thread
///     assert_eq!(*FOO.get().unwrap().get(), 2);
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
    /// This may lazily initializes an OS-based TLS key. This method in general
    /// is quite cheap to call, however.
    ///
    /// This function will return `None` if the TLS value is currently being
    /// destroyed, and it may also return `None` after the key has been
    /// destroyed.
    pub fn get(&'static self) -> Option<Ref<T>> {
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
        // Place the inner bits in an `UnsafeCell` to currently get around the
        // "only Sync statics" restriction. This allows any type to be placed in
        // the cell.
        //
        // Note that all access required `T: 'static` so it can't be a type with
        // any borrowed pointers still.
        pub inner: UnsafeCell<T>,

        // Metadata to keep track of the state of the destructor. Remember that
        // these variables are thread-local, not global.
        pub dtor_registered: UnsafeCell<bool>, // should be Cell
        pub dtor_running: UnsafeCell<bool>, // should be Cell

        // These shouldn't be copied around.
        pub nc: marker::NoCopy,
    }

    #[macro_export]
    macro_rules! tls(
        (static $name:ident: $t:ty = $init:expr) => (
            #[thread_local]
            static $name: ::tls::statik::Key<$t> = tls!($init, $t);
        );
        ($init:expr, $t:ty) => (
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
        pub fn get(&'static self) -> Option<Ref<T>> {
            unsafe {
                if intrinsics::needs_drop::<T>() && *self.dtor_running.get() {
                    return None
                }
                self.register_dtor();
                Some(Ref::new(&*self.inner.get()))
            }
        }

        unsafe fn register_dtor(&self) {
            if !intrinsics::needs_drop::<T>() || *self.dtor_registered.get() {
                return
            }

            register_dtor(self as *const _ as *mut u8,
                          destroy_value::<T>);
            *self.dtor_registered.get() = true;
        }
    }

    // Since what appears to be glibc 2.18 this symbol has been shipped which
    // GCC and clang both use to invoke destructors in thread_local globals, so
    // let's do the same!
    //
    // Note, however, that we run on lots older linuxes, as well as cross
    // compiling from a newer linux to an older linux, so we also have a
    // fallback implementation to use as well.
    //
    // Due to rust-lang/rust#18804, make sure this is not generic!
    #[cfg(target_os = "linux")]
    unsafe fn register_dtor(t: *mut u8, dtor: unsafe extern fn(*mut u8)) {
        use std::mem;
        use os;
        use libc;
        extern {
            static __dso_handle: *mut u8;
            #[linkage = "extern_weak"]
            static __cxa_thread_atexit_impl: *const ();
        }
        if !__cxa_thread_atexit_impl.is_null() {
            type F = unsafe extern fn(dtor: unsafe extern fn(*mut u8),
                                      arg: *mut u8,
                                      dso_handle: *mut u8) -> libc::c_int;
            mem::transmute::<*const (), F>(__cxa_thread_atexit_impl)
            (dtor, t, __dso_handle);
            return
        }

        // The fallback implementation uses a vanilla OS-based TLS key to track
        // the list of destructors that need to be run for this thread. The key
        // then has its own destructor which runs all the other destructors.
        //
        // The destructor for DTORS is a little special in that it has a `while`
        // loop to continuously drain the list of registered destructors. It
        // *should* be the case that this loop always terminates because we
        // provide the guarantee that a TLS key cannot be set after it is
        // flagged for destruction.
        static DTORS: os::StaticKey = os::StaticKey {
            inner: os::INIT_INNER,
            dtor: Some(run_dtors),
        };
        type List = Vec<(*mut u8, unsafe extern fn(*mut u8))>;
        if DTORS.get().is_null() {
            let v: Box<List> = box Vec::new();
            DTORS.set(mem::transmute(v));
        }
        let list: &mut List = &mut *(DTORS.get() as *mut List);
        list.push((t, dtor));

        unsafe extern fn run_dtors(mut ptr: *mut u8) {
            while !ptr.is_null() {
                let list: Box<List> = mem::transmute(ptr);
                for &(ptr, dtor) in list.iter() {
                    dtor(ptr);
                }
                ptr = DTORS.get();
                DTORS.set(0 as *mut _);
            }
        }
    }

    // OSX's analog of the above linux function is this _tlv_atexit function.
    // The disassembly of thread_local globals in C++ (at least produced by
    // clang) will have this show up in the output.
    #[cfg(target_os = "macos")]
    unsafe fn register_dtor(t: *mut u8, dtor: unsafe extern fn(*mut u8)) {
        extern {
            fn _tlv_atexit(dtor: unsafe extern fn(*mut u8),
                           arg: *mut u8);
        }
        _tlv_atexit(dtor, t);
    }

    #[doc(hidden)]
    pub unsafe extern fn destroy_value<T>(ptr: *mut u8) {
        let ptr = ptr as *mut Key<T>;
        // Right before we run the user destructor be sure to flag the
        // destructor as running for this thread so calls to `get` will return
        // `None`.
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
        pub valid: OsStaticKey,
    }

    struct Value<T: 'static> {
        key: &'static Key<T>,
        value: T,
    }

    #[macro_export]
    macro_rules! tls(
        (static $name:ident: $t:ty = $init:expr) => (
            static $name: ::tls::statik::Key<$t> = tls!($init, $t);
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
                    valid: ::tls::os::INIT,
                },
            }
        });
    )

    #[doc(hidden)]
    impl<T> Key<T> {
        pub fn get(&'static self) -> Option<Ref<T>> {
            unsafe {
                self.ptr().map(|p| Ref::new(&*p))
            }
        }

        unsafe fn ptr(&'static self) -> Option<*mut T> {
            let ptr = self.os.get() as *mut Value<T>;
            if !ptr.is_null() {
                if ptr as uint == 1 {
                    return None
                }
                return Some(&mut (*ptr).value as *mut T);
            }

            // If the lookup returned null, we haven't initialized our own local
            // copy, so do that now.
            //
            // Also note that this transmute_copy should be ok because the value
            // `inner` is already validated to be a valid `static` value, so we
            // should be able to freely copy the bits.
            let ptr: Box<Value<T>> = box Value {
                key: self,
                value: mem::transmute_copy(&self.inner),
            };
            let ptr: *mut Value<T> = mem::transmute(ptr);
            self.os.set(ptr as *mut u8);
            Some(&mut (*ptr).value as *mut T)
        }
    }

    #[doc(hidden)]
    pub unsafe extern fn destroy_value<T: 'static>(ptr: *mut u8) {
        // The OS TLS ensures that this key contains a NULL value when this
        // destructor starts to run. We set it back to a sentinel value of 1 to
        // ensure that any future calls to `get` for this thread will return
        // `None`.
        //
        // Note that to prevent an infinite loop we reset it back to null right
        // before we return from the destructor ourselves.
        let ptr: Box<Value<T>> = mem::transmute(ptr);
        let key = ptr.key;
        key.os.set(1 as *mut u8);
        drop(ptr);
        key.os.set(0 as *mut u8);
    }
}

#[cfg(test)]
mod tests {
    use std::cell::UnsafeCell;
    use std::rt::thread::Thread;

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
            let f = FOO.get().unwrap();
            assert_eq!(*f.get(), 1);
            *f.get() = 2;
            let (tx, rx) = channel();
            spawn(proc() {
                assert_eq!(*FOO.get().unwrap().get(), 1);
                tx.send(());
            });
            rx.recv();
            assert_eq!(*FOO.get().unwrap().get(), 2);
        }
    }

    #[test]
    fn smoke_dtor() {
        tls!(static FOO: UnsafeCell<Option<Foo>> = UnsafeCell { value: None })

        let (tx, rx) = channel();
        spawn(proc() unsafe {
            *FOO.get().unwrap().get() = Some(Foo(tx));
        });
        rx.recv();
    }

    #[test]
    fn circular() {
        struct S1;
        struct S2;
        tls!(static K1: UnsafeCell<Option<S1>> = UnsafeCell { value: None })
        tls!(static K2: UnsafeCell<Option<S2>> = UnsafeCell { value: None })
        static mut HITS: uint = 0;

        impl Drop for S1 {
            fn drop(&mut self) {
                unsafe {
                    HITS += 1;
                    match K2.get() {
                        Some(slot) => {
                            if HITS == 1 {
                                *slot.get() = Some(S2);
                            } else {
                                assert_eq!(HITS, 3);
                            }
                        }
                        None => assert_eq!(HITS, 3),
                    }
                }
            }
        }
        impl Drop for S2 {
            fn drop(&mut self) {
                unsafe {
                    HITS += 1;
                    match K1.get() {
                        Some(slot) => {
                            assert_eq!(HITS, 2);
                            *slot.get() = Some(S1);
                        }
                        None => unreachable!(),
                    }
                }
            }
        }

        Thread::start(proc() {
            drop(S1);
        }).join();
    }

    #[test]
    fn self_referential() {
        struct S1;
        tls!(static K1: UnsafeCell<Option<S1>> = UnsafeCell { value: None })

        impl Drop for S1 {
            fn drop(&mut self) {
                assert!(K1.get().is_none());
            }
        }

        Thread::start(proc() unsafe {
            *K1.get().unwrap().get() = Some(S1);
        }).join();
    }

    #[test]
    fn dtors_in_dtors_in_dtors() {
        struct S1(Sender<()>);
        tls!(static K1: UnsafeCell<Option<S1>> = UnsafeCell { value: None })
        tls!(static K2: UnsafeCell<Option<Foo>> = UnsafeCell { value: None })

        impl Drop for S1 {
            fn drop(&mut self) {
                let S1(ref tx) = *self;
                unsafe {
                    match K2.get() {
                        Some(slot) => *slot.get() = Some(Foo(tx.clone())),
                        None => {}
                    }
                }
            }
        }

        let (tx, rx) = channel();
        spawn(proc() unsafe {
            *K1.get().unwrap().get() = Some(S1(tx));
        });
        rx.recv();
    }
}
