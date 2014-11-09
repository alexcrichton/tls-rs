#![feature(phase)]

#[phase(plugin, link)] extern crate tls;

use std::cell::UnsafeCell;
use std::io::Command;
use std::os;
use std::rt::thread::Thread;
use std::str;

fn main() {
    let args = os::args();
    if args.len() > 1 && args[1].as_slice() == "child" {
        return test();
    }

    let output = Command::new(&args[0]).arg("child").output().unwrap();
    let out = str::from_utf8(output.output.as_slice()).unwrap();
    let err = str::from_utf8(output.error.as_slice()).unwrap();
    assert!(!output.status.success());
    assert!(err.contains("cannot access a TLS variable after it has been \
                          destroyed"), "bad output: {}", err);
    assert_eq!(out, "");
}

fn test() {
    struct S1;
    struct S2;
    tls!(static K1: UnsafeCell<Option<S1>> = UnsafeCell { value: None })
    tls!(static K2: UnsafeCell<Option<S2>> = UnsafeCell { value: None })

    impl Drop for S1 {
        fn drop(&mut self) {
            unsafe {
                *K2.get().get() = Some(S2);
            }
        }
    }
    impl Drop for S2 {
        fn drop(&mut self) {
            unsafe {
                *K1.get().get() = Some(S1);
            }
        }
    }

    Thread::start(proc() {
        drop((S1, S2));
    }).join();
}
