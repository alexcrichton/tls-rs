//! Dynamically initialized, owning TLS
//!
//! This module, built on top of the `statik` module, implements dynamically
//! initialized TLS. This means that the first reference to a `Key` will run the
//! dynamic initialization routine, and all accesses afterwards will reference
//! the same contents.
//!
//! As with "static TLS", this flavor of TLS also supports values with
//! destructors, but has the same caveats as the static module.
//!
//! # Example
//!
//! ```
//! # #![feature(phase)]
//! # #[phase(plugin, link)] extern crate tls;
//! # fn main() {
//! use std::cell::RefCell;
//!
//! dynamic_tls!(static FOO: RefCell<Vec<uint>> = RefCell::new(vec![1, 2, 3]))
//!
//! let vec = FOO.get().unwrap();
//! assert_eq!(vec.borrow().len(), 3);
//! vec.borrow_mut().push(4);
//! assert_eq!(vec.borrow()[3], 4);
//! # }
//! ```

#![macro_escape]

use std::kinds::marker;
use std::cell::UnsafeCell;

use statik::Ref as StaticRef;

/// A dynamically initialized TLS key.
///
/// This form of TLS key represents an owned value in TLS which is dynamically
/// initialized upon the first reference to the value.
///
/// # Example
///
/// ```
/// # #![feature(phase)]
/// # #[phase(plugin, link)] extern crate tls;
/// # fn main() {
/// use std::cell::RefCell;
///
/// dynamic_tls!(static FOO: RefCell<Vec<uint>> = RefCell::new(vec![1, 2, 3]))
///
/// let vec = FOO.get().unwrap();
/// assert_eq!(vec.borrow().len(), 3);
/// vec.borrow_mut().push(4);
/// assert_eq!(vec.borrow()[3], 4);
/// # }
/// ```
pub struct Key<T: 'static> {
    // The key itself may be tagged with #[thread_local], and this `Key` is
    // stored as a `static`, and it's not valid for a static to reference the
    // address of another thread_local static. For this reason we kinda wonkily
    // work aroun this by generating a shim function which will give us the
    // address of the inner TLS key at runtime.
    //
    // TODO: this shoudl be a `RefCell` but it can't be statically initialized.
    #[doc(hidden)]
    pub inner: fn() -> Option<StaticRef<UnsafeCell<Option<T>>>>,
    // initialization routine to invoke to create a value
    #[doc(hidden)]
    pub init: fn() -> T,
}

/// A reference to a `Key`.
///
/// References implement `Deref` to the type `T` and are not sendable.
pub struct Ref<T: 'static> {
    inner: &'static T,
    marker1: marker::NoSend,
    marker2: marker::NoSync,
}

#[macro_export]
macro_rules! dynamic_tls(
    (static $name:ident: $t:ty = $init:expr) => (
        static $name: ::tls::dynamic::Key<$t> = dynamic_tls!($init, $t);
    );
    ($init:expr, $t:ty) => ({
        use std::cell::UnsafeCell as __UnsafeCell;
        tls!(static __KEY: __UnsafeCell<Option<$t>> = __UnsafeCell { value: None });
        fn __init() -> $t { $init }
        fn __getit() -> Option<::tls::statik::Ref<__UnsafeCell<Option<$t>>>> {
            __KEY.get()
        }
        ::tls::dynamic::Key {
            inner: __getit,
            init: __init,
        }
    });
)

impl<T: 'static> Key<T> {
    /// Access this TLS variable, lazily initializing it if necessary.
    ///
    /// The first time this function is called on each thread the TLS key will
    /// be initialized by having the specified init expression evaluated on the
    /// current thread.
    ///
    /// This function can return `None` for the same reasons of static TLS
    /// returning `None` (destructors are running or may have run).
    pub fn get(&'static self) -> Option<Ref<T>> {
        let slot = match (self.inner)() {
            Some(slot) => slot,
            None => return None,
        };
        unsafe {
            if (*slot.get()).is_none() {
                *slot.get() = Some((self.init)());
            }
            Some(Ref::new((*slot.get()).as_ref().unwrap()))
        }
    }
}

impl<T: 'static> Ref<T> {
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
