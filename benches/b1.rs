#![feature(phase, thread_local)]

#[phase(plugin, link)]
extern crate tls;
extern crate test;

use std::cell::{Cell, UnsafeCell};
use test::Bencher;

const N: uint = 1000;

#[bench]
fn local_data(b: &mut Bencher) {
    local_data_key!(FOO: Cell<uint>)
    b.iter(|| {
        FOO.replace(Some(Cell::new(0)));
        for _ in range(0, N) {
            let s = FOO.get().unwrap();
            s.set(s.get() + 1);
        }
        FOO.get().unwrap()
    });
}

#[bench]
fn scoped(b: &mut Bencher) {
    scoped_tls!(static FOO: Cell<uint>)

    #[inline(never)]
    fn doit() -> uint {
        for _ in range(0, N) {
            FOO.with(|slot| {
                let slot = slot.unwrap();
                slot.set(slot.get() + 1);
            });
        }
        FOO.with(|slot| slot.unwrap().get())
    }

    b.iter(|| {
        let slot = Cell::new(0);
        FOO.set(&slot, doit)
    });
}

#[bench]
fn statik(b: &mut Bencher) {
    tls!(static FOO: UnsafeCell<uint> = UnsafeCell { value: 0 })

    #[inline(never)]
    unsafe fn doit() -> uint {
        for _ in range(0, N) {
            let slot = FOO.get().unwrap();
            *slot.get() = *slot.get() + 1;
            test::black_box(&FOO);
        }
        *FOO.get().unwrap().get()
    }

    b.iter(|| unsafe { doit() });
}

#[bench]
fn dynamic(b: &mut Bencher) {
    dynamic_tls!(static FOO: Cell<uint> = Cell::new(0))

    #[inline(never)]
    fn doit() -> uint {
        for _ in range(0, N) {
            let slot = FOO.get().unwrap();
            slot.set(slot.get() + 1);
            test::black_box(&slot);
        }
        FOO.get().unwrap().get()
    }

    b.iter(doit);
}

#[bench]
fn statik_os(b: &mut Bencher) {
    static FOO: tls::os::StaticKey = tls::os::INIT;

    #[inline(never)]
    unsafe fn doit() -> uint {
        for _ in range(0, N) {
            let val = FOO.get() as uint;
            FOO.set((val + 1) as *mut _);
        }
        FOO.get() as uint
    }

    b.iter(|| unsafe { doit() });
}

#[bench]
fn os(b: &mut Bencher) {
    let key = tls::os::Key::new(None);

    #[inline(never)]
    fn doit(key: &tls::os::Key) -> uint {
        for _ in range(0, N) {
            let val = key.get() as uint;
            key.set((val + 1) as *mut _);
        }
        key.get() as uint
    }

    b.iter(|| doit(&key));
}

#[bench]
fn thread_local(b: &mut Bencher) {
    #[thread_local]
    static FOO: UnsafeCell<uint> = UnsafeCell { value: 0 };

    #[inline(never)]
    unsafe fn doit() -> uint {
        for _ in range(0, N) {
            *FOO.get() = *FOO.get() + 1;
            test::black_box(&FOO);
        }
        *FOO.get()
    }

    b.iter(|| unsafe { doit() });
}

#[bench]
fn local_variable(b: &mut Bencher) {
    #[inline(never)]
    fn doit(slot: &mut uint) -> uint {
        for _ in range(0, N) {
            *slot += 1;
            test::black_box(&slot);
        }
        *slot
    }

    b.iter(|| {
        let mut slot = 0;
        doit(&mut slot)
    });
}

#[bench]
fn global_variable(b: &mut Bencher) {
    static mut FOO: uint = 0;
    #[inline(never)]
    unsafe fn doit() -> uint {
        for _ in range(0, N) {
            FOO += 1;
            test::black_box(&FOO);
        }
        FOO
    }

    b.iter(|| unsafe { doit() });
}

#[bench]
fn noop(b: &mut Bencher) {
    #[inline(never)]
    fn doit(slot: &mut uint) -> uint {
        for _ in range(0, N) {
            test::black_box(&slot);
        }
        *slot
    }

    b.iter(|| {
        let mut slot = 0;
        doit(&mut slot)
    });
}
